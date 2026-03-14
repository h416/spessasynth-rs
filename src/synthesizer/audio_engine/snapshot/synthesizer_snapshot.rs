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

use crate::synthesizer::audio_engine::effects::chorus::ChorusSnapshot;
use crate::synthesizer::audio_engine::effects::delay::DelaySnapshot;
use crate::synthesizer::audio_engine::effects::reverb::ReverbSnapshot;
use crate::synthesizer::audio_engine::engine_components::key_modifier_manager::KeyModifier;
use crate::synthesizer::audio_engine::snapshot::channel_snapshot::ChannelSnapshot;
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::types::{MasterParameterChangeCallback, MasterParameterType};

/// Snapshot of insertion effect state.
/// Equivalent to: InsertionProcessorSnapshot in types.ts
#[derive(Clone, Debug)]
pub struct InsertionSnapshot {
    /// EFX type (MSB << 8 | LSB).
    pub efx_type: u16,
    /// Parameter cache (255 = unchanged).
    pub params: [u8; 20],
    /// Send levels (0-127 scale).
    pub send_level_to_reverb: u8,
    pub send_level_to_chorus: u8,
    pub send_level_to_delay: u8,
}

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

    /// Reverb effect processor snapshot.
    pub reverb_snapshot: ReverbSnapshot,

    /// Chorus effect processor snapshot.
    pub chorus_snapshot: ChorusSnapshot,

    /// Delay effect processor snapshot.
    pub delay_snapshot: DelaySnapshot,

    /// Insertion effect processor snapshot.
    pub insertion_snapshot: InsertionSnapshot,
}

impl SynthesizerSnapshot {
    /// Creates a new synthesizer snapshot with explicit field values.
    /// Equivalent to: constructor(channelSnapshots, masterParameters, keyMappings)
    pub fn new(
        channel_snapshots: Vec<ChannelSnapshot>,
        master_parameters: MasterParameterType,
        key_mappings: HashMap<(u8, u8), KeyModifier>,
        reverb_snapshot: ReverbSnapshot,
        chorus_snapshot: ChorusSnapshot,
        delay_snapshot: DelaySnapshot,
    ) -> Self {
        Self {
            channel_snapshots,
            master_parameters,
            key_mappings,
            reverb_snapshot,
            chorus_snapshot,
            delay_snapshot,
            insertion_snapshot: InsertionSnapshot {
                efx_type: 0x0000,
                params: [255u8; 20],
                send_level_to_reverb: 40,
                send_level_to_chorus: 0,
                send_level_to_delay: 0,
            },
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
            reverb_snapshot: core.reverb_processor.get_snapshot(),
            chorus_snapshot: core.chorus_processor.get_snapshot(),
            delay_snapshot: core.delay_processor.get_snapshot(),
            insertion_snapshot: InsertionSnapshot {
                efx_type: core.insertion_processor.effect_type(),
                params: core.insertion_params,
                send_level_to_reverb: (core.insertion_processor.send_level_to_reverb() * 127.0).round() as u8,
                send_level_to_chorus: (core.insertion_processor.send_level_to_chorus() * 127.0).round() as u8,
                send_level_to_delay: (core.insertion_processor.send_level_to_delay() * 127.0).round() as u8,
            },
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

        // --- Effect snapshots ---
        let rev = &self.reverb_snapshot;
        core.reverb_processor.set_character(rev.character);
        core.reverb_processor.set_pre_lowpass(rev.pre_lowpass);
        core.reverb_processor.set_level(rev.level);
        core.reverb_processor.set_time(rev.time);
        core.reverb_processor.set_delay_feedback(rev.delay_feedback);
        core.reverb_processor.set_pre_delay_time(rev.pre_delay_time);

        let chr = &self.chorus_snapshot;
        core.chorus_processor.set_level(chr.level);
        core.chorus_processor.set_pre_lowpass(chr.pre_lowpass);
        core.chorus_processor.set_depth(chr.depth);
        core.chorus_processor.set_delay(chr.delay);
        core.chorus_processor.set_send_level_to_delay(chr.send_level_to_delay);
        core.chorus_processor.set_send_level_to_reverb(chr.send_level_to_reverb);
        core.chorus_processor.set_rate(chr.rate);
        core.chorus_processor.set_feedback(chr.feedback);

        let dly = &self.delay_snapshot;
        core.delay_processor.set_level(dly.level);
        core.delay_processor.set_pre_lowpass(dly.pre_lowpass);
        core.delay_processor.set_time_center(dly.time_center);
        core.delay_processor.set_time_ratio_right(dly.time_ratio_right);
        core.delay_processor.set_time_ratio_left(dly.time_ratio_left);
        core.delay_processor.set_level_center(dly.level_center);
        core.delay_processor.set_level_left(dly.level_left);
        core.delay_processor.set_level_right(dly.level_right);
        core.delay_processor.set_feedback(dly.feedback);
        core.delay_processor.set_send_level_to_reverb(dly.send_level_to_reverb);

        // --- Insertion effect snapshot ---
        let ins = &self.insertion_snapshot;
        if let Some(proc) = crate::synthesizer::audio_engine::effects::insertion::create_insertion_processor(ins.efx_type, core.sample_rate) {
            core.insertion_processor = proc;
        } else {
            core.insertion_processor = Box::new(crate::synthesizer::audio_engine::effects::insertion::thru::ThruFx::new(core.sample_rate));
        }
        core.insertion_processor.reset();
        core.insertion_params = ins.params;
        // Restore parameters from cache
        for (i, &p) in ins.params.iter().enumerate() {
            if p != 255 {
                core.insertion_processor.set_parameter((i + 3) as u8, p);
            }
        }
        core.insertion_processor.set_send_level_to_reverb(ins.send_level_to_reverb as f64 / 127.0);
        core.insertion_processor.set_send_level_to_chorus(ins.send_level_to_chorus as f64 / 127.0);
        core.insertion_processor.set_send_level_to_delay(ins.send_level_to_delay as f64 / 127.0);

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

    fn default_effect_snapshots() -> (ReverbSnapshot, ChorusSnapshot, DelaySnapshot) {
        (
            ReverbSnapshot { level: 0, pre_lowpass: 0, character: 0, time: 0, delay_feedback: 0, pre_delay_time: 0 },
            ChorusSnapshot { level: 64, pre_lowpass: 0, depth: 0, delay: 0, send_level_to_delay: 0, send_level_to_reverb: 0, rate: 0, feedback: 0 },
            DelaySnapshot { level: 64, pre_lowpass: 0, time_center: 0, time_ratio_right: 0, time_ratio_left: 0, level_center: 127, level_left: 0, level_right: 0, feedback: 16, send_level_to_reverb: 0 },
        )
    }

    fn test_snap_new(
        channels: Vec<ChannelSnapshot>,
        mp: MasterParameterType,
        km: HashMap<(u8, u8), KeyModifier>,
    ) -> SynthesizerSnapshot {
        let (r, c, d) = default_effect_snapshots();
        SynthesizerSnapshot::new(channels, mp, km, r, c, d)
    }

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
        let snap = test_snap_new(
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
        let snap = test_snap_new(Vec::new(), mp, HashMap::new());
        assert!((snap.master_parameters.master_gain - 2.5).abs() < 1e-9);
    }

    #[test]
    fn test_new_stores_key_mappings() {
        let mut km: HashMap<(u8, u8), KeyModifier> = HashMap::new();
        km.insert((0, 60), KeyModifier::default());
        let snap = test_snap_new(Vec::new(), MasterParameterType::default(), km);
        assert!(snap.key_mappings.contains_key(&(0, 60)));
    }

    // -----------------------------------------------------------------------
    // SynthesizerSnapshot::copy_from — deep copy
    // -----------------------------------------------------------------------

    #[test]
    fn test_copy_from_is_independent_master_gain() {
        let mut snap = test_snap_new(
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
        let mut snap = test_snap_new(
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
        let snap = test_snap_new(Vec::new(), mp, HashMap::new());
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
            test_snap_new(
                Vec::new(),
                MasterParameterType::default(),
                HashMap::new(),
            );
        snap.master_parameters.master_gain = 0.5;
        snap.apply(&mut core);
        assert!((core.master_parameters.master_gain - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_apply_restores_voice_cap() {
        let (mut core, _) = make_core();
        let mut snap =
            test_snap_new(
                Vec::new(),
                MasterParameterType::default(),
                HashMap::new(),
            );
        snap.master_parameters.voice_cap = 100;
        snap.apply(&mut core);
        assert_eq!(core.master_parameters.voice_cap, 100);
    }

    #[test]
    fn test_apply_restores_device_id() {
        let (mut core, _) = make_core();
        let mut snap =
            test_snap_new(
                Vec::new(),
                MasterParameterType::default(),
                HashMap::new(),
            );
        snap.master_parameters.device_id = 10;
        snap.apply(&mut core);
        assert_eq!(core.master_parameters.device_id, 10);
    }

    #[test]
    fn test_apply_restores_reverb_gain() {
        let (mut core, _) = make_core();
        let mut snap =
            test_snap_new(
                Vec::new(),
                MasterParameterType::default(),
                HashMap::new(),
            );
        snap.master_parameters.reverb_gain = 0.3;
        snap.apply(&mut core);
        assert!((core.master_parameters.reverb_gain - 0.3).abs() < 1e-9);
    }

    #[test]
    fn test_apply_restores_chorus_gain() {
        let (mut core, _) = make_core();
        let mut snap =
            test_snap_new(
                Vec::new(),
                MasterParameterType::default(),
                HashMap::new(),
            );
        snap.master_parameters.chorus_gain = 0.7;
        snap.apply(&mut core);
        assert!((core.master_parameters.chorus_gain - 0.7).abs() < 1e-9);
    }

    #[test]
    fn test_apply_restores_black_midi_mode() {
        let (mut core, _) = make_core();
        let mut snap =
            test_snap_new(
                Vec::new(),
                MasterParameterType::default(),
                HashMap::new(),
            );
        snap.master_parameters.black_midi_mode = true;
        snap.apply(&mut core);
        assert!(core.master_parameters.black_midi_mode);
    }

    #[test]
    fn test_apply_restores_monophonic_retrigger_mode() {
        let (mut core, _) = make_core();
        let mut snap =
            test_snap_new(
                Vec::new(),
                MasterParameterType::default(),
                HashMap::new(),
            );
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
            test_snap_new(Vec::new(), MasterParameterType::default(), km);
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
            test_snap_new(
                Vec::new(),
                MasterParameterType::default(),
                HashMap::new(),
            );
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
