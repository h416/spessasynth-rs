/// process_event.rs
/// purpose: Processes a single MIDI event from the sequencer.
/// Ported from: src/sequencer/process_event.ts (spessasynth_core 4.3.0)
///
/// Changes from 4.2.0 (reviewed against the 4.3.0 diff):
/// - `getEvent(event.statusByte)` (the free function from `midi_message.ts`, removed in 4.3.0) is
///   replaced by inlining the same channel/status split directly in this function, matching TS.
///   `get_event`/`get_channel`/`data_bytes_amount`/`MidiEventInfo` were removed from
///   `midi/midi_message.rs` in this same task (this function was their last caller).
/// - `noteOn`/`noteOff` update `this.playingNotes[channel]` (now a `Map<number, number>`) via
///   `.set`/`.delete` instead of a flat array `push`/`findIndex`+`splice`. Ported here via
///   `playing_notes_set`/`playing_notes_delete` on the sequencer (see `sequencer.rs`).
/// - `midiMessageTypes.reset` case: `this.synth.resetAllControllers()` → `this.synth.reset()`
///   (pure rename with an identical default system parameter — see `sequencer.rs`'s module doc
///   comment for details). This Rust file keeps calling the existing
///   `reset_all_controllers(DEFAULT_SYNTH_MODE)` (out of scope: `processor.rs`).
/// - `SpessaSynthInfo` → `SpessaLog::info` (Task 18 API; behavior-identical).
/// - The 4.3.0 top-of-function `if (this.externalMIDIPlayback && event.statusByte >= 0x80) { ...
///   return; }` early-out is not reachable in this port: `externalMIDIPlayback` is out of scope
///   for this offline-WAV-rendering project and was never implemented (see `sequencer.rs`'s
///   module doc comment), so it is intentionally omitted here, matching the pre-existing Rust
///   convention.
/// - Cosmetic-only: `midiMessageTypes`/`midiControllers` → `MIDIMessageTypes`/`MIDIController`
///   (TS casing convention change only; Rust's `midi_message_types` module already uses
///   SCREAMING_SNAKE_CASE consts and needs no change).
use crate::midi::enums::midi_message_types;
use crate::midi::midi_message::MidiMessage;
use crate::sequencer::sequencer::SpessaSynthSequencer;
use crate::sequencer::types::{MetaEventEventData, SequencerEvent};
use crate::utils::byte_functions::big_endian::read_big_endian;
use crate::utils::loggin::SpessaLog;

impl SpessaSynthSequencer {
    /// Processes a MIDI event.
    /// The event must be cloned before calling to avoid borrow conflicts.
    /// Equivalent to: processEventInternal(event, trackIndex)
    pub(crate) fn process_event(&mut self, event: MidiMessage, track_index: usize) {
        let song_idx = match self.current_song_index {
            Some(i) => i,
            None => return,
        };

        // Inlined equivalent of the removed `getEvent(statusByte)`: voice messages
        // (0x80..0xF0) split into (status = high nibble, channel = low nibble); everything
        // else (system/meta) keeps the full status byte with channel 0.
        let (status, mut channel) = if (0x80..0xf0).contains(&event.status_byte) {
            (event.status_byte & 0xf0, (event.status_byte & 0x0f) as usize)
        } else {
            (event.status_byte, 0)
        };
        let port = self.current_midi_ports[track_index];
        let offset = self.midi_port_channel_offsets.get(&port).copied().unwrap_or(0);
        channel += offset;

        match status {
            midi_message_types::NOTE_ON => {
                let velocity = event.data[1];
                if velocity > 0 {
                    self.synth.note_on(channel, event.data[0], velocity);
                    self.playing_notes_set(channel, event.data[0], velocity);
                } else {
                    self.synth.note_off(channel, event.data[0]);
                    self.playing_notes_delete(channel, event.data[0]);
                }
            }

            midi_message_types::NOTE_OFF => {
                self.synth.note_off(channel, event.data[0]);
                self.playing_notes_delete(channel, event.data[0]);
            }

            midi_message_types::PITCH_WHEEL => {
                let pitch = ((event.data[1] as i16) << 7) | event.data[0] as i16;
                self.synth.pitch_wheel(channel, pitch, -1);
            }

            midi_message_types::CONTROLLER_CHANGE => {
                // Empty tracks cannot cc change in multi-port mode
                if self.songs[song_idx].is_multi_port
                    && self.songs[song_idx].tracks[track_index]
                        .channels
                        .is_empty()
                {
                    return;
                }
                self.synth
                    .controller_change(channel, event.data[0], event.data[1]);
            }

            midi_message_types::PROGRAM_CHANGE => {
                // Empty tracks cannot program change in multi-port mode
                if self.songs[song_idx].is_multi_port
                    && self.songs[song_idx].tracks[track_index]
                        .channels
                        .is_empty()
                {
                    return;
                }
                self.synth.program_change(channel, event.data[0]);
            }

            midi_message_types::POLY_PRESSURE => {
                self.synth
                    .poly_pressure(channel, event.data[0], event.data[1]);
            }

            midi_message_types::CHANNEL_PRESSURE => {
                self.synth.channel_pressure(channel, event.data[0]);
            }

            midi_message_types::SYSTEM_EXCLUSIVE => {
                self.synth.system_exclusive(&event.data, offset);
            }

            midi_message_types::SET_TEMPO => {
                let tempo_bpm =
                    60_000_000.0 / read_big_endian(&event.data, 3, 0) as f64;
                let time_division = self.songs[song_idx].time_division;
                self.one_tick_to_seconds = 60.0 / (tempo_bpm * time_division as f64);
                if self.one_tick_to_seconds == 0.0 {
                    self.one_tick_to_seconds = 60.0 / (120.0 * time_division as f64);
                    SpessaLog::info("invalid tempo! falling back to 120 BPM");
                }
            }

            midi_message_types::MIDI_PORT => {
                self.assign_midi_port(track_index, event.data[0] as u32);
            }

            midi_message_types::RESET => {
                self.synth.stop_all_channels(false);
                self.synth
                    .reset();
            }

            // Recognized but ignored
            midi_message_types::TIME_SIGNATURE
            | midi_message_types::END_OF_TRACK
            | midi_message_types::MIDI_CHANNEL_PREFIX
            | midi_message_types::SONG_POSITION
            | midi_message_types::ACTIVE_SENSING
            | midi_message_types::KEY_SIGNATURE
            | midi_message_types::SEQUENCE_NUMBER
            | midi_message_types::SEQUENCE_SPECIFIC
            | midi_message_types::TEXT
            | midi_message_types::LYRIC
            | midi_message_types::COPYRIGHT
            | midi_message_types::TRACK_NAME
            | midi_message_types::MARKER
            | midi_message_types::CUE_POINT
            | midi_message_types::INSTRUMENT_NAME
            | midi_message_types::PROGRAM_NAME => {}

            _ => {
                SpessaLog::info(&format!(
                    "Unrecognized Event: 0x{:02X}",
                    event.status_byte
                ));
            }
        }

        // Fire meta event for status bytes < 0x80
        if status < 0x80 {
            self.call_event(SequencerEvent::MetaEvent(MetaEventEventData {
                event,
                track_index,
            }));
        }
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

    fn make_loaded_sequencer() -> SpessaSynthSequencer {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        let mut midi = BasicMidi::new();
        midi.time_division = 480;
        midi.duration = 2.0;
        midi.first_note_on = 0;
        midi.last_voice_event_tick = 960;
        midi.tempo_changes = vec![TempoChange {
            ticks: 0,
            tempo: 120.0,
        }];
        let mut track = MidiTrack::new();
        track.channels.insert(0);
        track.push_event(MidiMessage::new(0, 0x90, vec![60, 100]));
        track.push_event(MidiMessage::new(960, 0x2F, vec![]));
        midi.tracks.push(track);
        build_timeline(&mut midi);
        seq.load_new_song_list(vec![midi]);
        seq.play();
        seq
    }

    // -- note on --

    #[test]
    fn test_process_event_note_on_adds_playing_note() {
        let mut seq = make_loaded_sequencer();
        let event = MidiMessage::new(0, 0x90, vec![60, 100]);
        seq.process_event(event, 0);
        assert!(seq.playing_notes[0].iter().any(|&(n, v)| n == 60 && v == 100));
    }

    #[test]
    fn test_process_event_note_on_velocity_zero_is_note_off() {
        let mut seq = make_loaded_sequencer();
        // Add a playing note first
        seq.playing_notes_set(0, 60, 100);
        let event = MidiMessage::new(480, 0x90, vec![60, 0]);
        seq.process_event(event, 0);
        assert!(!seq.playing_notes[0].iter().any(|&(n, _)| n == 60));
    }

    // -- note off --

    #[test]
    fn test_process_event_note_off_removes_playing_note() {
        let mut seq = make_loaded_sequencer();
        seq.playing_notes_set(0, 60, 100);
        let event = MidiMessage::new(480, 0x80, vec![60, 0]);
        seq.process_event(event, 0);
        assert!(!seq.playing_notes[0].iter().any(|&(n, _)| n == 60));
    }

    // -- tempo change --

    #[test]
    fn test_process_event_set_tempo() {
        let mut seq = make_loaded_sequencer();
        // Tempo 120 BPM = 500000 microseconds per beat = [0x07, 0xA1, 0x20]
        let event = MidiMessage::new(0, midi_message_types::SET_TEMPO, vec![0x07, 0xA1, 0x20]);
        seq.process_event(event, 0);
        // oneTickToSeconds should have been updated
        let expected = 60.0 / (120.0 * 480.0);
        assert!((seq.one_tick_to_seconds - expected).abs() < 1e-12);
    }

    // -- controller change --

    #[test]
    fn test_process_event_controller_change_no_panic() {
        let mut seq = make_loaded_sequencer();
        let event = MidiMessage::new(0, 0xB0, vec![7, 100]);
        seq.process_event(event, 0);
    }

    // -- program change --

    #[test]
    fn test_process_event_program_change_no_panic() {
        let mut seq = make_loaded_sequencer();
        let event = MidiMessage::new(0, 0xC0, vec![10]);
        seq.process_event(event, 0);
    }

    // -- pitch wheel --

    #[test]
    fn test_process_event_pitch_wheel_no_panic() {
        let mut seq = make_loaded_sequencer();
        let event = MidiMessage::new(0, 0xE0, vec![0x00, 0x40]);
        seq.process_event(event, 0);
    }

    // -- unrecognized event --

    #[test]
    fn test_process_event_unrecognized_no_panic() {
        let mut seq = make_loaded_sequencer();
        let event = MidiMessage::new(0, 0x77, vec![]);
        seq.process_event(event, 0);
    }

    // -- meta event callback --

    #[test]
    fn test_process_event_meta_fires_callback() {
        use std::sync::{Arc, Mutex};

        let events: Arc<Mutex<Vec<SequencerEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let ev_clone = Arc::clone(&events);

        let mut seq = make_loaded_sequencer();
        seq.on_event_call = Some(Box::new(move |ev| {
            ev_clone.lock().unwrap().push(ev);
        }));

        // A tempo event has status < 0x80, so it fires metaEvent callback
        let event = MidiMessage::new(0, midi_message_types::SET_TEMPO, vec![0x07, 0xA1, 0x20]);
        seq.process_event(event, 0);

        let evs = events.lock().unwrap();
        assert!(evs.iter().any(|e| matches!(e, SequencerEvent::MetaEvent(_))));
    }
}
