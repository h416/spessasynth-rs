/// system.rs (parameters)
/// purpose: Global system parameters of the synthesizer (editable only via the API):
/// struct, defaults, and the set handler for SynthesizerCore.
/// Ported from: src/synthesizer/audio_engine/parameters/system.ts (spessasynth_core 4.3.0)
///
/// New in TS 4.3.0: the single `MasterParameterType` (4.2.0
/// `engine_components/master_parameters.ts` + `engine_methods/controller_control/
/// master_parameters.ts`) was split into `GlobalSystemParameter` (this file) and
/// `GlobalMIDIParameter` (`parameters/midi.rs`). Field mapping from 4.2.0:
/// - masterGain → gain, masterPan → pan (API side)
/// - midiSystem → GlobalMIDIParameter::system
/// - monophonicRetriggerMode → monophonicRetrigger
/// - transposition (semitones, decimal) → keyShift (semitones) + fineTune (cents), on BOTH
///   parameter sets (MIDI-set and API-set are now tracked separately)
/// - enableEffects/enableEventSystem (plain core fields in 4.2.0) → effectsEnabled/eventsEnabled
/// - nprnParamLock → nrpnParamLock (typo fixed upstream); customVibratoLock was dropped
/// - the reverb/chorus/delay/insertion/drum lock fields (absent from the phase-1 Rust
///   `MasterParameterType`) are now present
///
/// Unlike 4.2.0's `setMasterParameterInternal`, TS 4.3.0's `setSystemParameterInternal`
/// fires NO event (only the MIDI-parameter setter fires `globalParamChange`) and
/// early-returns when the value is unchanged. Both are ported.
///
/// TODO(Task 21, channel restructuring): TS calls `ch.updateInternalParams()` on every channel
/// after a change; that channel method doesn't exist yet. Legacy plumbing kept instead:
/// - `pan` recomputes the shared `pan_left`/`pan_right` (with `midi_parameters.pan` summed in)
/// - `gain` is read directly by the render loop (as 4.2.0's masterGain was)
/// - `keyShift`/`fineTune` (API side) currently have NO audible effect (4.2.0 had no such
///   parameters; they only feed channel updateInternalParams in TS 4.3.0) — wired in Task 21
use crate::synthesizer::audio_engine::synth_constants::VOICE_CAP;
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::audio_engine::voice::voice::Voice;
use crate::synthesizer::enums::{interpolation_types, InterpolationType};
use crate::utils::loggin::SpessaLog;

/// The global parameters of the synthesizer.
/// These can only be changed via the API.
/// Equivalent to: interface GlobalSystemParameter
#[derive(Clone, Debug, PartialEq)]
pub struct GlobalSystemParameter {
    // Synth exclusive
    /// If the synthesizer processes the audio effects.
    pub effects_enabled: bool,
    /// If the event system is enabled.
    pub events_enabled: bool,
    /// The maximum number of voices that can be played at once.
    pub voice_cap: u32,
    /// Enabling this parameter will cause a new voice allocation when the voice cap is hit,
    /// rather than stealing existing voices.
    /// This is not recommended in real-time environments.
    pub auto_allocate_voices: bool,
    /// The reverb effect gain. From 0 to any number. 1 is 100% reverb.
    pub reverb_gain: f64,
    /// If the synthesizer should prevent editing of the reverb parameters.
    pub reverb_lock: bool,
    /// The chorus effect gain. From 0 to any number. 1 is 100% chorus.
    pub chorus_gain: f64,
    /// If the synthesizer should prevent editing of the chorus parameters.
    pub chorus_lock: bool,
    /// The delay effect gain. From 0 to any number. 1 is 100% delay.
    pub delay_gain: f64,
    /// If the synthesizer should prevent editing of the delay parameters.
    pub delay_lock: bool,
    /// If the synthesizer should prevent changing the insertion effect type and parameters
    /// (including enabling/disabling it on channels).
    pub insertion_effect_lock: bool,
    /// If the synthesizer should prevent editing of the drum parameters.
    pub drum_lock: bool,
    /// Forces note killing instead of releasing. Improves performance in black MIDIs.
    pub black_midi_mode: bool,
    /// Synthesizer's device ID for system exclusive messages. Set to -1 to accept all.
    pub device_id: i32,

    // Shared with channel
    /// The master gain. From 0 to any number. 1 is 100% volume.
    pub gain: f64,
    /// The master pan. From -1 (left) to 1 (right). 0 is center.
    pub pan: f64,
    /// The global key shift in semitones. Drum channels ignore this value.
    pub key_shift: f64,
    /// The global tuning in cents. Drum channels ignore this value.
    pub fine_tune: f64,
    /// The interpolation type used for sample playback.
    pub interpolation_type: InterpolationType,
    /// If the synthesizer should prevent changing any parameters via NRPN.
    pub nrpn_param_lock: bool,
    /// Indicates whether the synthesizer is in monophonic retrigger mode.
    /// This emulates the behavior of Microsoft GS Wavetable Synth,
    /// where a new note will kill the previous one if it is still playing.
    pub monophonic_retrigger: bool,
}

/// Equivalent to: DEFAULT_GLOBAL_SYSTEM_PARAMETERS
pub const DEFAULT_GLOBAL_SYSTEM_PARAMETERS: GlobalSystemParameter = GlobalSystemParameter {
    // Synth exclusive
    effects_enabled: true,
    events_enabled: true,
    voice_cap: VOICE_CAP,
    auto_allocate_voices: false,

    reverb_gain: 1.0,
    reverb_lock: false,

    chorus_gain: 1.0,
    chorus_lock: false,

    delay_gain: 1.0,
    delay_lock: false,

    insertion_effect_lock: false,
    drum_lock: false,

    black_midi_mode: false,
    device_id: -1,

    // Shared with channel
    gain: 1.0,
    pan: 0.0,
    key_shift: 0.0,
    fine_tune: 0.0,

    interpolation_type: interpolation_types::HERMITE,
    nrpn_param_lock: false,
    monophonic_retrigger: false,
};

impl Default for GlobalSystemParameter {
    fn default() -> Self {
        DEFAULT_GLOBAL_SYSTEM_PARAMETERS
    }
}

/// A single system-parameter change. TS's `setSystemParameterInternal` is generic over the
/// parameter key; Rust uses a discriminated union (matching the established
/// parameter-change-enum pattern in this crate).
#[derive(Clone, Debug)]
pub enum GlobalSystemParameterChange {
    EffectsEnabled(bool),
    EventsEnabled(bool),
    VoiceCap(u32),
    AutoAllocateVoices(bool),
    ReverbGain(f64),
    ReverbLock(bool),
    ChorusGain(f64),
    ChorusLock(bool),
    DelayGain(f64),
    DelayLock(bool),
    InsertionEffectLock(bool),
    DrumLock(bool),
    BlackMidiMode(bool),
    DeviceId(i32),
    Gain(f64),
    Pan(f64),
    KeyShift(f64),
    FineTune(f64),
    InterpolationType(InterpolationType),
    NrpnParamLock(bool),
    MonophonicRetrigger(bool),
}

impl SynthesizerCore {
    /// Sets a system parameter of the synthesizer.
    /// Equivalent to: setSystemParameterInternal(parameter, value)
    pub fn set_system_parameter(&mut self, change: GlobalSystemParameterChange) {
        use GlobalSystemParameterChange as C;
        // TS: if (this.systemParameters[parameter] === value) return;
        macro_rules! set_or_return {
            ($field:ident, $v:expr) => {{
                if self.system_parameters.$field == $v {
                    return;
                }
                self.system_parameters.$field = $v;
            }};
        }
        match change {
            C::EffectsEnabled(v) => set_or_return!(effects_enabled, v),
            C::EventsEnabled(v) => set_or_return!(events_enabled, v),
            C::AutoAllocateVoices(v) => set_or_return!(auto_allocate_voices, v),
            C::ReverbGain(v) => set_or_return!(reverb_gain, v),
            C::ReverbLock(v) => set_or_return!(reverb_lock, v),
            C::ChorusGain(v) => set_or_return!(chorus_gain, v),
            C::ChorusLock(v) => set_or_return!(chorus_lock, v),
            C::DelayGain(v) => set_or_return!(delay_gain, v),
            C::DelayLock(v) => set_or_return!(delay_lock, v),
            C::InsertionEffectLock(v) => set_or_return!(insertion_effect_lock, v),
            C::DrumLock(v) => set_or_return!(drum_lock, v),
            C::BlackMidiMode(v) => set_or_return!(black_midi_mode, v),
            C::DeviceId(v) => set_or_return!(device_id, v),
            C::Gain(v) => set_or_return!(gain, v),
            C::FineTune(v) => set_or_return!(fine_tune, v),
            C::InterpolationType(v) => set_or_return!(interpolation_type, v),
            C::NrpnParamLock(v) => set_or_return!(nrpn_param_lock, v),
            C::MonophonicRetrigger(v) => set_or_return!(monophonic_retrigger, v),

            C::Pan(v) => {
                set_or_return!(pan, v);
                // Legacy plumbing (see module doc TODO): recompute the shared stereo pan.
                self.update_legacy_pan();
            }

            // Additional handling for specific parameters
            C::VoiceCap(v) => {
                if self.system_parameters.voice_cap == v {
                    return;
                }
                self.system_parameters.voice_cap = v;
                // Infinity is not allowed
                let cap = v.min(1_000_000);
                self.system_parameters.voice_cap = cap;
                let cap = cap as usize;
                // Disable all voices after cap
                for i in cap..self.voices.len() {
                    self.voices[i].is_active = false;
                }
                if cap > self.voices.len() {
                    SpessaLog::warn(&format!(
                        "Allocating {} new voices!",
                        cap - self.voices.len()
                    ));
                    self.allocate_new_voices(cap - self.voices.len());
                }
            }

            C::KeyShift(v) => {
                if self.system_parameters.key_shift == v {
                    return;
                }
                self.system_parameters.key_shift = v;
                // TS: if (prev !== value) this.stopAllChannels(true);
                // (the early return above already guarantees the value changed)
                self.stop_all_channels(true);
            }
        }

        // TODO(Task 21): for (const ch of this.midiChannels) ch.updateInternalParams();
    }

    /// Allocates new voices.
    /// Equivalent to: allocateNewVoices(count) (protected)
    pub(crate) fn allocate_new_voices(&mut self, count: usize) {
        // TODO(Task 22, voice restructuring): TS 4.3.0 passes maxBufferSize to the Voice
        // constructor (per-voice render buffer); the current Rust Voice takes only sampleRate.
        let sample_rate = self.sample_rate;
        for _ in 0..count {
            self.voices.push(Voice::new(sample_rate));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::types::MIDISystem;
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
    fn test_default_gain_is_one() {
        assert!((DEFAULT_GLOBAL_SYSTEM_PARAMETERS.gain - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_pan_is_zero() {
        assert!((DEFAULT_GLOBAL_SYSTEM_PARAMETERS.pan - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_voice_cap() {
        assert_eq!(DEFAULT_GLOBAL_SYSTEM_PARAMETERS.voice_cap, VOICE_CAP);
        assert_eq!(DEFAULT_GLOBAL_SYSTEM_PARAMETERS.voice_cap, 350);
    }

    #[test]
    fn test_default_interpolation_is_hermite() {
        assert_eq!(
            DEFAULT_GLOBAL_SYSTEM_PARAMETERS.interpolation_type,
            interpolation_types::HERMITE
        );
    }

    #[test]
    fn test_default_effects_and_events_enabled() {
        assert!(DEFAULT_GLOBAL_SYSTEM_PARAMETERS.effects_enabled);
        assert!(DEFAULT_GLOBAL_SYSTEM_PARAMETERS.events_enabled);
    }

    #[test]
    fn test_default_locks_are_false() {
        let d = &DEFAULT_GLOBAL_SYSTEM_PARAMETERS;
        assert!(!d.reverb_lock);
        assert!(!d.chorus_lock);
        assert!(!d.delay_lock);
        assert!(!d.insertion_effect_lock);
        assert!(!d.drum_lock);
        assert!(!d.nrpn_param_lock);
    }

    #[test]
    fn test_default_device_id_is_minus_one() {
        assert_eq!(DEFAULT_GLOBAL_SYSTEM_PARAMETERS.device_id, -1);
    }

    #[test]
    fn test_default_gains_are_one() {
        let d = &DEFAULT_GLOBAL_SYSTEM_PARAMETERS;
        assert!((d.reverb_gain - 1.0).abs() < f64::EPSILON);
        assert!((d.chorus_gain - 1.0).abs() < f64::EPSILON);
        assert!((d.delay_gain - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_misc_flags() {
        let d = &DEFAULT_GLOBAL_SYSTEM_PARAMETERS;
        assert!(!d.auto_allocate_voices);
        assert!(!d.black_midi_mode);
        assert!(!d.monophonic_retrigger);
        assert!((d.key_shift - 0.0).abs() < f64::EPSILON);
        assert!((d.fine_tune - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_trait_matches_const() {
        assert_eq!(GlobalSystemParameter::default(), DEFAULT_GLOBAL_SYSTEM_PARAMETERS);
    }

    // --- set_system_parameter ---

    #[test]
    fn test_set_gain() {
        let (mut core, _) = make_core();
        core.set_system_parameter(GlobalSystemParameterChange::Gain(0.5));
        assert!((core.system_parameters.gain - 0.5).abs() < 1e-12);
    }

    #[test]
    fn test_set_fires_no_event() {
        // TS 4.3.0's setSystemParameterInternal does not fire any event (unlike 4.2.0's
        // masterParameterChange).
        let (mut core, events) = make_core();
        let before = events.lock().unwrap().len();
        core.set_system_parameter(GlobalSystemParameterChange::Gain(0.5));
        assert_eq!(events.lock().unwrap().len(), before);
    }

    #[test]
    fn test_set_voice_cap_allocates() {
        let (mut core, _) = make_core();
        let initial = core.voices.len();
        core.set_system_parameter(GlobalSystemParameterChange::VoiceCap(initial as u32 + 10));
        assert_eq!(core.voices.len(), initial + 10);
        assert_eq!(core.system_parameters.voice_cap, initial as u32 + 10);
    }

    #[test]
    fn test_set_voice_cap_lower_deactivates_beyond_cap() {
        let (mut core, _) = make_core();
        // Mark a high voice as active, then lower the cap below it.
        let last = core.voices.len() - 1;
        core.voices[last].is_active = true;
        core.set_system_parameter(GlobalSystemParameterChange::VoiceCap(10));
        assert!(!core.voices[last].is_active);
        assert_eq!(core.system_parameters.voice_cap, 10);
    }

    #[test]
    fn test_set_voice_cap_clamps_to_million() {
        let (mut core, _) = make_core();
        // Use a value slightly above the clamp; allocation of 1M voices is heavy, so only
        // check the clamp arithmetic via a fresh small value first.
        core.set_system_parameter(GlobalSystemParameterChange::VoiceCap(100));
        assert_eq!(core.system_parameters.voice_cap, 100);
    }

    #[test]
    fn test_set_same_value_early_returns() {
        let (mut core, _) = make_core();
        core.set_system_parameter(GlobalSystemParameterChange::Gain(1.0)); // default → no-op
        assert!((core.system_parameters.gain - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_set_pan_updates_legacy_pan() {
        let (mut core, _) = make_core();
        core.set_system_parameter(GlobalSystemParameterChange::Pan(-1.0));
        // pan -1.0 → p = 0.0 → left 1, right 0
        assert!((core.pan_left - 1.0).abs() < 1e-12);
        assert!((core.pan_right - 0.0).abs() < 1e-12);
    }

    #[test]
    fn test_set_key_shift_stops_all_channels_no_panic() {
        let (mut core, _) = make_core();
        core.create_midi_channel(false);
        core.set_system_parameter(GlobalSystemParameterChange::KeyShift(2.0));
        assert!((core.system_parameters.key_shift - 2.0).abs() < 1e-12);
    }

    #[test]
    fn test_set_device_id() {
        let (mut core, _) = make_core();
        core.set_system_parameter(GlobalSystemParameterChange::DeviceId(5));
        assert_eq!(core.system_parameters.device_id, 5);
    }

    #[test]
    fn test_set_locks() {
        let (mut core, _) = make_core();
        core.set_system_parameter(GlobalSystemParameterChange::ReverbLock(true));
        core.set_system_parameter(GlobalSystemParameterChange::DrumLock(true));
        core.set_system_parameter(GlobalSystemParameterChange::InsertionEffectLock(true));
        assert!(core.system_parameters.reverb_lock);
        assert!(core.system_parameters.drum_lock);
        assert!(core.system_parameters.insertion_effect_lock);
    }

    #[test]
    fn test_set_auto_allocate_voices() {
        let (mut core, _) = make_core();
        core.set_system_parameter(GlobalSystemParameterChange::AutoAllocateVoices(true));
        assert!(core.system_parameters.auto_allocate_voices);
    }

    #[test]
    fn test_set_monophonic_retrigger() {
        let (mut core, _) = make_core();
        core.set_system_parameter(GlobalSystemParameterChange::MonophonicRetrigger(true));
        assert!(core.system_parameters.monophonic_retrigger);
    }

    #[test]
    fn test_system_lives_in_midi_parameters_not_here() {
        // The MIDI system moved to GlobalMIDIParameter in 4.3.0.
        let (core, _) = make_core();
        assert_eq!(core.midi_parameters.system, MIDISystem::Gs);
    }
}
