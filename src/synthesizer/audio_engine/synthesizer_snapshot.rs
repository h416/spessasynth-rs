//! synthesizer_snapshot.rs
//! purpose: Full-synthesizer state snapshot.
//! Ported from: src/synthesizer/audio_engine/synthesizer_snapshot.ts
//!
//! `applySnapshot`/`getSynthesizerSnapshot` are bound to `SpessaSynthProcessor` in TS;
//! Rust binds them to `SynthesizerCore`, which owns the channels and the effects.
use crate::midi::drum_parameters::{DrumParameterUtils, UserDrumSetParameter};
use crate::midi::midi_tools::midi_utils::MidiUtils;
use crate::synthesizer::audio_engine::channel::channel_snapshot::ChannelSnapshot;
use crate::synthesizer::audio_engine::channel::midi_channel::DrumLocks;
use crate::synthesizer::audio_engine::effects::gs::implementation::chorus::ChorusSnapshot;
use crate::synthesizer::audio_engine::effects::gs::implementation::delay::DelaySnapshot;
use crate::synthesizer::audio_engine::effects::gs::implementation::insertion::GSInsertionProcessorSnapshot;
use crate::synthesizer::audio_engine::effects::gs::implementation::reverb::ReverbSnapshot;
use crate::synthesizer::audio_engine::effects::xg::implementation::chorus::XGChorusBlockSnapshot;
use crate::synthesizer::audio_engine::effects::xg::implementation::insertion::XGInsertionBlockSnapshot;
use crate::synthesizer::audio_engine::effects::xg::implementation::system_effect::XGSystemEffectBlockSnapshot;
use crate::synthesizer::audio_engine::effects::xg::implementation::variation::XGVariationBlockSnapshot;
use crate::synthesizer::audio_engine::parameters::midi::{
    GLOBAL_MIDI_PARAMETER_COUNT, GlobalMIDIParameter,
};
use crate::synthesizer::audio_engine::parameters::system::GlobalSystemParameter;
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;

/// This interface is a snapshot of a `SpessaSynthProcessor`,
/// capturing its current state, which can be saved and restored.
///
/// This can be useful for creating a different processor
/// (for example, for rendering to an audio file)
/// and copying the current processor's state.
/// Equivalent to: interface SynthesizerSnapshot
#[derive(Clone, Debug)]
pub struct SynthesizerSnapshot {
    /// The snapshots of all MIDI channels of the synth.
    /// Equivalent to: midiChannels
    pub midi_channels: Vec<ChannelSnapshot>,

    /// All Global MIDI Parameters of the synthesizer.
    /// Equivalent to: midiParameters
    pub midi_parameters: GlobalMIDIParameter,
    /// Locks of all Global MIDI Parameters of the synthesizer, indexed by
    /// `GlobalMidiParameterKey`.
    /// Equivalent to: lockedMIDIParameters
    pub locked_midi_parameters: [bool; GLOBAL_MIDI_PARAMETER_COUNT],
    /// All Global System Parameters of the synthesizer.
    /// Equivalent to: systemParameters
    pub system_parameters: GlobalSystemParameter,

    /// A snapshot of the reverb processor.
    /// Equivalent to: gsReverbProcessor
    pub gs_reverb_processor: ReverbSnapshot,
    /// A snapshot of the chorus processor.
    /// Equivalent to: gsChorusProcessor
    pub gs_chorus_processor: ChorusSnapshot,
    /// A snapshot of the delay processor.
    /// Equivalent to: gsDelayProcessor
    pub gs_delay_processor: DelaySnapshot,
    /// A snapshot of the insertion effect processor.
    /// Equivalent to: gsInsertionProcessor
    pub gs_insertion_processor: GSInsertionProcessorSnapshot,

    /// A snapshot of the XG reverb block.
    /// Equivalent to: xgReverbBlock
    pub xg_reverb_block: XGSystemEffectBlockSnapshot,
    /// A snapshot of the XG chorus block.
    /// Equivalent to: xgChorusBlock
    pub xg_chorus_block: XGChorusBlockSnapshot,
    /// A snapshot of the XG variation block.
    /// Equivalent to: xgVariationBlock
    pub xg_variation_block: XGVariationBlockSnapshot,
    /// Snapshots of the XG insertion blocks.
    /// Equivalent to: xgInsertionBlocks
    pub xg_insertion_blocks: Vec<XGInsertionBlockSnapshot>,

    /// A snapshot of the User Drum Set parameters.
    /// Equivalent to: userDrumSets
    pub user_drum_sets: Vec<Vec<UserDrumSetParameter>>,
}

/// Equivalent to: applySnapshot(snapshot)
pub fn apply_snapshot(core: &mut SynthesizerCore, snapshot: &SynthesizerSnapshot) {
    // Add channels if more needed
    while core.midi_channels.len() < snapshot.midi_channels.len() {
        core.create_midi_channel(true);
    }

    // Restore channels
    for i in 0..snapshot.midi_channels.len() {
        let current_system = core.midi_parameters.system;
        let locks = DrumLocks::of(&core.system_parameters);
        let enable_event_system = core.system_parameters.events_enabled;
        let current_time = core.current_time;
        let events = core.midi_channels[i].apply_snapshot(
            &snapshot.midi_channels[i],
            &mut core.voices,
            &core.sound_bank_manager,
            current_system,
            locks,
            enable_event_system,
            current_time,
        );
        for ev in events {
            core.call_event(ev);
        }
    }
    // Activate the restored receive channel routing
    core.custom_channel_numbers = core
        .midi_channels
        .iter()
        .any(|c| c.midi_parameters.rx_channel != c.channel as i32);

    // Restore effect processors
    // (Object.entries follows the key order of each processor's getSnapshot)
    let rev = &snapshot.gs_reverb_processor;
    core.reverb_processor.set_level(rev.level);
    core.reverb_processor.set_pre_lowpass(rev.pre_lowpass);
    core.reverb_processor.set_character(rev.character);
    core.reverb_processor.set_time(rev.time);
    core.reverb_processor.set_delay_feedback(rev.delay_feedback);
    core.reverb_processor.set_pre_delay_time(rev.pre_delay_time);
    let cho = &snapshot.gs_chorus_processor;
    core.chorus_processor.set_pre_lowpass(cho.pre_lowpass);
    core.chorus_processor.set_depth(cho.depth);
    core.chorus_processor.set_delay(cho.delay);
    core.chorus_processor.set_send_level_to_delay(cho.send_level_to_delay);
    core.chorus_processor.set_send_level_to_reverb(cho.send_level_to_reverb);
    core.chorus_processor.set_rate(cho.rate);
    core.chorus_processor.set_feedback(cho.feedback);
    core.chorus_processor.set_level(cho.level);
    let dly = &snapshot.gs_delay_processor;
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

    // Restore insertion
    // Call the handler directly so the device ID filter and the port offset do not apply
    let is = &snapshot.gs_insertion_processor;
    core.handle_gs(
        &MidiUtils::gs_data(0x40, 0x03, 0x00, &[(is.r#type >> 8) as u8, (is.r#type & 0x7f) as u8]),
        0,
    );

    for i in 0..is.params.len() {
        if is.params[i] != 255 {
            core.handle_gs(&MidiUtils::gs_data(0x40, 0x03, (3 + i) as u8, &[is.params[i]]), 0);
        }
    }

    // Restore XG effects
    core.xg_reverb_block.apply_snapshot(&snapshot.xg_reverb_block);
    core.xg_chorus_block.apply_snapshot(&snapshot.xg_chorus_block);
    core.xg_variation_block.apply_snapshot(&snapshot.xg_variation_block);
    for i in 0..core.xg_insertion_blocks.len() {
        let insertion = &mut core.xg_insertion_blocks[i];
        insertion.apply_snapshot(&snapshot.xg_insertion_blocks[i]);
    }

    // Restore user drum sets
    for (drum_set, user_drum_set) in snapshot.user_drum_sets.iter().enumerate() {
        for (midi_note, param) in user_drum_set.iter().enumerate() {
            DrumParameterUtils::copy_into_user(
                param,
                &mut core.sound_bank_manager.user_drum_sets[drum_set].key_params[midi_note],
            );
        }
    }

    // Restore MIDI parameters
    // Unlock them first
    core.locked_midi_parameters = [false; GLOBAL_MIDI_PARAMETER_COUNT];

    // Then set
    for value in snapshot.midi_parameters.entries() {
        core.set_midi_parameter(value);
    }

    // Then re-lock!
    core.locked_midi_parameters = snapshot.locked_midi_parameters;

    // Restore system parameters last
    for value in snapshot.system_parameters.entries() {
        core.set_system_parameter(value);
    }

    // Then update active effects
    core.update_active_gs_effects();
}

/// Equivalent to: getSynthesizerSnapshot()
pub fn get_synthesizer_snapshot(core: &SynthesizerCore) -> SynthesizerSnapshot {
    SynthesizerSnapshot {
        midi_parameters: core.midi_parameters,
        locked_midi_parameters: core.locked_midi_parameters,
        system_parameters: core.system_parameters.clone(),
        midi_channels: core.midi_channels.iter().map(|c| c.get_snapshot()).collect(),
        gs_reverb_processor: core.reverb_processor.get_snapshot(),
        gs_chorus_processor: core.chorus_processor.get_snapshot(),
        gs_delay_processor: core.delay_processor.get_snapshot(),
        gs_insertion_processor: core.get_gs_insertion_snapshot(),
        xg_reverb_block: core.xg_reverb_block.get_snapshot(),
        xg_chorus_block: core.xg_chorus_block.get_snapshot(),
        xg_variation_block: core.xg_variation_block.get_snapshot(),
        xg_insertion_blocks: core.xg_insertion_blocks.iter().map(|i| i.get_snapshot()).collect(),
        user_drum_sets: core
            .sound_bank_manager
            .user_drum_sets
            .iter()
            .map(|d| d.get_snapshot())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthesizer::audio_engine::channel::parameters::midi::ChannelMidiParameterValue;
    use crate::synthesizer::audio_engine::parameters::midi::GlobalMidiParameterKey;
    use crate::synthesizer::audio_engine::parameters::system::GlobalSystemParameterChange;
    use crate::synthesizer::types::{GlobalMIDIParameterChangeCallback, SynthProcessorOptions};

    fn make_core_with_channels(n: usize) -> SynthesizerCore {
        let mut core = SynthesizerCore::new(|_| {}, 44100.0, SynthProcessorOptions::default());
        for _ in 0..n {
            core.create_midi_channel(false);
        }
        core
    }

    #[test]
    fn test_get_captures_channels_and_blocks() {
        let core = make_core_with_channels(3);
        let snap = get_synthesizer_snapshot(&core);
        assert_eq!(snap.midi_channels.len(), 3);
        assert_eq!(snap.xg_insertion_blocks.len(), core.xg_insertion_blocks.len());
        assert_eq!(snap.user_drum_sets.len(), 2);
        // Default sends after resetGSInsertionParams: reverb 40, chorus 0, delay 0
        assert_eq!(snap.gs_insertion_processor.params[20], 40);
        assert_eq!(snap.gs_insertion_processor.params[21], 0);
        assert_eq!(snap.gs_insertion_processor.params[22], 0);
    }

    #[test]
    fn test_apply_adds_missing_channels_and_keeps_extra_ones() {
        let snap = get_synthesizer_snapshot(&make_core_with_channels(3));
        let mut core = make_core_with_channels(0);
        apply_snapshot(&mut core, &snap);
        assert_eq!(core.midi_channels.len(), 3);
        let mut core = make_core_with_channels(5);
        apply_snapshot(&mut core, &snap);
        assert_eq!(core.midi_channels.len(), 5);
    }

    /// Loads the type defaults: the processors' parameters stay zero until the first type
    /// change (TS and Rust alike), and applying zeros clamps them to the parameter minimums.
    fn reset_xg_blocks(core: &mut SynthesizerCore) {
        core.xg_reverb_block.reset();
        core.xg_chorus_block.reset();
        core.xg_variation_block.reset();
    }

    #[test]
    fn test_apply_restores_effects() {
        let mut core = make_core_with_channels(1);
        reset_xg_blocks(&mut core);
        core.set_reverb_macro(6);
        core.set_chorus_macro(5);
        core.xg_chorus_block.send_to_reverb = 33;
        core.xg_variation_block.part_number = 4;
        core.xg_reverb_block.system.r#return = 50;
        let snap = get_synthesizer_snapshot(&core);

        let mut other = make_core_with_channels(1);
        reset_xg_blocks(&mut other);
        apply_snapshot(&mut other, &snap);
        assert_eq!(other.reverb_processor.get_snapshot().time, snap.gs_reverb_processor.time);
        assert_eq!(other.chorus_processor.get_snapshot(), snap.gs_chorus_processor);
        assert_eq!(other.xg_chorus_block.get_snapshot(), snap.xg_chorus_block);
        assert_eq!(other.xg_variation_block.get_snapshot(), snap.xg_variation_block);
        assert_eq!(other.xg_reverb_block.get_snapshot(), snap.xg_reverb_block);
    }

    #[test]
    fn test_apply_restores_global_parameters_and_locks() {
        let mut core = make_core_with_channels(1);
        core.set_midi_parameter(GlobalMIDIParameterChangeCallback::Volume(0.25));
        core.lock_midi_parameter(GlobalMidiParameterKey::Volume, true);
        core.set_system_parameter(GlobalSystemParameterChange::Gain(0.5));
        let snap = get_synthesizer_snapshot(&core);

        let mut other = make_core_with_channels(1);
        other.lock_midi_parameter(GlobalMidiParameterKey::Pan, true);
        apply_snapshot(&mut other, &snap);
        assert_eq!(other.midi_parameters.volume, 0.25);
        assert_eq!(other.system_parameters.gain, 0.5);
        assert_eq!(other.locked_midi_parameters, snap.locked_midi_parameters);
        // A locked parameter ignores changes
        other.set_midi_parameter(GlobalMIDIParameterChangeCallback::Volume(1.0));
        assert_eq!(other.midi_parameters.volume, 0.25);
    }

    #[test]
    fn test_apply_restores_receive_channel_routing() {
        let mut core = make_core_with_channels(2);
        core.midi_channels[1].set_midi_parameter(ChannelMidiParameterValue::RxChannel(0));
        let snap = get_synthesizer_snapshot(&core);
        let mut other = make_core_with_channels(2);
        apply_snapshot(&mut other, &snap);
        assert!(other.custom_channel_numbers);
    }
}
