/// midi.rs (parameters)
/// purpose: Global MIDI parameters of the synthesizer (set via MIDI messages: SysEx master
/// volume/pan/tuning/transpose and GM/GS/XG system changes) — struct, defaults, and the
/// set/reset handlers for SynthesizerCore.
/// Ported from: src/synthesizer/audio_engine/parameters/midi.ts (spessasynth_core 4.3.0)
///
/// New in TS 4.3.0: the single `MasterParameterType` was split into `GlobalMIDIParameter`
/// (this file — parameters editable only via MIDI messages) and `GlobalSystemParameter`
/// (`parameters/system.rs` — parameters editable only via the API).
///
/// TS 4.3.14 renamed `gain` to `volume` and squares it when folding it into the channel's
/// gain (`Math.pow(globalMIDI.volume, 2)` — "it corresponds to CC volume, so volume is
/// squared"). See `SynthesizerCore::set_midi_volume` for where the squaring is applied.
///
/// TODO(Task 21, channel restructuring): TS's `setMIDIParameterInternal` calls
/// `ch.updateInternalParams()` on every channel; that channel method (which folds
/// global MIDI volume/pan/keyShift/fineTune into per-channel gain/pan/tuning) does not exist
/// in the current (pre-4.3.0) Rust channel architecture. Until it lands, each setter below
/// also updates the equivalent legacy plumbing (`midi_volume`, `pan_left`/`pan_right`, the
/// per-channel transpose loop, and the MASTER_TUNING custom controller) so that the audible
/// behavior matches the current 4.2.0-era render path.
use crate::soundbank::types::MIDISystem;
use crate::synthesizer::audio_engine::synth_constants::DEFAULT_SYNTH_MODE;
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::types::{GlobalMIDIParameterChangeCallback, SynthProcessorEvent};

/// The global MIDI parameters of the synthesizer.
/// These are only editable via MIDI messages.
/// Equivalent to: interface GlobalMIDIParameter
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlobalMIDIParameter {
    /// The currently enabled MIDI system used by the synthesizer
    /// for bank selects and system exclusives. (GM, GM2, GS, XG)
    pub system: MIDISystem,
    /// The global key shift in semitones.
    /// Drum channels ignore this value.
    pub key_shift: f64,
    /// The global tuning in cents.
    /// Drum channels ignore this value.
    pub fine_tune: f64,
    /// The master volume.
    /// From 0 (silent) to 1 (full volume).
    ///
    /// This differs from the `gain` system parameter in that it is squared internally
    /// (see `SynthesizerCore::set_midi_volume`).
    pub volume: f64,
    /// The master pan.
    /// From -1 (left) to 1 (right). 0 is center.
    /// This uses the cosine panning law, so the perceived loudness remains constant as the
    /// pan changes.
    pub pan: f64,
}

/// Equivalent to: DEFAULT_GLOBAL_MIDI_PARAMETERS
pub const DEFAULT_GLOBAL_MIDI_PARAMETERS: GlobalMIDIParameter = GlobalMIDIParameter {
    volume: 1.0,
    pan: 0.0,
    key_shift: 0.0,
    fine_tune: 0.0,
    system: DEFAULT_SYNTH_MODE,
};

impl Default for GlobalMIDIParameter {
    fn default() -> Self {
        DEFAULT_GLOBAL_MIDI_PARAMETERS
    }
}

impl SynthesizerCore {
    /// Sets a global MIDI parameter of the synthesizer.
    /// Equivalent to: setMIDIParameterInternal(parameter, value)
    pub fn set_midi_parameter(&mut self, change: GlobalMIDIParameterChangeCallback) {
        match change {
            GlobalMIDIParameterChangeCallback::System(system) => {
                self.midi_parameters.system = system;
            }

            GlobalMIDIParameterChangeCallback::KeyShift(semitones) => {
                // Legacy plumbing (see the module doc TODO): replicate the 4.2.0
                // "transposition" master-parameter behavior — temporarily zero the global
                // value so that transpose_channel computes relative to 0, transpose every
                // channel, then store.
                self.midi_parameters.key_shift = 0.0;
                let current_time = self.current_time;
                let events_enabled = self.system_parameters.events_enabled;
                let voices = &mut self.voices;
                let mut events = Vec::new();
                for ch in self.midi_channels.iter_mut() {
                    if let Some(ev) = ch.transpose_channel(
                        semitones,
                        false,
                        0.0,
                        voices,
                        current_time,
                        events_enabled,
                    ) {
                        events.push(ev);
                    }
                }
                self.midi_parameters.key_shift = semitones;
                for ev in events {
                    self.call_event(ev);
                }
            }

            GlobalMIDIParameterChangeCallback::FineTune(cents) => {
                self.midi_parameters.fine_tune = cents;
                // Legacy plumbing: the 4.2.0 setMasterTuning custom-controller loop.
                self.set_master_tuning(cents);
            }

            GlobalMIDIParameterChangeCallback::Volume(volume) => {
                self.midi_parameters.volume = volume;
                // Legacy plumbing (see module doc TODO): `set_midi_volume` applies the
                // `Math.pow(volume, 2)` squaring that TS 4.3.14 performs in
                // `updateInternalParams`.
                self.set_midi_volume(volume);
            }

            GlobalMIDIParameterChangeCallback::Pan(pan) => {
                self.midi_parameters.pan = pan;
                // Legacy plumbing: recompute the shared stereo pan from the sum of the
                // system and MIDI pans (TS 4.3.0 adds them in updateInternalParams).
                self.update_legacy_pan();
            }
        }

        // TODO(Task 21): for (const ch of this.midiChannels) ch.updateInternalParams();

        self.call_event(SynthProcessorEvent::GlobalParamChange(change));
    }

    /// Resets all global MIDI parameters to their default values.
    ///
    /// TS 4.3.14 inlines this directly in `reset()` (the separate bound
    /// `resetMIDIParameters` method was removed), in the order
    /// system, volume, pan, keyShift, fineTune.
    /// Equivalent to: (inlined in) reset(system)
    pub fn reset_midi_parameters(&mut self, system: MIDISystem) {
        self.set_midi_parameter(GlobalMIDIParameterChangeCallback::System(system));
        self.set_midi_parameter(GlobalMIDIParameterChangeCallback::Volume(1.0));
        self.set_midi_parameter(GlobalMIDIParameterChangeCallback::Pan(0.0));
        self.set_midi_parameter(GlobalMIDIParameterChangeCallback::KeyShift(0.0));
        self.set_midi_parameter(GlobalMIDIParameterChangeCallback::FineTune(0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthesizer::types::{SynthProcessorEvent, SynthProcessorOptions};
    use std::sync::{Arc, Mutex};

    fn make_core() -> (SynthesizerCore, Arc<Mutex<Vec<SynthProcessorEvent>>>) {
        let events: Arc<Mutex<Vec<SynthProcessorEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let ev_clone = Arc::clone(&events);
        let core = SynthesizerCore::new(
            move |ev| ev_clone.lock().unwrap().push(ev),
            44100.0,
            SynthProcessorOptions::default(),
        );
        (core, events)
    }

    // --- defaults ---

    #[test]
    fn test_default_volume_is_one() {
        assert!((DEFAULT_GLOBAL_MIDI_PARAMETERS.volume - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_pan_is_zero() {
        assert!((DEFAULT_GLOBAL_MIDI_PARAMETERS.pan - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_key_shift_is_zero() {
        assert!((DEFAULT_GLOBAL_MIDI_PARAMETERS.key_shift - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_fine_tune_is_zero() {
        assert!((DEFAULT_GLOBAL_MIDI_PARAMETERS.fine_tune - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_system_is_gs() {
        assert_eq!(DEFAULT_GLOBAL_MIDI_PARAMETERS.system, MIDISystem::Gs);
    }

    #[test]
    fn test_default_trait_matches_const() {
        assert_eq!(GlobalMIDIParameter::default(), DEFAULT_GLOBAL_MIDI_PARAMETERS);
    }

    // --- set_midi_parameter ---

    #[test]
    fn test_set_system() {
        let (mut core, _) = make_core();
        core.set_midi_parameter(GlobalMIDIParameterChangeCallback::System(MIDISystem::Xg));
        assert_eq!(core.midi_parameters.system, MIDISystem::Xg);
    }

    #[test]
    fn test_set_volume_updates_legacy_midi_volume() {
        let (mut core, _) = make_core();
        core.set_midi_parameter(GlobalMIDIParameterChangeCallback::Volume(0.5));
        assert!((core.midi_parameters.volume - 0.5).abs() < 1e-12);
        // 4.3.14: the global MIDI volume is squared when folded into the channel gain
        // (`Math.pow(volume, 2)`), so midi_volume == volume^2.
        assert!((core.midi_volume - 0.25).abs() < 1e-12);
    }

    #[test]
    fn test_set_pan_updates_legacy_pan() {
        let (mut core, _) = make_core();
        core.set_midi_parameter(GlobalMIDIParameterChangeCallback::Pan(1.0));
        // pan 1.0 → p = 1.0 → left 0, right 1
        assert!((core.pan_left - 0.0).abs() < 1e-12);
        assert!((core.pan_right - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_set_fine_tune_stores_value() {
        let (mut core, _) = make_core();
        core.set_midi_parameter(GlobalMIDIParameterChangeCallback::FineTune(25.0));
        assert!((core.midi_parameters.fine_tune - 25.0).abs() < 1e-12);
    }

    #[test]
    fn test_set_fires_global_param_change_event() {
        let (mut core, events) = make_core();
        core.set_midi_parameter(GlobalMIDIParameterChangeCallback::Volume(0.7));
        let evs = events.lock().unwrap();
        assert!(evs
            .iter()
            .any(|e| matches!(e, SynthProcessorEvent::GlobalParamChange(_))));
    }

    // --- reset_midi_parameters ---

    #[test]
    fn test_reset_restores_defaults() {
        let (mut core, _) = make_core();
        core.set_midi_parameter(GlobalMIDIParameterChangeCallback::Volume(0.5));
        core.set_midi_parameter(GlobalMIDIParameterChangeCallback::Pan(-1.0));
        core.set_midi_parameter(GlobalMIDIParameterChangeCallback::System(MIDISystem::Xg));
        core.reset_midi_parameters(MIDISystem::Gs);
        assert_eq!(core.midi_parameters, DEFAULT_GLOBAL_MIDI_PARAMETERS);
        // Legacy plumbing reset too
        assert!((core.midi_volume - 1.0).abs() < 1e-12);
        assert!((core.pan_left - 0.5).abs() < 1e-12);
        assert!((core.pan_right - 0.5).abs() < 1e-12);
    }

    #[test]
    fn test_reset_sets_given_system() {
        let (mut core, _) = make_core();
        core.reset_midi_parameters(MIDISystem::Gm2);
        assert_eq!(core.midi_parameters.system, MIDISystem::Gm2);
    }
}
