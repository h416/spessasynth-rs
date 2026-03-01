/// handle_gs.rs
/// purpose: Handles GS (Roland) system exclusive messages.
/// Ported from: src/synthesizer/audio_engine/engine_methods/system_exclusive/handle_gs.ts
/// References:
///   http://www.bandtrax.com.au/sysex.htm
///   https://cdn.roland.com/assets/media/pdf/AT-20R_30R_MI.pdf
use crate::midi::enums::midi_controllers;
use crate::soundbank::basic_soundbank::generator_types::generator_types;
use crate::soundbank::enums::modulator_sources;
use crate::synthesizer::audio_engine::engine_components::controller_tables::NON_CC_INDEX_OFFSET;
use crate::synthesizer::audio_engine::engine_methods::system_exclusive::helpers::{
    sys_ex_logging, sys_ex_not_recognized,
};
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::enums::custom_controllers;
use crate::synthesizer::types::{MasterParameterChangeCallback, SynthSystem};
use crate::utils::loggin::spessa_synth_info;
use crate::utils::string::read_binary_string;

impl SynthesizerCore {
    /// Handles a GS system exclusive message.
    /// Equivalent to: handleGS(syx, channelOffset)
    pub fn handle_gs(&mut self, syx: &[u8], channel_offset: usize) {
        // 0x12: DT1 (Device Transmit)
        if syx[3] != 0x12 {
            sys_ex_not_recognized(syx, "Roland GS");
            return;
        }

        // Model ID
        match syx[2] {
            0x42 => {
                // This is a GS sysex
                let message_value = syx[7];

                // syx[5] and [6] is the system parameter, syx[7] is the value.
                // Either patch common or SC-88 mode set.
                if syx[4] == 0x40 || (syx[4] == 0x00 && syx[6] == 0x7f) {
                    // This is a channel parameter
                    if (syx[5] & 0x10) > 0 {
                        // This is an individual part (channel) parameter.
                        // Determine the channel: 0 means channel 10 (default), 1 means 1, etc.
                        // SC-88Pro manual page 196
                        let channel_table =
                            [9u8, 0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15];
                        let channel =
                            channel_table[(syx[5] & 0x0f) as usize] as usize + channel_offset;

                        // Extract borrow-checker-safe copies of self fields
                        let current_time = self.current_time;
                        let current_system = self.master_parameters.midi_system;
                        let enable_event_system = self.enable_event_system;

                        match syx[6] {
                            0x15 => {
                                // Use for Drum Part sysex (multiple drums)
                                let is_drums = message_value > 0 && (syx[5] >> 4) > 0;
                                self.midi_channels[channel].set_gs_drums(is_drums);
                                spessa_synth_info(&format!(
                                    "Channel {} {} via: {:02X?}",
                                    channel,
                                    if is_drums {
                                        "is now a drum channel"
                                    } else {
                                        "now isn't a drum channel"
                                    },
                                    syx
                                ));
                            }

                            0x16 => {
                                // Pitch key shift sysex
                                let key_shift = message_value as i32 - 64;
                                self.midi_channels[channel].set_custom_controller(
                                    custom_controllers::CHANNEL_KEY_SHIFT,
                                    key_shift as f32,
                                );
                                sys_ex_logging(syx, channel as u8, &key_shift, "key shift", "keys");
                            }

                            0x1c => {
                                // Pan position: 0 is random
                                let pan_position = message_value;
                                if pan_position == 0 {
                                    self.midi_channels[channel].random_pan = true;
                                    spessa_synth_info(&format!(
                                        "Random pan is set to ON for {}",
                                        channel
                                    ));
                                } else {
                                    self.midi_channels[channel].random_pan = false;
                                    let voices = &mut self.voices;
                                    let evs = self.midi_channels[channel].controller_change(
                                        midi_controllers::PAN,
                                        pan_position,
                                        voices,
                                        current_time,
                                        current_system,
                                        enable_event_system,
                                    );
                                    for ev in evs {
                                        self.call_event(ev);
                                    }
                                }
                            }

                            0x21 => {
                                // Chorus send
                                let voices = &mut self.voices;
                                let evs = self.midi_channels[channel].controller_change(
                                    midi_controllers::CHORUS_DEPTH,
                                    message_value,
                                    voices,
                                    current_time,
                                    current_system,
                                    enable_event_system,
                                );
                                for ev in evs {
                                    self.call_event(ev);
                                }
                            }

                            0x22 => {
                                // Reverb send
                                let voices = &mut self.voices;
                                let evs = self.midi_channels[channel].controller_change(
                                    midi_controllers::REVERB_DEPTH,
                                    message_value,
                                    voices,
                                    current_time,
                                    current_system,
                                    enable_event_system,
                                );
                                for ev in evs {
                                    self.call_event(ev);
                                }
                            }

                            0x40..=0x4b => {
                                // Scale tuning: up to 12 bytes
                                let tuning_bytes = syx.len().saturating_sub(9); // Data starts at 7, minus checksum and f7
                                let mut new_tuning = [0i8; 12];
                                for i in 0..tuning_bytes.min(12) {
                                    new_tuning[i] = (syx[i + 7] as i16 - 64) as i8;
                                }
                                self.midi_channels[channel].set_octave_tuning(&new_tuning);
                                let cents = message_value as i32 - 64;
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &format!("{:?}", new_tuning),
                                    "octave scale tuning",
                                    "cents",
                                );
                                self.midi_channels[channel].set_tuning(cents as f32, false);
                            }

                            _ => {
                                // This is some other GS sysex...
                                sys_ex_not_recognized(syx, "Roland GS");
                            }
                        }
                    } else if (syx[5] & 0x20) > 0 {
                        // This is also a channel parameter.
                        // Determine the channel: 0 means channel 10 (default), 1 means 1, etc.
                        let channel_table =
                            [9u8, 0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15];
                        let channel =
                            channel_table[(syx[5] & 0x0f) as usize] as usize + channel_offset;

                        let centered_value = message_value as i32 - 64;
                        let normalized_value = centered_value as f64 / 64.0;
                        let normalized_not_centered = message_value as f64 / 128.0;

                        // Determine source and bipolar flag based on upper nibble of syx[6]
                        // SC88 manual page 198
                        let (source, source_name, is_bipolar): (usize, &str, bool) = match syx[6]
                            & 0xf0
                        {
                            0x00 => (
                                midi_controllers::MODULATION_WHEEL as usize,
                                "mod wheel",
                                false,
                            ),
                            0x10 => (
                                NON_CC_INDEX_OFFSET + modulator_sources::PITCH_WHEEL as usize,
                                "pitch wheel",
                                true,
                            ),
                            0x20 => (
                                NON_CC_INDEX_OFFSET + modulator_sources::CHANNEL_PRESSURE as usize,
                                "channel pressure",
                                false,
                            ),
                            0x30 => (
                                NON_CC_INDEX_OFFSET + modulator_sources::POLY_PRESSURE as usize,
                                "poly pressure",
                                false,
                            ),
                            _ => {
                                sys_ex_not_recognized(syx, "Roland GS");
                                return;
                            }
                        };

                        let current_time = self.current_time;
                        let current_system = self.master_parameters.midi_system;
                        let enable_event_system = self.enable_event_system;

                        // Setup receivers for CC to parameter mapping (SC-88 manual page 198)
                        match syx[6] & 0x0f {
                            0x00 => {
                                // Pitch control
                                // Special case: if the source is pitch wheel, it's a way of
                                // setting the pitch wheel range. Testcase: th07_03.mid
                                if source
                                    == NON_CC_INDEX_OFFSET + modulator_sources::PITCH_WHEEL as usize
                                {
                                    let voices = &mut self.voices;
                                    let ch = &mut self.midi_channels[channel];
                                    let mut evs = ch.controller_change(
                                        midi_controllers::REGISTERED_PARAMETER_MSB,
                                        0x0,
                                        voices,
                                        current_time,
                                        current_system,
                                        enable_event_system,
                                    );
                                    for ev in evs.drain(..) {
                                        self.call_event(ev);
                                    }
                                    let mut evs = self.midi_channels[channel].controller_change(
                                        midi_controllers::REGISTERED_PARAMETER_LSB,
                                        0x0,
                                        &mut self.voices,
                                        current_time,
                                        current_system,
                                        enable_event_system,
                                    );
                                    for ev in evs.drain(..) {
                                        self.call_event(ev);
                                    }
                                    let mut evs = self.midi_channels[channel].controller_change(
                                        midi_controllers::DATA_ENTRY_MSB,
                                        centered_value.max(0) as u8,
                                        &mut self.voices,
                                        current_time,
                                        current_system,
                                        enable_event_system,
                                    );
                                    for ev in evs.drain(..) {
                                        self.call_event(ev);
                                    }
                                } else {
                                    self.midi_channels[channel].sys_ex_modulators.set_modulator(
                                        source,
                                        generator_types::FINE_TUNE,
                                        centered_value as f64 * 100.0,
                                        is_bipolar,
                                        false,
                                    );
                                    sys_ex_logging(
                                        syx,
                                        channel as u8,
                                        &centered_value,
                                        &format!("{} pitch control", source_name),
                                        "semitones",
                                    );
                                }
                            }

                            0x01 => {
                                // Cutoff
                                self.midi_channels[channel].sys_ex_modulators.set_modulator(
                                    source,
                                    generator_types::INITIAL_FILTER_FC,
                                    normalized_value * 9600.0,
                                    is_bipolar,
                                    false,
                                );
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &(normalized_value * 9600.0),
                                    &format!("{} pitch control", source_name),
                                    "cents",
                                );
                            }

                            0x02 => {
                                // Amplitude
                                self.midi_channels[channel].sys_ex_modulators.set_modulator(
                                    source,
                                    generator_types::INITIAL_ATTENUATION,
                                    normalized_value * 960.0,
                                    is_bipolar,
                                    false,
                                );
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &(normalized_value * 960.0),
                                    &format!("{} amplitude", source_name),
                                    "cB",
                                );
                            }

                            // Rate control is ignored as it is in hertz (case 0x03)
                            0x04 => {
                                // LFO1 pitch depth
                                self.midi_channels[channel].sys_ex_modulators.set_modulator(
                                    source,
                                    generator_types::VIB_LFO_TO_PITCH,
                                    normalized_not_centered * 600.0,
                                    is_bipolar,
                                    false,
                                );
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &(normalized_not_centered * 600.0),
                                    &format!("{} LFO1 pitch depth", source_name),
                                    "cents",
                                );
                            }

                            0x05 => {
                                // LFO1 filter depth
                                self.midi_channels[channel].sys_ex_modulators.set_modulator(
                                    source,
                                    generator_types::VIB_LFO_TO_FILTER_FC,
                                    normalized_not_centered * 2400.0,
                                    is_bipolar,
                                    false,
                                );
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &(normalized_not_centered * 2400.0),
                                    &format!("{} LFO1 filter depth", source_name),
                                    "cents",
                                );
                            }

                            0x06 => {
                                // LFO1 amplitude depth
                                self.midi_channels[channel].sys_ex_modulators.set_modulator(
                                    source,
                                    generator_types::VIB_LFO_TO_VOLUME,
                                    normalized_value * 960.0,
                                    is_bipolar,
                                    false,
                                );
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &(normalized_value * 960.0),
                                    &format!("{} LFO1 amplitude depth", source_name),
                                    "cB",
                                );
                            }

                            // Rate control is ignored as it is in hertz (case 0x07)
                            0x08 => {
                                // LFO2 pitch depth
                                self.midi_channels[channel].sys_ex_modulators.set_modulator(
                                    source,
                                    generator_types::MOD_LFO_TO_PITCH,
                                    normalized_not_centered * 600.0,
                                    is_bipolar,
                                    false,
                                );
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &(normalized_not_centered * 600.0),
                                    &format!("{} LFO2 pitch depth", source_name),
                                    "cents",
                                );
                            }

                            0x09 => {
                                // LFO2 filter depth
                                self.midi_channels[channel].sys_ex_modulators.set_modulator(
                                    source,
                                    generator_types::MOD_LFO_TO_FILTER_FC,
                                    normalized_not_centered * 2400.0,
                                    is_bipolar,
                                    false,
                                );
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &(normalized_not_centered * 2400.0),
                                    &format!("{} LFO2 filter depth", source_name),
                                    "cents",
                                );
                            }

                            0x0a => {
                                // LFO2 amplitude depth
                                self.midi_channels[channel].sys_ex_modulators.set_modulator(
                                    source,
                                    generator_types::MOD_LFO_TO_VOLUME,
                                    normalized_value * 960.0,
                                    is_bipolar,
                                    false,
                                );
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &(normalized_value * 960.0),
                                    &format!("{} LFO2 amplitude depth", source_name),
                                    "cB",
                                );
                            }

                            _ => {}
                        }
                    } else if syx[5] == 0x00 {
                        // This is a global system parameter
                        match syx[6] {
                            0x7f => {
                                // Roland mode set / GS mode set
                                if message_value == 0x00 {
                                    // This is a GS reset
                                    spessa_synth_info("GS Reset received!");
                                    self.reset_all_controllers(SynthSystem::Gs);
                                } else if message_value == 0x7f {
                                    // GS mode off
                                    spessa_synth_info("GS system off, switching to GM");
                                    self.reset_all_controllers(SynthSystem::Gm);
                                }
                            }

                            0x06 => {
                                // Roland master pan
                                spessa_synth_info(&format!(
                                    "Roland GS Master Pan set to: {} with: {:02X?}",
                                    message_value, syx
                                ));
                                self.set_master_parameter(
                                    MasterParameterChangeCallback::MasterPan(
                                        (message_value as f64 - 64.0) / 64.0,
                                    ),
                                );
                            }

                            0x04 => {
                                // Roland GS master volume
                                spessa_synth_info(&format!(
                                    "Roland GS Master Volume set to: {} with: {:02X?}",
                                    message_value, syx
                                ));
                                self.set_midi_volume(message_value as f64 / 127.0);
                            }

                            0x05 => {
                                // Roland master key shift (transpose)
                                let transpose = message_value as i32 - 64;
                                spessa_synth_info(&format!(
                                    "Roland GS Master Key-Shift set to: {} with: {:02X?}",
                                    transpose, syx
                                ));
                                self.set_master_tuning(transpose as f64 * 100.0);
                            }

                            _ => {
                                sys_ex_not_recognized(syx, "Roland GS");
                            }
                        }
                    } else if syx[5] == 0x01 {
                        // This is also a global system parameter
                        match syx[6] {
                            0x00 => {
                                // Patch name
                                let patch_name = read_binary_string(syx, 16, 7);
                                spessa_synth_info(&format!("GS Patch name: {}", patch_name));
                            }

                            0x33 => {
                                // Reverb level
                                spessa_synth_info(&format!("GS Reverb level: {}", message_value));
                                // 64 is the default
                                self.reverb_send = message_value as f64 / 64.0;
                            }

                            // Unsupported reverb params
                            0x30 | 0x31 | 0x32 | 0x34 | 0x35 | 0x37 => {
                                spessa_synth_info(&format!(
                                    "Unsupported GS Reverb Parameter: {:02x}",
                                    syx[6]
                                ));
                            }

                            0x3a => {
                                // Chorus level
                                spessa_synth_info(&format!("GS Chorus level: {}", message_value));
                                // 64 is the default
                                self.chorus_send = message_value as f64 / 64.0;
                            }

                            // Unsupported chorus params
                            0x38 | 0x39 | 0x3b | 0x3c | 0x3d | 0x3e | 0x3f | 0x40 => {
                                spessa_synth_info(&format!(
                                    "Unsupported GS Chorus Parameter: {:02x}",
                                    syx[6]
                                ));
                            }

                            _ => {
                                sys_ex_not_recognized(syx, "Roland GS");
                            }
                        }
                    }
                } else {
                    // This is some other GS sysex...
                    sys_ex_not_recognized(syx, "Roland GS");
                }
            }

            0x45 => {
                // 0x45: GS Display Data
                // Check for embedded copyright (Roland SC display sysex)
                // http://www.bandtrax.com.au/sysex.htm
                if syx[4] == 0x10 {
                    // Sound Canvas Display
                    if syx[5] == 0x00 {
                        // Display letters
                        self.call_event(
                            crate::synthesizer::types::SynthProcessorEvent::SynthDisplay(
                                syx.to_vec(),
                            ),
                        );
                    } else if syx[5] == 0x01 {
                        // Matrix display
                        self.call_event(
                            crate::synthesizer::types::SynthProcessorEvent::SynthDisplay(
                                syx.to_vec(),
                            ),
                        );
                    } else {
                        sys_ex_not_recognized(syx, "Roland GS");
                    }
                }
            }

            0x16 => {
                // Some Roland
                if syx[4] == 0x10 {
                    // This is a roland master volume message
                    self.set_midi_volume(syx[7] as f64 / 100.0);
                    spessa_synth_info(&format!(
                        "Roland Master Volume control set to: {} via: {:02X?}",
                        syx[7], syx
                    ));
                }
            }

            _ => {}
        }
    }
}
