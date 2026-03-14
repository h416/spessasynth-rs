/// process_tick.rs
/// purpose: Main playback loop — processes MIDI events up to the current time.
/// Ported from: src/sequencer/process_tick.ts
use crate::midi::types::MidiLoopType;
use crate::sequencer::sequencer::SpessaSynthSequencer;
use crate::sequencer::types::{LoopCountChangeEventData, SequencerEvent};

impl SpessaSynthSequencer {
    /// Processes a single MIDI tick.
    /// Call this every rendering quantum to process the sequencer events in real-time.
    /// Equivalent to: processTick()
    pub fn process_tick(&mut self) {
        if self.paused() || self.current_song_index.is_none() {
            return;
        }
        let song_idx = self.current_song_index.unwrap();
        let current_time = self.current_time();

        while self.played_time < current_time {
            // Find the next event and process it
            let track_index = self.find_first_event_index();
            let ei = self.event_indexes[track_index];
            let event = self.songs[song_idx].tracks[track_index].events[ei].clone();
            self.event_indexes[track_index] += 1;
            let event_ticks = event.ticks;
            self.process_event(event, track_index);

            // Find the next event
            let next_track_index = self.find_first_event_index();
            let next_ei = self.event_indexes[next_track_index];

            // Check for loop
            let loop_end = self.songs[song_idx].midi_loop.end;
            let loop_start = self.songs[song_idx].midi_loop.start;
            let loop_type = self.songs[song_idx].midi_loop.loop_type;
            if self.loop_count > 0 && loop_end <= event_ticks {
                if self.loop_count != u32::MAX {
                    self.loop_count -= 1;
                    self.call_event(SequencerEvent::LoopCountChange(
                        LoopCountChangeEventData {
                            new_count: self.loop_count,
                        },
                    ));
                }
                if loop_type == MidiLoopType::Soft {
                    self.jump_to_tick(loop_start);
                } else {
                    self.set_time_ticks(loop_start);
                }
                return;
            }

            // Check for end of track
            let next_track_len = self.songs[song_idx].tracks[next_track_index].events.len();
            let last_voice_tick = self.songs[song_idx].last_voice_event_tick;
            if next_track_len <= next_ei || event_ticks >= last_voice_tick {
                self.song_is_finished();
                return;
            }

            let next_event_ticks =
                self.songs[song_idx].tracks[next_track_index].events[next_ei].ticks;
            self.played_time +=
                self.one_tick_to_seconds * (next_event_ticks - event_ticks) as f64;
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
    use crate::midi::enums::midi_message_types;
    use crate::midi::midi_message::MidiMessage;
    use crate::midi::midi_track::MidiTrack;
    use crate::midi::types::{MidiLoop, MidiLoopType, TempoChange};
    use crate::synthesizer::processor::SpessaSynthProcessor;
    use crate::synthesizer::types::{SynthProcessorEvent, SynthProcessorOptions};

    fn make_processor() -> SpessaSynthProcessor {
        SpessaSynthProcessor::new(
            44100.0,
            |_: SynthProcessorEvent| {},
            SynthProcessorOptions::default(),
        )
    }

    fn make_midi_with_events(events: Vec<MidiMessage>, duration: f64) -> BasicMidi {
        let mut midi = BasicMidi::new();
        midi.time_division = 480;
        midi.duration = duration;
        midi.first_note_on = 0;
        midi.last_voice_event_tick = events.last().map_or(0, |e| e.ticks);
        midi.tempo_changes = vec![TempoChange {
            ticks: 0,
            tempo: 120.0,
        }];
        let mut track = MidiTrack::new();
        track.channels.insert(0);
        for e in events {
            track.push_event(e);
        }
        midi.tracks.push(track);
        midi
    }

    // -- process_tick when paused --

    #[test]
    fn test_process_tick_paused_does_nothing() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        let midi = make_midi_with_events(
            vec![
                MidiMessage::new(0, 0x90, vec![60, 100]),
                MidiMessage::new(480, 0x80, vec![60, 0]),
            ],
            2.0,
        );
        seq.load_new_song_list(vec![midi]);
        // Don't call play() — stays paused
        seq.process_tick();
        // Event indexes should remain at their reset position (set by set_time_to during load)
    }

    // -- process_tick when no midi --

    #[test]
    fn test_process_tick_no_midi_does_nothing() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        seq.paused_time = None; // Unpause without loading
        seq.process_tick(); // Should not panic
    }

    // -- process_tick advances through events --

    #[test]
    fn test_process_tick_advances_events() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        let midi = make_midi_with_events(
            vec![
                MidiMessage::new(0, 0x90, vec![60, 100]),
                MidiMessage::new(240, 0x90, vec![62, 80]),
                MidiMessage::new(480, 0x80, vec![60, 0]),
                MidiMessage::new(480, 0x80, vec![62, 0]),
                MidiMessage::new(960, 0x2F, vec![]),
            ],
            2.0,
        );
        seq.load_new_song_list(vec![midi]);
        seq.play();

        // Render some audio to advance synth time
        let samples = 44100; // 1 second
        let mut out = vec![vec![0.0f32; samples]; 2];
        seq.synth.render_audio(&mut out, 0, samples);

        // Now process tick should advance
        seq.process_tick();

        // The song should have finished (all events processed within 1 second at 120 BPM, 480 tpq → 960 ticks = 1 second)
        assert!(seq.is_finished || seq.event_indexes[0] > 0);
    }

    // -- process_tick finishes song --

    #[test]
    fn test_process_tick_finishes_song() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        let midi = make_midi_with_events(
            vec![
                MidiMessage::new(0, 0x90, vec![60, 100]),
                MidiMessage::new(480, 0x80, vec![60, 0]),
            ],
            1.0,
        );
        seq.load_new_song_list(vec![midi]);
        seq.play();

        // Advance synth time past the song duration
        let samples = 44100 * 2; // 2 seconds
        let mut out = vec![vec![0.0f32; samples]; 2];
        seq.synth.render_audio(&mut out, 0, samples);

        seq.process_tick();
        assert!(seq.is_finished);
    }

    // -- process_tick with loop --

    #[test]
    fn test_process_tick_loop_decrements() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        let mut midi = make_midi_with_events(
            vec![
                MidiMessage::new(0, 0x90, vec![60, 100]),
                MidiMessage::new(480, 0x80, vec![60, 0]),
                MidiMessage::new(960, 0x2F, vec![]),
            ],
            2.0,
        );
        midi.midi_loop = MidiLoop {
            start: 0,
            end: 480,
            loop_type: MidiLoopType::Hard,
        };
        seq.load_new_song_list(vec![midi]);
        seq.loop_count = 2;
        seq.play();

        // Advance synth time
        let samples = 44100 * 3;
        let mut out = vec![vec![0.0f32; samples]; 2];
        seq.synth.render_audio(&mut out, 0, samples);

        seq.process_tick();
        // Loop count should have decremented
        assert!(seq.loop_count < 2);
    }
}
