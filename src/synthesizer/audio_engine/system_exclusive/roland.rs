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
use crate::synthesizer::enums::custom_controllers;
use crate::synthesizer::audio_engine::system_exclusive::system_exclusive::{
    sys_ex_logging, sys_ex_not_recognized,
};
use crate::synthesizer::audio_engine::synth_constants::EFX_SENDS_GAIN_CORRECTION;
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::types::GlobalMIDIParameterChangeCallback;
use crate::soundbank::types::MIDISystem;
use crate::utils::loggin::spessa_synth_info;
use crate::utils::byte_functions::string::read_binary_string;
use crate::midi::midi_tools::sysex_data::{DEFAULT_GS_DRUM_MAP, MELODIC_MAP};
use crate::midi::drum_parameters::UserDrumSetParameterChange;

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
                        // For example, 0x1A means A = 11, which corresponds to channel 12 (counting from 1)
                        if channel >= self.midi_channels.len() {
                            // Testcase: Sarah Geronimo - Maybe This Time
                            sys_ex_not_recognized(syx, "Roland GS Patch Parameter");
                            return;
                        }

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

                            0x00 => {
                                // Tone number (program change)
                                self.controller_change_channel(
                                    channel,
                                    midi_controllers::BANK_SELECT,
                                    message_value,
                                );
                                self.program_change_channel(channel, syx[8]);
                            }

                            0x02 => {
                                // Rx. channel (0x10 is OFF)
                                let rx_channel = if message_value == 0x10 {
                                    -1
                                } else {
                                    message_value as i32 + channel_offset as i32
                                };
                                self.midi_channels[channel]
                                    .set_midi_parameter(ChannelMidiParameterValue::RxChannel(rx_channel));
                                self.custom_channel_numbers |=
                                    rx_channel != self.midi_channels[channel].channel as i32;
                                spessa_synth_info(&format!(
                                    "GS Rx. Channel on {}: {}",
                                    channel, rx_channel
                                ));
                            }

                            0x13 => {
                                // Mono/poly
                                self.midi_channels[channel].set_midi_parameter(
                                    ChannelMidiParameterValue::PolyMode(message_value == 1),
                                );
                                spessa_synth_info(&format!(
                                    "GS Mono/poly on {}: {}",
                                    channel,
                                    if message_value == 1 { "POLY" } else { "MONO" }
                                ));
                            }

                            0x15 => {
                                // This is the Use for Rhythm Part sysex (multiple drums)
                                let prev_map = self.midi_channels[channel].midi_parameters.drum_map;
                                self.midi_channels[channel]
                                    .set_midi_parameter(ChannelMidiParameterValue::DrumMap(message_value));
                                let new_map = self.midi_channels[channel].midi_parameters.drum_map;
                                // Non-melodic means a drum channel
                                let is_drums = message_value > MELODIC_MAP;
                                // Testcase: gs_drum_change_test
                                // GS resets to the default kit not only when toggling drums,
                                // But on any map change too.
                                let ch = &mut self.midi_channels[channel];
                                if !ch.lock_preset
                                    && (is_drums != ch.patch.is_gm_gs_drum || new_map != prev_map)
                                {
                                    ch.patch.bank_msb = 0;
                                    ch.patch.bank_lsb = 0;
                                    ch.patch.is_gm_gs_drum = is_drums;
                                    self.program_change_channel(channel, 0);
                                }
                                spessa_synth_info(&format!(
                                    "GS Drums on {}: {}",
                                    channel, is_drums
                                ));
                                return;
                            }

                            0x16 => {
                                // Pitch key shift sysex
                                let key_shift = message_value as i32 - 64;
                                self.midi_channels[channel].set_midi_parameter(
                                    ChannelMidiParameterValue::KeyShift(key_shift as f64),
                                );
                                sys_ex_logging(syx, channel as u8, &key_shift, "key shift", "keys");
                            }

                            0x19 => {
                                // Part level (cc#7)
                                self.controller_change_channel(
                                    channel,
                                    midi_controllers::MAIN_VOLUME,
                                    message_value,
                                );
                                return;
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

                            0x1f => {
                                // CC1 controller number
                                self.midi_channels[channel]
                                    .set_midi_parameter(ChannelMidiParameterValue::Cc1(message_value));
                                spessa_synth_info(&format!(
                                    "GS CC1 Controller Number for {}: {}",
                                    channel, message_value
                                ));
                            }

                            0x20 => {
                                // CC2 controller number
                                self.midi_channels[channel]
                                    .set_midi_parameter(ChannelMidiParameterValue::Cc2(message_value));
                                spessa_synth_info(&format!(
                                    "GS CC2 Controller Number for {}: {}",
                                    channel, message_value
                                ));
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
                                self.midi_channels[channel]
                                    .set_midi_parameter(ChannelMidiParameterValue::FineTune(cents));
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &(cents.round() as i32),
                                    "fine tuning",
                                    "cents",
                                );
                            }

                            0x2c => {
                                // Delay send
                                self.controller_change_channel(
                                    channel,
                                    midi_controllers::VARIATION_DEPTH,
                                    message_value,
                                );
                            }

                            0x30 => {
                                // Vibrato rate
                                self.controller_change_channel(
                                    channel,
                                    midi_controllers::VIBRATO_RATE,
                                    message_value,
                                );
                            }

                            0x31 => {
                                // Vibrato depth
                                self.controller_change_channel(
                                    channel,
                                    midi_controllers::VIBRATO_DEPTH,
                                    message_value,
                                );
                            }

                            0x32 => {
                                // Filter cutoff
                                // It's so out of order, Roland...
                                self.controller_change_channel(
                                    channel,
                                    midi_controllers::BRIGHTNESS,
                                    message_value,
                                );
                            }

                            0x33 => {
                                // Filter resonance
                                self.controller_change_channel(
                                    channel,
                                    midi_controllers::FILTER_RESONANCE,
                                    message_value,
                                );
                            }

                            0x34 => {
                                // Attack time
                                self.controller_change_channel(
                                    channel,
                                    midi_controllers::ATTACK_TIME,
                                    message_value,
                                );
                            }

                            0x35 => {
                                // Decay time
                                self.controller_change_channel(
                                    channel,
                                    midi_controllers::DECAY_TIME,
                                    message_value,
                                );
                            }

                            0x36 => {
                                // Release time
                                self.controller_change_channel(
                                    channel,
                                    midi_controllers::RELEASE_TIME,
                                    message_value,
                                );
                            }

                            0x37 => {
                                // Vibrato delay
                                // It seems that they forgot about it and put it last...
                                self.controller_change_channel(
                                    channel,
                                    midi_controllers::VIBRATO_DELAY,
                                    message_value,
                                );
                            }

                            0x40 => {
                                // Scale tuning: up to 12 bytes
                                let tuning_bytes = syx.len().saturating_sub(9); // Data starts at 7, minus checksum and f7
                                let mut new_tuning = [0i8; 12];
                                for i in 0..tuning_bytes.min(12) {
                                    new_tuning[i] = (syx[i + 7] as i16 - 64) as i8;
                                }
                                // TS 4.3.14 (roland.ts, case 0x40) calls ONLY
                                // `ch.setOctaveTuning(newTuning)` here — no additional
                                // channel-wide fine tune. A prior Rust revision also called
                                // `set_tuning((messageValue - 64), false)`, double-applying a
                                // channel-wide tuning offset on top of the octave tuning table;
                                // that call does not exist upstream and has been removed.
                                self.midi_channels[channel].set_octave_tuning(&new_tuning);
                                sys_ex_logging(
                                    syx,
                                    channel as u8,
                                    &format!("{:?}", new_tuning),
                                    "octave scale tuning",
                                    "cents",
                                );
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
                                    self.midi_channels[channel].sys_ex_modulators.setup_receiver_gs(
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
                                    self.midi_channels[channel].sys_ex_modulators.setup_receiver_gs(
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
                                self.midi_channels[channel].sys_ex_modulators.setup_receiver_gs(
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
                                self.midi_channels[channel].sys_ex_modulators.setup_receiver_gs(
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
                                self.midi_channels[channel].sys_ex_modulators.setup_receiver_gs(
                                    a3, data, cc1, true, "CC1", false,
                                );
                            }
                            0x50 => {
                                // CC2
                                let cc2 = self.midi_channels[channel].midi_parameters.cc2 as usize;
                                self.midi_channels[channel].sys_ex_modulators.setup_receiver_gs(
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
                                if self.has_gs_insertion_processor(efx_type) {
                                    spessa_synth_info(&format!("GS EFX Type: {:04X}", efx_type));
                                    self.select_gs_insertion_processor(Some(efx_type));
                                } else {
                                    self.select_gs_insertion_processor(None);
                                    spessa_synth_info(&format!("Unsupported EFX processor: {:04X}, using Thru", efx_type));
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
                                self.update_active_gs_effects();
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
                                // Chorus send level to delay — may activate delay
                                spessa_synth_info(&format!("GS Chorus Send To Delay: {}", message_value));
                                self.chorus_processor.set_send_level_to_delay(message_value);
                                self.update_active_gs_effects();
                            }

                            // --- Delay parameters (0x50-0x5A) ---
                            0x50 => {
                                // Delay macro
                                spessa_synth_info(&format!("GS Delay Macro: {}", message_value));
                                self.set_delay_macro(message_value);
                            }
                            0x51 => {
                                // Delay pre-LPF
                                spessa_synth_info(&format!("GS Delay Pre-LPF: {}", message_value));
                                self.delay_processor.set_pre_lowpass(message_value);
                            }
                            0x52 => {
                                // Delay time center
                                spessa_synth_info(&format!("GS Delay Time Center: {}", message_value));
                                self.delay_processor.set_time_center(message_value);
                            }
                            0x53 => {
                                // Delay time ratio left
                                spessa_synth_info(&format!("GS Delay Time Ratio Left: {}", message_value));
                                self.delay_processor.set_time_ratio_left(message_value);
                            }
                            0x54 => {
                                // Delay time ratio right
                                spessa_synth_info(&format!("GS Delay Time Ratio Right: {}", message_value));
                                self.delay_processor.set_time_ratio_right(message_value);
                            }
                            0x55 => {
                                // Delay level center
                                spessa_synth_info(&format!("GS Delay Level Center: {}", message_value));
                                self.delay_processor.set_level_center(message_value);
                            }
                            0x56 => {
                                // Delay level left
                                spessa_synth_info(&format!("GS Delay Level Left: {}", message_value));
                                self.delay_processor.set_level_left(message_value);
                            }
                            0x57 => {
                                // Delay level right
                                spessa_synth_info(&format!("GS Delay Level Right: {}", message_value));
                                self.delay_processor.set_level_right(message_value);
                            }
                            0x58 => {
                                // Delay level
                                spessa_synth_info(&format!("GS Delay Level: {}", message_value));
                                self.delay_processor.set_level(message_value);
                            }
                            0x59 => {
                                // Delay feedback
                                spessa_synth_info(&format!("GS Delay Feedback: {}", message_value));
                                self.delay_processor.set_feedback(message_value);
                            }
                            0x5a => {
                                // Delay send level to reverb
                                spessa_synth_info(&format!("GS Delay Send To Reverb: {}", message_value));
                                self.delay_processor.set_send_level_to_reverb(message_value);
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
                                self.midi_channels[channel].set_midi_parameter(
                                    ChannelMidiParameterValue::EfxAssign(efx),
                                );
                                spessa_synth_info(&format!(
                                    "Insertion for {}: {}",
                                    channel,
                                    if efx { "ON" } else { "OFF" }
                                ));
                                self.update_active_gs_effects();
                            }
                            _ => {
                                sys_ex_not_recognized(syx, "Roland GS Patch Part Parameter");
                            }
                        }
                    }
                } else if syx[4] == 0x41 || syx[4] == 0x51 {
                    // Drum Setup
                    // 51 means BLOCK B (+16 channels)
                    // Testcase: 95043-2.KYC.mid
                    self.handle_gs_drum_setup(syx);
                } else if syx[4] == 0x21 {
                    // User Drum Set
                    self.handle_user_drum(syx[5], syx[6], syx[7].min(127), syx);
                } else if syx[4] == 0x29 {
                    // User Drum Set Bulk Dump
                    self.handle_user_drum_bulk_dump(syx);
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
                    self.set_midi_parameter(GlobalMIDIParameterChangeCallback::Volume(
                        syx[7] as f64 / 100.0,
                    ));
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

impl SynthesizerCore {
    /// GS User Drum Set parameter (a1 = 0x21).
    /// Equivalent to: handleUserDrum(a2, a3, data, syx)
    fn handle_user_drum(&mut self, a2: u8, a3: u8, data: u8, syx: &[u8]) {
        use UserDrumSetParameterChange as C;
        if self.system_parameters.user_drum_lock {
            return;
        }
        let drum_set = (a2 >> 4) as usize;
        let midi_note = a3 as usize;
        let command = a2 & 0xf;
        let change = match command {
            // User drum set name
            0x0 => {
                let new_name = read_binary_string(syx, 12, 7);
                spessa_synth_info(&format!(
                    "GS User Drum Set {} Name: {}",
                    drum_set,
                    new_name.trim()
                ));
                return;
            }
            // Here it's relative to 60, not 64 like NRPN. For some reason...
            // Use the full 100 cents here as we choose the correct pitch (50 or 100 cents)
            // when committing changes
            0x1 => C::PitchCoarse(data as f64 - 60.0),
            // Drum Level
            0x2 => C::Level(data),
            // Drum Assign Group (exclusive class)
            0x3 => C::AssignGroup(data),
            // Pan
            0x4 => C::Pan(data),
            // Reverb
            0x5 => C::ReverbSend(data),
            // Chorus
            0x6 => C::ChorusSend(data),
            // Receive Note Off
            0x7 => C::RxNoteOff(data == 1),
            // Receive Note On
            0x8 => C::RxNoteOn(data == 1),
            // Delay
            0x9 => C::VariationSend(data),
            // Source drum set
            0xa => C::SourceDrumSet(data),
            // Program number
            0xb => C::Program(data),
            // Source note number
            0xc => C::SourceNoteNumber(data),
            _ => {
                sys_ex_not_recognized(syx, "GS User Drum set");
                return;
            }
        };
        self.set_user_drum_set_param(drum_set, midi_note, change);
    }

    /// GS User Drum Set bulk dump (a1 = 0x29).
    /// Top half of a2 stays the same (indicates which user drum)
    /// While the bottom param is something else
    /// Guessed by analyzing 95043-2.KYC.mid
    /// Equivalent to: the `a1 === 0x29` branch of rolandSystemExclusive
    fn handle_user_drum_bulk_dump(&mut self, syx: &[u8]) {
        let a2 = syx[5];
        let data_length = syx.len().saturating_sub(9);
        spessa_synth_info(&format!(
            "User Drum Set Bulk Dump detected! Keys: {}",
            data_length
        ));

        let actual_drum_param = match a2 & 0x0f {
            // Most at 60 = play note?
            0x0 => 1,
            // Level?
            0x1 => 2,
            // Matches gm.dls exclusive class pretty well, assign group?
            0x2 => 3,
            // Most at 64, so pan?
            0x3 => 4,
            // 0 on bass, so reverb?
            0x4 => 5,
            // 0 on all, so chorus?
            0x5 => 6,
            0x6 => {
                // 16 on all
                // In the order, rx notes should be here, it's 0x10, so maybe
                // It's both? on << 4 | off?
                // Special handling is needed
                let address2_off = (a2 & 0xf0) | 7;
                let address2_on = (a2 & 0xf0) | 8;
                for midi_note in 0..data_length {
                    let v = syx[midi_note + 7];
                    self.handle_user_drum(address2_off, midi_note as u8, v & 0xf, syx);
                    self.handle_user_drum(address2_on, midi_note as u8, v >> 4, syx);
                }
                return;
            }
            // All 0, so delay
            0x7 => 9,
            // All 2 and this is a 88pro midi, so the map is 2
            0x8 => 0xa,
            // 0xA matches note number so the only one left is the program
            0x9 => 0xb,
            // Drum numbers increase so source note
            0xa => 0xc,
            // 16 chars, seems to be a name (spec is wrong? says 12)
            0xb => 0,
            _ => {
                sys_ex_not_recognized(syx, "User Drum Bulk Dump System Exclusive");
                return;
            }
        };

        let address2 = (a2 & 0xf0) | actual_drum_param;
        for midi_note in 0..data_length {
            self.handle_user_drum(address2, midi_note as u8, syx[midi_note + 7], syx);
        }
    }

    /// GS Drum Setup (a1 = 0x41 / 0x51).
    /// Applies to every channel whose drum map matches.
    /// Equivalent to: the `a1 === 0x41 || a1 === 0x51` branch of rolandSystemExclusive
    fn handle_gs_drum_setup(&mut self, syx: &[u8]) {
        if self.system_parameters.drum_lock {
            return;
        }
        let a2 = syx[5];
        let a3 = syx[6];
        // Sanity check
        let data = syx[7].min(127);
        // In gs, the map is offset by the default (e.g. 1)
        // So 0 means drum map 1, 1 means drum map 1, etc.
        let map = (a2 >> 4) + DEFAULT_GS_DRUM_MAP;
        let drum_key = a3 as usize;
        let param = a2 & 0xf;
        if param == 0x0 {
            // Drum map name
            let patch_name = read_binary_string(syx, 12, 7);
            spessa_synth_info(&format!("GS Drum Map Name for MAP{}: {}", map, patch_name));
            return;
        }
        if !(0x1..=0x9).contains(&param) {
            sys_ex_not_recognized(&[param], "GS Drum Setup");
            return;
        }
        for ch in self.midi_channels.iter_mut() {
            if ch.midi_parameters.drum_map != map {
                continue;
            }
            let p = &mut ch.drum_params[drum_key];
            match param {
                0x1 => {
                    // Here it's relative to 60, not 64 like NRPN. For some reason...
                    let pitch = data as f64 - 60.0;
                    // Apply same thing: SC-55 uses 100 cents, SC-88 and above is 50
                    p.pitch_coarse = pitch * if ch.patch.bank_lsb == 1 { 1.0 } else { 0.5 };
                }
                // Drum Level
                0x2 => p.level = data,
                // Drum Assign Group (exclusive class)
                0x3 => p.assign_group = data,
                // Pan
                0x4 => p.pan = data,
                // Reverb
                0x5 => p.reverb_send = data,
                // Chorus
                0x6 => p.chorus_send = data,
                // Receive Note Off
                0x7 => p.rx_note_off = data == 1,
                // Receive Note On
                0x8 => p.rx_note_on = data == 1,
                // Delay
                0x9 => p.variation_send = data,
                _ => unreachable!(),
            }
        }
        spessa_synth_info(&format!(
            "GS Drum Setup param {} for MAP{}, key {}: {}",
            param, map, drum_key, data
        ));
    }
}
