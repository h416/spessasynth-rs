/// channel_snapshot.rs
/// purpose: Snapshot of a single MIDI channel's state.
/// Ported from: src/synthesizer/audio_engine/snapshot/channel_snapshot.ts
///
/// # Design note
/// The original TypeScript `create` and `apply` methods receive a `SpessaSynthProcessor`.
/// In Rust, `MidiChannel` is unified inside `SynthesizerCore`, so we accept
/// `&SynthesizerCore` / `&mut SynthesizerCore` directly, eliminating the
/// dependency on the (not-yet-ported) `processor.rs`.
use crate::soundbank::basic_soundbank::midi_patch::MidiPatchNamed;
use crate::synthesizer::audio_engine::engine_components::controller_tables::{
    CONTROLLER_TABLE_SIZE, CUSTOM_CONTROLLER_TABLE_SIZE,
};
use crate::synthesizer::audio_engine::engine_components::drum_parameters::DrumParameters;
use crate::synthesizer::audio_engine::synthesizer_core::{ChannelVibrato, SynthesizerCore};
use crate::synthesizer::types::SynthSystem;

/// Snapshot of a single MIDI channel's state.
/// Equivalent to: class ChannelSnapshot
#[derive(Clone, Debug)]
pub struct ChannelSnapshot {
    /// The MIDI patch assigned to the channel (with preset name).
    /// Equivalent to: patch: MIDIPatchNamed
    pub patch: MidiPatchNamed,

    /// Whether the channel's program change is disabled.
    /// Equivalent to: lockPreset
    pub lock_preset: bool,

    /// The MIDI system active when the preset was locked.
    /// Equivalent to: lockedSystem
    pub locked_system: SynthSystem,

    /// Full MIDI controller table (14-bit values).
    /// Equivalent to: midiControllers: Int16Array
    pub midi_controllers: [i16; CONTROLLER_TABLE_SIZE],

    /// Locked controller flags.
    /// Equivalent to: lockedControllers: boolean[]
    pub locked_controllers: Vec<bool>,

    /// Custom (non-SF2) controller values.
    /// Equivalent to: customControllers: Float32Array
    pub custom_controllers: [f32; CUSTOM_CONTROLLER_TABLE_SIZE],

    /// Whether GS NRPN parameters (including custom vibrato) are locked.
    /// Maps to `lockGSNRPNParams` in MidiChannel.
    /// Equivalent to: lockVibrato
    pub lock_vibrato: bool,

    /// Custom vibrato settings for this channel (GS NRPN).
    /// Equivalent to: channelVibrato
    pub channel_vibrato: ChannelVibrato,

    /// Key shift in semitones.
    /// Equivalent to: channelTransposeKeyShift
    pub channel_transpose_key_shift: i16,

    /// Per-note octave tuning (cents, 128 entries).
    /// Equivalent to: channelOctaveTuning: Int8Array
    pub channel_octave_tuning: [i8; 128],

    /// Whether the channel is muted.
    /// Equivalent to: isMuted
    pub is_muted: bool,

    /// Whether the channel is a drum/percussion channel.
    /// Equivalent to: drumChannel
    pub drum_channel: bool,

    /// Zero-based channel index this snapshot represents.
    /// Equivalent to: channelNumber
    pub channel_number: u8,

    /// Per-drum-note parameters (128 entries).
    pub drum_params: Vec<DrumParameters>,

    /// Whether insertion effect routing is enabled for this channel.
    pub insertion_enabled: bool,
}

impl ChannelSnapshot {
    /// Creates a new channel snapshot with explicit field values.
    /// Equivalent to: constructor(...)
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        patch: MidiPatchNamed,
        lock_preset: bool,
        locked_system: SynthSystem,
        midi_controllers: [i16; CONTROLLER_TABLE_SIZE],
        locked_controllers: Vec<bool>,
        custom_controllers: [f32; CUSTOM_CONTROLLER_TABLE_SIZE],
        lock_vibrato: bool,
        channel_vibrato: ChannelVibrato,
        channel_transpose_key_shift: i16,
        channel_octave_tuning: [i8; 128],
        is_muted: bool,
        drum_channel: bool,
        channel_number: u8,
        drum_params: Vec<DrumParameters>,
    ) -> Self {
        Self {
            patch,
            lock_preset,
            locked_system,
            midi_controllers,
            locked_controllers,
            custom_controllers,
            lock_vibrato,
            channel_vibrato,
            channel_transpose_key_shift,
            channel_octave_tuning,
            is_muted,
            drum_channel,
            channel_number,
            drum_params,
            insertion_enabled: false,
        }
    }

    /// Creates a deep copy of an existing snapshot.
    /// Equivalent to: static copyFrom(snapshot: ChannelSnapshot)
    pub fn copy_from(snapshot: &ChannelSnapshot) -> Self {
        snapshot.clone()
    }

    /// Creates a snapshot of the specified channel from the synthesizer state.
    ///
    /// Equivalent to: static create(spessaSynthProcessor, channelNumber)
    ///
    /// The TypeScript original receives a `SpessaSynthProcessor` and accesses
    /// `processor.midiChannels[channelNumber]`. In Rust, `MidiChannel` lives
    /// inside `SynthesizerCore`, so we accept `&SynthesizerCore` directly.
    pub fn create(core: &SynthesizerCore, channel_number: usize) -> Self {
        let ch = &core.midi_channels[channel_number];
        let preset_name = ch
            .preset
            .as_ref()
            .map_or_else(|| "undefined".to_string(), |p| p.name.clone());
        Self {
            patch: MidiPatchNamed {
                patch: ch.patch,
                name: preset_name,
            },
            lock_preset: ch.lock_preset,
            locked_system: ch.locked_system,
            midi_controllers: ch.midi_controllers,
            locked_controllers: ch.locked_controllers.clone(),
            custom_controllers: ch.custom_controllers,
            lock_vibrato: ch.lock_gs_nrpn_params,
            channel_vibrato: ch.channel_vibrato.clone(),
            channel_transpose_key_shift: ch.channel_transpose_key_shift,
            channel_octave_tuning: ch.channel_octave_tuning,
            is_muted: ch.is_muted,
            drum_channel: ch.drum_channel,
            channel_number: channel_number as u8,
            drum_params: ch.drum_params.clone(),
            insertion_enabled: ch.insertion_enabled,
        }
    }

    /// Applies the snapshot to the specified channel in the synthesizer.
    ///
    /// Equivalent to: apply(spessaSynthProcessor: SpessaSynthProcessor)
    ///
    /// The TypeScript original receives a `SpessaSynthProcessor`. In Rust we
    /// accept `&mut SynthesizerCore` directly so that `processor.rs` is not needed.
    pub fn apply(&self, core: &mut SynthesizerCore) {
        let channel_idx = self.channel_number as usize;
        let current_system = core.master_parameters.midi_system;
        let enable_event_system = core.enable_event_system;

        // Restore mute flag (set directly; notes should be stopped by the caller)
        core.midi_channels[channel_idx].is_muted = self.is_muted;

        // Restore drum flag (direct; program_change below will set drum_channel via
        // preset detection, but we also need the flag correct before the call)
        core.midi_channels[channel_idx].drum_channel = self.drum_channel;

        // Restore MIDI controllers
        core.midi_channels[channel_idx].midi_controllers = self.midi_controllers;
        core.midi_channels[channel_idx].locked_controllers = self.locked_controllers.clone();
        core.midi_channels[channel_idx].custom_controllers = self.custom_controllers;
        core.midi_channels[channel_idx].update_channel_tuning();

        // Restore GS NRPN lock, vibrato, and transpose
        core.midi_channels[channel_idx].lock_gs_nrpn_params = self.lock_vibrato;
        core.midi_channels[channel_idx].channel_vibrato = self.channel_vibrato.clone();
        core.midi_channels[channel_idx].channel_transpose_key_shift =
            self.channel_transpose_key_shift;
        core.midi_channels[channel_idx].channel_octave_tuning = self.channel_octave_tuning;

        // Unlock preset so that the upcoming program_change is allowed
        core.midi_channels[channel_idx].lock_preset = false;

        // Restore bank and drum-patch flag (equivalent to setPatch's first three steps)
        let patch = self.patch.patch;
        core.midi_channels[channel_idx].set_bank_msb(patch.bank_msb);
        core.midi_channels[channel_idx].set_bank_lsb(patch.bank_lsb);
        core.midi_channels[channel_idx].set_gs_drums(patch.is_gm_gs_drum);

        // Execute program change (looks up the preset from the sound bank and fires events).
        // Split borrow: `midi_channels[i]` (mut) and `sound_bank_manager` (shared) are
        // separate fields — the same pattern used in handle_xg.rs.
        let events = core.midi_channels[channel_idx].program_change(
            patch.program,
            &core.sound_bank_manager,
            current_system,
            enable_event_system,
        );

        // Restore lock state, bypassing set_preset_lock() so that we can
        // also restore the exact `locked_system` value from the snapshot.
        core.midi_channels[channel_idx].lock_preset = self.lock_preset;
        core.midi_channels[channel_idx].locked_system = self.locked_system;

        // Restore drum parameters
        core.midi_channels[channel_idx].drum_params = self.drum_params.clone();

        // Restore insertion effect assignment
        core.midi_channels[channel_idx].insertion_enabled = self.insertion_enabled;
        if self.insertion_enabled {
            core.insertion_active = true;
        }

        // Dispatch events collected during program_change
        for ev in events {
            core.call_event(ev);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
    use crate::synthesizer::audio_engine::engine_components::controller_tables::CUSTOM_CONTROLLER_TABLE_SIZE;
    use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
    use crate::synthesizer::types::{SynthProcessorEvent, SynthProcessorOptions, SynthSystem};
    use std::sync::{Arc, Mutex};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_core() -> (SynthesizerCore, Arc<Mutex<Vec<SynthProcessorEvent>>>) {
        let events: Arc<Mutex<Vec<SynthProcessorEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let ev_clone = Arc::clone(&events);
        let core = SynthesizerCore::new(
            move |ev| {
                ev_clone.lock().unwrap().push(ev);
            },
            44100.0,
            SynthProcessorOptions {
                enable_event_system: true,
                ..Default::default()
            },
        );
        (core, events)
    }

    fn make_core_with_channel() -> (SynthesizerCore, Arc<Mutex<Vec<SynthProcessorEvent>>>) {
        let (mut core, events) = make_core();
        core.create_midi_channel(false);
        (core, events)
    }

    fn default_patch_named(name: &str) -> MidiPatchNamed {
        MidiPatchNamed {
            patch: MidiPatch {
                program: 0,
                bank_msb: 0,
                bank_lsb: 0,
                is_gm_gs_drum: false,
            },
            name: name.to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // ChannelSnapshot::new — field storage
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_stores_all_fields() {
        let patch = default_patch_named("Piano");
        let midi_ctrl = [1i16; CONTROLLER_TABLE_SIZE];
        let locked = vec![false; CONTROLLER_TABLE_SIZE];
        let custom = [0.5f32; CUSTOM_CONTROLLER_TABLE_SIZE];
        let vibrato = ChannelVibrato {
            delay: 0.1,
            depth: 2.0,
            rate: 5.0,
        };
        let octave = [3i8; 128];

        let snap = ChannelSnapshot::new(
            patch.clone(),
            true,
            SynthSystem::Gm,
            midi_ctrl,
            locked.clone(),
            custom,
            true,
            vibrato.clone(),
            12,
            octave,
            true,
            false,
            3,
            (0..128).map(|_| DrumParameters::default()).collect(),
        );

        assert_eq!(snap.patch.name, "Piano");
        assert!(snap.lock_preset);
        assert_eq!(snap.locked_system, SynthSystem::Gm);
        assert_eq!(snap.midi_controllers[0], 1);
        assert_eq!(snap.locked_controllers.len(), CONTROLLER_TABLE_SIZE);
        assert!((snap.custom_controllers[0] - 0.5).abs() < 1e-6);
        assert!(snap.lock_vibrato);
        assert!((snap.channel_vibrato.delay - 0.1).abs() < 1e-9);
        assert_eq!(snap.channel_transpose_key_shift, 12);
        assert_eq!(snap.channel_octave_tuning[0], 3);
        assert!(snap.is_muted);
        assert!(!snap.drum_channel);
        assert_eq!(snap.channel_number, 3);
    }

    // -----------------------------------------------------------------------
    // ChannelSnapshot::copy_from — deep copy
    // -----------------------------------------------------------------------

    #[test]
    fn test_copy_from_is_independent() {
        let patch = default_patch_named("Violin");
        let snap = ChannelSnapshot::new(
            patch,
            false,
            SynthSystem::Gs,
            [0i16; CONTROLLER_TABLE_SIZE],
            vec![false; CONTROLLER_TABLE_SIZE],
            [0.0f32; CUSTOM_CONTROLLER_TABLE_SIZE],
            false,
            ChannelVibrato::default(),
            0,
            [0i8; 128],
            false,
            false,
            0,
            (0..128).map(|_| DrumParameters::default()).collect(),
        );
        let mut copy = ChannelSnapshot::copy_from(&snap);

        // Mutate the copy; original must be unaffected
        copy.lock_preset = true;
        copy.channel_number = 5;
        copy.midi_controllers[0] = 99;
        copy.locked_controllers[0] = true;

        assert!(!snap.lock_preset);
        assert_eq!(snap.channel_number, 0);
        assert_eq!(snap.midi_controllers[0], 0);
        assert!(!snap.locked_controllers[0]);
    }

    #[test]
    fn test_copy_from_preserves_values() {
        let patch = default_patch_named("Bass");
        let snap = ChannelSnapshot::new(
            patch,
            true,
            SynthSystem::Xg,
            [7i16; CONTROLLER_TABLE_SIZE],
            vec![true; CONTROLLER_TABLE_SIZE],
            [1.0f32; CUSTOM_CONTROLLER_TABLE_SIZE],
            true,
            ChannelVibrato {
                delay: 0.2,
                depth: 3.0,
                rate: 6.0,
            },
            -5,
            [1i8; 128],
            true,
            true,
            9,
            (0..128).map(|_| DrumParameters::default()).collect(),
        );
        let copy = ChannelSnapshot::copy_from(&snap);

        assert_eq!(copy.patch.name, snap.patch.name);
        assert_eq!(copy.lock_preset, snap.lock_preset);
        assert_eq!(copy.locked_system, snap.locked_system);
        assert_eq!(copy.midi_controllers, snap.midi_controllers);
        assert_eq!(copy.locked_controllers, snap.locked_controllers);
        assert_eq!(copy.custom_controllers, snap.custom_controllers);
        assert_eq!(copy.lock_vibrato, snap.lock_vibrato);
        assert!((copy.channel_vibrato.delay - snap.channel_vibrato.delay).abs() < 1e-9);
        assert_eq!(copy.channel_transpose_key_shift, snap.channel_transpose_key_shift);
        assert_eq!(copy.channel_octave_tuning, snap.channel_octave_tuning);
        assert_eq!(copy.is_muted, snap.is_muted);
        assert_eq!(copy.drum_channel, snap.drum_channel);
        assert_eq!(copy.channel_number, snap.channel_number);
    }

    // -----------------------------------------------------------------------
    // ChannelSnapshot::create — reads from SynthesizerCore
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_channel_number() {
        let (mut core, _) = make_core();
        core.create_midi_channel(false);
        core.create_midi_channel(false);
        // Snapshot of channel 1 must store channel_number = 1
        let snap = ChannelSnapshot::create(&core, 1);
        assert_eq!(snap.channel_number, 1);
    }

    #[test]
    fn test_create_preset_name_undefined_when_no_preset() {
        let (mut core, _) = make_core_with_channel();
        // No sound bank is loaded → preset is None
        assert!(core.midi_channels[0].preset.is_none());
        let snap = ChannelSnapshot::create(&core, 0);
        assert_eq!(snap.patch.name, "undefined");
    }

    #[test]
    fn test_create_captures_mute_state() {
        let (mut core, _) = make_core_with_channel();
        core.midi_channels[0].is_muted = true;
        let snap = ChannelSnapshot::create(&core, 0);
        assert!(snap.is_muted);
    }

    #[test]
    fn test_create_captures_drum_channel() {
        let (mut core, _) = make_core_with_channel();
        core.midi_channels[0].drum_channel = true;
        let snap = ChannelSnapshot::create(&core, 0);
        assert!(snap.drum_channel);
    }

    #[test]
    fn test_create_captures_transpose_key_shift() {
        let (mut core, _) = make_core_with_channel();
        core.midi_channels[0].channel_transpose_key_shift = 7;
        let snap = ChannelSnapshot::create(&core, 0);
        assert_eq!(snap.channel_transpose_key_shift, 7);
    }

    #[test]
    fn test_create_captures_lock_preset() {
        let (mut core, _) = make_core_with_channel();
        core.midi_channels[0].lock_preset = true;
        core.midi_channels[0].locked_system = SynthSystem::Xg;
        let snap = ChannelSnapshot::create(&core, 0);
        assert!(snap.lock_preset);
        assert_eq!(snap.locked_system, SynthSystem::Xg);
    }

    #[test]
    fn test_create_captures_lock_vibrato() {
        let (mut core, _) = make_core_with_channel();
        core.midi_channels[0].lock_gs_nrpn_params = true;
        let snap = ChannelSnapshot::create(&core, 0);
        assert!(snap.lock_vibrato);
    }

    #[test]
    fn test_create_captures_vibrato_params() {
        let (mut core, _) = make_core_with_channel();
        core.midi_channels[0].channel_vibrato = ChannelVibrato {
            delay: 1.5,
            depth: 10.0,
            rate: 4.0,
        };
        let snap = ChannelSnapshot::create(&core, 0);
        assert!((snap.channel_vibrato.delay - 1.5).abs() < 1e-9);
        assert!((snap.channel_vibrato.depth - 10.0).abs() < 1e-9);
        assert!((snap.channel_vibrato.rate - 4.0).abs() < 1e-9);
    }

    #[test]
    fn test_create_captures_controllers() {
        let (mut core, _) = make_core_with_channel();
        core.midi_channels[0].midi_controllers[7] = 9000; // main volume (14-bit)
        let snap = ChannelSnapshot::create(&core, 0);
        assert_eq!(snap.midi_controllers[7], 9000);
    }

    #[test]
    fn test_create_captures_locked_controllers() {
        let (mut core, _) = make_core_with_channel();
        core.midi_channels[0].locked_controllers[10] = true;
        let snap = ChannelSnapshot::create(&core, 0);
        assert!(snap.locked_controllers[10]);
    }

    // -----------------------------------------------------------------------
    // ChannelSnapshot::apply — writes back to SynthesizerCore
    // -----------------------------------------------------------------------

    #[test]
    fn test_apply_restores_mute_state() {
        let (mut core, _) = make_core_with_channel();
        core.midi_channels[0].is_muted = false;

        let mut snap = ChannelSnapshot::create(&core, 0);
        snap.is_muted = true;
        snap.apply(&mut core);

        assert!(core.midi_channels[0].is_muted);
    }

    #[test]
    fn test_apply_restores_transpose_key_shift() {
        let (mut core, _) = make_core_with_channel();
        let mut snap = ChannelSnapshot::create(&core, 0);
        snap.channel_transpose_key_shift = -12;
        snap.apply(&mut core);
        assert_eq!(core.midi_channels[0].channel_transpose_key_shift, -12);
    }

    #[test]
    fn test_apply_restores_lock_vibrato() {
        let (mut core, _) = make_core_with_channel();
        let mut snap = ChannelSnapshot::create(&core, 0);
        snap.lock_vibrato = true;
        snap.apply(&mut core);
        assert!(core.midi_channels[0].lock_gs_nrpn_params);
    }

    #[test]
    fn test_apply_restores_vibrato_params() {
        let (mut core, _) = make_core_with_channel();
        let mut snap = ChannelSnapshot::create(&core, 0);
        snap.channel_vibrato = ChannelVibrato {
            delay: 0.3,
            depth: 5.0,
            rate: 2.0,
        };
        snap.apply(&mut core);
        assert!((core.midi_channels[0].channel_vibrato.delay - 0.3).abs() < 1e-9);
        assert!((core.midi_channels[0].channel_vibrato.depth - 5.0).abs() < 1e-9);
        assert!((core.midi_channels[0].channel_vibrato.rate - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_apply_restores_locked_controllers() {
        let (mut core, _) = make_core_with_channel();
        let mut snap = ChannelSnapshot::create(&core, 0);
        snap.locked_controllers[5] = true;
        snap.apply(&mut core);
        assert!(core.midi_channels[0].locked_controllers[5]);
    }

    #[test]
    fn test_apply_restores_midi_controllers() {
        let (mut core, _) = make_core_with_channel();
        let mut snap = ChannelSnapshot::create(&core, 0);
        snap.midi_controllers[7] = 8000;
        snap.apply(&mut core);
        assert_eq!(core.midi_channels[0].midi_controllers[7], 8000);
    }

    #[test]
    fn test_apply_restores_lock_preset_and_locked_system() {
        let (mut core, _) = make_core_with_channel();
        let mut snap = ChannelSnapshot::create(&core, 0);
        snap.lock_preset = true;
        snap.locked_system = SynthSystem::Gm2;
        snap.apply(&mut core);
        assert!(core.midi_channels[0].lock_preset);
        assert_eq!(core.midi_channels[0].locked_system, SynthSystem::Gm2);
    }

    #[test]
    fn test_apply_restores_octave_tuning() {
        let (mut core, _) = make_core_with_channel();
        let mut snap = ChannelSnapshot::create(&core, 0);
        snap.channel_octave_tuning[60] = 10;
        snap.apply(&mut core);
        assert_eq!(core.midi_channels[0].channel_octave_tuning[60], 10);
    }

    // -----------------------------------------------------------------------
    // create → apply round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_apply_round_trip_mute() {
        let (mut core, _) = make_core_with_channel();
        core.midi_channels[0].is_muted = true;
        let snap = ChannelSnapshot::create(&core, 0);

        // Reset, then restore
        core.midi_channels[0].is_muted = false;
        snap.apply(&mut core);

        assert!(core.midi_channels[0].is_muted);
    }

    #[test]
    fn test_create_apply_round_trip_transpose() {
        let (mut core, _) = make_core_with_channel();
        core.midi_channels[0].channel_transpose_key_shift = 5;
        let snap = ChannelSnapshot::create(&core, 0);

        core.midi_channels[0].channel_transpose_key_shift = 0;
        snap.apply(&mut core);

        assert_eq!(core.midi_channels[0].channel_transpose_key_shift, 5);
    }
}
