/// handle_xg.rs
/// purpose: Handles XG (Yamaha) system exclusive messages.
/// Ported from: src/synthesizer/audio_engine/engine_methods/system_exclusive/handle_xg.ts
/// Reference: http://www.studio4all.de/htmle/main91.html
use crate::midi::enums::midi_controllers;
use crate::synthesizer::audio_engine::engine_methods::system_exclusive::helpers::sys_ex_not_recognized;
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::enums::custom_controllers;
use crate::synthesizer::types::{MasterParameterChangeCallback, SynthSystem};
use crate::utils::loggin::spessa_synth_info;
use crate::utils::midi_hacks::BankSelectHacks;

impl SynthesizerCore {
    /// Handles a XG system exclusive message.
    /// Equivalent to: handleXG(syx, channelOffset)
    pub fn handle_xg(&mut self, syx: &[u8], channel_offset: usize) {
        // XG sysex
        if syx[2] != 0x4c {
            sys_ex_not_recognized(syx, "Yamaha");
            return;
        }

        let a1 = syx[3]; // Address 1
        let a2 = syx[4]; // Address 2

        // XG system parameter
        if a1 == 0x00 && a2 == 0x00 {
            match syx[5] {
                // Master tune
                0x00 => {
                    let tune = ((syx[6] as u32 & 15) << 12)
                        | ((syx[7] as u32 & 15) << 8)
                        | ((syx[8] as u32 & 15) << 4)
                        | (syx[9] as u32 & 15);
                    let cents = (tune as f64 - 1024.0) / 10.0;
                    self.set_master_tuning(cents);
                    spessa_synth_info(&format!("XG master tune. Cents: {}", cents));
                }

                // Master volume
                0x04 => {
                    let vol = syx[6];
                    self.set_midi_volume(vol as f64 / 127.0);
                    spessa_synth_info(&format!("XG master volume. Volume: {}", vol));
                }

                // Master attenuation
                0x05 => {
                    let vol = 127u8.saturating_sub(syx[6]);
                    self.set_midi_volume(vol as f64 / 127.0);
                    spessa_synth_info(&format!("XG master attenuation. Volume: {}", vol));
                }

                // Master transpose
                0x06 => {
                    let transpose = syx[6] as f64 - 64.0;
                    self.set_master_parameter(MasterParameterChangeCallback::Transposition(
                        transpose,
                    ));
                    spessa_synth_info(&format!("XG master transpose. Semitones: {}", transpose));
                }

                // XG on
                0x7e => {
                    spessa_synth_info("XG system on");
                    self.reset_all_controllers(SynthSystem::Xg);
                }

                _ => {}
            }
        } else if a1 == 0x02 && a2 == 0x01 {
            let effect = syx[5];
            let effect_type = if effect <= 0x15 {
                "Reverb"
            } else if effect <= 35 {
                "Chorus"
            } else {
                "Variation"
            };
            spessa_synth_info(&format!(
                "Unsupported XG {} Parameter: {:02X}",
                effect_type, effect
            ));
        } else if a1 == 0x08 {
            // A2 is the channel number
            // XG part parameter
            if !BankSelectHacks::is_system_xg(self.master_parameters.midi_system) {
                return;
            }
            let channel = a2 as usize + channel_offset;
            if channel >= self.midi_channels.len() {
                // Invalid channel
                return;
            }
            let value = syx[6];

            let current_time = self.current_time;
            let current_system = self.master_parameters.midi_system;
            let enable_event_system = self.enable_event_system;

            match syx[5] {
                // Bank-select MSB
                0x01 => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::BANK_SELECT,
                        value,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Bank-select LSB
                0x02 => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::BANK_SELECT_LSB,
                        value,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Program change
                0x03 => {
                    let evs = self.midi_channels[channel].program_change(
                        value,
                        &self.sound_bank_manager,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Part mode (drum channel flag)
                0x07 => {
                    // setDrums: for XG, set bank MSB to drum bank (127) if drum
                    let is_drum = value != 0;
                    if is_drum {
                        if let Some(drum_bank) = BankSelectHacks::get_drum_bank(current_system) {
                            self.midi_channels[channel].set_bank_msb(drum_bank);
                            self.midi_channels[channel].set_bank_lsb(0);
                        }
                    } else {
                        self.midi_channels[channel].set_bank_msb(0);
                        self.midi_channels[channel].set_bank_lsb(0);
                    }
                    if let Some(ev) = self.midi_channels[channel].set_drum_flag(is_drum) {
                        self.call_event(ev);
                    }
                    // Extract program before the mutable borrow of program_change
                    let program = self.midi_channels[channel].patch.program;
                    let evs = self.midi_channels[channel].program_change(
                        program,
                        &self.sound_bank_manager,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Note shift
                0x08 => {
                    if self.midi_channels[channel].drum_channel {
                        // Skip for drum channels
                    } else {
                        self.midi_channels[channel].set_custom_controller(
                            custom_controllers::CHANNEL_KEY_SHIFT,
                            (value as i32 - 64) as f64,
                        );
                    }
                }

                // Volume
                0x0b => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::MAIN_VOLUME,
                        value,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Pan position
                0x0e => {
                    let pan = value;
                    if pan == 0 {
                        // 0 means random
                        self.midi_channels[channel].random_pan = true;
                        spessa_synth_info(&format!("Random pan is set to ON for {}", channel));
                    } else {
                        let evs = self.midi_channels[channel].controller_change(
                            midi_controllers::PAN,
                            pan,
                            &mut self.voices,
                            current_time,
                            current_system,
                            enable_event_system,
                        );
                        for ev in evs {
                            self.call_event(ev);
                        }
                    }
                }

                // Dry (same as main volume)
                0x11 => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::MAIN_VOLUME,
                        value,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Chorus
                0x12 => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::CHORUS_DEPTH,
                        value,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Reverb
                0x13 => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::REVERB_DEPTH,
                        value,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Vibrato rate
                0x15 => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::VIBRATO_RATE,
                        value,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Vibrato depth
                0x16 => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::VIBRATO_DEPTH,
                        value,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Vibrato delay
                0x17 => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::VIBRATO_DELAY,
                        value,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Filter cutoff
                0x18 => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::BRIGHTNESS,
                        value,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Filter resonance
                0x19 => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::FILTER_RESONANCE,
                        value,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Attack time
                0x1a => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::ATTACK_TIME,
                        value,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Decay time
                0x1b => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::DECAY_TIME,
                        value,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Release time
                0x1c => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::RELEASE_TIME,
                        value,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                _ => {
                    spessa_synth_info(&format!(
                        "Unsupported Yamaha XG Part Setup: {:02X} for channel {}",
                        syx[5], channel
                    ));
                }
            }
        } else if a1 == 0x06 || a1 == 0x07 {
            // Display letters (0x06) or Display bitmap (0x07)
            self.call_event(
                crate::synthesizer::types::SynthProcessorEvent::SynthDisplay(syx.to_vec()),
            );
        } else if BankSelectHacks::is_system_xg(self.master_parameters.midi_system) {
            sys_ex_not_recognized(syx, "Yamaha XG");
        }
    }
}
