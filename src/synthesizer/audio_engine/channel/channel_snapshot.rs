//! channel_snapshot.rs
//! purpose: Snapshot of a single MIDI channel's state.
//! Ported from: src/synthesizer/audio_engine/channel/channel_snapshot.ts
//!
//! `getChannelSnapshot`/`applySnapshot` are bound to `MIDIChannel` in TS. The Rust channel
//! does not own the voices, the sound bank manager or the drum locks, so `apply_snapshot`
//! takes them as arguments and returns the events the program change produced.
use crate::midi::drum_parameters::{DrumParameter, DrumParameterUtils};
use crate::soundbank::basic_soundbank::midi_patch::MidiPatchFull;
use crate::soundbank::enums::modulator_sources;
use crate::soundbank::types::MIDISystem;
use crate::synthesizer::audio_engine::channel::awe32_nrpn::ChannelGenerators;
use crate::synthesizer::audio_engine::channel::midi_channel::{
    ChannelVibrato, DrumLocks, MidiChannel,
};
use crate::synthesizer::audio_engine::channel::parameters::midi::{
    CHANNEL_MIDI_PARAMETER_COUNT, ChannelMidiParameter, ChannelMidiParameterKey,
};
use crate::synthesizer::audio_engine::channel::parameters::system::{
    ChannelSystemParameter, ChannelSystemParameterValue,
};
use crate::synthesizer::audio_engine::sound_bank_manager::SoundBankManager;
use crate::synthesizer::audio_engine::synth_constants::CONTROLLER_TABLE_SIZE;
use crate::synthesizer::audio_engine::voice::voice::Voice;
use crate::synthesizer::types::SynthProcessorEvent;

/// This interface is a snapshot of a `MidiChannel`,
/// capturing its current state, which can be saved and restored.
///
/// See also `SynthesizerSnapshot`.
/// Equivalent to: interface ChannelSnapshot
#[derive(Clone, Debug)]
pub struct ChannelSnapshot {
    /// The currently selected MIDI patch of the channel.
    /// Equivalent to: patch?: MIDIPatchFull
    pub patch: Option<MidiPatchFull>,
    /// Indicates the MIDI system when the preset was locked.
    /// Equivalent to: lockedSystem
    pub locked_system: MIDISystem,

    /// An array of MIDI controllers for the channel (14-bit values).
    /// Equivalent to: midiControllers
    pub midi_controllers: [i16; CONTROLLER_TABLE_SIZE],
    /// An array indicating if a controller, at the equivalent index in the
    /// `midi_controllers` array, is locked (i.e., not allowed changing).
    /// Equivalent to: lockedControllers
    pub locked_controllers: Vec<bool>,
    /// An array for the MIDI 2.0 Per-note pitch wheels.
    /// Equivalent to: pitchWheels
    pub pitch_wheels: [i16; 128],
    /// Used for handling SF2/AWE32 NRPN generator adjustments.
    /// Equivalent to: generators
    pub generators: ChannelGenerators,

    /// The Channel MIDI Parameters of this channel.
    /// These are only editable via MIDI messages.
    /// Equivalent to: midiParameters
    pub midi_parameters: ChannelMidiParameter,
    /// Whether a Channel MIDI parameter is locked (i.e., not allowed changing), indexed by
    /// `ChannelMidiParameterKey`.
    /// Equivalent to: lockedMIDIParameters
    pub locked_midi_parameters: [bool; CHANNEL_MIDI_PARAMETER_COUNT],
    /// The Channel System Parameters of this channel.
    /// These are only editable via the API.
    /// Equivalent to: systemParameters
    pub system_parameters: ChannelSystemParameter,
    /// An array of octave tuning values for each note on the channel.
    /// Each index corresponds to a note (0 = C, 1 = C#, ..., 11 = B).
    /// Note: Repeated every 12 notes.
    /// Equivalent to: octaveTuning
    pub octave_tuning: [i8; 128],

    /// Per-note pitch wheel mode uses the pitchWheels table as source
    /// instead of the regular entry in the midiControllers table.
    /// Equivalent to: perNotePitch
    pub per_note_pitch: bool,

    /// The vibrato settings for the channel.
    /// Equivalent to: customVibrato
    pub custom_vibrato: ChannelVibrato,

    /// Parameters for each drum instrument.
    /// Equivalent to: drumParams
    pub drum_params: Vec<DrumParameter>,

    /// Indicates whether this channel is a drum channel.
    /// Equivalent to: drumChannel
    pub drum_channel: bool,

    /// The channel's number (0-based index).
    /// Equivalent to: channel
    pub channel: u8,
}

impl MidiChannel {
    /// Equivalent to: getChannelSnapshot() (MIDIChannel.getSnapshot)
    pub fn get_snapshot(&self) -> ChannelSnapshot {
        ChannelSnapshot {
            patch: self.preset.as_ref().map(|preset| MidiPatchFull {
                patch: self.patch,
                name: preset.name.clone(),
                // Equivalent to: isDrum: this.preset.isDrum
                // (the channel's drum flag tracks preset.isDrum on program change)
                is_drum: self.drum_channel,
            }),
            locked_system: self.locked_system,

            midi_controllers: self.midi_controllers,
            locked_controllers: self.locked_controllers.clone(),
            pitch_wheels: self.pitch_wheels,
            generators: ChannelGenerators {
                offsets: self.generator_offsets,
                offsets_enabled: self.generator_offsets_enabled,
                overrides: self.generator_overrides,
                overrides_enabled: self.generator_overrides_enabled,
            },

            midi_parameters: self.midi_parameters.clone(),
            locked_midi_parameters: self.locked_midi_parameters,
            system_parameters: self.system_parameters.clone(),
            octave_tuning: self.channel_octave_tuning,
            per_note_pitch: self.per_note_pitch,

            custom_vibrato: self.channel_vibrato.clone(),

            drum_params: self.drum_params.clone(),
            drum_channel: self.drum_channel,
            channel: self.channel,
        }
    }

    /// Equivalent to: applySnapshot(snapshot) (MIDIChannel.applySnapshot)
    #[allow(clippy::too_many_arguments)]
    pub fn apply_snapshot(
        &mut self,
        snapshot: &ChannelSnapshot,
        voices: &mut [Voice],
        sound_bank_manager: &SoundBankManager,
        current_system: MIDISystem,
        locks: DrumLocks,
        enable_event_system: bool,
        current_time: f64,
    ) -> Vec<SynthProcessorEvent> {
        self.midi_controllers = snapshot.midi_controllers;
        for i in 0..CONTROLLER_TABLE_SIZE {
            self.lock_controller(i, snapshot.locked_controllers[i]);
        }

        self.pitch_wheels = snapshot.pitch_wheels;
        self.channel_octave_tuning = snapshot.octave_tuning;

        self.per_note_pitch = snapshot.per_note_pitch;

        self.channel_vibrato.rate = snapshot.custom_vibrato.rate;
        self.channel_vibrato.delay = snapshot.custom_vibrato.delay;
        self.channel_vibrato.depth = snapshot.custom_vibrato.depth;

        self.generator_offsets = snapshot.generators.offsets;
        self.generator_overrides = snapshot.generators.overrides;
        self.generator_offsets_enabled = snapshot.generators.offsets_enabled;
        self.generator_overrides_enabled = snapshot.generators.overrides_enabled;

        for i in 0..128 {
            DrumParameterUtils::copy_into(&snapshot.drum_params[i], &mut self.drum_params[i]);
        }

        // Disable to set patch
        // Restored in system params
        self.set_system_parameter(ChannelSystemParameterValue::PresetLock(false), voices, current_time);
        let mut events = Vec::new();
        if let Some(patch) = &snapshot.patch {
            self.set_bank_msb(patch.patch.bank_msb);
            self.set_bank_lsb(patch.patch.bank_lsb);
            self.set_is_gm_gs_drum(patch.patch.is_gm_gs_drum);
            events = self.program_change(
                patch.patch.program,
                sound_bank_manager,
                current_system,
                locks,
                enable_event_system,
            );
            // Fallback if no preset matched and the flag didn't sync
            self.set_drum_flag(snapshot.drum_channel);
        } else {
            self.set_drum_flag(snapshot.drum_channel);
        }
        self.locked_system = snapshot.locked_system;

        // Restore MIDI parameters
        // Unlock them first
        self.locked_midi_parameters = [false; CHANNEL_MIDI_PARAMETER_COUNT];

        // Then set
        for value in snapshot.midi_parameters.entries() {
            let key = value.key();
            self.set_midi_parameter(value);
            // setMIDIParameter recomputes these two (see `set_midi_parameter`)
            match key {
                ChannelMidiParameterKey::PitchWheel => {
                    self.compute_modulators_all_impl(voices, 0, modulator_sources::PITCH_WHEEL as usize);
                }
                ChannelMidiParameterKey::Pressure => {
                    self.compute_modulators_all_impl(
                        voices,
                        0,
                        modulator_sources::CHANNEL_PRESSURE as usize,
                    );
                }
                _ => {}
            }
        }

        // Then re-lock!
        self.locked_midi_parameters = snapshot.locked_midi_parameters;

        // Restore system parameters last
        for value in snapshot.system_parameters.entries() {
            self.set_system_parameter(value, voices, current_time);
        }

        // The controllers were written directly, update the playing voices
        self.compute_modulators_all_impl(voices, -1, 0);
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthesizer::audio_engine::channel::parameters::midi::ChannelMidiParameterValue;
    use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
    use crate::synthesizer::types::SynthProcessorOptions;

    fn make_core() -> SynthesizerCore {
        let mut core = SynthesizerCore::new(|_| {}, 44100.0, SynthProcessorOptions::default());
        core.create_midi_channel(false);
        core.create_midi_channel(false);
        core
    }

    fn apply(core: &mut SynthesizerCore, channel: usize, snapshot: &ChannelSnapshot) {
        let current_system = core.midi_parameters.system;
        let locks = DrumLocks::of(&core.system_parameters);
        let current_time = core.current_time;
        core.midi_channels[channel].apply_snapshot(
            snapshot,
            &mut core.voices,
            &core.sound_bank_manager,
            current_system,
            locks,
            false,
            current_time,
        );
    }

    #[test]
    fn test_get_snapshot_without_preset_has_no_patch() {
        let core = make_core();
        let snap = core.midi_channels[1].get_snapshot();
        assert!(snap.patch.is_none());
        assert_eq!(snap.channel, 1);
    }

    #[test]
    fn test_round_trip_restores_parameters_and_tables() {
        let mut core = make_core();
        {
            let ch = &mut core.midi_channels[0];
            ch.midi_controllers[7] = 100 << 7;
            ch.locked_controllers[7] = true;
            ch.pitch_wheels[60] = 1234;
            ch.per_note_pitch = true;
            ch.channel_octave_tuning[3] = -5;
            ch.channel_vibrato.depth = 30.0;
            ch.generator_offsets[8] = 300;
            ch.generator_offsets_enabled = true;
            ch.drum_params[36].pitch_fine = 7.0;
            ch.set_midi_parameter(ChannelMidiParameterValue::PitchWheelRange(12.0));
            ch.set_midi_parameter(ChannelMidiParameterValue::FineTune(-20.0));
            ch.lock_midi_parameter(ChannelMidiParameterKey::FineTune, true);
            ch.set_system_parameter(ChannelSystemParameterValue::Gain(0.5), &mut [], 0.0);
        }
        let snap = core.midi_channels[0].get_snapshot();
        apply(&mut core, 1, &snap);

        let ch = &core.midi_channels[1];
        assert_eq!(ch.midi_controllers[7], 100 << 7);
        assert!(ch.locked_controllers[7]);
        assert_eq!(ch.pitch_wheels[60], 1234);
        assert!(ch.per_note_pitch);
        assert_eq!(ch.channel_octave_tuning[3], -5);
        assert_eq!(ch.channel_vibrato.depth, 30.0);
        assert_eq!(ch.generator_offsets[8], 300);
        assert!(ch.generator_offsets_enabled);
        assert_eq!(ch.drum_params[36].pitch_fine, 7.0);
        assert_eq!(ch.midi_parameters, snap.midi_parameters);
        assert!(ch.locked_midi_parameters[ChannelMidiParameterKey::FineTune as usize]);
        assert_eq!(ch.system_parameters.gain, 0.5);
    }

    /// The locks are cleared before the parameters are set and restored afterwards, so a
    /// locked parameter on the target channel does not block the restore.
    #[test]
    fn test_apply_overrides_target_locks() {
        let mut core = make_core();
        core.midi_channels[0].set_midi_parameter(ChannelMidiParameterValue::KeyShift(3.0));
        let snap = core.midi_channels[0].get_snapshot();
        core.midi_channels[1].lock_midi_parameter(ChannelMidiParameterKey::KeyShift, true);
        apply(&mut core, 1, &snap);
        let ch = &core.midi_channels[1];
        assert_eq!(ch.midi_parameters.key_shift, 3.0);
        assert!(!ch.locked_midi_parameters[ChannelMidiParameterKey::KeyShift as usize]);
    }

    /// The system parameters are restored last: `presetLock` records the current MIDI
    /// system as the locked system, overwriting the snapshot's `lockedSystem`.
    #[test]
    fn test_preset_lock_restored_last_records_current_system() {
        let mut core = make_core();
        core.midi_channels[0].locked_system = MIDISystem::Xg;
        core.midi_channels[0].system_parameters.preset_lock = true;
        let snap = core.midi_channels[0].get_snapshot();
        apply(&mut core, 1, &snap);
        let ch = &core.midi_channels[1];
        assert!(ch.system_parameters.preset_lock);
        assert_eq!(ch.locked_system, core.midi_parameters.system);
    }
}
