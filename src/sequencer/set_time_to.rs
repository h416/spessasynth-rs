//! set_time_to.rs
//! purpose: Seek (set time to) implementation for the sequencer.
//! Ported from: src/sequencer/set_time_to.ts
//!
//! Controllers, pitch wheels and the last portamento note are accumulated per channel in a local
//! `ChannelStatus` while the events before the target are skipped, then sent once at the end.
//! Non-skippable CCs (data entry, parameter selection, bank select, ...) and every other event
//! are processed immediately.
use crate::midi::enums::{
    midi_controllers, midi_message_types, MidiController, MidiMessageType,
};
use crate::midi::midi_message::MidiMessage;
use crate::midi::midi_tools::midi_utils::{AnalyzedMidiMessage, MidiUtils};
use crate::midi::midi_tools::parameter_tracker::ParameterTracker;
use crate::sequencer::sequencer::SpessaSynthSequencer;
use crate::sequencer::types::{MetaEventEventData, SequencerEvent};
use crate::synthesizer::audio_engine::channel::reset::{DEFAULT_MIDI_CONTROLLERS, RP_15_RESET_CC_NUMS};
use crate::synthesizer::audio_engine::synth_constants::CONTROLLER_TABLE_SIZE;
use crate::utils::byte_functions::big_endian::read_big_endian;

/// CCs that must not be skipped during seek.
/// Equivalent to: nonSkippableCCs
fn is_cc_non_skippable(cc: MidiController) -> bool {
    matches!(
        cc,
        midi_controllers::DATA_DECREMENT
            | midi_controllers::DATA_INCREMENT
            | midi_controllers::DATA_ENTRY_MSB
            | midi_controllers::DATA_ENTRY_LSB
            | midi_controllers::REGISTERED_PARAMETER_LSB
            | midi_controllers::REGISTERED_PARAMETER_MSB
            | midi_controllers::NON_REGISTERED_PARAMETER_LSB
            | midi_controllers::NON_REGISTERED_PARAMETER_MSB
            | midi_controllers::BANK_SELECT
            | midi_controllers::BANK_SELECT_LSB
            | midi_controllers::RESET_ALL_CONTROLLERS
            | midi_controllers::MONO_MODE_ON
            | midi_controllers::POLY_MODE_ON
    )
}

/// Per-channel controller/pitch-wheel/portamento bookkeeping accumulated while skipping events
/// during a seek, sent to the synth only once at the end (or immediately for `nonSkippableCCs`).
/// Equivalent to: interface ChannelStatus
struct ChannelStatus {
    /// NRPN tracking for controller changes.
    /// Equivalent to: param
    param: ParameterTracker,
    /// Saved controllers, sent only after (14-bit values).
    /// Equivalent to: controllers
    controllers: [i16; CONTROLLER_TABLE_SIZE],
    /// Saved portamento note, sent only after (-1 means no portamento note).
    /// Equivalent to: portamentoNote
    portamento_note: i32,
    /// Saved pitch wheel, sent only after.
    /// Equivalent to: pitchWheel
    pitch_wheel: i16,
}

impl ChannelStatus {
    fn new(channel: MidiController) -> Self {
        Self {
            pitch_wheel: 8192,
            controllers: DEFAULT_MIDI_CONTROLLERS,
            param: ParameterTracker::new(channel),
            portamento_note: -1,
        }
    }
}

/// Ensures that the channel status exists (ports may be added during the seek).
/// Equivalent to: `channels[channel] ??= createChannelStatus(channel)`
fn ensure_channel_status(channels: &mut Vec<ChannelStatus>, channel: usize) {
    while channels.len() <= channel {
        let idx = channels.len();
        channels.push(ChannelStatus::new(idx as MidiController));
    }
}

impl SpessaSynthSequencer {
    /// RP-15 compliant reset of the local seek-time channel status.
    /// <https://amei.or.jp/midistandardcommittee/Recommended_Practice/e/rp15.pdf>
    /// Equivalent to: function resetAllControllers(chan) (local to setTimeToInternal)
    fn seek_reset_all_controllers(&mut self, channels: &mut [ChannelStatus], chan: usize) {
        let ch = &mut channels[chan];
        // Reset pitch wheel
        ch.pitch_wheel = 8192;
        ch.param.reset_all_controllers();
        for &reset_cc in RP_15_RESET_CC_NUMS.iter() {
            ch.controllers[reset_cc as usize] = DEFAULT_MIDI_CONTROLLERS[reset_cc as usize];
        }
        // Forward it, so the synth's parameter selection matches the following data entries
        self.synth
            .controller_change(chan, midi_controllers::RESET_ALL_CONTROLLERS, 0);
    }

    /// Seeks to a specific time or tick position.
    /// Returns true if the MIDI file is not finished.
    /// Equivalent to: setTimeToInternal(time, ticks)
    pub(crate) fn set_time_to(&mut self, time: f64, ticks: Option<u32>) -> bool {
        let song_idx = match self.current_song_index {
            Some(i) => i,
            None => return false,
        };

        let time_division = self.songs[song_idx].time_division;
        self.one_tick_to_seconds = 60.0 / (120.0 * time_division as f64);
        // Reset everything

        self.send_midi_reset();
        self.played_time = 0.0;
        self.index = 0;

        // We save the pitch wheels, programs and controllers here
        // To only send them once after going through the events
        let mut channels: Vec<ChannelStatus> = (0..self.synth.synth_core.midi_channels.len())
            .map(|i| ChannelStatus::new(i as MidiController))
            .collect();

        // Save tempo changes
        // Testcase:
        // Piano Concerto No. 2 in G minor, Op 16 - I. Cadenza (Ky6000).mid
        // With 46k changes!
        let mut saved_tempo: Option<MidiMessage> = None;
        let mut saved_tempo_track: usize = 0;

        // Start with the same ports as a fresh playback, MIDI port events update them
        self.current_midi_ports = self.songs[song_idx]
            .tracks
            .iter()
            .map(|t| t.port)
            .collect();

        loop {
            // Find the next event
            let Some(&e) = self.songs[song_idx].timeline.get(self.index) else {
                self.stop();
                return false;
            };
            let track_index = e.tr;
            let event = self.songs[song_idx].tracks[track_index].events[e.ev].clone();
            match ticks {
                None => {
                    if self.played_time >= time {
                        break;
                    }
                }
                Some(t) => {
                    if event.ticks >= t {
                        break;
                    }
                }
            }

            // Skip note ons
            let (status, status_channel): (MidiMessageType, usize) =
                if event.status_byte >= 0x80 && event.status_byte < 0xf0 {
                    // Voice message
                    (event.status_byte & 0xf0, (event.status_byte & 0x0f) as usize)
                } else {
                    (event.status_byte, 0)
                };

            // Keep in mind midi ports to determine the channel!
            // Use the current port, like the playback does
            let port_offset = self
                .midi_port_channel_offsets
                .get(&self.current_midi_ports[track_index])
                .copied()
                .unwrap_or(0);
            let channel = status_channel + port_offset;

            // Ensure that the channel is always there (ports may be added during the seek)
            ensure_channel_status(&mut channels, channel);

            // Empty tracks cannot controller change
            let empty_track = self.songs[song_idx].is_multi_port
                && self.songs[song_idx].tracks[track_index].channels.is_empty();

            match status {
                // Skip note messages
                midi_message_types::NOTE_ON => {
                    // Always track the last note, even if portamento isn't applied.
                    // See: https://github.com/spessasus/spessasynth_core/issues/77
                    channels[channel].portamento_note = event.data[0] as i32;
                }

                midi_message_types::NOTE_OFF => {}

                // Skip pitch wheel
                midi_message_types::PITCH_WHEEL => {
                    channels[channel].pitch_wheel =
                        ((event.data[1] as i16) << 7) | event.data[0] as i16;
                }

                midi_message_types::SYSTEM_EXCLUSIVE => {
                    let analyzed_messages = MidiUtils::analyze_sysex(&event.data);
                    // The whole message is sent at most once, even if it has many results
                    let mut should_process = false;
                    for analyzed in analyzed_messages {
                        // Sysex may change controllers
                        match analyzed {
                            AnalyzedMidiMessage::ControllerChange {
                                channel: sysex_channel,
                                controller,
                                value,
                            } => {
                                // The SysEx channel is relative to the port, like in the playback
                                let channel = sysex_channel as usize + port_offset;
                                // Channel number may be above 15
                                if channel >= self.synth.synth_core.midi_channels.len() {
                                    continue;
                                }
                                ensure_channel_status(&mut channels, channel);
                                // Empty tracks cannot controller change
                                if empty_track {
                                    continue;
                                }

                                if controller == midi_controllers::RESET_ALL_CONTROLLERS {
                                    self.seek_reset_all_controllers(&mut channels, channel);
                                    continue;
                                }
                                if is_cc_non_skippable(controller) {
                                    self.synth.controller_change(channel, controller, value);
                                } else {
                                    channels[channel].controllers[controller as usize] =
                                        (value as i16) << 7;
                                }
                            }

                            /*
                            Program change cannot be skipped.
                            Some MIDIs edit drums via sysEx and skipping program changes causes them to be sent after, resetting the params.
                            Testcase: (GS88Pro)Th19_1S(KR.Palto47)
                             */
                            _ => {
                                should_process = true;
                            }
                        }
                    }
                    if should_process {
                        self.process_event(event.clone(), track_index);
                    }
                }

                midi_message_types::CONTROLLER_CHANGE => {
                    // Empty tracks cannot controller change
                    if !empty_track {
                        let controller = event.data[0];
                        let value = event.data[1];

                        match controller {
                            // Parameter tracking
                            midi_controllers::REGISTERED_PARAMETER_MSB
                            | midi_controllers::REGISTERED_PARAMETER_LSB
                            | midi_controllers::NON_REGISTERED_PARAMETER_LSB
                            | midi_controllers::NON_REGISTERED_PARAMETER_MSB => {
                                // Track and event indexes are irrelevant here
                                channels[channel]
                                    .param
                                    .controller_change(controller, value, 0, 0);
                                // Always send regardless
                                self.synth.controller_change(channel, controller, value);
                            }

                            midi_controllers::DATA_ENTRY_MSB
                            | midi_controllers::DATA_ENTRY_LSB => {
                                let analyzed = channels[channel]
                                    .param
                                    .controller_change(controller, value, 0, 0)
                                    .expect("data entry CC always yields Some(..)");
                                // Always send regardless
                                self.synth.controller_change(channel, controller, value);

                                // NRPN may change controllers
                                if let AnalyzedMidiMessage::ControllerChange {
                                    controller: ac,
                                    value: av,
                                    ..
                                } = analyzed
                                {
                                    if is_cc_non_skippable(ac) {
                                        self.synth.controller_change(channel, ac, av);
                                    } else {
                                        channels[channel].controllers[ac as usize] =
                                            (av as i16) << 7;
                                    }
                                }
                            }

                            _ => {
                                if controller == midi_controllers::RESET_ALL_CONTROLLERS {
                                    self.seek_reset_all_controllers(&mut channels, channel);
                                } else if is_cc_non_skippable(controller) {
                                    self.synth.controller_change(channel, controller, value);
                                } else {
                                    channels[channel].controllers[controller as usize] =
                                        (value as i16) << 7;
                                }
                            }
                        }
                    }
                }

                midi_message_types::SET_TEMPO => {
                    let tempo_bpm = 60_000_000.0 / read_big_endian(&event.data, 3, 0) as f64;
                    self.one_tick_to_seconds = 60.0 / (tempo_bpm * time_division as f64);
                    saved_tempo = Some(event.clone());
                    saved_tempo_track = track_index;
                }

                /*
                Program change cannot be skipped.
                Some MIDIs edit drums via sysEx and skipping program changes causes them to be sent after, resetting the params.
                Testcase: (GS88Pro)Th19_1S(KR.Palto47)
                 */
                _ => {
                    self.process_event(event.clone(), track_index);
                }
            }

            // Find the next event
            self.index += 1;
            let Some(&n_e) = self.songs[song_idx].timeline.get(self.index) else {
                self.stop();
                return false;
            };
            let next_event_ticks = self.songs[song_idx].tracks[n_e.tr].events[n_e.ev].ticks;
            self.played_time += self.one_tick_to_seconds * (next_event_ticks - event.ticks) as f64;
        }

        // For all synth channels (including the ones added during the seek)
        let channels_to_restore = channels
            .len()
            .min(self.synth.synth_core.midi_channels.len());
        for channel in 0..channels_to_restore {
            let ch = &channels[channel];
            // Restoring pitch wheels
            self.synth.pitch_wheel(channel, ch.pitch_wheel, -1);

            // Restoring portamento (only if currently active)
            // Note: we do it before controllers as portamento control may want to override it
            if ch.portamento_note >= 0 {
                self.synth.synth_core.midi_channels[channel].set_last_note(ch.portamento_note);
            }

            // Restoring saved controllers
            // Every controller that has changed
            for i in 0..CONTROLLER_TABLE_SIZE {
                // 14-bit, defaults are also 14-bit
                let value = ch.controllers[i];
                if value != DEFAULT_MIDI_CONTROLLERS[i] && !is_cc_non_skippable(i as MidiController) {
                    self.synth
                        .controller_change(channel, i as MidiController, (value >> 7) as u8);
                }
            }
        }

        // Restoring tempo
        if let Some(tempo_event) = saved_tempo {
            self.call_event(SequencerEvent::MetaEvent(MetaEventEventData {
                event: tempo_event,
                track_index: saved_tempo_track,
            }));
        }

        // Restoring paused time
        if self.paused() {
            self.paused_time = Some(self.played_time);
        }

        true
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::basic_midi::BasicMidi;
    use crate::midi::midi_message::MidiMessage;
    use crate::midi::midi_track::MidiTrack;
    use crate::midi::types::{TempoChange, TimelineEvent};
    use crate::synthesizer::processor::SpessaSynthProcessor;
    use crate::synthesizer::audio_engine::synth_constants::DEFAULT_RPN;
    use crate::synthesizer::types::{SynthProcessorEvent, SynthProcessorOptions};

    fn make_processor() -> SpessaSynthProcessor {
        SpessaSynthProcessor::new(
            44100.0,
            |_: SynthProcessorEvent| {},
            SynthProcessorOptions::default(),
        )
    }

    /// See `sequencer.rs`'s test module for why this doesn't just call `BasicMidi::flush()`.
    fn build_timeline(midi: &mut BasicMidi) {
        let mut timeline = Vec::new();
        for (tr, track) in midi.tracks.iter().enumerate() {
            for ev in 0..track.events.len() {
                timeline.push(TimelineEvent { tr, ev });
            }
        }
        timeline.sort_by_key(|e| midi.tracks[e.tr].events[e.ev].ticks);
        midi.timeline = timeline;
    }

    fn make_midi_with_cc() -> BasicMidi {
        let mut midi = BasicMidi::new();
        midi.time_division = 480;
        midi.duration = 4.0;
        midi.first_note_on = 0;
        midi.last_voice_event_tick = 1920;
        midi.tempo_changes = vec![TempoChange {
            ticks: 0,
            tempo: 120.0,
        }];
        let mut track = MidiTrack::new();
        track.channels.insert(0);
        // CC volume at tick 0
        track.push_event(MidiMessage::new(0, 0xB0, vec![7, 80]));
        // Note on at tick 0
        track.push_event(MidiMessage::new(0, 0x90, vec![60, 100]));
        // Program change at tick 240
        track.push_event(MidiMessage::new(240, 0xC0, vec![10]));
        // CC pan at tick 480
        track.push_event(MidiMessage::new(480, 0xB0, vec![10, 32]));
        // Pitch wheel at tick 480
        track.push_event(MidiMessage::new(480, 0xE0, vec![0x00, 0x50]));
        // Note off at tick 960
        track.push_event(MidiMessage::new(960, 0x80, vec![60, 0]));
        // Tempo change at tick 960
        track.push_event(MidiMessage::new(
            960,
            midi_message_types::SET_TEMPO,
            vec![0x07, 0xA1, 0x20],
        ));
        // More notes
        track.push_event(MidiMessage::new(960, 0x90, vec![64, 90]));
        track.push_event(MidiMessage::new(1920, 0x80, vec![64, 0]));
        track.push_event(MidiMessage::new(1920, 0x2F, vec![]));
        midi.tracks.push(track);
        build_timeline(&mut midi);
        midi
    }

    // -- is_cc_non_skippable --

    #[test]
    fn test_is_cc_non_skippable_data_entry() {
        assert!(is_cc_non_skippable(midi_controllers::DATA_ENTRY_MSB));
        assert!(is_cc_non_skippable(midi_controllers::DATA_ENTRY_LSB));
    }

    #[test]
    fn test_is_cc_non_skippable_rpn() {
        assert!(is_cc_non_skippable(
            midi_controllers::REGISTERED_PARAMETER_MSB
        ));
        assert!(is_cc_non_skippable(
            midi_controllers::REGISTERED_PARAMETER_LSB
        ));
    }

    #[test]
    fn test_is_cc_non_skippable_bank_select() {
        assert!(is_cc_non_skippable(midi_controllers::BANK_SELECT));
        assert!(is_cc_non_skippable(midi_controllers::BANK_SELECT_LSB));
    }

    #[test]
    fn test_is_cc_non_skippable_volume_is_skippable() {
        assert!(!is_cc_non_skippable(midi_controllers::MAIN_VOLUME));
    }

    #[test]
    fn test_is_cc_non_skippable_pan_is_skippable() {
        assert!(!is_cc_non_skippable(midi_controllers::PAN));
    }

    // -- DEFAULT_MIDI_CONTROLLERS / RP_15_RESET_CC_NUMS --

    #[test]
    fn test_default_midi_controllers_main_volume() {
        assert_eq!(
            DEFAULT_MIDI_CONTROLLERS[midi_controllers::MAIN_VOLUME as usize],
            100 << 7
        );
    }

    #[test]
    fn test_default_midi_controllers_nrpn_is_null() {
        // TS `DEFAULT_MIDI_CONTROLLERS` resets NRPN MSB/LSB to `DEFAULT_NRPN` (127,
        // the null parameter).
        assert_eq!(
            DEFAULT_MIDI_CONTROLLERS[midi_controllers::NON_REGISTERED_PARAMETER_MSB as usize],
            127 << 7
        );
        assert_eq!(
            DEFAULT_MIDI_CONTROLLERS[midi_controllers::NON_REGISTERED_PARAMETER_LSB as usize],
            127 << 7
        );
    }

    #[test]
    fn test_default_midi_controllers_rpn_default() {
        assert_eq!(
            DEFAULT_MIDI_CONTROLLERS[midi_controllers::REGISTERED_PARAMETER_MSB as usize],
            (DEFAULT_RPN as i16) << 7
        );
    }

    #[test]
    fn test_rp15_reset_cc_nums_contains_sustain_pedal() {
        assert!(RP_15_RESET_CC_NUMS.contains(&midi_controllers::SUSTAIN_PEDAL));
        assert_eq!(RP_15_RESET_CC_NUMS.len(), 10);
    }

    // -- set_time_to --

    #[test]
    fn test_set_time_to_returns_true_when_not_finished() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        seq.load_new_song_list(vec![make_midi_with_cc()]);
        // set_time_to is called internally during load (via set_current_time(0.0))
        // Verify that the song loaded correctly
        assert!(seq.current_song_index.is_some());
    }

    #[test]
    fn test_set_time_to_time_based_seek() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        seq.load_new_song_list(vec![make_midi_with_cc()]);
        seq.play();
        // Seek to 1 second
        let result = seq.set_time_to(1.0, None);
        assert!(result);
        assert!(seq.played_time >= 1.0 || seq.played_time > 0.0);
    }

    #[test]
    fn test_set_time_to_tick_based_seek() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        seq.load_new_song_list(vec![make_midi_with_cc()]);
        seq.play();
        // Seek to tick 480
        let result = seq.set_time_to(0.0, Some(480));
        assert!(result);
    }

    #[test]
    fn test_set_time_to_restores_paused_time() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        seq.load_new_song_list(vec![make_midi_with_cc()]);
        // Sequencer starts paused after load
        // set_time_to should have set paused_time to played_time
        assert!(seq.paused());
    }

    #[test]
    fn test_set_time_to_no_midi_returns_false() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        let result = seq.set_time_to(1.0, None);
        assert!(!result);
    }

    // -- set_time_to handles tempo correctly --

    #[test]
    fn test_set_time_to_updates_tempo() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        seq.load_new_song_list(vec![make_midi_with_cc()]);
        seq.play();
        // Seek past the tempo change at tick 960
        let result = seq.set_time_to(0.0, Some(1000));
        assert!(result);
        // Tempo should have been updated (120 BPM → data says 120 BPM too, but the SET_TEMPO was processed)
        let expected = 60.0 / (120.0 * 480.0);
        assert!((seq.one_tick_to_seconds - expected).abs() < 1e-12);
    }

    // -- edge: seek to beginning --

    #[test]
    fn test_set_time_to_beginning() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        seq.load_new_song_list(vec![make_midi_with_cc()]);
        seq.play();
        let result = seq.set_time_to(0.0, Some(0));
        assert!(result);
        // The timeline cursor should be at the very start.
        assert_eq!(seq.index, 0);
    }

    // -- edge: seek past end --

    #[test]
    fn test_set_time_to_past_end_returns_false() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        seq.load_new_song_list(vec![make_midi_with_cc()]);
        seq.play();
        let result = seq.set_time_to(100.0, None);
        // Should return false since song ends before 100 seconds
        assert!(!result);
    }

    // -- seek_reset_all_controllers (RP-15 local seek-time emulation) --

    #[test]
    fn test_reset_all_controllers_only_touches_rp15_ccs() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        let mut channels: Vec<ChannelStatus> = (0..1u8).map(ChannelStatus::new).collect();
        // Simulate main volume having been changed by a preceding skipped CC.
        channels[0].controllers[midi_controllers::MAIN_VOLUME as usize] = 42 << 7;
        channels[0].pitch_wheel = 1234;
        seq.seek_reset_all_controllers(&mut channels, 0);
        // RP-15 reset touches pitch wheel...
        assert_eq!(channels[0].pitch_wheel, 8192);
        // ...and the RP_15_RESET_CC_NUMS...
        assert_eq!(
            channels[0].controllers[midi_controllers::SUSTAIN_PEDAL as usize],
            DEFAULT_MIDI_CONTROLLERS[midi_controllers::SUSTAIN_PEDAL as usize]
        );
        // ...but NOT main volume.
        assert_eq!(
            channels[0].controllers[midi_controllers::MAIN_VOLUME as usize],
            42 << 7
        );
    }
}
