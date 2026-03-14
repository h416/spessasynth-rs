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
                                    key_shift as f64,
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
                                self.midi_channels[channel].set_tuning(cents as f64, false);
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
                    } else if syx[5] == 0x03 {
                        // EFX (Insertion Effect) Parameter
                        let addr3 = syx[6];
                        let data = syx[7].min(127);

                        if addr3 >= 0x03 && addr3 <= 0x16 {
                            // EFX parameter set
                            self.insertion_processor.set_parameter(addr3, data);
                            if (addr3 - 3) < 20 {
                                self.insertion_params[(addr3 - 3) as usize] = data;
                            }
                            spessa_synth_info(&format!("GS EFX Parameter {} = {}", addr3 - 2, data));
                            return;
                        }
                        match addr3 {
                            0x00 => {
                                // EFX Type selection (16-bit: data << 8 | syx[8])
                                let efx_type = (data as u16) << 8 | syx.get(8).copied().unwrap_or(0) as u16;
                                if let Some(proc) = crate::synthesizer::audio_engine::effects::insertion::create_insertion_processor(efx_type, self.sample_rate) {
                                    spessa_synth_info(&format!("GS EFX Type: {:04X}", efx_type));
                                    self.insertion_processor = proc;
                                } else {
                                    spessa_synth_info(&format!("Unsupported EFX processor: {:04X}, using Thru", efx_type));
                                    self.insertion_processor = Box::new(crate::synthesizer::audio_engine::effects::insertion::thru::ThruFx::new(self.sample_rate));
                                }
                                self.insertion_params = [255u8; 20];
                                self.insertion_processor.reset();
                            }

                            0x17 => {
                                // EFX send level to reverb
                                self.insertion_processor.set_send_level_to_reverb(data as f64 / 127.0);
                                spessa_synth_info(&format!("GS EFX Send Level to Reverb: {}", data));
                            }

                            0x18 => {
                                // EFX send level to chorus
                                self.insertion_processor.set_send_level_to_chorus(data as f64 / 127.0);
                                spessa_synth_info(&format!("GS EFX Send Level to Chorus: {}", data));
                            }

                            0x19 => {
                                // EFX send level to delay
                                self.insertion_processor.set_send_level_to_delay(data as f64 / 127.0);
                                self.delay_active = true;
                                spessa_synth_info(&format!("GS EFX Send Level to Delay: {}", data));
                            }

                            _ => {
                                sys_ex_not_recognized(syx, "Roland GS EFX");
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

                            // --- Reverb parameters (0x30-0x37) ---
                            0x30 => {
                                // Reverb macro
                                spessa_synth_info(&format!("GS Reverb Macro: {}", message_value));
                                self.set_reverb_macro(message_value);
                            }
                            0x31 => {
                                // Reverb character
                                spessa_synth_info(&format!("GS Reverb Character: {}", message_value));
                                self.reverb_processor.set_character(message_value);
                            }
                            0x32 => {
                                // Reverb pre-LPF
                                spessa_synth_info(&format!("GS Reverb Pre-LPF: {}", message_value));
                                self.reverb_processor.set_pre_lowpass(message_value);
                            }
                            0x33 => {
                                // Reverb level
                                spessa_synth_info(&format!("GS Reverb Level: {}", message_value));
                                self.reverb_processor.set_level(message_value);
                            }
                            0x34 => {
                                // Reverb time
                                spessa_synth_info(&format!("GS Reverb Time: {}", message_value));
                                self.reverb_processor.set_time(message_value);
                            }
                            0x35 => {
                                // Reverb delay feedback
                                spessa_synth_info(&format!("GS Reverb Delay Feedback: {}", message_value));
                                self.reverb_processor.set_delay_feedback(message_value);
                            }
                            0x36 => {
                                // Reverb send to chorus (legacy SC-55, unsupported)
                            }
                            0x37 => {
                                // Reverb predelay time
                                spessa_synth_info(&format!("GS Reverb Predelay Time: {}", message_value));
                                self.reverb_processor.set_pre_delay_time(message_value);
                            }

                            // --- Chorus parameters (0x38-0x40) ---
                            0x38 => {
                                // Chorus macro
                                spessa_synth_info(&format!("GS Chorus Macro: {}", message_value));
                                self.set_chorus_macro(message_value);
                            }
                            0x39 => {
                                // Chorus pre-LPF
                                spessa_synth_info(&format!("GS Chorus Pre-LPF: {}", message_value));
                                self.chorus_processor.set_pre_lowpass(message_value);
                            }
                            0x3a => {
                                // Chorus level
                                spessa_synth_info(&format!("GS Chorus Level: {}", message_value));
                                self.chorus_processor.set_level(message_value);
                            }
                            0x3b => {
                                // Chorus feedback
                                spessa_synth_info(&format!("GS Chorus Feedback: {}", message_value));
                                self.chorus_processor.set_feedback(message_value);
                            }
                            0x3c => {
                                // Chorus delay
                                spessa_synth_info(&format!("GS Chorus Delay: {}", message_value));
                                self.chorus_processor.set_delay(message_value);
                            }
                            0x3d => {
                                // Chorus rate
                                spessa_synth_info(&format!("GS Chorus Rate: {}", message_value));
                                self.chorus_processor.set_rate(message_value);
                            }
                            0x3e => {
                                // Chorus depth
                                spessa_synth_info(&format!("GS Chorus Depth: {}", message_value));
                                self.chorus_processor.set_depth(message_value);
                            }
                            0x3f => {
                                // Chorus send level to reverb
                                spessa_synth_info(&format!("GS Chorus Send To Reverb: {}", message_value));
                                self.chorus_processor.set_send_level_to_reverb(message_value);
                            }
                            0x40 => {
                                // Chorus send level to delay — also activates delay
                                spessa_synth_info(&format!("GS Chorus Send To Delay: {}", message_value));
                                self.chorus_processor.set_send_level_to_delay(message_value);
                                self.delay_active = true;
                            }

                            // --- Delay parameters (0x50-0x5A) ---
                            0x50 => {
                                // Delay macro
                                spessa_synth_info(&format!("GS Delay Macro: {}", message_value));
                                self.set_delay_macro(message_value);
                                self.delay_active = true;
                            }
                            0x51 => {
                                // Delay pre-LPF
                                spessa_synth_info(&format!("GS Delay Pre-LPF: {}", message_value));
                                self.delay_processor.set_pre_lowpass(message_value);
                                self.delay_active = true;
                            }
                            0x52 => {
                                // Delay time center
                                spessa_synth_info(&format!("GS Delay Time Center: {}", message_value));
                                self.delay_processor.set_time_center(message_value);
                                self.delay_active = true;
                            }
                            0x53 => {
                                // Delay time ratio left
                                spessa_synth_info(&format!("GS Delay Time Ratio Left: {}", message_value));
                                self.delay_processor.set_time_ratio_left(message_value);
                                self.delay_active = true;
                            }
                            0x54 => {
                                // Delay time ratio right
                                spessa_synth_info(&format!("GS Delay Time Ratio Right: {}", message_value));
                                self.delay_processor.set_time_ratio_right(message_value);
                                self.delay_active = true;
                            }
                            0x55 => {
                                // Delay level center
                                spessa_synth_info(&format!("GS Delay Level Center: {}", message_value));
                                self.delay_processor.set_level_center(message_value);
                                self.delay_active = true;
                            }
                            0x56 => {
                                // Delay level left
                                spessa_synth_info(&format!("GS Delay Level Left: {}", message_value));
                                self.delay_processor.set_level_left(message_value);
                                self.delay_active = true;
                            }
                            0x57 => {
                                // Delay level right
                                spessa_synth_info(&format!("GS Delay Level Right: {}", message_value));
                                self.delay_processor.set_level_right(message_value);
                                self.delay_active = true;
                            }
                            0x58 => {
                                // Delay level
                                spessa_synth_info(&format!("GS Delay Level: {}", message_value));
                                self.delay_processor.set_level(message_value);
                                self.delay_active = true;
                            }
                            0x59 => {
                                // Delay feedback
                                spessa_synth_info(&format!("GS Delay Feedback: {}", message_value));
                                self.delay_processor.set_feedback(message_value);
                                self.delay_active = true;
                            }
                            0x5a => {
                                // Delay send level to reverb
                                spessa_synth_info(&format!("GS Delay Send To Reverb: {}", message_value));
                                self.delay_processor.set_send_level_to_reverb(message_value);
                                self.delay_active = true;
                            }

                            _ => {
                                sys_ex_not_recognized(syx, "Roland GS");
                            }
                        }
                    } else if (syx[5] >> 4) == 4 {
                        // Patch Parameter Tone Map (addr2 = 0x4X)
                        let channel_table =
                            [9u8, 0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15];
                        let channel =
                            channel_table[(syx[5] & 0x0f) as usize] as usize + channel_offset;
                        match syx[6] {
                            0x00 | 0x01 => {
                                // Tone map number (bank select LSB)
                                let voices = &mut self.voices;
                                let evs = self.midi_channels[channel].controller_change(
                                    midi_controllers::BANK_SELECT_LSB,
                                    message_value,
                                    voices,
                                    self.current_time,
                                    self.master_parameters.midi_system,
                                    self.enable_event_system,
                                );
                                for ev in evs {
                                    self.call_event(ev);
                                }
                            }
                            0x22 => {
                                // EFX assign
                                let efx = message_value == 1;
                                self.midi_channels[channel].insertion_enabled = efx;
                                if efx {
                                    self.insertion_active = true;
                                }
                                spessa_synth_info(&format!(
                                    "Insertion for {}: {}",
                                    channel,
                                    if efx { "ON" } else { "OFF" }
                                ));
                            }
                            _ => {
                                sys_ex_not_recognized(syx, "Roland GS Patch Part Parameter");
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
