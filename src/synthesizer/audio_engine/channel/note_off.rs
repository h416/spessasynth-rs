/// note_off.rs
/// purpose: MIDI note-off handler for MidiChannel.
/// Ported from: src/synthesizer/audio_engine/engine_methods/stopping_notes/note_off.ts
use crate::midi::enums::midi_controllers;
use crate::synthesizer::audio_engine::synth_constants::MIN_NOTE_LENGTH;
use crate::synthesizer::audio_engine::voice::voice::Voice;
use crate::synthesizer::audio_engine::channel::midi_channel::MidiChannel;
use crate::synthesizer::types::{NoteOffCallback, SynthProcessorEvent};
use crate::utils::loggin::spessa_synth_warn;

impl MidiChannel {
    /// Releases a note by its MIDI note number.
    ///
    /// If the synthesizer is in black MIDI mode and the channel is not a drum channel,
    /// the note is killed nearly instantly instead of being released gracefully.
    ///
    /// Returns the events to dispatch, plus — when in mono mode and this Note Off
    /// uncovers a still-held lower note — a `(midi_note, velocity)` pair describing the
    /// legato retrigger that the caller (which owns the full Note On pipeline) must run
    /// via a non-emitting Note On.
    ///
    /// Equivalent to: noteOff(midiNote: number)
    pub fn note_off(
        &mut self,
        midi_note: u8,
        voices: &mut [Voice],
        current_time: f64,
        black_midi_mode: bool,
    ) -> (Vec<SynthProcessorEvent>, Option<(u8, u8)>) {
        if midi_note > 127 {
            spessa_synth_warn(&format!(
                "Received a noteOff for note {} Ignoring.",
                midi_note
            ));
            return (Vec::new(), None);
        }

        let event = SynthProcessorEvent::NoteOff(NoteOffCallback {
            midi_note,
            channel: self.channel,
        });

        // Black MIDI mode: kill the note immediately
        if black_midi_mode && !self.drum_channel {
            self.kill_note(midi_note, -12_000, voices, current_time);
            return (vec![event], None);
        }

        self.playing_notes[midi_note as usize] = false;
        // Mono mode overrides sustain.
        let mono = !self.midi_parameters.poly_mode;
        let sustain =
            self.midi_controllers[midi_controllers::SUSTAIN_PEDAL as usize] >= 8192 && !mono;

        let note_id = self.note_off_id[midi_note as usize];
        // Only update if note on is above this.
        // Testcase: overlapping_notes_test (multiple note off)
        if note_id < self.note_on_id[midi_note as usize] {
            self.note_off_id[midi_note as usize] += 1;
        }

        let mut vc = 0u32;
        if self.voice_count > 0 {
            for v in voices.iter_mut() {
                if v.channel == self.channel
                    && v.is_active
                    && v.midi_note == midi_note
                    && v.note_id == note_id
                    && !v.is_in_release
                {
                    if sustain {
                        v.is_held = true;
                    } else {
                        v.release_voice(current_time, MIN_NOTE_LENGTH);
                    }
                    vc += 1;
                    if vc >= self.voice_count {
                        break;
                    }
                }
            }
        }

        // Mono mode: restore the highest still-pressed note (legato).
        let mut retrigger = None;
        if mono {
            match self.playing_notes.iter().rposition(|&playing| playing) {
                None => {
                    // No note is playing.
                    self.last_mono_note = -1;
                }
                Some(highest) if self.last_mono_note == midi_note as i32 => {
                    // The guard above ensures that we don't retrigger a note that isn't
                    // this one. For example notes might go like this:
                    // On 50, 60, 70
                    // Off 50 -> Jumps to 70
                    // Off 60 -> We're not the last note so no change, don't jump to 70 again
                    // The note will be set automatically by the retriggered Note On.
                    retrigger = Some((highest as u8, self.last_mono_velocity));
                }
                Some(_) => {}
            }
        }

        (vec![event], retrigger)
    }
}
