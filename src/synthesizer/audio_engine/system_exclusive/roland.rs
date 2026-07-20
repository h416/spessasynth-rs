/// handle_gs.rs
/// purpose: Handles GS (Roland) system exclusive messages.
/// Ported from: src/synthesizer/audio_engine/engine_methods/system_exclusive/handle_gs.ts
/// References:
///   http://www.bandtrax.com.au/sysex.htm
///   https://cdn.roland.com/assets/media/pdf/AT-20R_30R_MI.pdf
use crate::midi::enums::midi_controllers;
use crate::soundbank::basic_soundbank::generator_types::generator_types;
use crate::soundbank::enums::modulator_sources;
use crate::synthesizer::audio_engine::channel::parameters::midi::ChannelMidiParameterValue;
use crate::synthesizer::audio_engine::system_exclusive::system_exclusive::{
    sys_ex_logging, sys_ex_not_recognized,
};
use crate::synthesizer::audio_engine::synth_constants::EFX_SENDS_GAIN_CORRECTION;
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::types::GlobalMIDIParameterChangeCallback;
use crate::soundbank::types::MIDISystem;
use crate::utils::loggin::spessa_synth_info;
use crate::utils::byte_functions::string::read_binary_string;

impl SynthesizerCore {
    /// Handles a GS system exclusive message.
    /// Equivalent to: handleGS(syx, channelOffset)
    pub fn handle_gs(&mut self, syx: &[u8], channel_offset: usize) {
        // Mutable copy: a1 === 0x50 (BLOCK B) adds 16 to reach the second bank of channels.
        let mut channel_offset = channel_offset;
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
                if syx[4] == 0x40 || syx[4] == 0x50 || (syx[4] == 0x00 && syx[6] == 0x7f) {
                    // 0x50 means BLOCK B (+16 channels). Testcase: 95043-2.KYC.mid
                    if syx[4] == 0x50 {
                        channel_offset += 16;
                    }
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
                        let current_system = self.midi_parameters.system;
                        let enable_event_system = self.system_parameters.events_enabled;

                        match syx[6] {
                            0x14 => {
                                // Assign mode
                                self.midi_channels[channel].set_midi_parameter(
                                    ChannelMidiParameterValue::AssignMode(message_value),
                                );
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &message_value,
                                    "assign mode",
                                    "",
                                );
                            }

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
                                self.midi_channels[channel].set_midi_parameter(
                                    ChannelMidiParameterValue::KeyShift(key_shift as f64),
                                );
                                sys_ex_logging(syx, channel as u8, &key_shift, "key shift", "keys");
                            }

                            0x1a => {
                                // Velocity Sense Depth
                                self.midi_channels[channel].set_midi_parameter(
                                    ChannelMidiParameterValue::VelocitySenseDepth(message_value),
                                );
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &message_value,
                                    "velocity sense depth",
                                    "",
                                );
                            }

                            0x1b => {
                                // Velocity Sense Offset
                                self.midi_channels[channel].set_midi_parameter(
                                    ChannelMidiParameterValue::VelocitySenseOffset(message_value),
                                );
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &message_value,
                                    "velocity sense offset",
                                    "",
                                );
                            }

                            0x1c => {
                                // Pan position: 0 is random
                                let pan_position = message_value;
                                if pan_position == 0 {
                                    self.midi_channels[channel]
                                        .set_midi_parameter(ChannelMidiParameterValue::RandomPan(true));
                                    spessa_synth_info(&format!(
                                        "Random pan is set to ON for {}",
                                        channel
                                    ));
                                } else {
                                    self.midi_channels[channel]
                                        .set_midi_parameter(ChannelMidiParameterValue::RandomPan(false));
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

                            0x2a => {
                                // Per-channel fine tune.
                                // 14-bit value (0-16383) centered at 8192;
                                // cents = (tune - 8192) / 81.92.
                                let tune = ((message_value as i32) << 7) | syx[8] as i32;
                                let cents = (tune as f64 - 8192.0) / 81.92;
                                self.midi_channels[channel].set_midi_parameter(
                                    ChannelMidiParameterValue::FineTune(cents),
                                );
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &(cents.round() as i32),
                                    "fine tuning",
                                    "cents",
                                );
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

                        let data = message_value;
                        let a3 = syx[6];

                        // Patch parameter controller (SC-88 manual page 198).
                        // Upper nibble of a3 selects the modulation source; the lower
                        // nibble selects the parameter (dispatched inside setupReceiver).
                        match a3 & 0xf0 {
                            0x00 => {
                                // Modulation wheel
                                if (a3 & 0x0f) == 0x04 {
                                    // LFO1 pitch depth special case: a mod wheel here is a
                                    // strange way of setting the modulation depth.
                                    // Testcase: J-Cycle.mid (affects gm.dls which uses LFO1).
                                    let cents = (data as f64 / 127.0) * 600.0;
                                    self.midi_channels[channel].set_modulation_depth(cents);
                                } else {
                                    self.midi_channels[channel].sys_ex_modulators.setup_receiver(
                                        a3,
                                        data,
                                        midi_controllers::MODULATION_WHEEL as usize,
                                        true,
                                        "mod wheel",
                                        false,
                                    );
                                }
                            }
                            0x10 => {
                                // Pitch wheel
                                if (a3 & 0x0f) == 0x00 {
                                    // Pitch control special case: a pitch wheel here is a
                                    // strange way of setting the pitch wheel range.
                                    // Testcase: th07_03.mid.
                                    let centered_value = data as i32 - 64;
                                    self.midi_channels[channel].set_midi_parameter(
                                        ChannelMidiParameterValue::PitchWheelRange(
                                            centered_value as f64,
                                        ),
                                    );
                                } else {
                                    self.midi_channels[channel].sys_ex_modulators.setup_receiver(
                                        a3,
                                        data,
                                        modulator_sources::PITCH_WHEEL as usize,
                                        false,
                                        "pitch wheel",
                                        true,
                                    );
                                }
                            }
                            0x20 => {
                                // Channel pressure
                                self.midi_channels[channel].sys_ex_modulators.setup_receiver(
                                    a3,
                                    data,
                                    modulator_sources::CHANNEL_PRESSURE as usize,
                                    false,
                                    "channel pressure",
                                    false,
                                );
                            }
                            0x30 => {
                                // Poly pressure
                                self.midi_channels[channel].sys_ex_modulators.setup_receiver(
                                    a3,
                                    data,
                                    modulator_sources::POLY_PRESSURE as usize,
                                    false,
                                    "poly pressure",
                                    false,
                                );
                            }
                            0x40 => {
                                // CC1
                                let cc1 = self.midi_channels[channel].midi_parameters.cc1 as usize;
                                self.midi_channels[channel].sys_ex_modulators.setup_receiver(
                                    a3, data, cc1, true, "CC1", false,
                                );
                            }
                            0x50 => {
                                // CC2
                                let cc2 = self.midi_channels[channel].midi_parameters.cc2 as usize;
                                self.midi_channels[channel].sys_ex_modulators.setup_receiver(
                                    a3, data, cc2, true, "CC2", false,
                                );
                            }
                            _ => {
                                // This is some other GS sysex...
                                sys_ex_not_recognized(syx, "Roland GS");
                            }
                        }
                    } else if syx[5] == 0x00 {
                        // This is a global system parameter
                        match syx[6] {
                            0x00 => {
                                // Roland GS master tune.
                                // A 16-bit value assembled from four nibbles
                                // (syx[7..=10]); cents = (tune - 1024) / 10.
                                let tune = ((message_value as i32) << 12)
                                    | ((syx[8] as i32) << 8)
                                    | ((syx[9] as i32) << 4)
                                    | syx[10] as i32;
                                let cents = (tune as f64 - 1024.0) / 10.0;
                                spessa_synth_info(&format!(
                                    "Roland GS Master Tune: {} cents with: {:02X?}",
                                    cents, syx
                                ));
                                self.set_midi_parameter(
                                    GlobalMIDIParameterChangeCallback::FineTune(cents),
                                );
                            }

                            0x7f => {
                                // Roland mode set / GS mode set.
                                // data === 0x01 (Double Module) is only meaningful on the true
                                // top-level reset address (a1 === 0x00); it is not valid for the
                                // a1 === 0x40 patch-parameter "mode set" alias.
                                if message_value == 0x00 || (message_value == 0x01 && syx[4] == 0x00) {
                                    if message_value == 0x01 {
                                        // Double Module mode: ensure at least 32 channels.
                                        spessa_synth_info("GS Mode: Double Module");
                                        while self.midi_channels.len() < 32 {
                                            self.create_midi_channel(true);
                                        }
                                    }
                                    // This is a GS reset
                                    spessa_synth_info("GS Reset received!");
                                    self.reset(MIDISystem::Gs);
                                } else if message_value == 0x7f {
                                    // GS mode off
                                    spessa_synth_info("GS system off, switching to GM");
                                    self.reset(MIDISystem::Gm);
                                }
                            }

                            0x06 => {
                                // Roland master pan.
                                // Ranges from 1 to 127, NOT 0 to 127, hence the /63 divisor.
                                let pan = (message_value as f64 - 64.0) / 63.0;
                                spessa_synth_info(&format!(
                                    "Roland GS Master Pan set to: {} with: {:02X?}",
                                    pan, syx
                                ));
                                self.set_midi_parameter(
                                    GlobalMIDIParameterChangeCallback::Pan(pan),
                                );
                            }

                            0x04 => {
                                // Roland GS master volume.
                                spessa_synth_info(&format!(
                                    "Roland GS Master Volume: {} with: {:02X?}",
                                    message_value, syx
                                ));
                                self.set_midi_parameter(
                                    GlobalMIDIParameterChangeCallback::Volume(
                                        message_value as f64 / 127.0,
                                    ),
                                );
                            }

                            0x05 => {
                                // Roland master key shift (transpose).
                                // TS 4.3.0: setMIDIParameter("keyShift", transpose) — integer
                                // semitone shift (drum channels ignore it), not a cents tuning.
                                let transpose = message_value as i32 - 64;
                                spessa_synth_info(&format!(
                                    "Roland GS Master Key-Shift: {} keys with: {:02X?}",
                                    transpose, syx
                                ));
                                self.set_midi_parameter(GlobalMIDIParameterChangeCallback::KeyShift(
                                    transpose as f64,
                                ));
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
                                if let Some(proc) = crate::synthesizer::audio_engine::effects::insertion::create_insertion_processor(efx_type, self.sample_rate, self.max_buffer_size) {
                                    spessa_synth_info(&format!("GS EFX Type: {:04X}", efx_type));
                                    self.insertion_processor = proc;
                                } else {
                                    spessa_synth_info(&format!("Unsupported EFX processor: {:04X}, using Thru", efx_type));
                                    self.insertion_processor = Box::new(crate::synthesizer::audio_engine::effects::insertion::thru::ThruFx::new(self.sample_rate));
                                }
                                self.reset_insertion_params();
                                self.insertion_processor.reset();
                            }

                            0x17 => {
                                // EFX send level to reverb.
                                // TS 4.3.0 scales the raw send by EFX_SENDS_GAIN_CORRECTION.
                                self.insertion_processor.set_send_level_to_reverb(
                                    (data as f64 / 127.0) * EFX_SENDS_GAIN_CORRECTION,
                                );
                                spessa_synth_info(&format!("GS EFX Send Level to Reverb: {}", data));
                            }

                            0x18 => {
                                // EFX send level to chorus.
                                self.insertion_processor.set_send_level_to_chorus(
                                    (data as f64 / 127.0) * EFX_SENDS_GAIN_CORRECTION,
                                );
                                spessa_synth_info(&format!("GS EFX Send Level to Chorus: {}", data));
                            }

                            0x19 => {
                                // EFX send level to delay.
                                self.insertion_processor.set_send_level_to_delay(
                                    (data as f64 / 127.0) * EFX_SENDS_GAIN_CORRECTION,
                                );
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
                                    self.midi_parameters.system,
                                    self.system_parameters.events_enabled,
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
                            crate::synthesizer::types::SynthProcessorEvent::DisplayMessage(
                                syx.to_vec(),
                            ),
                        );
                    } else if syx[5] == 0x01 {
                        // Matrix display
                        self.call_event(
                            crate::synthesizer::types::SynthProcessorEvent::DisplayMessage(
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
