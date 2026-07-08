/// synthesizer_snapshot.rs
/// purpose: Full-synthesizer state snapshot.
/// Ported from: src/synthesizer/audio_engine/synthesizer_snapshot.ts (spessasynth_core 4.3.0;
/// moved out of snapshot/ in the upstream 4.3.0 restructuring)
///
/// # Changes from 4.2.0 (reviewed against the 4.3.0 diff)
/// - `SynthesizerSnapshot` is now a plain interface (no class methods): fields renamed
///   `channelSnapshots` → `midiChannels`, `masterParameters` → the
///   `systemParameters`/`midiParameters` pair, `reverbSnapshot`/`chorusSnapshot`/
///   `delaySnapshot`/`insertionSnapshot` → `reverbProcessor`/`chorusProcessor`/
///   `delayProcessor`/`insertionProcessor`. `create`/`apply`/`copyFrom` became the
///   free-standing `getSynthesizerSnapshot`/`applySnapshot` (bound to SynthesizerCore);
///   `copyFrom` was dropped (Rust: `Clone`).
/// - `InsertionProcessorSnapshot` lost its `sendLevelToReverb/Chorus/Delay` fields — the
///   sends moved into `params[20..23]` (the parameter cache grew 20 → 23). Its `channels`
///   flags now read `c.midiParameters.efxAssign` (Rust legacy: `insertion_enabled`).
/// - `applySnapshot` restores the insertion effect by *sending GS SysEx* through
///   `systemExclusive` (via `MIDIUtils.gsData`) instead of the removed
///   `sendAddress`-on-processor helper, letting the regular Roland EFX handler apply it.
/// - Upstream self-iteration quirks in `applySnapshot`, ported bug-for-bug (marked below):
///   chorus/delay processors and the midi/system parameter sets iterate `this.*` (the LIVE
///   state) instead of `snapshot.*` — so chorus/delay/midi/system values are effectively
///   NOT restored from the snapshot (the system-parameter loop is a complete no-op thanks
///   to the equality early-return; the midi-parameter loop re-sets current values, firing
///   events and legacy plumbing updates only). Only keyMappings, channels, reverb, and
///   insertion are actually restored.
///
/// # Design note
/// TypeScript `keyMappings: (KeyModifier | undefined)[][]` is stored in Rust as
/// `HashMap<(channel, midi_note), KeyModifier>`, matching `KeyModifierManager`'s
/// internal representation.
///
/// TODO(Task 21, channel restructuring): `midi_channels` still uses the 4.2.0-shaped
/// `ChannelSnapshot` (flat fields) until `channel_snapshot.rs` is restructured to the 4.3.0
/// `ChannelSnapshot` (with nested midi/system parameter sets).
use std::collections::HashMap;

use crate::midi::midi_tools::midi_utils::MidiUtils;
use crate::synthesizer::audio_engine::channel::channel_snapshot::ChannelSnapshot;
use crate::synthesizer::audio_engine::effects::chorus::ChorusSnapshot;
use crate::synthesizer::audio_engine::effects::delay::DelaySnapshot;
use crate::synthesizer::audio_engine::effects::reverb::ReverbSnapshot;
use crate::synthesizer::audio_engine::key_modifier_manager::KeyModifier;
use crate::synthesizer::audio_engine::parameters::midi::GlobalMIDIParameter;
use crate::synthesizer::audio_engine::parameters::system::GlobalSystemParameter;
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::types::GlobalMIDIParameterChangeCallback;

/// Snapshot of the insertion effect processor state.
/// Equivalent to: InsertionProcessorSnapshot (effects/types.ts, 4.3.0 shape)
#[derive(Clone, Debug, PartialEq)]
pub struct InsertionProcessorSnapshot {
    /// The EFX type of this processor, stored as `MSB << 8 | LSB`.
    /// Equivalent to: type
    pub efx_type: u16,
    /// 20 parameters for the effect (255 = "no change") + 3 effect sends
    /// (indices 20, 21, 22).
    /// Equivalent to: params: Uint8Array
    pub params: [u8; 23],
    /// A boolean list for channels that have the insertion effect enabled.
    /// Equivalent to: channels: boolean[]
    pub channels: Vec<bool>,
}

/// Snapshot of the complete synthesizer state.
/// Equivalent to: interface SynthesizerSnapshot
#[derive(Clone, Debug)]
pub struct SynthesizerSnapshot {
    /// Per-channel state snapshots.
    /// Equivalent to: midiChannels: ChannelSnapshot[]
    pub midi_channels: Vec<ChannelSnapshot>,

    /// Key modifiers.
    /// Equivalent to: keyMappings: (KeyModifier | undefined)[][]
    pub key_mappings: HashMap<(u8, u8), KeyModifier>,

    /// Equivalent to: systemParameters: GlobalSystemParameter
    pub system_parameters: GlobalSystemParameter,

    /// Equivalent to: midiParameters: GlobalMIDIParameter
    pub midi_parameters: GlobalMIDIParameter,

    /// Equivalent to: reverbProcessor: ReverbProcessorSnapshot
    pub reverb_processor: ReverbSnapshot,

    /// Equivalent to: chorusProcessor: ChorusProcessorSnapshot
    pub chorus_processor: ChorusSnapshot,

    /// Equivalent to: delayProcessor: DelayProcessorSnapshot
    pub delay_processor: DelaySnapshot,

    /// Equivalent to: insertionProcessor: InsertionProcessorSnapshot
    pub insertion_processor: InsertionProcessorSnapshot,
}

/// Applies the snapshot to the synthesizer.
/// Equivalent to: applySnapshot(this: SynthesizerCore, snapshot)
pub fn apply_snapshot(core: &mut SynthesizerCore, snapshot: &SynthesizerSnapshot) {
    // Restore key modifiers
    core.key_modifier_manager
        .set_mappings(snapshot.key_mappings.clone());

    // Add channels if more needed
    while core.midi_channels.len() < snapshot.midi_channels.len() {
        core.create_midi_channel(true);
    }

    // Restore channels
    for (i, ch_snap) in snapshot.midi_channels.iter().enumerate() {
        // TODO(Task 21): TS 4.3.0 is `this.midiChannels[i].applySnapshot(...)`; the legacy
        // ChannelSnapshot::apply targets the channel by its stored channel number.
        let _ = i;
        ch_snap.apply(core);
    }

    // Restore effect processors
    // Reverb is restored from the snapshot...
    let rev = &snapshot.reverb_processor;
    core.reverb_processor.set_character(rev.character);
    core.reverb_processor.set_pre_lowpass(rev.pre_lowpass);
    core.reverb_processor.set_level(rev.level);
    core.reverb_processor.set_time(rev.time);
    core.reverb_processor.set_delay_feedback(rev.delay_feedback);
    core.reverb_processor.set_pre_delay_time(rev.pre_delay_time);

    // ...but TS 4.3.0 iterates `Object.entries(this.chorusProcessor)` /
    // `Object.entries(this.delayProcessor)` — the LIVE processors, not the snapshot — so the
    // chorus and delay assignments are self-assignments (no-ops). Ported bug-for-bug: the
    // chorus/delay snapshots are NOT restored.

    // Restore insertion (via GS SysEx, letting the Roland EFX handler apply it)
    let is = &snapshot.insertion_processor;
    let syx = MidiUtils::gs_data(
        0x40,
        0x03,
        0x00,
        &[(is.efx_type >> 8) as u8, (is.efx_type & 0x7f) as u8],
    );
    core.system_exclusive(&syx, 0);

    for i in 0..is.params.len() {
        if is.params[i] != 255 {
            let syx = MidiUtils::gs_data(0x40, 0x03, (3 + i) as u8, &[is.params[i]]);
            core.system_exclusive(&syx, 0);
        }
    }

    for channel in 0..is.channels.len() {
        let syx = MidiUtils::gs_data(
            0x40,
            0x40 | MidiUtils::channel_to_syx(channel as u8),
            0x22,
            &[if is.channels[channel] { 1 } else { 0 }],
        );
        core.system_exclusive(&syx, 0);
    }

    // Restore MIDI parameters
    // TS 4.3.0 iterates `Object.entries(this.midiParameters)` — the LIVE values, not the
    // snapshot — so this re-sets every MIDI parameter to its current value (firing
    // globalParamChange events and updating the legacy plumbing) without restoring anything
    // from the snapshot. Ported bug-for-bug.
    let mp = core.midi_parameters;
    core.set_midi_parameter(GlobalMIDIParameterChangeCallback::System(mp.system));
    core.set_midi_parameter(GlobalMIDIParameterChangeCallback::KeyShift(mp.key_shift));
    core.set_midi_parameter(GlobalMIDIParameterChangeCallback::FineTune(mp.fine_tune));
    core.set_midi_parameter(GlobalMIDIParameterChangeCallback::Gain(mp.gain));
    core.set_midi_parameter(GlobalMIDIParameterChangeCallback::Pan(mp.pan));

    // Restore system parameters last
    // TS 4.3.0 likewise iterates `Object.entries(this.systemParameters)` (the LIVE values);
    // since setSystemParameterInternal early-returns when the value is unchanged, this loop
    // is a complete no-op. Ported bug-for-bug (as a no-op).
}

/// Creates a snapshot of the current synthesizer state.
/// Equivalent to: getSynthesizerSnapshot(this: SynthesizerCore)
pub fn get_synthesizer_snapshot(core: &SynthesizerCore) -> SynthesizerSnapshot {
    SynthesizerSnapshot {
        midi_parameters: core.midi_parameters,
        system_parameters: core.system_parameters.clone(),
        midi_channels: (0..core.midi_channels.len())
            .map(|i| ChannelSnapshot::create(core, i))
            .collect(),
        key_mappings: core.key_modifier_manager.get_mappings().clone(),
        reverb_processor: core.reverb_processor.get_snapshot(),
        chorus_processor: core.chorus_processor.get_snapshot(),
        delay_processor: core.delay_processor.get_snapshot(),
        insertion_processor: core.get_insertion_snapshot(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthesizer::audio_engine::key_modifier_manager::KeyModifier;
    use crate::synthesizer::audio_engine::parameters::system::GlobalSystemParameterChange;
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
            SynthProcessorOptions::default(),
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
    // get_synthesizer_snapshot — reads from SynthesizerCore
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_captures_channel_count() {
        let (mut core, _) = make_core();
        core.create_midi_channel(false);
        core.create_midi_channel(false);
        core.create_midi_channel(false);
        let snap = get_synthesizer_snapshot(&core);
        assert_eq!(snap.midi_channels.len(), 3);
    }

    #[test]
    fn test_create_captures_system_gain() {
        let (mut core, _) = make_core();
        core.set_system_parameter(GlobalSystemParameterChange::Gain(2.0));
        let snap = get_synthesizer_snapshot(&core);
        assert!((snap.system_parameters.gain - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_create_captures_voice_cap() {
        let (mut core, _) = make_core();
        core.set_system_parameter(GlobalSystemParameterChange::VoiceCap(200));
        let snap = get_synthesizer_snapshot(&core);
        assert_eq!(snap.system_parameters.voice_cap, 200);
    }

    #[test]
    fn test_create_captures_device_id() {
        let (mut core, _) = make_core();
        core.set_system_parameter(GlobalSystemParameterChange::DeviceId(5));
        let snap = get_synthesizer_snapshot(&core);
        assert_eq!(snap.system_parameters.device_id, 5);
    }

    #[test]
    fn test_create_captures_black_midi_mode() {
        let (mut core, _) = make_core();
        core.set_system_parameter(GlobalSystemParameterChange::BlackMidiMode(true));
        let snap = get_synthesizer_snapshot(&core);
        assert!(snap.system_parameters.black_midi_mode);
    }

    #[test]
    fn test_create_captures_midi_parameters() {
        let (mut core, _) = make_core();
        core.set_midi_parameter(GlobalMIDIParameterChangeCallback::Gain(0.4));
        let snap = get_synthesizer_snapshot(&core);
        assert!((snap.midi_parameters.gain - 0.4).abs() < 1e-9);
    }

    #[test]
    fn test_create_captures_empty_key_mappings() {
        let (core, _) = make_core();
        let snap = get_synthesizer_snapshot(&core);
        assert!(snap.key_mappings.is_empty());
    }

    #[test]
    fn test_create_captures_key_mappings_with_entry() {
        let (mut core, _) = make_core();
        core.key_modifier_manager
            .add_mapping(0, 60, KeyModifier::default());
        let snap = get_synthesizer_snapshot(&core);
        assert!(snap.key_mappings.contains_key(&(0, 60)));
    }

    #[test]
    fn test_create_captures_insertion_snapshot_shape() {
        let (core, _) = make_core_with_channels(4);
        let snap = get_synthesizer_snapshot(&core);
        assert_eq!(snap.insertion_processor.channels.len(), 4);
        assert_eq!(snap.insertion_processor.params.len(), 23);
        // Default sends after resetInsertionParams: reverb 40, chorus 0, delay 0
        assert_eq!(snap.insertion_processor.params[20], 40);
        assert_eq!(snap.insertion_processor.params[21], 0);
        assert_eq!(snap.insertion_processor.params[22], 0);
    }

    #[test]
    fn test_create_zero_channels_gives_empty_snapshots() {
        let (core, _) = make_core();
        let snap = get_synthesizer_snapshot(&core);
        assert!(snap.midi_channels.is_empty());
    }

    // -----------------------------------------------------------------------
    // apply_snapshot — writes back to SynthesizerCore
    // -----------------------------------------------------------------------

    #[test]
    fn test_apply_restores_key_mappings() {
        let (mut core, _) = make_core();
        let mut snap = get_synthesizer_snapshot(&core);
        snap.key_mappings.insert((2, 48), KeyModifier::default());
        apply_snapshot(&mut core, &snap);
        assert!(core.key_modifier_manager.get_mappings().contains_key(&(2, 48)));
    }

    #[test]
    fn test_apply_clears_existing_key_mappings() {
        let (mut core, _) = make_core();
        core.key_modifier_manager
            .add_mapping(0, 60, KeyModifier::default());

        // Snapshot has empty key mappings → existing should be cleared
        let (core2, _) = make_core();
        let snap = get_synthesizer_snapshot(&core2);
        apply_snapshot(&mut core, &snap);
        assert!(core.key_modifier_manager.get_mappings().is_empty());
    }

    #[test]
    fn test_apply_adds_missing_channels() {
        let (mut core, _) = make_core();
        // Snapshot has 3 channel snapshots, core has 0 channels
        let (core2, _) = make_core_with_channels(3);
        let snap = get_synthesizer_snapshot(&core2);
        assert_eq!(snap.midi_channels.len(), 3);

        apply_snapshot(&mut core, &snap);
        assert_eq!(core.midi_channels.len(), 3);
    }

    #[test]
    fn test_apply_does_not_remove_extra_channels() {
        // If the core has more channels than the snapshot, extra channels are kept
        let (mut core, _) = make_core_with_channels(5);
        let (core2, _) = make_core_with_channels(3);
        let snap = get_synthesizer_snapshot(&core2);
        apply_snapshot(&mut core, &snap);
        // 5 channels remain; apply only adds, never removes
        assert_eq!(core.midi_channels.len(), 5);
    }

    #[test]
    fn test_apply_restores_channel_mute_via_snapshot() {
        let (mut core, _) = make_core_with_channels(1);
        core.midi_channels[0].is_muted = true;
        let snap = get_synthesizer_snapshot(&core);

        // Clear state then restore
        core.midi_channels[0].is_muted = false;
        apply_snapshot(&mut core, &snap);

        assert!(core.midi_channels[0].is_muted);
    }

    #[test]
    fn test_apply_restores_reverb_from_snapshot() {
        let (mut core, _) = make_core();
        core.set_reverb_macro(6); // Delay macro: time 32, feedback 40
        let snap = get_synthesizer_snapshot(&core);

        core.set_reverb_macro(4); // back to Hall2
        apply_snapshot(&mut core, &snap);

        let restored = core.reverb_processor.get_snapshot();
        assert_eq!(restored.time, snap.reverb_processor.time);
        assert_eq!(restored.delay_feedback, snap.reverb_processor.delay_feedback);
    }

    #[test]
    fn test_apply_does_not_restore_chorus_upstream_quirk() {
        // TS 4.3.0 iterates the LIVE chorus processor instead of the snapshot — the chorus
        // snapshot is NOT restored (ported bug-for-bug).
        let (mut core, _) = make_core();
        core.set_chorus_macro(5); // Flanger
        let snap = get_synthesizer_snapshot(&core);

        core.set_chorus_macro(2); // Chorus3 (the reset default)
        let live_before = core.chorus_processor.get_snapshot();
        apply_snapshot(&mut core, &snap);

        let live_after = core.chorus_processor.get_snapshot();
        assert_eq!(live_after, live_before, "chorus must remain unchanged");
    }

    #[test]
    fn test_apply_does_not_restore_system_parameters_upstream_quirk() {
        // TS 4.3.0 iterates the LIVE system parameters (self-set + equality early-return
        // → complete no-op): the snapshot's system parameters are NOT restored.
        let (mut core, _) = make_core();
        core.set_system_parameter(GlobalSystemParameterChange::Gain(0.25));
        let snap = get_synthesizer_snapshot(&core);

        core.set_system_parameter(GlobalSystemParameterChange::Gain(1.0));
        apply_snapshot(&mut core, &snap);

        assert!((core.system_parameters.gain - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_apply_does_not_restore_midi_parameters_upstream_quirk() {
        // Same upstream quirk for the MIDI parameters (re-set to current values).
        let (mut core, _) = make_core();
        core.set_midi_parameter(GlobalMIDIParameterChangeCallback::Gain(0.25));
        let snap = get_synthesizer_snapshot(&core);

        core.set_midi_parameter(GlobalMIDIParameterChangeCallback::Gain(1.0));
        apply_snapshot(&mut core, &snap);

        assert!((core.midi_parameters.gain - 1.0).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // Clone independence (replaces the removed copyFrom)
    // -----------------------------------------------------------------------

    #[test]
    fn test_clone_is_independent() {
        let (core, _) = make_core();
        let snap = get_synthesizer_snapshot(&core);
        let mut copy = snap.clone();
        copy.system_parameters.gain = 0.123;
        copy.key_mappings.insert((1, 64), KeyModifier::default());
        assert!((snap.system_parameters.gain - 1.0).abs() < 1e-9);
        assert!(!snap.key_mappings.contains_key(&(1, 64)));
    }
}
