/// reset_controllers.rs
/// purpose: Controller reset handlers for MidiChannel.
/// Ported from: src/synthesizer/audio_engine/engine_methods/controller_control/reset_controllers.ts
use crate::midi::enums::midi_controllers;
use crate::synthesizer::audio_engine::engine_components::controller_tables::{
    CUSTOM_RESET_ARRAY, DEFAULT_MIDI_CONTROLLER_VALUES,
};
use crate::synthesizer::audio_engine::engine_components::drum_parameters::reset_drum_params;
use crate::synthesizer::audio_engine::engine_components::synth_constants::DEFAULT_PERCUSSION;
use crate::synthesizer::audio_engine::engine_components::voice::Voice;
use crate::synthesizer::audio_engine::synthesizer_core::MidiChannel;
use crate::synthesizer::enums::{custom_controllers, data_entry_states};
use crate::synthesizer::types::{SynthProcessorEvent, SynthSystem};
use crate::utils::midi_hacks::BankSelectHacks;

/// MIDI controller numbers that are NOT reset by resetControllers (RP-15).
/// Equivalent to: nonResettableCCs
pub fn is_non_resettable(cc: u8) -> bool {
    matches!(
        cc,
        midi_controllers::BANK_SELECT
            | midi_controllers::BANK_SELECT_LSB
            | midi_controllers::MAIN_VOLUME
            | midi_controllers::MAIN_VOLUME_LSB
            | midi_controllers::PAN
            | midi_controllers::PAN_LSB
            | midi_controllers::REVERB_DEPTH
            | midi_controllers::TREMOLO_DEPTH
            | midi_controllers::CHORUS_DEPTH
            | midi_controllers::VARIATION_DEPTH
            | midi_controllers::PHASER_DEPTH
            | midi_controllers::SOUND_VARIATION
            | midi_controllers::FILTER_RESONANCE
            | midi_controllers::RELEASE_TIME
            | midi_controllers::ATTACK_TIME
            | midi_controllers::BRIGHTNESS
            | midi_controllers::DECAY_TIME
            | midi_controllers::VIBRATO_RATE
            | midi_controllers::VIBRATO_DEPTH
            | midi_controllers::VIBRATO_DELAY
            | midi_controllers::SOUND_CONTROLLER10
            | midi_controllers::POLY_MODE_ON
            | midi_controllers::MONO_MODE_ON
            | midi_controllers::OMNI_MODE_ON
            | midi_controllers::OMNI_MODE_OFF
            | midi_controllers::REGISTERED_PARAMETER_LSB
            | midi_controllers::REGISTERED_PARAMETER_MSB
            | midi_controllers::NON_REGISTERED_PARAMETER_LSB
            | midi_controllers::NON_REGISTERED_PARAMETER_MSB
    )
}

impl MidiChannel {
    /// Resets portamento control to the XG default (60) or 0 for other systems.
    ///
    /// Equivalent to: resetPortamento(sendCC)
    fn reset_portamento(
        &mut self,
        send_cc: bool,
        voices: &mut [Voice],
        current_time: f64,
        current_system: SynthSystem,
    ) -> Vec<SynthProcessorEvent> {
        if self.locked_controllers[midi_controllers::PORTAMENTO_CONTROL as usize] {
            return Vec::new();
        }
        let ch_system = self.channel_system(current_system);
        let value = if ch_system == SynthSystem::Xg { 60 } else { 0 };
        self.controller_change(
            midi_controllers::PORTAMENTO_CONTROL,
            value,
            voices,
            current_time,
            current_system,
            send_cc,
        )
    }

    /// Resets all controllers to their default values (full reset).
    ///
    /// Resets the octave tuning, all non-locked MIDI controllers,
    /// portamento, vibrato, pitch wheel, sysex modulators, and custom controllers.
    /// Preserves channelTransposeFine custom controller.
    ///
    /// Equivalent to: resetControllers(sendCCEvents = true)
    pub fn reset_controllers(
        &mut self,
        send_event: bool,
        voices: &mut [Voice],
        current_time: f64,
        current_system: SynthSystem,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        self.channel_octave_tuning.fill(0);

        let mut events = Vec::new();

        // Reset all non-locked controllers to their default values
        #[allow(clippy::needless_range_loop)]
        for cc in 0..127usize {
            if self.locked_controllers[cc] {
                continue;
            }
            let reset_value = DEFAULT_MIDI_CONTROLLER_VALUES[cc];
            if self.midi_controllers[cc] != reset_value {
                // Skip controllers handled separately
                if cc != midi_controllers::PORTAMENTO_CONTROL as usize
                    && cc != midi_controllers::DATA_ENTRY_MSB as usize
                    && cc != midi_controllers::REGISTERED_PARAMETER_MSB as usize
                    && cc != midi_controllers::REGISTERED_PARAMETER_LSB as usize
                    && cc != midi_controllers::NON_REGISTERED_PARAMETER_MSB as usize
                    && cc != midi_controllers::NON_REGISTERED_PARAMETER_LSB as usize
                {
                    let mut sub = self.controller_change(
                        cc as u8,
                        (reset_value >> 7) as u8,
                        voices,
                        current_time,
                        current_system,
                        send_event && enable_event_system,
                    );
                    events.append(&mut sub);
                }
            } else {
                // Out-of-range index or value already matches: direct reset
                self.midi_controllers[cc] = reset_value;
            }
        }

        // Reset portamento
        let mut sub = self.reset_portamento(
            send_event && enable_event_system,
            voices,
            current_time,
            current_system,
        );
        events.append(&mut sub);

        // Reset custom vibrato
        self.channel_vibrato.rate = 0.0;
        self.channel_vibrato.depth = 0.0;
        self.channel_vibrato.delay = 0.0;
        self.random_pan = false;

        // Restore poly mode
        if !self.locked_controllers[midi_controllers::MONO_MODE_ON as usize]
            && !self.locked_controllers[midi_controllers::POLY_MODE_ON as usize]
        {
            self.poly_mode = true;
        }

        // Reset pitch wheel
        self.per_note_pitch = false;
        let mut sub = self.pitch_wheel(voices, 8192, -1, send_event && enable_event_system);
        events.append(&mut sub);

        // Reset sysex modulators
        self.sys_ex_modulators.reset_modulators();

        // Reset custom controllers (preserve transpose fine)
        let transpose =
            self.custom_controllers[custom_controllers::CHANNEL_TRANSPOSE_FINE as usize];
        self.custom_controllers.copy_from_slice(&CUSTOM_RESET_ARRAY);
        self.set_custom_controller(custom_controllers::CHANNEL_TRANSPOSE_FINE, transpose as f64);
        self.reset_parameters();

        // Reset drum parameters (SC-88 standard reverb values, etc.)
        reset_drum_params(&mut self.drum_params);

        events
    }

    /// Resets the channel preset to the default for the current system.
    ///
    /// Sets bank MSB to the system default, bank LSB to 0, GS drums off,
    /// drum flag to channel 9, and program to 0.
    ///
    /// Equivalent to: resetPreset()
    pub fn reset_preset(
        &mut self,
        sound_bank_manager: &crate::synthesizer::audio_engine::engine_components::sound_bank_manager::SoundBankManager,
        current_system: SynthSystem,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        let ch_system = self.channel_system(current_system);
        self.set_bank_msb(BankSelectHacks::get_default_bank(ch_system));
        self.set_bank_lsb(0);
        self.set_gs_drums(false);

        let is_drum = self.channel % 16 == DEFAULT_PERCUSSION;
        let mut events = self.set_drums(
            is_drum,
            sound_bank_manager,
            current_system,
            enable_event_system,
        );

        events
    }

    /// Resets controllers according to RP-15 Recommended Practice.
    ///
    /// Resets octave tuning, pitch wheel, vibrato, non-resettable CCs,
    /// portamento, and generator overrides/offsets.
    ///
    /// Equivalent to: resetControllersRP15Compliant()
    pub fn reset_controllers_rp15(
        &mut self,
        voices: &mut [Voice],
        current_time: f64,
        current_system: SynthSystem,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        self.channel_octave_tuning.fill(0);

        let mut events = Vec::new();

        // Reset pitch wheel
        self.per_note_pitch = false;
        let mut sub = self.pitch_wheel(voices, 8192, -1, enable_event_system);
        events.append(&mut sub);

        // Reset custom vibrato
        self.channel_vibrato.rate = 0.0;
        self.channel_vibrato.depth = 0.0;
        self.channel_vibrato.delay = 0.0;

        // Reset all non-locked, non-special controllers
        for cc in 0..128u8 {
            let reset_value = DEFAULT_MIDI_CONTROLLER_VALUES[cc as usize];
            if !is_non_resettable(cc)
                && reset_value != self.midi_controllers[cc as usize]
                && cc != midi_controllers::PORTAMENTO_CONTROL
            {
                let mut sub = self.controller_change(
                    cc,
                    (reset_value >> 7) as u8,
                    voices,
                    current_time,
                    current_system,
                    enable_event_system,
                );
                events.append(&mut sub);
            }
        }

        self.reset_generator_overrides();
        self.reset_generator_offsets();

        events
    }

    /// Resets all RPN/NRPN parameters, generator overrides, and offsets.
    ///
    /// Sets the data entry state machine to Idle and resets RPN/NRPN
    /// controller values to their defaults (0x7F << 7).
    ///
    /// Equivalent to: resetParameters()
    pub fn reset_parameters(&mut self) {
        self.data_entry_state = data_entry_states::IDLE;
        self.midi_controllers[midi_controllers::NON_REGISTERED_PARAMETER_LSB as usize] = 127 << 7;
        self.midi_controllers[midi_controllers::NON_REGISTERED_PARAMETER_MSB as usize] = 127 << 7;
        self.midi_controllers[midi_controllers::REGISTERED_PARAMETER_LSB as usize] = 127 << 7;
        self.midi_controllers[midi_controllers::REGISTERED_PARAMETER_MSB as usize] = 127 << 7;
        self.reset_generator_overrides();
        self.reset_generator_offsets();
    }
}
