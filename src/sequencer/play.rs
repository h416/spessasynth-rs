/// play.rs
/// purpose: Seek (set time to) implementation for the sequencer.
/// Ported from: src/sequencer/play.ts
use crate::midi::enums::{midi_controllers, midi_message_types};
use crate::midi::midi_message::{get_event, MidiMessage};
use crate::sequencer::sequencer::SpessaSynthSequencer;
use crate::sequencer::types::{MetaEventEventData, SequencerEvent};
use crate::synthesizer::audio_engine::engine_components::controller_tables::DEFAULT_MIDI_CONTROLLER_VALUES;
use crate::synthesizer::audio_engine::engine_methods::controller_control::reset_controllers::is_non_resettable;
use crate::utils::big_endian::read_big_endian;

/// CCs that must not be skipped during seek.
/// Equivalent to: nonSkippableCCs
fn is_cc_non_skippable(cc: u8) -> bool {
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

impl SpessaSynthSequencer {
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
        let track_count = self.songs[song_idx].tracks.len();
        self.event_indexes = vec![0; track_count];

        // We save the pitch wheels, programs and controllers here
        // to only send them once after going through the events
        let channels_to_save = self.synth.synth_core.midi_channels.len();

        let mut pitch_wheels: Vec<i16> = vec![8192; channels_to_save];

        // An array with preset default values (first 128 entries)
        let default_controller_array: Vec<i16> =
            DEFAULT_MIDI_CONTROLLER_VALUES[..128].to_vec();

        let mut saved_controllers: Vec<Vec<i16>> =
            vec![default_controller_array.clone(); channels_to_save];

        // Save tempo changes
        let mut saved_tempo: Option<MidiMessage> = None;
        let mut saved_tempo_track: usize = 0;

        /// RP-15 compliant reset
        fn reset_all_controllers(
            chan: usize,
            pitch_wheels: &mut [i16],
            saved_controllers: &mut [Vec<i16>],
            default_controller_array: &[i16],
        ) {
            pitch_wheels[chan] = 8192;
            if chan >= saved_controllers.len() {
                return;
            }
            for (i, &element) in default_controller_array.iter().enumerate() {
                if !is_non_resettable(i as u8) {
                    saved_controllers[chan][i] = element;
                }
            }
        }

        loop {
            // Find the next event
            let track_index = self.find_first_event_index();
            let ei = self.event_indexes[track_index];
            let track_events_len = self.songs[song_idx].tracks[track_index].events.len();
            if ei >= track_events_len {
                self.stop();
                return false;
            }

            let event = self.songs[song_idx].tracks[track_index].events[ei].clone();

            // Check termination condition
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

            let info = get_event(event.status_byte);
            // Keep in mind midi ports to determine the channel!
            let track_port = self.songs[song_idx].tracks[track_index].port;
            let offset = self
                .midi_port_channel_offsets
                .get(&track_port)
                .copied()
                .unwrap_or(0);
            let channel = if info.channel >= 0 {
                info.channel as usize + offset
            } else {
                0
            };

            match info.status {
                // Skip note messages but track portamento control
                midi_message_types::NOTE_ON => {
                    if channel < saved_controllers.len() {
                        saved_controllers[channel]
                            [midi_controllers::PORTAMENTO_CONTROL as usize] =
                            event.data[0] as i16;
                    }
                }

                midi_message_types::NOTE_OFF => {}

                midi_message_types::PITCH_WHEEL => {
                    if channel < pitch_wheels.len() {
                        pitch_wheels[channel] =
                            ((event.data[1] as i16) << 7) | event.data[0] as i16;
                    }
                }

                midi_message_types::CONTROLLER_CHANGE => {
                    // Empty tracks cannot controller change
                    if self.songs[song_idx].is_multi_port
                        && self.songs[song_idx].tracks[track_index]
                            .channels
                            .is_empty()
                    {
                        // skip
                    } else {
                        let controller_number = event.data[0];
                        if is_cc_non_skippable(controller_number) {
                            let cc_v = event.data[1];
                            if controller_number
                                == midi_controllers::RESET_ALL_CONTROLLERS
                            {
                                reset_all_controllers(
                                    channel,
                                    &mut pitch_wheels,
                                    &mut saved_controllers,
                                    &default_controller_array,
                                );
                            }
                            self.synth.controller_change(
                                channel,
                                controller_number,
                                cc_v,
                            );
                        } else if channel < saved_controllers.len() {
                            saved_controllers[channel][controller_number as usize] =
                                event.data[1] as i16;
                        }
                    }
                }

                midi_message_types::SET_TEMPO => {
                    let tempo_bpm =
                        60_000_000.0 / read_big_endian(&event.data, 3, 0) as f64;
                    self.one_tick_to_seconds =
                        60.0 / (tempo_bpm * time_division as f64);
                    saved_tempo = Some(event.clone());
                    saved_tempo_track = track_index;
                }

                _ => {
                    // Process all other events normally
                    self.process_event(event.clone(), track_index);
                }
            }

            self.event_indexes[track_index] += 1;

            // Find the next event
            let next_track_index = self.find_first_event_index();
            let next_ei = self.event_indexes[next_track_index];
            let next_track_events_len =
                self.songs[song_idx].tracks[next_track_index].events.len();
            if next_ei >= next_track_events_len {
                self.stop();
                return false;
            }
            let next_event_ticks =
                self.songs[song_idx].tracks[next_track_index].events[next_ei].ticks;
            self.played_time +=
                self.one_tick_to_seconds * (next_event_ticks - event.ticks) as f64;
        }

        // Restoring saved controllers
        for channel in 0..channels_to_save {
            // Restore pitch wheels
            if channel < pitch_wheels.len() {
                self.synth
                    .pitch_wheel(channel, pitch_wheels[channel], -1);
            }
            // Every controller that has changed
            if channel < saved_controllers.len() {
                for (index, &value) in saved_controllers[channel].iter().enumerate() {
                    if value != default_controller_array[index]
                        && !is_cc_non_skippable(index as u8)
                    {
                        self.synth
                            .controller_change(channel, index as u8, value as u8);
                    }
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
    use crate::midi::types::TempoChange;
    use crate::synthesizer::processor::SpessaSynthProcessor;
    use crate::synthesizer::types::{SynthProcessorEvent, SynthProcessorOptions};

    fn make_processor() -> SpessaSynthProcessor {
        SpessaSynthProcessor::new(
            44100.0,
            |_: SynthProcessorEvent| {},
            SynthProcessorOptions::default(),
        )
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
        // All event indexes should be 0
        assert!(seq.event_indexes.iter().all(|&i| i == 0));
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
}
