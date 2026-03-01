/// handle_gm.rs
/// purpose: Handles GM (General MIDI) system exclusive messages.
/// Ported from: src/synthesizer/audio_engine/engine_methods/system_exclusive/handle_gm.ts
use crate::synthesizer::audio_engine::engine_methods::system_exclusive::helpers::sys_ex_not_recognized;
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::types::{MasterParameterChangeCallback, SynthSystem};
use crate::utils::loggin::{spessa_synth_info, spessa_synth_warn};
use crate::utils::string::read_binary_string;

/// Calculates the frequency for MIDI Tuning Standard.
/// Returns -1.0 if all three bytes are 0x7F (no change).
/// Equivalent to: getTuning(byte1, byte2, byte3)
fn get_tuning(byte1: u8, byte2: u8, byte3: u8) -> f32 {
    let midi_note = byte1 as f32;
    // Combine byte2 and byte3 into a 14-bit number
    let fraction = ((byte2 as u32) << 7) | (byte3 as u32);

    // No change
    if byte1 == 0x7f && byte2 == 0x7f && byte3 == 0x7f {
        return -1.0;
    }

    // Calculate cent tuning (divide cents by 100 so it works in semitones)
    midi_note + fraction as f32 * 0.000_061
}

impl SynthesizerCore {
    /// Handles a GM system exclusive message (realtime/non-realtime).
    /// Equivalent to: handleGM(syx, channelOffset)
    pub fn handle_gm(&mut self, syx: &[u8], channel_offset: usize) {
        match syx[2] {
            0x04 => {
                // Device control
                match syx[3] {
                    0x01 => {
                        // Main volume
                        let vol = ((syx[5] as u32) << 7) | syx[4] as u32;
                        self.set_midi_volume(vol as f64 / 16_384.0);
                        spessa_synth_info(&format!("Master Volume. Volume: {}", vol));
                    }

                    0x02 => {
                        // Main balance (MIDI spec page 62)
                        let balance = ((syx[5] as u32) << 7) | syx[4] as u32;
                        let pan = (balance as f64 - 8192.0) / 8192.0;
                        self.set_master_parameter(MasterParameterChangeCallback::MasterPan(pan));
                        spessa_synth_info(&format!("Master Pan. Pan: {}", pan));
                    }

                    0x03 => {
                        // Fine-tuning
                        let tuning_value = (((syx[5] as i32) << 7) | syx[6] as i32) - 8192;
                        let cents = (tuning_value as f64 / 81.92).floor(); // [-100;+99] cents range
                        self.set_master_tuning(cents);
                        spessa_synth_info(&format!("Master Fine Tuning. Cents: {}", cents));
                    }

                    0x04 => {
                        // Coarse tuning (LSB is ignored)
                        let semitones = syx[5] as i32 - 64;
                        let cents = semitones * 100;
                        self.set_master_tuning(cents as f64);
                        spessa_synth_info(&format!("Master Coarse Tuning. Cents: {}", cents));
                    }

                    _ => {
                        spessa_synth_info(&format!(
                            "Unrecognized MIDI Device Control Real-time message: {:02X?}",
                            syx
                        ));
                    }
                }
            }

            0x09 => {
                // GM system related
                if syx[3] == 0x01 {
                    spessa_synth_info("GM1 system on");
                    self.reset_all_controllers(SynthSystem::Gm);
                } else if syx[3] == 0x03 {
                    spessa_synth_info("GM2 system on");
                    self.reset_all_controllers(SynthSystem::Gm2);
                } else {
                    spessa_synth_info("GM system off, defaulting to GS");
                    self.set_master_parameter(MasterParameterChangeCallback::MidiSystem(
                        SynthSystem::Gs,
                    ));
                }
            }

            // MIDI Tuning Standard
            // https://midi.org/midi-tuning-updated-specification
            0x08 => {
                let mut current_message_index = 4usize;
                match syx[3] {
                    // Bulk tuning dump: all 128 notes
                    0x01 => {
                        let program = syx[current_message_index] as usize;
                        current_message_index += 1;
                        // Read the name
                        let tuning_name = read_binary_string(syx, 16, current_message_index);
                        current_message_index += 16;
                        if syx.len() < 384 {
                            spessa_synth_warn(&format!(
                                "The Bulk Tuning Dump is too short! ({} bytes, at least 384 are expected)",
                                syx.len()
                            ));
                            return;
                        }
                        // 128 frequencies follow
                        for midi_note in 0..128usize {
                            let b1 = syx[current_message_index];
                            current_message_index += 1;
                            let b2 = syx[current_message_index];
                            current_message_index += 1;
                            let b3 = syx[current_message_index];
                            current_message_index += 1;
                            self.tunings[program * 128 + midi_note] = get_tuning(b1, b2, b3);
                        }
                        spessa_synth_info(&format!(
                            "Bulk Tuning Dump {} Program: {}",
                            tuning_name, program
                        ));
                    }

                    // Single note change
                    // Single note change bank
                    0x02 | 0x07 => {
                        if syx[3] == 0x07 {
                            // Skip the bank
                            current_message_index += 1;
                        }
                        // Get program and number of changes
                        let tuning_program = syx[current_message_index] as usize;
                        current_message_index += 1;
                        let number_of_changes = syx[current_message_index] as usize;
                        current_message_index += 1;
                        for _ in 0..number_of_changes {
                            let midi_note = syx[current_message_index] as usize;
                            current_message_index += 1;
                            let b1 = syx[current_message_index];
                            current_message_index += 1;
                            let b2 = syx[current_message_index];
                            current_message_index += 1;
                            let b3 = syx[current_message_index];
                            current_message_index += 1;
                            self.tunings[tuning_program * 128 + midi_note] = get_tuning(b1, b2, b3);
                        }
                        spessa_synth_info(&format!(
                            "Single Note Tuning. Program: {} Keys affected: {}",
                            tuning_program, number_of_changes
                        ));
                    }

                    // Octave tuning (1 byte)
                    // Octave tuning (2 bytes)
                    0x08 | 0x09 => {
                        let mut new_octave_tuning = [0i8; 12];
                        // Start from bit 7
                        if syx[3] == 0x08 {
                            // 1 byte tuning: 0 is -64 cents, 64 is 0, 127 is +63
                            for i in 0..12usize {
                                new_octave_tuning[i] = (syx[7 + i] as i16 - 64) as i8;
                            }
                        } else {
                            // 2 byte tuning: 0 is -100 cents, 8192 is 0, 16383 is +100
                            for i in (0..24usize).step_by(2) {
                                let tuning =
                                    (((syx[7 + i] as i32) << 7) | syx[8 + i] as i32) - 8192;
                                new_octave_tuning[i / 2] = (tuning as f64 / 81.92).floor() as i8;
                            }
                        }
                        // Apply to channels (ordered from 0)
                        // Bit 1: channels 14 and 15
                        if (syx[4] & 1) == 1 {
                            self.midi_channels[14 + channel_offset]
                                .set_octave_tuning(&new_octave_tuning);
                        }
                        if ((syx[4] >> 1) & 1) == 1 {
                            self.midi_channels[15 + channel_offset]
                                .set_octave_tuning(&new_octave_tuning);
                        }

                        // Bit 2: channels 7 to 13
                        for i in 0..7usize {
                            let bit = (syx[5] >> i) & 1;
                            if bit == 1 {
                                self.midi_channels[7 + i + channel_offset]
                                    .set_octave_tuning(&new_octave_tuning);
                            }
                        }

                        // Bit 3: channels 0 to 6
                        for i in 0..7usize {
                            let bit = (syx[6] >> i) & 1;
                            if bit == 1 {
                                self.midi_channels[i + channel_offset]
                                    .set_octave_tuning(&new_octave_tuning);
                            }
                        }

                        spessa_synth_info(&format!(
                            "MIDI Octave Scale {} tuning via Tuning: {:?}",
                            if syx[3] == 0x08 {
                                "(1 byte)"
                            } else {
                                "(2 bytes)"
                            },
                            new_octave_tuning
                        ));
                    }

                    _ => {
                        sys_ex_not_recognized(syx, "MIDI Tuning Standard");
                    }
                }
            }

            _ => {
                sys_ex_not_recognized(syx, "General MIDI");
            }
        }
    }
}
