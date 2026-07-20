/// data_entry.rs
/// purpose: Unified MIDI data entry (RPN/NRPN) handler for MidiChannel.
/// Ported from: src/synthesizer/audio_engine/channel/data_entry.ts
///
/// # 4.3.0 notes
/// TS 4.3.0 merged the previous `dataEntryCoarse`/`dataEntryFine` pair into a single
/// `dataEntry()` that is invoked for both the Data Entry MSB and LSB controllers and
/// reads the full 14-bit value stored in the Data Entry MSB slot. RPN vs NRPN is now
/// selected by `lastParameterIsRegistered` instead of the old 4-state machine.
///
/// The 4.3.0 channel-parameter migration (pitchWheelRange/keyShift/fineTune/modulationDepth
/// as `_midiParameters`) and the custom-vibrato removal are handled in later tasks; here the
/// existing Rust primitives (`set_tuning`, `set_modulation_depth`, `set_custom_controller`,
/// `midi_controllers[NON_CC + ...]`, `channel_vibrato`) are reused, so the numeric fixes
/// (LSB retention, fractional fine-tuning, 14-bit modulation depth, fractional pitch wheel
/// range) are applied without touching the LFO/pan rendering path.
use crate::midi::enums::midi_controllers;
use crate::soundbank::basic_soundbank::generator_types::GeneratorType;
use crate::soundbank::enums::modulator_sources;
use crate::soundbank::types::MIDISystem;
use crate::synthesizer::audio_engine::channel::midi_channel::MidiChannel;
use crate::synthesizer::audio_engine::channel::parameters::midi::{
    ChannelMidiParameterValue, NON_CC_INDEX_OFFSET,
};
use crate::synthesizer::audio_engine::voice::voice::Voice;
use crate::synthesizer::enums::custom_controllers;
use crate::synthesizer::types::SynthProcessorEvent;
use crate::utils::loggin::spessa_synth_info;

/// Registered parameter number types (RPN).
/// Equivalent to: RegisteredParameterTypes
pub mod registered_parameter_types {
    pub const PITCH_WHEEL_RANGE: i32 = 0x00_00;
    pub const FINE_TUNING: i32 = 0x00_01;
    pub const COARSE_TUNING: i32 = 0x00_02;
    pub const MODULATION_DEPTH: i32 = 0x00_05;
    pub const RESET_PARAMETERS: i32 = 0x3f_ff;
}

/// Non-registered parameter MSB values.
/// Equivalent to: NonRegisteredMSB
pub mod non_registered_msb {
    pub const PART_PARAMETER: u8 = 0x01;
    pub const DRUM_PITCH: u8 = 0x18;
    pub const DRUM_PITCH_FINE: u8 = 0x19;
    pub const DRUM_LEVEL: u8 = 0x1a;
    pub const DRUM_PAN: u8 = 0x1c;
    pub const DRUM_REVERB: u8 = 0x1d;
    pub const DRUM_CHORUS: u8 = 0x1e;
    pub const DRUM_DELAY: u8 = 0x1f;
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
    /// Executes a data entry change for the current channel.
    ///
    /// Reads the full 14-bit Data Entry value and dispatches to RPN or NRPN
    /// handling depending on `last_parameter_is_registered`.
    ///
    /// Equivalent to: dataEntry()
    pub fn data_entry(
        &mut self,
        voices: &mut [Voice],
        current_time: f64,
        current_system: MIDISystem,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        let mut events = Vec::new();

        // Stored in the cc table as a 14-bit value.
        let data_value = self.midi_controllers[midi_controllers::DATA_ENTRY_MSB as usize] as i32;

        // --- RPN handling ---
        if self.last_parameter_is_registered {
            let rpn_value = (self.midi_controllers
                [midi_controllers::REGISTERED_PARAMETER_MSB as usize]
                as i32)
                | ((self.midi_controllers[midi_controllers::REGISTERED_PARAMETER_LSB as usize] >> 7)
                    as i32);

            use registered_parameter_types as rpt;
            match rpn_value {
                // Pitch wheel range: may be fractional, so store the full 14-bit value
                // (the consumer divides by 128 to obtain the semitone amount).
                rpt::PITCH_WHEEL_RANGE => {
                    self.midi_controllers
                        [NON_CC_INDEX_OFFSET + modulator_sources::PITCH_WHEEL_RANGE as usize] =
                        data_value as i16;
                    spessa_synth_info(&format!(
                        "Pitch Wheel Range for {}: {} semitones",
                        self.channel,
                        data_value as f64 / 128.0
                    ));
                }

                // Coarse tuning: semitones, discard the LSB.
                // TS 4.3.14 treats this as a (non-real-time) key shift via
                // setMIDIParameter("keyShift", semitones), which changes the sound-bank
                // note chosen at note-on (sample selection + root key), NOT a pitch bend
                // of the currently selected sample. Routing it through the channel tuning
                // cents (pitch bend) plays the wrong sample.
                rpt::COARSE_TUNING => {
                    let semitones = (data_value >> 7) - 64;
                    self.set_midi_parameter(ChannelMidiParameterValue::KeyShift(semitones as f64));
                    spessa_synth_info(&format!(
                        "Key shift for {}: {} semitones",
                        self.channel, semitones
                    ));
                }

                // Fine-tuning: resolution is 100/8192 cents.
                // TS 4.3.14 uses setMIDIParameter("fineTune", cents) with the FULL float
                // value (no rounding). Rounding to whole cents quantizes sub-cent real-time
                // tuning and makes held notes drift out of phase from TS. Route the full
                // float value into the channel tuning cents consumed by render_voice.
                rpt::FINE_TUNING => {
                    let final_tuning = data_value - 8192;
                    let cents = final_tuning as f64 / 81.92;
                    self.set_custom_controller(custom_controllers::CHANNEL_TUNING, cents);
                    spessa_synth_info(&format!(
                        "Fine tuning for {} is now set to {} cents.",
                        self.channel,
                        cents.round()
                    ));
                }

                // Modulation depth: cents, so data / 128 * 100 == data / 1.28.
                rpt::MODULATION_DEPTH => {
                    self.set_modulation_depth(data_value as f64 / 1.28);
                }

                rpt::RESET_PARAMETERS => {
                    // TS 4.3.0 ignores the RPN "reset parameters" (0x7F,0x7F) data entry.
                }

                _ => {
                    spessa_synth_info(&format!(
                        "Unrecognized RPN for ch {}: (0x{:04X}) data value: {}",
                        self.channel, rpn_value, data_value
                    ));
                }
            }
            return events;
        }

        // --- NRPN handling ---
        // Keep the existing Rust GS-NRPN lock gate (equivalent to the drumLock/nrpnParamLock
        // gating used by TS; consolidating to the exact TS system parameters is deferred to
        // the channel-parameter migration).
        if self.lock_gs_nrpn_params {
            return events;
        }

        let param_coarse = (self.midi_controllers
            [midi_controllers::NON_REGISTERED_PARAMETER_MSB as usize]
            >> 7) as u8;
        let param_fine = (self.midi_controllers
            [midi_controllers::NON_REGISTERED_PARAMETER_LSB as usize]
            >> 7) as u8;
        let data_coarse = (data_value >> 7) as u8;

        match param_coarse {
            // Part parameters (vibrato and EG)
            non_registered_msb::PART_PARAMETER => {
                let mut sub = self.handle_nrpn_part_parameter(
                    param_fine,
                    data_coarse,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
            }

            // Drum pitch: it is actually 50 cents (not for XG, and not when the SC-55
            // preset is explicitly requested via MAP1 / bank LSB 1, where it is 100 cents).
            // https://github.com/spessasus/spessasynth_core/pull/58#issuecomment-3893343073
            non_registered_msb::DRUM_PITCH => {
                let pitch = if self.channel_system(current_system) == MIDISystem::Xg
                    || self.patch.bank_lsb == 1
                {
                    (data_coarse as i32 - 64) * 100
                } else {
                    (data_coarse as i32 - 64) * 50
                };
                self.drum_params[param_fine as usize].pitch = pitch as f64;
                spessa_synth_info(&format!(
                    "Drum {} pitch for {}: {} cents",
                    param_fine, self.channel, pitch
                ));
            }

            non_registered_msb::DRUM_PITCH_FINE => {
                let pitch = data_coarse as i32 - 64;
                self.drum_params[param_fine as usize].pitch += pitch as f64;
                spessa_synth_info(&format!(
                    "Drum {} pitch fine for {}: {} cents",
                    param_fine, self.channel, self.drum_params[param_fine as usize].pitch
                ));
            }

            non_registered_msb::DRUM_LEVEL => {
                self.drum_params[param_fine as usize].gain = data_coarse as f64 / 120.0;
                spessa_synth_info(&format!(
                    "Drum {} level for {}: {}",
                    param_fine, self.channel, data_coarse
                ));
            }

            non_registered_msb::DRUM_PAN => {
                self.drum_params[param_fine as usize].pan = data_coarse;
                spessa_synth_info(&format!(
                    "Drum {} pan for {}: {}",
                    param_fine, self.channel, data_coarse
                ));
            }

            non_registered_msb::DRUM_REVERB => {
                self.drum_params[param_fine as usize].reverb_gain = data_coarse as f64 / 127.0;
                spessa_synth_info(&format!(
                    "Drum {} reverb level for {}: {}",
                    param_fine, self.channel, data_coarse
                ));
            }

            non_registered_msb::DRUM_CHORUS => {
                self.drum_params[param_fine as usize].chorus_gain = data_coarse as f64 / 127.0;
                spessa_synth_info(&format!(
                    "Drum {} chorus level for {}: {}",
                    param_fine, self.channel, data_coarse
                ));
            }

            non_registered_msb::DRUM_DELAY => {
                self.drum_params[param_fine as usize].delay_gain = data_coarse as f64 / 127.0;
                spessa_synth_info(&format!(
                    "Drum {} delay level for {}: {}",
                    param_fine, self.channel, data_value
                ));
            }

            // SoundBlaster AWE32 NRPN
            non_registered_msb::AWE32 => {
                self.handle_awe32_nrpn(param_fine as usize, data_value, voices);
            }

            // SF2 NRPN
            non_registered_msb::SF2 => {
                // Per SF spec, NRPN Select LSB > 100 is for setup only and should not
                // be used on its own to select a generator parameter.
                if param_fine <= 100 {
                    let r#gen = self.custom_controllers
                        [custom_controllers::SF2_NPRN_GENERATOR_LSB as usize]
                        as GeneratorType;
                    let offset = data_value - 8192;
                    self.set_generator_offset(r#gen, offset as i16, voices);
                }
            }

            _ => {
                spessa_synth_info(&format!(
                    "Unrecognized NRPN for ch {}: (0x{:02X} 0x{:02X}) data value: {}",
                    self.channel, param_coarse, param_fine, data_coarse
                ));
            }
        }

        events
    }

    /// Processes NRPN part parameter messages (NRPN MSB = 0x01): custom vibrato and EG.
    ///
    /// The custom channel vibrato is a pre-4.3.0 mechanism retained until the LFO rework;
    /// the filter/EG cases route through the corresponding CCs as in TS.
    #[allow(clippy::too_many_arguments)]
    fn handle_nrpn_part_parameter(
        &mut self,
        nrpn_fine: u8,
        data_coarse: u8,
        voices: &mut [Voice],
        current_time: f64,
        current_system: MIDISystem,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        use non_registered_lsb as nrl;
        let mut events = Vec::new();

        match nrpn_fine {
            // TS 4.3.0 removed the pre-4.3.0 custom channel vibrato: GS NRPN vibrato
            // rate/depth/delay now route through the vibrato-rate/depth/delay CCs
            // (76/77/78), which drive the vibLfoRate / vibLfoToPitch / delayVibLFO
            // generators via the default modulators.
            nrl::VIBRATO_RATE => {
                let mut sub = self.controller_change(
                    midi_controllers::VIBRATO_RATE,
                    data_coarse,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
                spessa_synth_info(&format!("Vibrato rate for {}: {}", self.channel, data_coarse));
            }

            nrl::VIBRATO_DEPTH => {
                let mut sub = self.controller_change(
                    midi_controllers::VIBRATO_DEPTH,
                    data_coarse,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
                spessa_synth_info(&format!("Vibrato depth for {}: {}", self.channel, data_coarse));
            }

            nrl::VIBRATO_DELAY => {
                let mut sub = self.controller_change(
                    midi_controllers::VIBRATO_DELAY,
                    data_coarse,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
                spessa_synth_info(&format!("Vibrato delay for {}: {}", self.channel, data_coarse));
            }

            nrl::TVF_FILTER_CUTOFF => {
                let mut sub = self.controller_change(
                    midi_controllers::BRIGHTNESS,
                    data_coarse,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
                spessa_synth_info(&format!(
                    "Filter cutoff for {}: {}",
                    self.channel, data_coarse
                ));
            }

            nrl::TVF_FILTER_RESONANCE => {
                let mut sub = self.controller_change(
                    midi_controllers::FILTER_RESONANCE,
                    data_coarse,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
                spessa_synth_info(&format!(
                    "Filter resonance for {}: {}",
                    self.channel, data_coarse
                ));
            }

            nrl::EG_ATTACK_TIME => {
                let mut sub = self.controller_change(
                    midi_controllers::ATTACK_TIME,
                    data_coarse,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
                spessa_synth_info(&format!(
                    "EG attack time for {}: {}",
                    self.channel, data_coarse
                ));
            }

            nrl::EG_DECAY_TIME => {
                let mut sub = self.controller_change(
                    midi_controllers::DECAY_TIME,
                    data_coarse,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
                spessa_synth_info(&format!(
                    "EG decay time for {}: {}",
                    self.channel, data_coarse
                ));
            }

            nrl::EG_RELEASE_TIME => {
                let mut sub = self.controller_change(
                    midi_controllers::RELEASE_TIME,
                    data_coarse,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
                spessa_synth_info(&format!(
                    "EG release time for {}: {}",
                    self.channel, data_coarse
                ));
            }

            _ => {
                spessa_synth_info(&format!(
                    "Unrecognized NRPN for ch {}: (0x01 0x{:02X}) data value: {}",
                    self.channel, nrpn_fine, data_coarse
                ));
            }
        }

        events
    }
}
