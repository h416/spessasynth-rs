/// set_time_to.rs
/// purpose: Seek (set time to) implementation for the sequencer.
/// Ported from: src/sequencer/set_time_to.ts (spessasynth_core 4.3.0; renamed from
/// `src/sequencer/play.ts` in the upstream 4.3.0 restructuring — see `mod.rs`/phase-1 notes).
///
/// Changes from 4.2.0's `play.ts` (reviewed against the 4.3.0 diff):
/// - `eventIndexes: number[]` (one cursor per track) → single `index: number` cursor into
///   `BasicMIDI.timeline` (ported in Task 18), same as `sequencer.rs`/`process_tick.rs`.
///   `findFirstEventIndex()` is gone.
/// - `getEvent(statusByte)` (removed from `midi_message.ts` in 4.3.0) is replaced by inlining the
///   channel/status split, matching `process_event.ts`/`process_event.rs`.
/// - The controller-restore bookkeeping was rewritten around a new local `ChannelStatus` struct
///   (`{ param, controllers, portamentoNote, pitchWheel }`) per channel instead of three parallel
///   arrays (`pitchWheels`, `savedControllers`, and portamento piggy-backed into
///   `savedControllers[channel][portamentoControl]`):
///   - `controllers` is now a `CONTROLLER_TABLE_SIZE`-sized 14-bit array seeded from a new
///     `DEFAULT_MIDI_CONTROLLERS` table (all 147 entries), instead of the old 128-entry
///     "`defaultMIDIControllerValues.slice(0, 128)`" (7-bit-scale-vs-14-bit-scale mismatch and
///     all — see below).
///   - `param: ParameterTracker` (Task 18 API) tracks RPN/NRPN selection so that Data Entry
///     MSB/LSB during a seek can resolve to the RPN/NRPN it targets (fine tuning, coarse tuning,
///     and the handful of GS/XG "NRPN part parameters") via `MidiUtils::analyze_rpn`/
///     `analyze_nrpn`, and — if that resolves to a plain Controller Change (the NRPN case) — the
///     resulting CC is itself skip-tracked (sent immediately if non-skippable, else stashed into
///     `controllers[]`) exactly like a literal Controller Change event would be.
///   - `portamentoNote` (new): always records the last Note On's key (`-1` = none), regardless of
///     whether portamento is active, restored at the end via CC84 (portamentoControl) — see the
///     `TODO` below for why the TS 4.3.0 `setLastNote()` fast path isn't used here.
///   - System Exclusive events are now analyzed via `MIDIUtils::analyzeSysEx` (Task 18 API): a
///     recognized "Controller Change" (GS/XG bank-select / mono-poly-mode / etc. encoded as
///     SysEx) is skip-tracked exactly like a literal Controller Change event instead of being
///     processed immediately; anything else (including Program Change, which cannot be skipped —
///     see the code comment) is processed immediately via `processEvent`, same as before.
/// - The controller-restore comparison at the end compares the full 14-bit `ch.controllers[i]`
///   against the 14-bit `DEFAULT_MIDI_CONTROLLERS[i]` and resends the 7-bit MSB (TS 4.3.14 fix).
///   The older TS 4.3.0 code compared `ch.controllers[i] >> 7` (7-bit) against the 14-bit default,
///   so any controller at its default was spuriously "changed" and resent as its 7-bit default.
///   That is NOT harmless: it clobbers a live non-default controller (whose snapshot value equals
///   the default at the seek point) back to its default during a seek. Concretely it reset GS NRPN
///   vibrato rate (CC76) from 95 to 64 on J-cycle.mid ch6, disabling the LFO filter/amplitude
///   modulation and diverging the WAV around 57-61s. Now ported to match TS 4.3.14.
/// - `resetAllControllers(chan)` (the local seek-time emulation of receiving CC 121 "Reset All
///   Controllers" mid-seek) now performs the narrow RP-15 reset (`RP_15_RESET_CC_NUMS`, 8 CCs)
///   instead of the old "reset everything except `nonResettableCCs`" broad reset. This is a real
///   behavior change (ported below), matching what the actual runtime CC121 handler
///   (`resetRP15` in the new `channel/reset.ts`) does.
///
/// TODO(Task 20-22, synthesizer restructuring): `DEFAULT_MIDI_CONTROLLERS` and
/// `RP_15_RESET_CC_NUMS` are TS 4.3.0 exports of `synthesizer/audio_engine/channel/reset.ts`,
/// and `CONTROLLER_TABLE_SIZE` (= 128) is a TS 4.3.0 export of
/// `synthesizer/audio_engine/synth_constants.ts` — all three belong to the not-yet-ported 4.3.0
/// channel/voice architecture (out of scope for this sequencer-only task). Note that the 4.3.0
/// `CONTROLLER_TABLE_SIZE` is a *different* constant from the same-named 4.2.0-era one still in
/// `channel/parameters/midi.rs` (147 = 128 CCs + the non-CC modulator-source extension slots);
/// TS 4.3.0's controller table covers only the 128 real MIDI CCs. All three are pure data (no
/// behavior tied to the new architecture), so they are reproduced locally below rather than
/// blocked on that port; move them to their real homes once the channel restructuring lands.
/// Similarly, restoring portamento via `midiChannels[channel].setLastNote()` (bypassing the CC84
/// pipeline entirely) isn't available yet; this file keeps sending CC84 (portamentoControl)
/// through `controller_change`, which the current (pre-4.3.0) `note_on.rs` still reads directly
/// to determine the portamento source note, so behavior is equivalent for now. Revisit once the
/// channel restructuring lands.
use crate::midi::enums::{
    midi_controllers, midi_message_types, MidiController, MidiMessageType,
};
use crate::midi::midi_message::MidiMessage;
use crate::midi::midi_tools::midi_utils::{AnalyzedMidiMessage, MidiUtils};
use crate::midi::midi_tools::parameter_tracker::ParameterTracker;
use crate::sequencer::sequencer::SpessaSynthSequencer;
use crate::sequencer::types::{MetaEventEventData, SequencerEvent};
use crate::synthesizer::audio_engine::synth_constants::{DEFAULT_NRPN, DEFAULT_RPN};
use crate::utils::byte_functions::big_endian::read_big_endian;

/// The size of the MIDI controller table (the 128 real MIDI CCs).
/// Equivalent to (locally reproduced, see the TODO above): `CONTROLLER_TABLE_SIZE` in TS 4.3.0's
/// `synthesizer/audio_engine/synth_constants.ts`. NOT the same constant as the 4.2.0-era
/// `channel/parameters/midi.rs::CONTROLLER_TABLE_SIZE` (147, which appends non-CC
/// modulator-source slots that this seek bookkeeping never touches).
const CONTROLLER_TABLE_SIZE: usize = 128;

/// CCs that must not be skipped during seek.
/// Equivalent to: nonSkippableCCs (unchanged between 4.2.0 and 4.3.0, aside from the TS
/// `midiControllers` → `MIDIControllers` casing rename)
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

/// An array with the default MIDI controller values used by the sequencer's own seek-time
/// controller-restore bookkeeping. Note that these are 14-bit (a 7-bit shift to the right for
/// 7-bit values), matching only 18 of the `CONTROLLER_TABLE_SIZE` entries — the rest default to 0.
///
/// Equivalent to (locally reproduced, see the TODO above): `DEFAULT_MIDI_CONTROLLERS` in TS
/// 4.3.0's `synthesizer/audio_engine/channel/reset.ts`.
const fn build_default_midi_controllers() -> [i16; CONTROLLER_TABLE_SIZE] {
    let mut arr = [0i16; CONTROLLER_TABLE_SIZE];
    // Values come from Falcosoft MIDI Player
    arr[midi_controllers::MAIN_VOLUME as usize] = 100 << 7;
    arr[midi_controllers::BALANCE as usize] = 64 << 7;
    arr[midi_controllers::EXPRESSION as usize] = 127 << 7;
    arr[midi_controllers::PAN as usize] = 64 << 7;

    arr[midi_controllers::FILTER_RESONANCE as usize] = 64 << 7;
    arr[midi_controllers::RELEASE_TIME as usize] = 64 << 7;
    arr[midi_controllers::ATTACK_TIME as usize] = 64 << 7;
    arr[midi_controllers::BRIGHTNESS as usize] = 64 << 7;

    arr[midi_controllers::DECAY_TIME as usize] = 64 << 7;
    arr[midi_controllers::VIBRATO_RATE as usize] = 64 << 7;
    arr[midi_controllers::VIBRATO_DEPTH as usize] = 64 << 7;
    arr[midi_controllers::VIBRATO_DELAY as usize] = 64 << 7;
    arr[midi_controllers::GENERAL_PURPOSE_CONTROLLER6 as usize] = 64 << 7;
    arr[midi_controllers::GENERAL_PURPOSE_CONTROLLER8 as usize] = 64 << 7;

    arr[midi_controllers::REGISTERED_PARAMETER_LSB as usize] = (DEFAULT_RPN as i16) << 7;
    arr[midi_controllers::REGISTERED_PARAMETER_MSB as usize] = (DEFAULT_RPN as i16) << 7;
    arr[midi_controllers::NON_REGISTERED_PARAMETER_LSB as usize] = (DEFAULT_NRPN as i16) << 7;
    arr[midi_controllers::NON_REGISTERED_PARAMETER_MSB as usize] = (DEFAULT_NRPN as i16) << 7;

    arr
}

const DEFAULT_MIDI_CONTROLLERS: [i16; CONTROLLER_TABLE_SIZE] = build_default_midi_controllers();

/// The 8 CCs reset by the RP-15 Recommended Practice.
/// Equivalent to (locally reproduced, see the TODO above): `RP_15_RESET_CC_NUMS` in TS 4.3.0's
/// `synthesizer/audio_engine/channel/reset.ts`.
const RP_15_RESET_CC_NUMS: [MidiController; 8] = [
    midi_controllers::MODULATION_WHEEL,
    midi_controllers::EXPRESSION,
    midi_controllers::SUSTAIN_PEDAL,
    midi_controllers::PORTAMENTO_ON_OFF,
    midi_controllers::SOSTENUTO_PEDAL,
    midi_controllers::SOFT_PEDAL,
    midi_controllers::REGISTERED_PARAMETER_MSB,
    midi_controllers::REGISTERED_PARAMETER_LSB,
];

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

/// RP-15 compliant reset of the local seek-time channel status.
/// <https://amei.or.jp/midistandardcommittee/Recommended_Practice/e/rp15.pdf>
/// Equivalent to: function resetAllControllers(chan) (local to setTimeToInternal)
fn reset_all_controllers(channels: &mut [ChannelStatus], chan: usize) {
    let ch = &mut channels[chan];
    // Reset pitch wheel
    ch.pitch_wheel = 8192;
    ch.param.reset();
    for &reset_cc in RP_15_RESET_CC_NUMS.iter() {
        ch.controllers[reset_cc as usize] = DEFAULT_MIDI_CONTROLLERS[reset_cc as usize];
    }
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
        self.index = 0;

        // We save the pitch wheels, programs and controllers here
        // to only send them once after going through the events
        let channels_to_save = self.synth.synth_core.midi_channels.len();

        let mut channels: Vec<ChannelStatus> = (0..channels_to_save)
            .map(|i| ChannelStatus::new(i as MidiController))
            .collect();

        // Save tempo changes
        // Testcase:
        // Piano Concerto No. 2 in G minor, Op 16 - I. Cadenza (Ky6000).mid
        // With 46k changes!
        let mut saved_tempo: Option<MidiMessage> = None;
        let mut saved_tempo_track: usize = 0;

        loop {
            // Find the next event
            let timeline_len = self.songs[song_idx].timeline.len();
            if self.index >= timeline_len {
                // Ran out of events before reaching the requested time/ticks. Not reachable via
                // the public API (currentTime setter clamps to duration first), kept as a
                // Rust-safety net (TS would dereference `undefined` here and throw).
                self.stop();
                return false;
            }
            let e = self.songs[song_idx].timeline[self.index];
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

            // Skip note ons. Inlined equivalent of the removed `getEvent(statusByte)`, matching
            // `process_event.rs`.
            let (status, status_channel): (MidiMessageType, usize) =
                if (0x80..0xf0).contains(&event.status_byte) {
                    (event.status_byte & 0xf0, (event.status_byte & 0x0f) as usize)
                } else {
                    (event.status_byte, 0)
                };

            // Keep in mind midi ports to determine the channel!
            let track_port = self.songs[song_idx].tracks[track_index].port;
            let offset = self
                .midi_port_channel_offsets
                .get(&track_port)
                .copied()
                .unwrap_or(0);
            let channel = status_channel + offset;

            // Ensure that the channel is always there (safety precaution)
            while channels.len() <= channel {
                let idx = channels.len();
                channels.push(ChannelStatus::new(idx as MidiController));
            }

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
                    let analyzed = MidiUtils::analyze_sysex(&event.data);
                    // Sysex may change controllers
                    if let AnalyzedMidiMessage::ControllerChange {
                        channel: sysex_channel,
                        controller,
                        value,
                    } = analyzed
                    {
                        let sysex_channel = sysex_channel as usize;
                        // Empty tracks cannot controller change
                        if self.songs[song_idx].is_multi_port
                            && self.songs[song_idx].tracks[track_index]
                                .channels
                                .is_empty()
                        {
                            // Break (do nothing further for this event)
                        } else if controller == midi_controllers::RESET_ALL_CONTROLLERS {
                            reset_all_controllers(&mut channels, sysex_channel);
                        } else if is_cc_non_skippable(controller) {
                            self.synth
                                .controller_change(sysex_channel, controller, value);
                        } else {
                            channels[sysex_channel].controllers[controller as usize] =
                                (value as i16) << 7;
                        }
                    } else {
                        /*
                        Program change cannot be skipped.
                        Some MIDIs edit drums via sysEx and skipping program changes causes them to be sent after, resetting the params.
                        Testcase: (GS88Pro)Th19_1S(KR.Palto47)
                         */
                        self.process_event(event.clone(), track_index);
                    }
                }

                midi_message_types::CONTROLLER_CHANGE => {
                    // Empty tracks cannot controller change
                    if self.songs[song_idx].is_multi_port
                        && self.songs[song_idx].tracks[track_index]
                            .channels
                            .is_empty()
                    {
                        // Skip
                    } else {
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
                                    reset_all_controllers(&mut channels, channel);
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
            if self.index >= self.songs[song_idx].timeline.len() {
                self.stop();
                return false;
            }
            let n_e = self.songs[song_idx].timeline[self.index];
            let next_event_ticks = self.songs[song_idx].tracks[n_e.tr].events[n_e.ev].ticks;
            self.played_time += self.one_tick_to_seconds * (next_event_ticks - event.ticks) as f64;
        }

        // For all synth channels
        for channel in 0..channels_to_save {
            let ch = &channels[channel];
            // Restoring pitch wheels
            self.synth.pitch_wheel(channel, ch.pitch_wheel, -1);

            // Restoring portamento (only if currently active)
            // Note: we do it before controllers as portamento control may want to override it
            if ch.portamento_note >= 0 {
                // See the TODO in this file's module doc comment: TS 4.3.0 uses a dedicated
                // `midiChannels[channel].setLastNote()` here; this port still routes through
                // CC84, which `note_on.rs` reads the same way.
                self.synth.controller_change(
                    channel,
                    midi_controllers::PORTAMENTO_CONTROL,
                    ch.portamento_note as u8,
                );
            }

            // Restoring saved controllers
            // Every controller that has changed
            // TS 4.3.14 set_time_to.ts fix: compare the full 14-bit value against the
            // 14-bit default, then send the 7-bit MSB. The old TS 4.3.0 code compared
            // `controllers[i] >> 7` (7-bit) against `DEFAULT_MIDI_CONTROLLERS[i]` (14-bit),
            // so any controller sitting at its default was spuriously treated as "changed"
            // and unconditionally resent as its 7-bit default value. That clobbers a live
            // controller (e.g. GS NRPN vibrato rate CC76 set to 95) back to its default (64)
            // during a seek, silently disabling vibrato/filter/amplitude LFO depth on that
            // channel. Testcase: J-cycle.mid ch6 organ phrase around 57-61s.
            for i in 0..CONTROLLER_TABLE_SIZE {
                // 14-bit, defaults are also 14-bit.
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
    fn test_default_midi_controllers_nrpn_is_zero() {
        // Unlike the old 4.2.0 `defaultMIDIControllerValues` (127), TS 4.3.0's
        // `DEFAULT_MIDI_CONTROLLERS` resets NRPN MSB/LSB to `DEFAULT_NRPN` (0).
        assert_eq!(
            DEFAULT_MIDI_CONTROLLERS[midi_controllers::NON_REGISTERED_PARAMETER_MSB as usize],
            0
        );
        assert_eq!(
            DEFAULT_MIDI_CONTROLLERS[midi_controllers::NON_REGISTERED_PARAMETER_LSB as usize],
            0
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
        assert_eq!(RP_15_RESET_CC_NUMS.len(), 8);
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

    // -- reset_all_controllers (RP-15 local seek-time emulation) --

    #[test]
    fn test_reset_all_controllers_only_touches_rp15_ccs() {
        let mut channels: Vec<ChannelStatus> = (0..1u8).map(ChannelStatus::new).collect();
        // Simulate main volume having been changed by a preceding skipped CC.
        channels[0].controllers[midi_controllers::MAIN_VOLUME as usize] = 42 << 7;
        channels[0].pitch_wheel = 1234;
        reset_all_controllers(&mut channels, 0);
        // RP-15 reset touches pitch wheel...
        assert_eq!(channels[0].pitch_wheel, 8192);
        // ...and the 8 RP_15_RESET_CC_NUMS...
        assert_eq!(
            channels[0].controllers[midi_controllers::SUSTAIN_PEDAL as usize],
            DEFAULT_MIDI_CONTROLLERS[midi_controllers::SUSTAIN_PEDAL as usize]
        );
        // ...but NOT main volume (unlike the old 4.2.0 broad reset).
        assert_eq!(
            channels[0].controllers[midi_controllers::MAIN_VOLUME as usize],
            42 << 7
        );
    }
}
