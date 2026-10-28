/// reset_controllers.rs
/// purpose: Controller reset handlers for MidiChannel.
/// Ported from: src/synthesizer/audio_engine/engine_methods/controller_control/reset_controllers.ts
use crate::midi::enums::midi_controllers;
use crate::midi::midi_tools::sysex_data::{DEFAULT_GS_DRUM_MAP, DEFAULT_XG_DRUM_MAP, MELODIC_MAP};
use crate::soundbank::enums::modulator_sources;
use crate::synthesizer::audio_engine::channel::midi_channel::{DrumLocks, MidiChannel};
use crate::synthesizer::audio_engine::channel::parameters::midi::{
    ChannelMidiParameterValue, CUSTOM_RESET_ARRAY, DEFAULT_CHANNEL_MIDI_PARAMETERS,
    DEFAULT_MIDI_CONTROLLER_VALUES,
};
use crate::synthesizer::audio_engine::synth_constants::{DEFAULT_NRPN, MIDI_DRUM_CHANNEL};
use crate::synthesizer::audio_engine::voice::voice::Voice;
use crate::synthesizer::enums::custom_controllers;
use crate::synthesizer::types::SynthProcessorEvent;
use crate::soundbank::types::MIDISystem;
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
    /// Resets portamento state.
    ///
    /// Portamento has a quirk: for XG the last note is set to 60, for others it
    /// is set to none (-1), so no portamento happens on the first note-on.
    ///
    /// Equivalent to: the inline `lastPortamentoNote` reset in `resetChannelInternal()`
    /// (4.3.0: standalone `resetPortamento()`, inlined upstream in 4.3.14).
    fn reset_portamento(&mut self, current_system: MIDISystem) {
        let ch_system = self.channel_system(current_system);
        self.last_portamento_note = if ch_system == MIDISystem::Xg { 60 } else { -1 };
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
        current_system: MIDISystem,
        locks: DrumLocks,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        self.channel_octave_tuning.fill(0);

        let mut events = Vec::new();

        // Reset all non-locked controllers to their default values
        // (TS: cc < CONTROLLER_TABLE_SIZE, which is 128 since 4.4.0)
        #[allow(clippy::needless_range_loop)]
        for cc in 0..128usize {
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

        // Reset Poly pressure
        self.poly_pressures.fill(0);

        // Reset MIDI parameters (TS: resetMIDIParameter(...) for each one).
        // The ones not listed here are reset further below.
        // TS: resetMIDIParameter("pressure") -> setMIDIParameter("pressure", 0), which
        // recomputes the channel pressure modulators (`channel_pressure` does both).
        events.append(&mut self.channel_pressure(voices, 0));
        self.set_midi_parameter(ChannelMidiParameterValue::RxChannel(self.channel as i32));
        self.set_midi_parameter(ChannelMidiParameterValue::KeyShift(
            DEFAULT_CHANNEL_MIDI_PARAMETERS.key_shift,
        ));
        self.set_midi_parameter(ChannelMidiParameterValue::FineTune(
            DEFAULT_CHANNEL_MIDI_PARAMETERS.fine_tune,
        ));
        self.set_midi_parameter(ChannelMidiParameterValue::AssignMode(
            DEFAULT_CHANNEL_MIDI_PARAMETERS.assign_mode,
        ));
        self.set_midi_parameter(ChannelMidiParameterValue::Cc1(DEFAULT_CHANNEL_MIDI_PARAMETERS.cc1));
        self.set_midi_parameter(ChannelMidiParameterValue::Cc2(DEFAULT_CHANNEL_MIDI_PARAMETERS.cc2));

        // Reset portamento
        self.reset_portamento(current_system);

        self.reset_vibrato_params();
        self.set_midi_parameter(ChannelMidiParameterValue::RandomPan(false));

        // Clear the insertion effect assignment (TS: setMIDIParameter("efxAssign", false)).
        // Previously compensated for in `SynthesizerCore::reset_insertion`; since 4.3.16
        // derives `insertionActive` from this flag, it has to be reset here like in TS.
        self.set_midi_parameter(ChannelMidiParameterValue::EfxAssign(false));

        // Restore poly mode.
        // TS 4.3.0 guarded this with the lockedControllers[monoModeOn/polyModeOn] check;
        // TS 4.3.14 drops the guard here (parameter locking moves to the not-yet-ported
        // lockedMIDIParameters mechanism), so it is now set unconditionally.
        self.set_midi_parameter(ChannelMidiParameterValue::PolyMode(true));

        // Reset velocity sense (new in 4.3.14): identity depth/offset of 64/64.
        self.set_midi_parameter(ChannelMidiParameterValue::VelocitySenseOffset(64));
        self.set_midi_parameter(ChannelMidiParameterValue::VelocitySenseDepth(64));
        // TS: resetMIDIParameter("pitchWheelRange"). The controller loop above only covers
        // the 128 real CCs, not the non-CC slot the modulator source reads.
        self.set_midi_parameter(ChannelMidiParameterValue::PitchWheelRange(2.0));
        // TS: resetMIDIParameter("drumMap") sets the correct default map
        let default_map = if self.channel_system(current_system) == MIDISystem::Xg {
            DEFAULT_XG_DRUM_MAP
        } else {
            DEFAULT_GS_DRUM_MAP
        };
        self.set_midi_parameter(ChannelMidiParameterValue::DrumMap(
            if self.channel % 16 == MIDI_DRUM_CHANNEL { default_map } else { MELODIC_MAP },
        ));
        self.set_midi_parameter(ChannelMidiParameterValue::DryLevel(127));
        self.xg_insertion_assigned = false;

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
        // TS 4.3.14 reset.ts: setMIDIParameter("modulationDepth", 50)
        // (set the value directly to avoid per-reset log spam).
        self.midi_parameters.modulation_depth = 50.0;
        self.reset_parameters();

        // Reset note-tracking state (new in 4.3.14: this.playingNotes.fill(false)).
        self.playing_notes.fill(false);

        // Reset drum parameters (SC-88 standard reverb values, etc.)
        self.reset_drum_params(locks.drum, current_system);

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
        sound_bank_manager: &crate::synthesizer::audio_engine::sound_bank_manager::SoundBankManager,
        current_system: MIDISystem,
        locks: DrumLocks,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        // Reset program: default bank, program 0,
        // Drums only on every 16th channel 10.
        let ch_system = self.channel_system(current_system);
        self.set_bank_msb(BankSelectHacks::get_default_bank(ch_system));
        self.set_bank_lsb(0);
        let mut events =
            self.program_change(0, sound_bank_manager, current_system, locks, enable_event_system);
        let is_drum = self.channel % 16 == MIDI_DRUM_CHANNEL;
        events.append(&mut self.set_drums(
            is_drum,
            sound_bank_manager,
            current_system,
            locks,
            enable_event_system,
        ));

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
        current_system: MIDISystem,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        let mut events = Vec::new();

        // Reset pitch wheel to center.
        self.per_note_pitch = false;
        events.append(&mut self.pitch_wheel(voices, 8192, -1, enable_event_system));

        // Reset channel pressure to 0.
        events.append(&mut self.channel_pressure(voices, 0));

        // TS 4.3.0 RP-15 resets ONLY these controllers (RP_15_RESET_CC_NUMS), each back to
        // its DEFAULT_MIDI_CONTROLLERS value (and only if it currently differs). The 4.2.0
        // "reset everything except a non-resettable exclusion list" behavior was removed.
        const RP_15_RESET_CC_NUMS: [u8; 10] = [
            midi_controllers::MODULATION_WHEEL,
            midi_controllers::EXPRESSION_CONTROLLER,
            midi_controllers::SUSTAIN_PEDAL,
            midi_controllers::PORTAMENTO_ON_OFF,
            midi_controllers::SOSTENUTO_PEDAL,
            midi_controllers::SOFT_PEDAL,
            midi_controllers::REGISTERED_PARAMETER_MSB,
            midi_controllers::REGISTERED_PARAMETER_LSB,
            midi_controllers::NON_REGISTERED_PARAMETER_MSB,
            midi_controllers::NON_REGISTERED_PARAMETER_LSB,
        ];
        for &cc in RP_15_RESET_CC_NUMS.iter() {
            let reset_value = DEFAULT_MIDI_CONTROLLER_VALUES[cc as usize];
            if reset_value != self.midi_controllers[cc as usize] {
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
        // Reset polyphonic pressure for all notes to 0
        self.poly_pressures.fill(0);
        self.compute_modulators_all_impl(voices, 0, modulator_sources::POLY_PRESSURE as usize);

        events
    }

    /// Resets all RPN/NRPN parameters, generator overrides, and offsets.
    ///
    /// Sets the data entry state machine to Idle and resets RPN/NRPN
    /// controller values to their defaults (0x7F << 7).
    ///
    /// Equivalent to: resetParameters()
    pub fn reset_parameters(&mut self) {
        self.last_parameter_is_registered = true;
        // TS 4.4.0 defaults: RPN = DEFAULT_RPN = 0x7F (null), NRPN = DEFAULT_NRPN = 0x7F (null).
        self.midi_controllers[midi_controllers::NON_REGISTERED_PARAMETER_LSB as usize] =
            (DEFAULT_NRPN as i16) << 7;
        self.midi_controllers[midi_controllers::NON_REGISTERED_PARAMETER_MSB as usize] =
            (DEFAULT_NRPN as i16) << 7;
        self.midi_controllers[midi_controllers::REGISTERED_PARAMETER_LSB as usize] = 127 << 7;
        self.midi_controllers[midi_controllers::REGISTERED_PARAMETER_MSB as usize] = 127 << 7;
        self.reset_generator_overrides();
        self.reset_generator_offsets();
    }
}
