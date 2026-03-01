/// data_entry_coarse.rs
/// purpose: MIDI data entry coarse (MSB) handler for MidiChannel.
/// Ported from: src/synthesizer/audio_engine/engine_methods/controller_control/data_entry/data_entry_coarse.ts
use crate::midi::enums::midi_controllers;
use crate::soundbank::basic_soundbank::generator_types::GeneratorType;
use crate::soundbank::enums::modulator_sources;
use crate::synthesizer::audio_engine::engine_components::controller_tables::NON_CC_INDEX_OFFSET;
use crate::synthesizer::audio_engine::engine_components::voice::Voice;
use crate::synthesizer::audio_engine::synthesizer_core::MidiChannel;
use crate::synthesizer::enums::{custom_controllers, data_entry_states};
use crate::synthesizer::types::{SynthProcessorEvent, SynthSystem};
use crate::utils::loggin::spessa_synth_info;

/// Registered parameter number types (RPN).
/// Equivalent to: registeredParameterTypes
pub mod registered_parameter_types {
    pub const PITCH_WHEEL_RANGE: u16 = 0x00_00;
    pub const FINE_TUNING: u16 = 0x00_01;
    pub const COARSE_TUNING: u16 = 0x00_02;
    pub const MODULATION_DEPTH: u16 = 0x00_05;
    pub const RESET_PARAMETERS: u16 = 0x3f_ff;
}

/// Non-registered parameter MSB values.
/// Equivalent to: nonRegisteredMSB
pub mod non_registered_msb {
    pub const PART_PARAMETER: u8 = 0x01;
    pub const AWE32: u8 = 0x7f;
    pub const SF2: u8 = 120;
}

/// Non-registered parameter LSB values (GS/XG vibrato and EG).
/// https://cdn.roland.com/assets/media/pdf/SC-88PRO_OM.pdf
mod non_registered_lsb {
    pub const VIBRATO_RATE: u8 = 0x08;
    pub const VIBRATO_DEPTH: u8 = 0x09;
    pub const VIBRATO_DELAY: u8 = 0x0a;
    pub const TVF_FILTER_CUTOFF: u8 = 0x20;
    pub const TVF_FILTER_RESONANCE: u8 = 0x21;
    pub const EG_ATTACK_TIME: u8 = 0x63;
    pub const EG_DECAY_TIME: u8 = 0x64;
    pub const EG_RELEASE_TIME: u8 = 0x66;
}

/// Ensures channel vibrato has non-zero defaults before adjusting.
fn add_default_vibrato(chan: &mut MidiChannel) {
    if chan.channel_vibrato.delay == 0.0
        && chan.channel_vibrato.rate == 0.0
        && chan.channel_vibrato.depth == 0.0
    {
        chan.channel_vibrato.depth = 50.0;
        chan.channel_vibrato.rate = 8.0;
        chan.channel_vibrato.delay = 0.6;
    }
}

impl MidiChannel {
    /// Handles MIDI data entry coarse (MSB) change for the current channel.
    ///
    /// Processes RPN and NRPN messages based on the current data entry state.
    ///
    /// Equivalent to: dataEntryCoarse(dataValue)
    pub fn data_entry_coarse(
        &mut self,
        data_value: u8,
        voices: &mut [Voice],
        current_time: f64,
        current_system: SynthSystem,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        // Store in cc table
        self.midi_controllers[midi_controllers::DATA_ENTRY_MSB as usize] = (data_value as i16) << 7;

        let mut events = Vec::new();

        match self.data_entry_state {
            data_entry_states::IDLE => {
                // No-op
            }

            data_entry_states::NRP_FINE => {
                if self.lock_gs_nrpn_params {
                    return events;
                }
                let nrpn_coarse = (self.midi_controllers
                    [midi_controllers::NON_REGISTERED_PARAMETER_MSB as usize]
                    >> 7) as u8;
                let nrpn_fine = (self.midi_controllers
                    [midi_controllers::NON_REGISTERED_PARAMETER_LSB as usize]
                    >> 7) as u8;
                let data_entry_fine =
                    (self.midi_controllers[midi_controllers::DATA_ENTRY_LSB as usize] >> 7) as u8;

                match nrpn_coarse {
                    non_registered_msb::PART_PARAMETER => {
                        let mut sub_events = self.handle_nrpn_part_parameter(
                            nrpn_fine,
                            data_value,
                            voices,
                            current_time,
                            current_system,
                            enable_event_system,
                        );
                        events.append(&mut sub_events);
                    }

                    non_registered_msb::AWE32 => {
                        // AWE32 is handled via data_entry_fine (LSB), not coarse
                    }

                    non_registered_msb::SF2 => {
                        if nrpn_fine <= 100 {
                            let r#gen = self.custom_controllers
                                [custom_controllers::SF2_NPRN_GENERATOR_LSB as usize]
                                as GeneratorType;
                            let offset =
                                (((data_value as i32) << 7) | data_entry_fine as i32) - 8192;
                            self.set_generator_offset(r#gen, offset as i16, voices);
                        }
                    }

                    _ => {
                        if data_value != 64 {
                            spessa_synth_info(&format!(
                                "Unrecognized NRPN for ch {}: (0x{:02X} 0x{:02X}) data value: {}",
                                self.channel, nrpn_coarse, nrpn_fine, data_value
                            ));
                        }
                    }
                }
            }

            // RPCoarse or RPFine
            _ => {
                let rpn_value = (self.midi_controllers
                    [midi_controllers::REGISTERED_PARAMETER_MSB as usize]
                    as u16)
                    | ((self.midi_controllers[midi_controllers::REGISTERED_PARAMETER_LSB as usize]
                        >> 7) as u16);

                let mut sub_events = self.handle_rpn_coarse(
                    rpn_value,
                    data_value,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub_events);
            }
        }

        events
    }

    /// Processes NRPN part parameter messages (NRPNCoarse = 0x01).
    fn handle_nrpn_part_parameter(
        &mut self,
        nrpn_fine: u8,
        data_value: u8,
        voices: &mut [Voice],
        current_time: f64,
        current_system: SynthSystem,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        use non_registered_lsb as nrl;
        let mut events = Vec::new();

        match nrpn_fine {
            nrl::VIBRATO_RATE => {
                if data_value == 64 {
                    return events;
                }
                add_default_vibrato(self);
                self.channel_vibrato.rate = (data_value as f64 / 64.0) * 8.0;
                spessa_synth_info(&format!(
                    "Vibrato rate for {}: {} = {} Hz",
                    self.channel, data_value, self.channel_vibrato.rate
                ));
            }

            nrl::VIBRATO_DEPTH => {
                if data_value == 64 {
                    return events;
                }
                add_default_vibrato(self);
                self.channel_vibrato.depth = data_value as f64 / 2.0;
                spessa_synth_info(&format!(
                    "Vibrato depth for {}: {} = {} cents",
                    self.channel, data_value, self.channel_vibrato.depth
                ));
            }

            nrl::VIBRATO_DELAY => {
                if data_value == 64 {
                    return events;
                }
                add_default_vibrato(self);
                self.channel_vibrato.delay = data_value as f64 / 64.0 / 3.0;
                spessa_synth_info(&format!(
                    "Vibrato delay for {}: {} = {} seconds",
                    self.channel, data_value, self.channel_vibrato.delay
                ));
            }

            nrl::TVF_FILTER_CUTOFF => {
                let mut sub = self.controller_change(
                    midi_controllers::BRIGHTNESS,
                    data_value,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
                spessa_synth_info(&format!(
                    "Filter cutoff for {}: {}",
                    self.channel, data_value
                ));
            }

            nrl::TVF_FILTER_RESONANCE => {
                let mut sub = self.controller_change(
                    midi_controllers::FILTER_RESONANCE,
                    data_value,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
                spessa_synth_info(&format!(
                    "Filter resonance for {}: {}",
                    self.channel, data_value
                ));
            }

            nrl::EG_ATTACK_TIME => {
                let mut sub = self.controller_change(
                    midi_controllers::ATTACK_TIME,
                    data_value,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
                spessa_synth_info(&format!(
                    "EG attack time for {}: {}",
                    self.channel, data_value
                ));
            }

            nrl::EG_DECAY_TIME => {
                let mut sub = self.controller_change(
                    midi_controllers::DECAY_TIME,
                    data_value,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
                spessa_synth_info(&format!(
                    "EG decay time for {}: {}",
                    self.channel, data_value
                ));
            }

            nrl::EG_RELEASE_TIME => {
                let mut sub = self.controller_change(
                    midi_controllers::RELEASE_TIME,
                    data_value,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
                spessa_synth_info(&format!(
                    "EG release time for {}: {}",
                    self.channel, data_value
                ));
            }

            _ => {
                if data_value != 64 {
                    spessa_synth_info(&format!(
                        "Unrecognized NRPN for ch {}: (0x01 0x{:02X}) data value: {}",
                        self.channel, nrpn_fine, data_value
                    ));
                }
            }
        }

        events
    }

    /// Processes RPN coarse/fine messages (registeredParameterTypes).
    fn handle_rpn_coarse(
        &mut self,
        rpn_value: u16,
        data_value: u8,
        voices: &mut [Voice],
        current_time: f64,
        current_system: SynthSystem,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        use registered_parameter_types as rpt;
        let mut events = Vec::new();

        match rpn_value {
            rpt::PITCH_WHEEL_RANGE => {
                self.midi_controllers
                    [NON_CC_INDEX_OFFSET + modulator_sources::PITCH_WHEEL_RANGE as usize] =
                    (data_value as i16) << 7;
                spessa_synth_info(&format!(
                    "Pitch wheel range for {}: {} semitones",
                    self.channel, data_value
                ));
            }

            rpt::COARSE_TUNING => {
                let semitones = data_value as i32 - 64;
                self.set_custom_controller(
                    custom_controllers::CHANNEL_TUNING_SEMITONES,
                    semitones as f32,
                );
                spessa_synth_info(&format!(
                    "Coarse tuning for {}: {} semitones",
                    self.channel, semitones
                ));
            }

            rpt::FINE_TUNING => {
                // Store raw value; LSB will be adjusted in data_entry_fine
                self.set_tuning(data_value as f32 - 64.0, false);
            }

            rpt::MODULATION_DEPTH => {
                self.set_modulation_depth(data_value as f32 * 100.0);
            }

            rpt::RESET_PARAMETERS => {
                self.reset_parameters();
            }

            _ => {
                spessa_synth_info(&format!(
                    "Unrecognized RPN for ch {}: (0x{:04X}) data value: {}",
                    self.channel, rpn_value, data_value
                ));
            }
        }

        let _ = (voices, current_time, current_system, enable_event_system); // consumed via sub-calls
        events
    }
}
