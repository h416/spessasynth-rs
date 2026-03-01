/// synthesizer_snapshot.rs
/// purpose: Full-synthesizer state snapshot.
/// Ported from: src/synthesizer/audio_engine/snapshot/synthesizer_snapshot.ts
///
/// # Design note
/// The original TypeScript `create` and `apply` methods receive a `SpessaSynthProcessor`.
/// In Rust, `MidiChannel` lives inside `SynthesizerCore` and all relevant methods are
/// already implemented there, so we accept `&SynthesizerCore` / `&mut SynthesizerCore`
/// directly, eliminating the dependency on the (not-yet-ported) `processor.rs`.
///
/// TypeScript `keyMappings: (KeyModifier | undefined)[][]` is stored in Rust as
/// `HashMap<(channel, midi_note), KeyModifier>`, matching `KeyModifierManager`'s
/// internal representation.
use std::collections::HashMap;

use crate::synthesizer::audio_engine::engine_components::key_modifier_manager::KeyModifier;
use crate::synthesizer::audio_engine::snapshot::channel_snapshot::ChannelSnapshot;
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::types::{MasterParameterChangeCallback, MasterParameterType};

/// Snapshot of the complete synthesizer state.
/// Equivalent to: class SynthesizerSnapshot
#[derive(Clone, Debug)]
pub struct SynthesizerSnapshot {
    /// Per-channel state snapshots.
    /// Equivalent to: channelSnapshots: ChannelSnapshot[]
    pub channel_snapshots: Vec<ChannelSnapshot>,

    /// Master parameters at snapshot time.
    /// Equivalent to: masterParameters: MasterParameterType
    pub master_parameters: MasterParameterType,

    /// Key modifier mappings at snapshot time.
    /// TypeScript stores `(KeyModifier | undefined)[][]`; Rust uses the same
    /// `HashMap<(channel, midi_note), KeyModifier>` as `KeyModifierManager`.
    /// Equivalent to: keyMappings: (KeyModifier | undefined)[][]
    pub key_mappings: HashMap<(u8, u8), KeyModifier>,
}

impl SynthesizerSnapshot {
    /// Creates a new synthesizer snapshot with explicit field values.
    /// Equivalent to: constructor(channelSnapshots, masterParameters, keyMappings)
    pub fn new(
        channel_snapshots: Vec<ChannelSnapshot>,
        master_parameters: MasterParameterType,
        key_mappings: HashMap<(u8, u8), KeyModifier>,
    ) -> Self {
        Self {
            channel_snapshots,
            master_parameters,
            key_mappings,
        }
    }

    /// Creates a snapshot of the current synthesizer state.
    ///
    /// Equivalent to: static create(processor: SpessaSynthProcessor)
    ///
    /// Uses `&SynthesizerCore` in place of `SpessaSynthProcessor`.
    pub fn create(core: &SynthesizerCore) -> Self {
        let channel_snapshots = (0..core.midi_channels.len())
            .map(|i| ChannelSnapshot::create(core, i))
            .collect();

        Self {
            channel_snapshots,
            master_parameters: core.get_all_master_parameters(),
            key_mappings: core.key_modifier_manager.get_mappings().clone(),
        }
    }

    /// Creates a deep copy of an existing snapshot.
    /// Equivalent to: static copyFrom(snapshot: SynthesizerSnapshot)
    pub fn copy_from(snapshot: &SynthesizerSnapshot) -> Self {
        snapshot.clone()
    }

    /// Applies the snapshot to the synthesizer, restoring all state.
    ///
    /// Equivalent to: apply(processor: SpessaSynthProcessor)
    ///
    /// Order matches the TypeScript original:
    /// 1. Restore every master parameter individually
    /// 2. Restore key modifier mappings
    /// 3. Add channels if the snapshot contains more than currently exist
    /// 4. Restore each channel's state via `ChannelSnapshot::apply`
    pub fn apply(&self, core: &mut SynthesizerCore) {
        // --- Master parameters ---
        // TypeScript iterates Object.entries(masterParameters); we enumerate
        // all fields manually using MasterParameterChangeCallback variants,
        // in the same order as MasterParameterType field declaration.
        let mp = &self.master_parameters;
        core.set_master_parameter(MasterParameterChangeCallback::MasterGain(mp.master_gain));
        core.set_master_parameter(MasterParameterChangeCallback::MasterPan(mp.master_pan));
        core.set_master_parameter(MasterParameterChangeCallback::VoiceCap(mp.voice_cap));
        core.set_master_parameter(MasterParameterChangeCallback::InterpolationType(
            mp.interpolation_type,
        ));
        core.set_master_parameter(MasterParameterChangeCallback::MidiSystem(mp.midi_system));
        core.set_master_parameter(MasterParameterChangeCallback::MonophonicRetriggerMode(
            mp.monophonic_retrigger_mode,
        ));
        core.set_master_parameter(MasterParameterChangeCallback::ReverbGain(mp.reverb_gain));
        core.set_master_parameter(MasterParameterChangeCallback::ChorusGain(mp.chorus_gain));
        core.set_master_parameter(MasterParameterChangeCallback::BlackMidiMode(
            mp.black_midi_mode,
        ));
        core.set_master_parameter(MasterParameterChangeCallback::Transposition(
            mp.transposition,
        ));
        core.set_master_parameter(MasterParameterChangeCallback::DeviceId(mp.device_id));

        // --- Key modifier mappings ---
        core.key_modifier_manager
            .set_mappings(self.key_mappings.clone());

        // --- Channels: add missing ones, then restore each ---
        while core.midi_channels.len() < self.channel_snapshots.len() {
            core.create_midi_channel(false);
        }
        for ch_snap in &self.channel_snapshots {
            ch_snap.apply(core);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthesizer::audio_engine::engine_components::key_modifier_manager::KeyModifier;
    use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
    use crate::synthesizer::types::{SynthProcessorEvent, SynthProcessorOptions};
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

    fn make_core_with_channels(n: usize) -> (SynthesizerCore, Arc<Mutex<Vec<SynthProcessorEvent>>>) {
        let (mut core, events) = make_core();
        for _ in 0..n {
            core.create_midi_channel(false);
        }
        (core, events)
    }

    // -----------------------------------------------------------------------
    // SynthesizerSnapshot::new — field storage
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_stores_channel_snapshots() {
        let snap = SynthesizerSnapshot::new(
            Vec::new(),
            MasterParameterType::default(),
            HashMap::new(),
        );
        assert!(snap.channel_snapshots.is_empty());
    }

    #[test]
    fn test_new_stores_master_parameters() {
        let mut mp = MasterParameterType::default();
        mp.master_gain = 2.5;
        let snap = SynthesizerSnapshot::new(Vec::new(), mp, HashMap::new());
        assert!((snap.master_parameters.master_gain - 2.5).abs() < 1e-9);
    }

    #[test]
    fn test_new_stores_key_mappings() {
        let mut km: HashMap<(u8, u8), KeyModifier> = HashMap::new();
        km.insert((0, 60), KeyModifier::default());
        let snap = SynthesizerSnapshot::new(Vec::new(), MasterParameterType::default(), km);
        assert!(snap.key_mappings.contains_key(&(0, 60)));
    }

    // -----------------------------------------------------------------------
    // SynthesizerSnapshot::copy_from — deep copy
    // -----------------------------------------------------------------------

    #[test]
    fn test_copy_from_is_independent_master_gain() {
        let mut snap = SynthesizerSnapshot::new(
            Vec::new(),
            MasterParameterType::default(),
            HashMap::new(),
        );
        snap.master_parameters.master_gain = 3.0;
        let mut copy = SynthesizerSnapshot::copy_from(&snap);
        copy.master_parameters.master_gain = 0.5;
        assert!((snap.master_parameters.master_gain - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_copy_from_is_independent_key_mappings() {
        let mut snap = SynthesizerSnapshot::new(
            Vec::new(),
            MasterParameterType::default(),
            HashMap::new(),
        );
        snap.key_mappings.insert((1, 64), KeyModifier::default());
        let mut copy = SynthesizerSnapshot::copy_from(&snap);
        copy.key_mappings.clear();
        assert!(snap.key_mappings.contains_key(&(1, 64)));
    }

    #[test]
    fn test_copy_from_preserves_voice_cap() {
        let mut mp = MasterParameterType::default();
        mp.voice_cap = 128;
        let snap = SynthesizerSnapshot::new(Vec::new(), mp, HashMap::new());
        let copy = SynthesizerSnapshot::copy_from(&snap);
        assert_eq!(copy.master_parameters.voice_cap, 128);
    }

    // -----------------------------------------------------------------------
    // SynthesizerSnapshot::create — reads from SynthesizerCore
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_captures_channel_count() {
        let (mut core, _) = make_core();
        core.create_midi_channel(false);
        core.create_midi_channel(false);
        core.create_midi_channel(false);
        let snap = SynthesizerSnapshot::create(&core);
        assert_eq!(snap.channel_snapshots.len(), 3);
    }

    #[test]
    fn test_create_captures_master_gain() {
        let (mut core, _) = make_core();
        core.set_master_parameter(MasterParameterChangeCallback::MasterGain(2.0));
        let snap = SynthesizerSnapshot::create(&core);
        assert!((snap.master_parameters.master_gain - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_create_captures_voice_cap() {
        let (mut core, _) = make_core();
        core.set_master_parameter(MasterParameterChangeCallback::VoiceCap(200));
        let snap = SynthesizerSnapshot::create(&core);
        assert_eq!(snap.master_parameters.voice_cap, 200);
    }

    #[test]
    fn test_create_captures_device_id() {
        let (mut core, _) = make_core();
        core.set_master_parameter(MasterParameterChangeCallback::DeviceId(5));
        let snap = SynthesizerSnapshot::create(&core);
        assert_eq!(snap.master_parameters.device_id, 5);
    }

    #[test]
    fn test_create_captures_black_midi_mode() {
        let (mut core, _) = make_core();
        core.set_master_parameter(MasterParameterChangeCallback::BlackMidiMode(true));
        let snap = SynthesizerSnapshot::create(&core);
        assert!(snap.master_parameters.black_midi_mode);
    }

    #[test]
    fn test_create_captures_empty_key_mappings() {
        let (core, _) = make_core();
        let snap = SynthesizerSnapshot::create(&core);
        assert!(snap.key_mappings.is_empty());
    }

    #[test]
    fn test_create_captures_key_mappings_with_entry() {
        let (mut core, _) = make_core();
        core.key_modifier_manager
            .add_mapping(0, 60, KeyModifier::default());
        let snap = SynthesizerSnapshot::create(&core);
        assert!(snap.key_mappings.contains_key(&(0, 60)));
    }

    #[test]
    fn test_create_zero_channels_gives_empty_snapshots() {
        let (core, _) = make_core();
        let snap = SynthesizerSnapshot::create(&core);
        assert!(snap.channel_snapshots.is_empty());
    }

    // -----------------------------------------------------------------------
    // SynthesizerSnapshot::apply — writes back to SynthesizerCore
    // -----------------------------------------------------------------------

    #[test]
    fn test_apply_restores_master_gain() {
        let (mut core, _) = make_core();
        let mut snap =
            SynthesizerSnapshot::new(Vec::new(), MasterParameterType::default(), HashMap::new());
        snap.master_parameters.master_gain = 0.5;
        snap.apply(&mut core);
        assert!((core.master_parameters.master_gain - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_apply_restores_voice_cap() {
        let (mut core, _) = make_core();
        let mut snap =
            SynthesizerSnapshot::new(Vec::new(), MasterParameterType::default(), HashMap::new());
        snap.master_parameters.voice_cap = 100;
        snap.apply(&mut core);
        assert_eq!(core.master_parameters.voice_cap, 100);
    }

    #[test]
    fn test_apply_restores_device_id() {
        let (mut core, _) = make_core();
        let mut snap =
            SynthesizerSnapshot::new(Vec::new(), MasterParameterType::default(), HashMap::new());
        snap.master_parameters.device_id = 10;
        snap.apply(&mut core);
        assert_eq!(core.master_parameters.device_id, 10);
    }

    #[test]
    fn test_apply_restores_reverb_gain() {
        let (mut core, _) = make_core();
        let mut snap =
            SynthesizerSnapshot::new(Vec::new(), MasterParameterType::default(), HashMap::new());
        snap.master_parameters.reverb_gain = 0.3;
        snap.apply(&mut core);
        assert!((core.master_parameters.reverb_gain - 0.3).abs() < 1e-9);
    }

    #[test]
    fn test_apply_restores_chorus_gain() {
        let (mut core, _) = make_core();
        let mut snap =
            SynthesizerSnapshot::new(Vec::new(), MasterParameterType::default(), HashMap::new());
        snap.master_parameters.chorus_gain = 0.7;
        snap.apply(&mut core);
        assert!((core.master_parameters.chorus_gain - 0.7).abs() < 1e-9);
    }

    #[test]
    fn test_apply_restores_black_midi_mode() {
        let (mut core, _) = make_core();
        let mut snap =
            SynthesizerSnapshot::new(Vec::new(), MasterParameterType::default(), HashMap::new());
        snap.master_parameters.black_midi_mode = true;
        snap.apply(&mut core);
        assert!(core.master_parameters.black_midi_mode);
    }

    #[test]
    fn test_apply_restores_monophonic_retrigger_mode() {
        let (mut core, _) = make_core();
        let mut snap =
            SynthesizerSnapshot::new(Vec::new(), MasterParameterType::default(), HashMap::new());
        snap.master_parameters.monophonic_retrigger_mode = true;
        snap.apply(&mut core);
        assert!(core.master_parameters.monophonic_retrigger_mode);
    }

    #[test]
    fn test_apply_restores_key_mappings() {
        let (mut core, _) = make_core();
        let mut km: HashMap<(u8, u8), KeyModifier> = HashMap::new();
        km.insert((2, 48), KeyModifier::default());
        let snap =
            SynthesizerSnapshot::new(Vec::new(), MasterParameterType::default(), km);
        snap.apply(&mut core);
        assert!(core.key_modifier_manager.get_mappings().contains_key(&(2, 48)));
    }

    #[test]
    fn test_apply_clears_existing_key_mappings() {
        let (mut core, _) = make_core();
        core.key_modifier_manager
            .add_mapping(0, 60, KeyModifier::default());

        // Snapshot has empty key mappings → existing should be cleared
        let snap =
            SynthesizerSnapshot::new(Vec::new(), MasterParameterType::default(), HashMap::new());
        snap.apply(&mut core);
        assert!(core.key_modifier_manager.get_mappings().is_empty());
    }

    #[test]
    fn test_apply_adds_missing_channels() {
        let (mut core, _) = make_core();
        // Snapshot has 3 channel snapshots, core has 0 channels
        let (core2, _) = make_core_with_channels(3);
        let snap = SynthesizerSnapshot::create(&core2);
        assert_eq!(snap.channel_snapshots.len(), 3);

        snap.apply(&mut core);
        assert_eq!(core.midi_channels.len(), 3);
    }

    #[test]
    fn test_apply_does_not_remove_extra_channels() {
        // If the core has more channels than the snapshot, extra channels are kept
        let (mut core, _) = make_core_with_channels(5);
        let (core2, _) = make_core_with_channels(3);
        let snap = SynthesizerSnapshot::create(&core2);
        snap.apply(&mut core);
        // 5 channels remain; apply() only adds, never removes
        assert_eq!(core.midi_channels.len(), 5);
    }

    #[test]
    fn test_apply_restores_channel_mute_via_snapshot() {
        let (mut core, _) = make_core_with_channels(1);
        core.midi_channels[0].is_muted = true;
        let snap = SynthesizerSnapshot::create(&core);

        // Clear state then restore
        core.midi_channels[0].is_muted = false;
        snap.apply(&mut core);

        assert!(core.midi_channels[0].is_muted);
    }

    // -----------------------------------------------------------------------
    // create → apply round-trips
    // -----------------------------------------------------------------------

    #[test]
    fn test_round_trip_master_gain() {
        let (mut core, _) = make_core();
        core.set_master_parameter(MasterParameterChangeCallback::MasterGain(0.25));
        let snap = SynthesizerSnapshot::create(&core);

        core.set_master_parameter(MasterParameterChangeCallback::MasterGain(1.0));
        snap.apply(&mut core);

        assert!((core.master_parameters.master_gain - 0.25).abs() < 1e-9);
    }

    #[test]
    fn test_round_trip_voice_cap() {
        let (mut core, _) = make_core();
        core.set_master_parameter(MasterParameterChangeCallback::VoiceCap(128));
        let snap = SynthesizerSnapshot::create(&core);

        core.set_master_parameter(MasterParameterChangeCallback::VoiceCap(350));
        snap.apply(&mut core);

        assert_eq!(core.master_parameters.voice_cap, 128);
    }

    #[test]
    fn test_round_trip_channel_count() {
        let (mut core, _) = make_core_with_channels(4);
        let snap = SynthesizerSnapshot::create(&core);
        assert_eq!(snap.channel_snapshots.len(), 4);

        // Add extra channels, then restore
        core.create_midi_channel(false);
        assert_eq!(core.midi_channels.len(), 5);

        let (mut core2, _) = make_core();
        snap.apply(&mut core2);
        assert_eq!(core2.midi_channels.len(), 4);
    }

    #[test]
    fn test_round_trip_key_mappings() {
        let (mut core, _) = make_core();
        core.key_modifier_manager
            .add_mapping(3, 72, KeyModifier::default());
        let snap = SynthesizerSnapshot::create(&core);

        core.key_modifier_manager.clear_mappings();
        snap.apply(&mut core);

        assert!(core.key_modifier_manager.get_mappings().contains_key(&(3, 72)));
    }

    #[test]
    fn test_round_trip_device_id() {
        let (mut core, _) = make_core();
        core.set_master_parameter(MasterParameterChangeCallback::DeviceId(7));
        let snap = SynthesizerSnapshot::create(&core);

        core.set_master_parameter(MasterParameterChangeCallback::DeviceId(-1));
        snap.apply(&mut core);

        assert_eq!(core.master_parameters.device_id, 7);
    }
}
