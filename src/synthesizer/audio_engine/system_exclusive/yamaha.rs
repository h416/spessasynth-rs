//! yamaha.rs
//! purpose: Handles Yamaha XG system exclusive messages.
//! Ported from: src/synthesizer/audio_engine/system_exclusive/yamaha.ts
//! Reference: <http://www.studio4all.de/htmle/main91.html>

use crate::midi::enums::midi_controllers;
use crate::midi::midi_tools::sysex_data::{DEFAULT_XG_DRUM_MAP, MELODIC_MAP};
use crate::soundbank::enums::modulator_sources;
use crate::synthesizer::audio_engine::channel::midi_channel::DrumLocks;
use crate::synthesizer::audio_engine::channel::parameters::midi::ChannelMidiParameterValue;
use crate::synthesizer::audio_engine::system_exclusive::system_exclusive::sys_ex_not_recognized;
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::types::GlobalMIDIParameterChangeCallback;
use crate::soundbank::types::MIDISystem;
use crate::utils::loggin::spessa_synth_info;

impl SynthesizerCore {
    /// Handles a Yamaha XG system exclusive message.
    /// Equivalent to: yamahaSystemExclusive(syx, channelOffset)
    pub fn handle_xg(&mut self, syx: &[u8], channel_offset: usize) {
        // XG sysex
        if syx[2] != 0x4c {
            sys_ex_not_recognized(syx, "Yamaha");
            return;
        }

        let a1 = syx[3]; // Address 1
        let a2 = syx[4]; // Address 2
        let a3 = syx[5]; // Address 3
        let data = syx[6];

        // XG system parameter
        if a1 == 0x00 && a2 == 0x00 {
            match a3 {
                // Master tune
                0x00 => {
                    let tune = ((syx[6] as u32 & 15) << 12)
                        | ((syx[7] as u32 & 15) << 8)
                        | ((syx[8] as u32 & 15) << 4)
                        | (syx[9] as u32 & 15);
                    let cents = (tune as f64 - 1024.0) / 10.0;
                    self.set_midi_parameter(GlobalMIDIParameterChangeCallback::FineTune(cents));
                    spessa_synth_info(&format!("XG Master Tune. Cents: {}", cents));
                }

                // Master volume
                0x04 => {
                    self.set_midi_parameter(GlobalMIDIParameterChangeCallback::Volume(
                        data as f64 / 127.0,
                    ));
                    spessa_synth_info(&format!("XG Master Volume: {}", data));
                }

                // Master attenuation
                0x05 => {
                    let vol = 127i32 - data as i32;
                    self.set_midi_parameter(GlobalMIDIParameterChangeCallback::Volume(
                        vol as f64 / 127.0,
                    ));
                    spessa_synth_info(&format!("XG Master Attenuation: {}", data));
                }

                // Master transpose
                0x06 => {
                    let transpose = data as f64 - 64.0;
                    self.set_midi_parameter(GlobalMIDIParameterChangeCallback::KeyShift(transpose));
                    spessa_synth_info(&format!("XG Master Transpose: {}", data));
                }

                // XG Reset
                // XG on
                0x7f | 0x7e => {
                    spessa_synth_info("MIDI System: Yamaha XG");
                    self.reset(MIDISystem::Xg);
                }

                _ => {}
            }
            return;
        }

        // XG EFFECT 1 (reverb, chorus, variation)
        if a1 == 0x02 && a2 == 0x01 {
            let is_reverb = a3 == 0x00 || (0x02..=0x0d).contains(&a3) || (0x10..=0x15).contains(&a3);
            if is_reverb && self.system_parameters.xg_reverb_lock {
                return;
            }
            let is_chorus = a3 == 0x20 || (0x22..=0x2e).contains(&a3) || (0x30..=0x35).contains(&a3);
            if is_chorus && self.system_parameters.xg_chorus_lock {
                return;
            }
            let is_variation =
                a3 == 0x40 || (0x42..=0x5b).contains(&a3) || (0x70..=0x75).contains(&a3);
            if is_variation && self.system_parameters.xg_variation_lock {
                return;
            }
            let data = data as i32;
            match a3 {
                0x00 => {
                    let effect_type = ((data << 8) | syx[7] as i32) as u16;
                    self.xg_reverb_block.system.effect.set_type(effect_type);
                    spessa_synth_info(&format!("XG Reverb Type: {:x}", effect_type));
                }

                0x02..=0x0b => {
                    self.xg_reverb_block.system.effect.set_parameter(a3 as i32 - 0x02, data);
                    spessa_synth_info(&format!("XG Reverb Parameter {}: {}", a3 - 0x01, data));
                }

                0x0c => {
                    self.xg_reverb_block.system.r#return = data;
                    spessa_synth_info(&format!("XG Reverb Return: {}", data));
                }

                0x0d => {
                    self.xg_reverb_block.system.pan = data;
                    spessa_synth_info(&format!("XG Reverb Pan: {}", data));
                }

                0x10..=0x15 => {
                    self.xg_reverb_block.system.effect.set_parameter(a3 as i32 - 0x06, data);
                    spessa_synth_info(&format!("XG Reverb Parameter {}: {}", a3 - 0x05, data));
                }

                0x20 => {
                    let effect_type = ((data << 8) | syx[7] as i32) as u16;
                    self.xg_chorus_block.system.effect.set_type(effect_type);
                    spessa_synth_info(&format!("XG Chorus Type: {:x}", effect_type));
                }

                0x22..=0x2b => {
                    self.xg_chorus_block.system.effect.set_parameter(a3 as i32 - 0x22, data);
                    spessa_synth_info(&format!("XG Chorus Parameter {}: {}", a3 - 0x21, data));
                }

                0x2c => {
                    self.xg_chorus_block.system.r#return = data;
                    spessa_synth_info(&format!("XG Chorus Return: {}", data));
                }

                0x2d => {
                    self.xg_chorus_block.system.pan = data;
                    spessa_synth_info(&format!("XG Chorus Pan: {}", data));
                }

                0x2e => {
                    self.xg_chorus_block.send_to_reverb = data;
                    spessa_synth_info(&format!("XG Chorus Send To Reverb: {}", data));
                }

                0x30..=0x35 => {
                    self.xg_chorus_block.system.effect.set_parameter(a3 as i32 - 0x26, data);
                    spessa_synth_info(&format!("XG Chorus Parameter {}: {}", a3 - 0x25, data));
                }

                0x40 => {
                    let effect_type = ((data << 8) | syx[7] as i32) as u16;
                    self.xg_variation_block.system.effect.set_type(effect_type);
                    spessa_synth_info(&format!("XG Variation Type: {:x}", effect_type));
                }

                0x42 | 0x44 | 0x46 | 0x48 | 0x4a | 0x4c | 0x4e | 0x50 | 0x52 | 0x54 => {
                    let value = (data << 7) | syx[7] as i32;
                    // Bit shift by 1 because address increases by two
                    self.xg_variation_block
                        .system
                        .effect
                        .set_parameter((a3 as i32 - 0x42) >> 1, value);
                    spessa_synth_info(&format!(
                        "XG Variation Parameter {} (14-bit): {}",
                        a3 - 0x41,
                        value
                    ));
                }

                0x56 => {
                    self.xg_variation_block.system.r#return = data;
                    spessa_synth_info(&format!("XG Variation Return: {}", data));
                }

                0x57 => {
                    self.xg_variation_block.system.pan = data;
                    spessa_synth_info(&format!("XG Variation Pan: {}", data));
                }

                0x58 => {
                    self.xg_variation_block.send_to_reverb = data;
                    spessa_synth_info(&format!("XG Variation Send To Reverb: {}", data));
                }

                0x59 => {
                    self.xg_variation_block.send_to_chorus = data;
                    spessa_synth_info(&format!("XG Variation Send To Chorus: {}", data));
                }

                0x5a => {
                    self.xg_variation_block.insertion_mode = data == 0;
                    spessa_synth_info(&format!(
                        "XG Variation Connection: {}",
                        if data == 0 { "INSERTION" } else { "SYSTEM" }
                    ));
                }

                0x5b => {
                    self.xg_variation_block.part_number = data;
                    spessa_synth_info(&format!("XG Variation Part Number: {}", data));
                }

                0x70..=0x75 => {
                    // These are 7-bit only
                    self.xg_variation_block.system.effect.set_parameter(a3 as i32 - 0x66, data);
                    spessa_synth_info(&format!("XG Variation Parameter {}: {}", a3 - 0x65, data));
                }

                _ => {
                    sys_ex_not_recognized(&[a3], "Yamaha XG EFFECT 1 Parameter");
                }
            }
            return;
        }

        // XG EFFECT 2 (insertion)
        if a1 == 0x03 {
            if self.system_parameters.xg_insertion_lock {
                return;
            }
            let Some(insertion) = self.xg_insertion_blocks.get_mut(a2 as usize) else {
                sys_ex_not_recognized(&[a2], "Yamaha XG Insertion Effect Number");
                return;
            };
            let data = data as i32;

            match a3 {
                0x00 => {
                    let effect_type = ((data << 8) | syx[7] as i32) as u16;
                    insertion.effect.set_type(effect_type);
                    spessa_synth_info(&format!("XG Insertion {} Type: {:x}", a2, effect_type));
                }

                0x02..=0x0b => {
                    insertion.effect.set_parameter(a3 as i32 - 0x02, data);
                    spessa_synth_info(&format!(
                        "XG Insertion {} Parameter {}: {}",
                        a2,
                        a3 - 0x01,
                        data
                    ));
                }

                0x0c => {
                    insertion.part_number = data;
                    spessa_synth_info(&format!("XG Insertion {} Part Number: {}", a2, data));
                }

                0x20..=0x25 => {
                    insertion.effect.set_parameter(a3 as i32 - 0x16, data);
                    spessa_synth_info(&format!(
                        "XG Insertion {} Parameter {}: {}",
                        a2,
                        a3 - 0x15,
                        data
                    ));
                }

                0x30 | 0x32 | 0x34 | 0x36 | 0x38 | 0x3a | 0x3c | 0x3e | 0x40 | 0x42 => {
                    let value = (data << 7) | syx[7] as i32;
                    // Bit shift by 1 because address increases by two
                    insertion.effect.set_parameter((a3 as i32 - 0x30) >> 1, value);
                    spessa_synth_info(&format!(
                        "XG Insertion {} Parameter {} (14-bit): {}",
                        a2,
                        a3 - 0x2f,
                        value
                    ));
                }

                _ => {
                    sys_ex_not_recognized(&[a3], "Yamaha XG EFFECT 2 Parameter");
                }
            }
            return;
        }

        if a1 == 0x08 {
            // A2 is the channel number
            let channel = a2 as usize + channel_offset;
            if channel >= self.midi_channels.len() {
                // Invalid channel
                sys_ex_not_recognized(syx, "Yamaha XG Part Setup");
                return;
            }

            let current_time = self.current_time;
            let current_system = self.midi_parameters.system;
            let enable_event_system = self.system_parameters.events_enabled;

            match a3 {
                // Bank-select MSB
                0x01 => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::BANK_SELECT,
                        data,
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
                        data,
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
                        data,
                        &self.sound_bank_manager,
                        current_system,
                        DrumLocks::of(&self.system_parameters),
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Rev. channel
                0x04 => {
                    // The rxChannel selects which channel this part receives on.
                    let rx_channel =
                        if data == 0x7f { -1 } else { data as i32 + channel_offset as i32 };
                    self.midi_channels[channel]
                        .set_midi_parameter(ChannelMidiParameterValue::RxChannel(rx_channel));
                    self.custom_channel_numbers |=
                        rx_channel != self.midi_channels[channel].channel as i32;
                    spessa_synth_info(&format!("XG Rev. Channel on {}: {}", channel, rx_channel));
                }

                // Poly/mono
                0x05 => {
                    let poly = data == 1;
                    self.midi_channels[channel]
                        .set_midi_parameter(ChannelMidiParameterValue::PolyMode(poly));
                    spessa_synth_info(&format!(
                        "XG Mono/poly on {}: {}",
                        channel,
                        if poly { "POLY" } else { "MONO" }
                    ));
                }

                // Same note number key on assign
                0x06 => {
                    self.midi_channels[channel]
                        .set_midi_parameter(ChannelMidiParameterValue::AssignMode(data));
                    spessa_synth_info(&format!(
                        "XG Same Note Number Key On Assign on {}: {}",
                        channel, data
                    ));
                }

                // Part mode
                0x07 => {
                    let drums = data != MELODIC_MAP;
                    // Testcase: xg part_mode_drum
                    // Verified with s-yxg50
                    // SetDrums switches the bank and keeps the program,
                    // But switching *to* drums re-initializes the kit
                    // To program 0 instead!
                    // But not if it's already drum
                    // Testcase: 15. U.N. Owen was her (yoimutu).mid
                    let drums_before = self.midi_channels[channel].drum_channel;
                    let evs = self.midi_channels[channel].set_drums(
                        drums,
                        &self.sound_bank_manager,
                        current_system,
                        DrumLocks::of(&self.system_parameters),
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                    if drums && !drums_before {
                        let evs = self.midi_channels[channel].program_change(
                            0,
                            &self.sound_bank_manager,
                            current_system,
                            DrumLocks::of(&self.system_parameters),
                            enable_event_system,
                        );
                        for ev in evs {
                            self.call_event(ev);
                        }
                    }
                    self.midi_channels[channel]
                        .set_midi_parameter(ChannelMidiParameterValue::DrumMap(data));
                    spessa_synth_info(&format!(
                        "XG Part Mode on {}: {}",
                        channel,
                        if drums { "DRUM" } else { "MELODIC" }
                    ));
                }

                // Note shift
                0x08 => {
                    let key_shift = data as f64 - 64.0;
                    self.midi_channels[channel]
                        .set_midi_parameter(ChannelMidiParameterValue::KeyShift(key_shift));
                }

                // Volume
                0x0b => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::MAIN_VOLUME,
                        data,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Velocity Sense Depth
                0x0c => {
                    self.midi_channels[channel].set_midi_parameter(
                        ChannelMidiParameterValue::VelocitySenseDepth(data),
                    );
                    spessa_synth_info(&format!(
                        "XG Velocity Sense Depth on {}: {}",
                        channel, data
                    ));
                    return;
                }

                // Velocity Sense Offset
                0x0d => {
                    self.midi_channels[channel].set_midi_parameter(
                        ChannelMidiParameterValue::VelocitySenseOffset(data),
                    );
                    spessa_synth_info(&format!(
                        "XG Velocity Sense Offset on {}: {}",
                        channel, data
                    ));
                    return;
                }

                // Pan position
                0x0e => {
                    let pan = data;
                    let random_pan = pan == 0;
                    self.midi_channels[channel]
                        .set_midi_parameter(ChannelMidiParameterValue::RandomPan(random_pan));
                    if random_pan {
                        // 0 means random
                        spessa_synth_info(&format!("Random Pan for {}: ON", channel));
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

                // Dry level
                0x11 => {
                    self.midi_channels[channel]
                        .set_midi_parameter(ChannelMidiParameterValue::DryLevel(data));
                    spessa_synth_info(&format!("XG Dry Level on {}: {}", channel, data));
                }

                // Chorus
                0x12 => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::CHORUS_DEPTH,
                        data,
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
                        data,
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
                        data,
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
                        data,
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
                        data,
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
                        data,
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
                        data,
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
                        data,
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
                        data,
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
                        data,
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // ---
                // XG Controller matrix starts here
                // ---
                // 2 Special cases which are aliases:

                // MW LFO PMOD Depth (alias to modulation wheel range)
                0x20 => {
                    let cents = (data as f64 / 127.0) * 600.0;
                    self.midi_channels[channel]
                        .set_midi_parameter(ChannelMidiParameterValue::ModulationDepth(cents));
                    spessa_synth_info(&format!(
                        "XG Modulation Wheel Range for {}: {} cents",
                        channel, cents
                    ));
                }

                // Bend pitch control (alias to pitch wheel range)
                0x23 => {
                    let centered_value = data as f64 - 64.0;
                    self.midi_channels[channel].set_midi_parameter(
                        ChannelMidiParameterValue::PitchWheelRange(centered_value),
                    );
                    spessa_synth_info(&format!(
                        "XG Pitch Wheel Range for {}: {} semitones",
                        channel, centered_value
                    ));
                }

                // Auxiliary controllers
                // AC1 Controller number
                0x59 => {
                    self.midi_channels[channel]
                        .set_midi_parameter(ChannelMidiParameterValue::Cc1(data));
                    spessa_synth_info(&format!("XG AC1 controller number for {}: {}", channel, data));
                }

                // AC2 Controller number
                0x60 => {
                    self.midi_channels[channel]
                        .set_midi_parameter(ChannelMidiParameterValue::Cc2(data));
                    spessa_synth_info(&format!("XG AC2 controller number for {}: {}", channel, data));
                }

                // The receivers themselves:
                // Modulation Wheel (0x20 is aliased to modulation depth range)
                0x1d | 0x1e | 0x1f | 0x21 | 0x22
                // Pitch Bend (0x23 is aliased to pitch bend range)
                | 0x24..=0x28
                // Channel Aftertouch
                | 0x4d..=0x52
                // Poly Aftertouch
                | 0x53..=0x58
                // AC1 (0x59 is number, handled above)
                | 0x5a..=0x5f
                // AC2 (0x60 is number, handled above)
                | 0x61..=0x66 => {
                    let ch = &self.midi_channels[channel];
                    let (start_addr, source, is_cc, source_name, bipolar) = if a3 <= 0x22 {
                        (0x1d, midi_controllers::MODULATION_WHEEL as usize, true, "mod wheel", false)
                    } else if a3 <= 0x28 {
                        (0x23, modulator_sources::PITCH_WHEEL as usize, false, "pitch wheel", true)
                    } else if a3 <= 0x52 {
                        (
                            0x4d,
                            modulator_sources::CHANNEL_PRESSURE as usize,
                            false,
                            "channel pressure",
                            false,
                        )
                    } else if a3 <= 0x58 {
                        (
                            0x53,
                            modulator_sources::POLY_PRESSURE as usize,
                            false,
                            "poly pressure",
                            false,
                        )
                    } else if a3 <= 0x5f {
                        (0x5a, ch.midi_parameters.cc1 as usize, true, "AC1", false)
                    } else {
                        (0x61, ch.midi_parameters.cc2 as usize, true, "AC2", false)
                    };

                    // Map to GS
                    self.midi_channels[channel].sys_ex_modulators.setup_receiver_xg(
                        a3 - start_addr,
                        data,
                        source,
                        is_cc,
                        source_name,
                        bipolar,
                    );
                }

                // ---
                // XG Controller Matrix ends here
                // ---

                // Portamento switch
                0x67 => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::PORTAMENTO_ON_OFF,
                        if data == 1 { 127 } else { 0 },
                        &mut self.voices,
                        current_time,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Portamento time
                0x68 => {
                    let evs = self.midi_channels[channel].controller_change(
                        midi_controllers::PORTAMENTO_TIME,
                        data,
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
                        a3, channel
                    ));
                }
            }
            return;
        }

        if a1 >> 4 == 3 {
            // Drum part setup
            if self.system_parameters.drum_lock {
                return;
            }

            // In xg, the map is offset by the default (e.g. 2)
            // So 0 means drum setup 2, 1 means drum setup 3, etc.
            let setup_number = (a1 & 0xf) + DEFAULT_XG_DRUM_MAP;
            let drum_key = a2 as usize;
            match a3 {
                0x00 => {
                    // Drum pitch coarse
                    let pitch = data as f64 - 64.0;
                    for ch in self.midi_channels.iter_mut() {
                        if ch.midi_parameters.drum_map != setup_number {
                            continue;
                        }
                        ch.drum_params[drum_key].pitch_coarse = pitch;
                    }
                    spessa_synth_info(&format!(
                        "Drum Pitch for key {}: {} semitones",
                        drum_key, pitch
                    ));
                }

                0x01 => {
                    // Drum pitch fine
                    let pitch = data as f64 - 64.0;
                    for ch in self.midi_channels.iter_mut() {
                        if ch.midi_parameters.drum_map != setup_number {
                            continue;
                        }
                        ch.drum_params[drum_key].pitch_fine = pitch;
                    }
                    spessa_synth_info(&format!("Drum Pitch Fine for key {}: {}", drum_key, pitch));
                }

                0x02 => {
                    // Drum Level
                    for ch in self.midi_channels.iter_mut() {
                        if ch.midi_parameters.drum_map != setup_number {
                            continue;
                        }
                        ch.drum_params[drum_key].level = data;
                    }
                    spessa_synth_info(&format!("Drum Level for key {}: {}", drum_key, data));
                }

                0x03 => {
                    // Drum Alternate Group (exclusive class)
                    for ch in self.midi_channels.iter_mut() {
                        if ch.midi_parameters.drum_map != setup_number {
                            continue;
                        }
                        ch.drum_params[drum_key].assign_group = data;
                    }
                    spessa_synth_info(&format!(
                        "Drum Alternate Group for key {}: {}",
                        drum_key, data
                    ));
                }

                0x04 => {
                    // Drum Pan
                    for ch in self.midi_channels.iter_mut() {
                        if ch.midi_parameters.drum_map != setup_number {
                            continue;
                        }
                        ch.drum_params[drum_key].pan = data;
                    }
                    spessa_synth_info(&format!("Drum Pan for key {}: {}", drum_key, data));
                }

                0x05 => {
                    // Drum Reverb
                    for ch in self.midi_channels.iter_mut() {
                        if ch.midi_parameters.drum_map != setup_number {
                            continue;
                        }
                        ch.drum_params[drum_key].reverb_send = data;
                    }
                    spessa_synth_info(&format!("Drum Reverb for key {}: {}", drum_key, data));
                }

                0x06 => {
                    // Drum Chorus
                    for ch in self.midi_channels.iter_mut() {
                        if ch.midi_parameters.drum_map != setup_number {
                            continue;
                        }
                        ch.drum_params[drum_key].chorus_send = data;
                    }
                    spessa_synth_info(&format!("Drum Chorus for key {}: {}", drum_key, data));
                }

                0x07 => {
                    // Drum Variation
                    for ch in self.midi_channels.iter_mut() {
                        if ch.midi_parameters.drum_map != setup_number {
                            continue;
                        }
                        ch.drum_params[drum_key].variation_send = data;
                    }
                    spessa_synth_info(&format!("Drum Variation for key {}: {}", drum_key, data));
                }

                0x09 => {
                    // Receive note off
                    for ch in self.midi_channels.iter_mut() {
                        if ch.midi_parameters.drum_map != setup_number {
                            continue;
                        }
                        ch.drum_params[drum_key].rx_note_off = data == 1;
                    }
                    spessa_synth_info(&format!(
                        "Drum Note Off for key {}: {}",
                        drum_key,
                        if data == 1 { "ON" } else { "OFF" }
                    ));
                }

                0x0a => {
                    // Receive note on
                    for ch in self.midi_channels.iter_mut() {
                        if ch.midi_parameters.drum_map != setup_number {
                            continue;
                        }
                        ch.drum_params[drum_key].rx_note_on = data == 1;
                    }
                    spessa_synth_info(&format!(
                        "Drum Note On for key {}: {}",
                        drum_key,
                        if data == 1 { "ON" } else { "OFF" }
                    ));
                }

                _ => {}
            }
            return;
        }

        if a1 == 0x06 || a1 == 0x07 {
            // Display letters (0x06) or Display bitmap (0x07)
            self.call_event(
                crate::synthesizer::types::SynthProcessorEvent::DisplayMessage(syx.to_vec()),
            );
            return;
        }

        sys_ex_not_recognized(syx, "Yamaha XG");
    }
}
