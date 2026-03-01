/// note_off.rs
/// purpose: MIDI note-off handler for MidiChannel.
/// Ported from: src/synthesizer/audio_engine/engine_methods/stopping_notes/note_off.ts
use crate::midi::enums::midi_controllers;
use crate::synthesizer::audio_engine::engine_components::synth_constants::MIN_NOTE_LENGTH;
use crate::synthesizer::audio_engine::engine_components::voice::Voice;
use crate::synthesizer::audio_engine::synthesizer_core::MidiChannel;
use crate::synthesizer::enums::custom_controllers;
use crate::synthesizer::types::{NoteOffCallback, SynthProcessorEvent};
use crate::utils::loggin::spessa_synth_warn;

impl MidiChannel {
    /// Releases a note by its MIDI note number.
    ///
    /// If the synthesizer is in black MIDI mode and the channel is not a drum channel,
    /// the note is killed nearly instantly instead of being released gracefully.
    ///
    /// Returns a `NoteOff` event to dispatch.
    ///
    /// Equivalent to: noteOff(midiNote: number)
    pub fn note_off(
        &mut self,
        midi_note: u8,
        voices: &mut [Voice],
        current_time: f64,
        black_midi_mode: bool,
    ) -> Vec<SynthProcessorEvent> {
        if midi_note > 127 {
            spessa_synth_warn(&format!(
                "Received a noteOff for note {} Ignoring.",
                midi_note
            ));
            return Vec::new();
        }

        // Adjust the MIDI note with channel transpose key shift
        let real_key = (midi_note as i16
            + self.channel_transpose_key_shift
            + self.custom_controllers[custom_controllers::CHANNEL_KEY_SHIFT as usize] as i16)
            as u8;

        let event = SynthProcessorEvent::NoteOff(NoteOffCallback {
            midi_note,
            channel: self.channel,
        });

        // Black MIDI mode: kill the note immediately
        if black_midi_mode && !self.drum_channel {
            self.kill_note(real_key, -12_000, voices, current_time);
            return vec![event];
        }

        let sustain = self.midi_controllers[midi_controllers::SUSTAIN_PEDAL as usize] >= 8192;
        let mut vc = 0u32;
        if self.voice_count > 0 {
            for v in voices.iter_mut() {
                if v.channel == self.channel
                    && v.is_active
                    && v.real_key == real_key
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

        vec![event]
    }
}
