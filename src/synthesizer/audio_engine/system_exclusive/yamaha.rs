/// yamaha.rs
/// purpose: Handles Yamaha XG system exclusive messages.
/// Ported from: src/synthesizer/audio_engine/system_exclusive/yamaha.ts
/// Reference: http://www.studio4all.de/htmle/main91.html
use crate::midi::enums::midi_controllers;
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

        if a1 == 0x02 && a2 == 0x01 {
            let effect = a3;
            let effect_type = if effect <= 0x15 {
                "Reverb"
            } else if effect <= 0x35 {
                "Chorus"
            } else {
                "Variation"
            };
            spessa_synth_info(&format!("Unsupported XG {} Parameter: {:02X}", effect_type, effect));
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
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                }

                // Rev. channel
                0x04 => {
                    // The rxChannel selects which channel this part receives on.
                    // NOTE: customChannelNumbers dispatch is not implemented yet
                    // (Task 21), so the value is stored but does not affect rendering.
                    let rx_channel = data as usize + channel_offset;
                    self.midi_channels[channel]
                        .set_midi_parameter(ChannelMidiParameterValue::RxChannel(rx_channel as u8));
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

                // Part mode (drum channel flag)
                0x07 => {
                    let drums = data != 0;
                    let evs = self.midi_channels[channel].set_drums(
                        drums,
                        &self.sound_bank_manager,
                        current_system,
                        enable_event_system,
                    );
                    for ev in evs {
                        self.call_event(ev);
                    }
                    spessa_synth_info(&format!(
                        "XG Part Mode on {}: {}",
                        channel,
                        if drums { "DRUM" } else { "MELODIC" }
                    ));
                }

                // Note shift
                0x08 => {
                    // Drum channels ignore key shift; reset to 0 to be sure.
                    let mut shift = data as f64 - 64.0;
                    if self.midi_channels[channel].drum_channel {
                        shift = 0.0;
                    }
                    if self.midi_channels[channel].midi_parameters.key_shift != shift {
                        self.midi_channels[channel]
                            .set_midi_parameter(ChannelMidiParameterValue::KeyShift(shift));
                    }
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

                // Bend pitch control (pitch wheel range)
                0x23 => {
                    let centered_value = data as f64 - 64.0;
                    self.midi_channels[channel].set_midi_parameter(
                        ChannelMidiParameterValue::PitchWheelRange(centered_value),
                    );
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
            let drum_key = a2 as usize;
            match a3 {
                0x00 => {
                    // Drum pitch coarse
                    let pitch = (data as f64 - 64.0) * 100.0;
                    for ch in self.midi_channels.iter_mut() {
                        if !ch.drum_channel {
                            continue;
                        }
                        ch.drum_params[drum_key].pitch = pitch;
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
                        if !ch.drum_channel {
                            continue;
                        }
                        ch.drum_params[drum_key].pitch += pitch;
                    }
                    spessa_synth_info(&format!("Drum Pitch Fine for key {}: {}", drum_key, pitch));
                }

                0x02 => {
                    // Drum Level
                    for ch in self.midi_channels.iter_mut() {
                        if !ch.drum_channel {
                            continue;
                        }
                        ch.drum_params[drum_key].gain = data as f64 / 120.0;
                    }
                    spessa_synth_info(&format!("Drum Level for key {}: {}", drum_key, data));
                }

                0x03 => {
                    // Drum Alternate Group (exclusive class)
                    for ch in self.midi_channels.iter_mut() {
                        if !ch.drum_channel {
                            continue;
                        }
                        ch.drum_params[drum_key].exclusive_class = data;
                    }
                    spessa_synth_info(&format!(
                        "Drum Alternate Group for key {}: {}",
                        drum_key, data
                    ));
                }

                0x04 => {
                    // Drum Pan
                    for ch in self.midi_channels.iter_mut() {
                        if !ch.drum_channel {
                            continue;
                        }
                        ch.drum_params[drum_key].pan = data;
                    }
                    spessa_synth_info(&format!("Drum Pan for key {}: {}", drum_key, data));
                }

                0x05 => {
                    // Drum Reverb
                    for ch in self.midi_channels.iter_mut() {
                        if !ch.drum_channel {
                            continue;
                        }
                        ch.drum_params[drum_key].reverb_gain = data as f64 / 127.0;
                    }
                    spessa_synth_info(&format!("Drum Reverb for key {}: {}", drum_key, data));
                }

                0x06 => {
                    // Drum Chorus
                    for ch in self.midi_channels.iter_mut() {
                        if !ch.drum_channel {
                            continue;
                        }
                        ch.drum_params[drum_key].chorus_gain = data as f64 / 127.0;
                    }
                    spessa_synth_info(&format!("Drum Chorus for key {}: {}", drum_key, data));
                }

                0x09 => {
                    // Receive note off
                    for ch in self.midi_channels.iter_mut() {
                        if !ch.drum_channel {
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
                        if !ch.drum_channel {
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

                _ => {
                    sys_ex_not_recognized(&[a3], "Yamaha XG Drum Setup");
                }
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
