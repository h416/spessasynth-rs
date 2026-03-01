/// controller_change.rs
/// purpose: MIDI controller change handler for MidiChannel.
/// Ported from: src/synthesizer/audio_engine/engine_methods/controller_control/controller_change.ts
use crate::midi::enums::midi_controllers;
use crate::synthesizer::audio_engine::engine_components::synth_constants::DEFAULT_PERCUSSION;
use crate::synthesizer::audio_engine::engine_components::voice::Voice;
use crate::synthesizer::audio_engine::synthesizer_core::MidiChannel;
use crate::synthesizer::enums::{custom_controllers, data_entry_states};
use crate::synthesizer::types::{ControllerChangeCallback, SynthProcessorEvent, SynthSystem};
use crate::utils::midi_hacks::BankSelectHacks;

use super::data_entry::data_entry_coarse::non_registered_msb;

impl MidiChannel {
    /// Handles a MIDI controller change for this channel.
    ///
    /// Updates the midiControllers table and dispatches special-case handling
    /// for bank select, data entry, sustain pedal, etc. Computes modulators
    /// for all active voices if the controller affects sound parameters.
    ///
    /// `controller`: MIDI controller number (0–127)
    /// `value`: Controller value (0–127)
    /// `enable_event_system`: Whether to emit a ControllerChange event
    ///
    /// Equivalent to: controllerChange(controllerNumber, controllerValue, sendEvent = true)
    pub fn controller_change(
        &mut self,
        controller: u8,
        value: u8,
        voices: &mut [Voice],
        current_time: f64,
        current_system: SynthSystem,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        if controller > 127 {
            // Invalid controller (caller should not send > 127, but guard anyway)
            return Vec::new();
        }

        let mut events = Vec::new();

        // LSB controllers (33–45, excluding dataEntryLSB = 38):
        // append as lower 7 bits of the 14-bit main controller value.
        if (midi_controllers::MODULATION_WHEEL_LSB..=midi_controllers::EFFECT_CONTROL2_LSB)
            .contains(&controller)
            && controller != midi_controllers::DATA_ENTRY_LSB
        {
            let actual_cc = (controller - 32) as usize;
            if self.locked_controllers[actual_cc] {
                return events;
            }
            self.midi_controllers[actual_cc] =
                (self.midi_controllers[actual_cc] & 0x3f_80) | (value as i16 & 0x7f);
            self.compute_modulators_all_impl(voices, 1, actual_cc);
        }

        if self.locked_controllers[controller as usize] {
            return events;
        }

        // Apply CC to table (14-bit, MSB in upper bits)
        self.midi_controllers[controller as usize] = (value as i16) << 7;

        // Interpret special CCs
        match controller {
            // Channel mode messages — stop all notes
            midi_controllers::OMNI_MODE_OFF
            | midi_controllers::OMNI_MODE_ON
            | midi_controllers::ALL_NOTES_OFF => {
                let mut sub = self.stop_all_notes(voices, current_time, false);
                events.append(&mut sub);
            }

            midi_controllers::ALL_SOUND_OFF => {
                let mut sub = self.stop_all_notes(voices, current_time, true);
                events.append(&mut sub);
            }

            midi_controllers::POLY_MODE_ON => {
                let mut sub = self.stop_all_notes(voices, current_time, true);
                events.append(&mut sub);
                self.poly_mode = true;
            }

            midi_controllers::MONO_MODE_ON => {
                let mut sub = self.stop_all_notes(voices, current_time, true);
                events.append(&mut sub);
                self.poly_mode = false;
            }

            // Bank select MSB
            midi_controllers::BANK_SELECT => {
                self.set_bank_msb(value);
                // For XG, drum channel (ch 9 mod 16) always uses bank 127
                let ch_system = self.channel_system(current_system);
                if self.channel % 16 == DEFAULT_PERCUSSION
                    && BankSelectHacks::is_system_xg(ch_system)
                {
                    self.set_bank_msb(127);
                }
            }

            // Bank select LSB
            midi_controllers::BANK_SELECT_LSB => {
                self.set_bank_lsb(value);
            }

            // RPN / NRPN state machine
            midi_controllers::REGISTERED_PARAMETER_LSB => {
                self.data_entry_state = data_entry_states::RP_FINE;
            }

            midi_controllers::REGISTERED_PARAMETER_MSB => {
                self.data_entry_state = data_entry_states::RP_COARSE;
            }

            midi_controllers::NON_REGISTERED_PARAMETER_MSB => {
                // SF2 spec section 9.6.2: reset SF2 NRPN generator LSB on new NRPN
                self.custom_controllers[custom_controllers::SF2_NPRN_GENERATOR_LSB as usize] = 0.0;
                self.data_entry_state = data_entry_states::NRP_COARSE;
            }

            midi_controllers::NON_REGISTERED_PARAMETER_LSB => {
                let nrpn_msb = (self.midi_controllers
                    [midi_controllers::NON_REGISTERED_PARAMETER_MSB as usize]
                    >> 7) as u8;
                if nrpn_msb == non_registered_msb::SF2 {
                    // Accumulate SF2 NRPN LSB selector
                    let current = self.custom_controllers
                        [custom_controllers::SF2_NPRN_GENERATOR_LSB as usize];
                    // Reset if previous value was not a multiple-of-100
                    let current_i = current as i32;
                    if current_i % 100 != 0 {
                        self.custom_controllers
                            [custom_controllers::SF2_NPRN_GENERATOR_LSB as usize] = 0.0;
                    }
                    match value {
                        100 => {
                            self.custom_controllers
                                [custom_controllers::SF2_NPRN_GENERATOR_LSB as usize] += 100.0;
                        }
                        101 => {
                            self.custom_controllers
                                [custom_controllers::SF2_NPRN_GENERATOR_LSB as usize] += 1000.0;
                        }
                        102 => {
                            self.custom_controllers
                                [custom_controllers::SF2_NPRN_GENERATOR_LSB as usize] += 10_000.0;
                        }
                        v if v < 100 => {
                            self.custom_controllers
                                [custom_controllers::SF2_NPRN_GENERATOR_LSB as usize] +=
                                value as f32;
                        }
                        _ => {}
                    }
                }
                self.data_entry_state = data_entry_states::NRP_FINE;
            }

            // Data entry MSB → process via dataEntryCoarse
            midi_controllers::DATA_ENTRY_MSB => {
                let mut sub = self.data_entry_coarse(
                    value,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
            }

            // Data entry LSB → process via dataEntryFine
            midi_controllers::DATA_ENTRY_LSB => {
                let mut sub = self.data_entry_fine(value, voices);
                events.append(&mut sub);
            }

            // Reset all controllers (RP-15)
            midi_controllers::RESET_ALL_CONTROLLERS => {
                let mut sub = self.reset_controllers_rp15(
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
            }

            // Sustain pedal release: release held voices
            midi_controllers::SUSTAIN_PEDAL => {
                if value < 64 {
                    let mut vc = 0u32;
                    if self.voice_count > 0 {
                        for v in voices.iter_mut() {
                            if v.channel == self.channel && v.is_active && v.is_held {
                                v.is_held = false;
                                v.release_voice(current_time, 0.0);
                                vc += 1;
                                if vc >= self.voice_count {
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            // Default: compute modulators for all active voices
            _ => {
                self.compute_modulators_all_impl(voices, 1, controller as usize);
            }
        }

        if enable_event_system {
            events.push(SynthProcessorEvent::ControllerChange(
                ControllerChangeCallback {
                    channel: self.channel,
                    controller_number: controller,
                    controller_value: value,
                },
            ));
        }

        events
    }
}
