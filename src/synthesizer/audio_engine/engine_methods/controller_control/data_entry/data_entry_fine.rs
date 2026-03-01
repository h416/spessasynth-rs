/// data_entry_fine.rs
/// purpose: MIDI data entry fine (LSB) handler for MidiChannel.
/// Ported from: src/synthesizer/audio_engine/engine_methods/controller_control/data_entry/data_entry_fine.ts
use crate::midi::enums::midi_controllers;
use crate::soundbank::enums::modulator_sources;
use crate::synthesizer::audio_engine::engine_components::controller_tables::NON_CC_INDEX_OFFSET;
use crate::synthesizer::audio_engine::engine_components::voice::Voice;
use crate::synthesizer::audio_engine::synthesizer_core::MidiChannel;
use crate::synthesizer::enums::{custom_controllers, data_entry_states};
use crate::synthesizer::types::SynthProcessorEvent;
use crate::utils::loggin::spessa_synth_info;

use super::data_entry_coarse::{non_registered_msb, registered_parameter_types as rpt};

impl MidiChannel {
    /// Handles MIDI data entry fine (LSB) change for the current channel.
    ///
    /// Processes RPN fine-tuning and AWE32 NRPN messages.
    ///
    /// Equivalent to: dataEntryFine(dataValue)
    pub fn data_entry_fine(
        &mut self,
        data_value: u8,
        voices: &mut [Voice],
    ) -> Vec<SynthProcessorEvent> {
        // Store in cc table
        self.midi_controllers[midi_controllers::DATA_ENTRY_LSB as usize] = (data_value as i16) << 7;

        match self.data_entry_state {
            data_entry_states::RP_COARSE | data_entry_states::RP_FINE => {
                let rpn_value = (self.midi_controllers
                    [midi_controllers::REGISTERED_PARAMETER_MSB as usize]
                    as u16)
                    | ((self.midi_controllers[midi_controllers::REGISTERED_PARAMETER_LSB as usize]
                        >> 7) as u16);

                match rpn_value {
                    rpt::PITCH_WHEEL_RANGE => {
                        if data_value != 0 {
                            // 14-bit value: upper 7 bits are coarse, lower 7 are fine
                            let idx =
                                NON_CC_INDEX_OFFSET + modulator_sources::PITCH_WHEEL_RANGE as usize;
                            self.midi_controllers[idx] |= data_value as i16;
                            let actual_tune = (self.midi_controllers[idx] >> 7) as f64
                                + data_value as f64 / 128.0;
                            spessa_synth_info(&format!(
                                "Channel {} pitch wheel range: {} semitones",
                                self.channel, actual_tune
                            ));
                        }
                    }

                    rpt::FINE_TUNING => {
                        // Combine coarse (stored in custom_controllers[CHANNEL_TUNING]) with fine
                        let coarse =
                            self.custom_controllers[custom_controllers::CHANNEL_TUNING as usize];
                        let final_tuning = ((coarse as i32) << 7) | data_value as i32;
                        // Multiply by 8192/100 to get cent increments: 0.01220703125
                        self.set_tuning(final_tuning as f32 * 0.012_207_031_f32, true);
                    }

                    rpt::MODULATION_DEPTH => {
                        let current_cents = self.custom_controllers
                            [custom_controllers::MODULATION_MULTIPLIER as usize]
                            * 50.0;
                        let cents = current_cents + (data_value as f32 / 128.0) * 100.0;
                        self.set_modulation_depth(cents);
                    }

                    rpt::RESET_PARAMETERS => {
                        self.reset_parameters();
                    }

                    _ => {
                        // Unrecognized RPN LSB; no-op
                    }
                }
            }

            data_entry_states::NRP_FINE => {
                let nrpn_coarse = (self.midi_controllers
                    [midi_controllers::NON_REGISTERED_PARAMETER_MSB as usize]
                    >> 7) as u8;
                let nrpn_fine = (self.midi_controllers
                    [midi_controllers::NON_REGISTERED_PARAMETER_LSB as usize]
                    >> 7) as u8;

                // SF2 NRPN: fine is not used here; coarse handles the full value
                if nrpn_coarse == non_registered_msb::SF2 {
                    return Vec::new();
                }

                match nrpn_coarse {
                    non_registered_msb::AWE32 => {
                        let data_msb = (self.midi_controllers
                            [midi_controllers::DATA_ENTRY_MSB as usize]
                            >> 7) as u8;
                        self.handle_awe32_nrpn(nrpn_fine as usize, data_value, data_msb, voices);
                    }

                    _ => {
                        spessa_synth_info(&format!(
                            "Unrecognized NRPN LSB for ch {}: (0x{:02X} 0x{:02X}) data value: {}",
                            self.channel, nrpn_coarse, nrpn_fine, data_value
                        ));
                    }
                }
            }

            _ => {
                // Idle or other state: no-op
            }
        }

        Vec::new()
    }
}
