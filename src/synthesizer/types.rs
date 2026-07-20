/// types.rs
/// purpose: Common data types for the synthesizer.
/// Ported from: src/synthesizer/types.ts (spessasynth_core 4.3.0)
///
/// Changes from 4.2.0 (reviewed against the 4.3.0 diff):
/// - `SynthSystem` moved to soundbank/types.ts as `MIDISystem` (Rust:
///   `crate::soundbank::types`).
/// - `MasterParameterType` was removed, split into `GlobalMIDIParameter`
///   (`audio_engine/parameters/midi.rs`) and `GlobalSystemParameter`
///   (`audio_engine/parameters/system.rs`).
/// - `SynthProcessorOptions` was reshaped: `enableEventSystem` → `eventsEnabled`,
///   `enableEffects` → `effectsEnabled`, new `maxBufferSize`; the reverb/chorus/delay
///   processor fields became optional (the core constructs the defaults itself — the Rust
///   port always constructs the defaults internally, unchanged).
/// - Event renames in `SynthProcessorEventData`: `newChannel` → `channelAdded`,
///   `allControllerReset` → `reset` (now carrying the `MIDISystem`), `synthDisplay` →
///   `displayMessage`, `masterParameterChange` → `globalParamChange` (now only for the
///   global *MIDI* parameters; system-parameter changes fire no event).
/// - TODO(Task 21, channel restructuring): TS 4.3.0 moved the per-channel callback payload
///   types (`NoteOnCallback`, `NoteOffCallback`, `ControllerChangeCallback`,
///   `ProgramChangeCallback`, `PolyPressureCallback`, `StopAllCallback`, ...) to
///   `audio_engine/channel/types.ts` and replaced `channelPropertyChange`/`ChannelProperty`
///   with `channelParamChange`/`ChannelMIDIParameterChange`. The Rust equivalents stay here
///   (with the 4.2.0 payload shapes for the channel-fired ones) until the channel
///   restructuring lands.
use crate::midi::enums::MidiController;
use crate::soundbank::basic_soundbank::midi_patch::{MidiPatch, MidiPatchFull};
use crate::soundbank::types::MIDISystem;

/// Equivalent to: NoteOnCallback
#[derive(Clone, Copy, Debug)]
pub struct NoteOnCallback {
    pub midi_note: u8,
    pub channel: u8,
    pub velocity: u8,
}

/// Equivalent to: NoteOffCallback
#[derive(Clone, Copy, Debug)]
pub struct NoteOffCallback {
    pub midi_note: u8,
    pub channel: u8,
}

/// Equivalent to: DrumChangeCallback
#[derive(Clone, Copy, Debug)]
pub struct DrumChangeCallback {
    pub channel: u8,
    pub is_drum_channel: bool,
}

/// Equivalent to: ProgramChangeCallback (extends MIDIPatch)
#[derive(Clone, Copy, Debug)]
pub struct ProgramChangeCallback {
    pub patch: MidiPatch,
    pub channel: u8,
}

/// Equivalent to: ControllerChangeCallback
#[derive(Clone, Copy, Debug)]
pub struct ControllerChangeCallback {
    pub channel: u8,
    pub controller_number: MidiController,
    pub controller_value: u8,
}

/// Equivalent to: MuteChannelCallback
#[derive(Clone, Copy, Debug)]
pub struct MuteChannelCallback {
    pub channel: u8,
    pub is_muted: bool,
}

/// A preset list entry. TS 4.3.0 exposes the preset list as `MIDIPatchFull[]`
/// (the separate PresetListEntry interface with `isAnyDrums` was removed).
/// Equivalent to: MIDIPatchFull
pub type PresetListEntry = MidiPatchFull;

/// Equivalent to: presetList: MIDIPatchFull[]
pub type PresetList = Vec<PresetListEntry>;

/// The synthesizer display system exclusive data, excluding the F0 byte.
/// Equivalent to: SynthDisplayCallback
pub type SynthDisplayCallback = Vec<u8>;

/// Equivalent to: PitchWheelCallback
#[derive(Clone, Copy, Debug)]
pub struct PitchWheelCallback {
    pub channel: u8,
    /// Unsigned 14-bit pitch value: 0 - 16383.
    pub pitch: u16,
    /// MIDI note number if note-specific; -1 otherwise.
    pub midi_note: i32,
}

/// Equivalent to: ChannelPressureCallback
#[derive(Clone, Copy, Debug)]
pub struct ChannelPressureCallback {
    pub channel: u8,
    pub pressure: u8,
}

/// Equivalent to: PolyPressureCallback
#[derive(Clone, Copy, Debug)]
pub struct PolyPressureCallback {
    pub channel: u8,
    pub midi_note: u8,
    pub pressure: u8,
}

/// Equivalent to: SoundBankErrorCallback
pub type SoundBankErrorCallback = String;

/// Equivalent to: StopAllCallback
#[derive(Clone, Copy, Debug)]
pub struct StopAllCallback {
    pub channel: u8,
    pub force: bool,
}

/// A single global MIDI parameter change (parameter + new value).
/// TS 4.3.0's `globalParamChange` event payload is `{ parameter, value }` over the
/// `GlobalMIDIParameter` keys; Rust uses a discriminated union.
/// Equivalent to: GlobalMIDIParameterChangeCallback
#[derive(Clone, Copy, Debug)]
pub enum GlobalMIDIParameterChangeCallback {
    System(MIDISystem),
    KeyShift(f64),
    FineTune(f64),
    /// Equivalent to: GlobalMIDIParameter.volume (renamed from `gain` in 4.3.14)
    Volume(f64),
    Pan(f64),
}

/// Channel property snapshot.
/// Equivalent to: ChannelProperty (removed in TS 4.3.0 — replaced by the channel MIDI
/// parameter set; kept until the Task 21 channel restructuring, see module doc)
#[derive(Clone, Copy, Debug)]
pub struct ChannelProperty {
    pub voices_amount: u32,
    /// Pitch wheel value: 0 - 16384.
    pub pitch_wheel: u16,
    /// Pitch wheel range in semitones.
    pub pitch_wheel_range: f64,
    pub is_muted: bool,
    pub is_drum: bool,
    pub transposition: f64,
}

/// Equivalent to: ChannelPropertyChangeCallback (removed in TS 4.3.0 — replaced by
/// `channelParamChange`/`ChannelMIDIParameterChange`; kept until Task 21, see module doc)
#[derive(Clone, Copy, Debug)]
pub struct ChannelPropertyChangeCallback {
    pub channel: u8,
    pub property: ChannelProperty,
}

/// The payload of an `effectChange` event.
/// Equivalent to: SynthProcessorEventData["effectChange"] (partial — the TS payload also
/// distinguishes the effect kind via a string union)
#[derive(Clone, Copy, Debug)]
pub struct EffectChangeCallback {
    /// Which effect changed: "reverb" | "chorus" | "delay" | "insertion".
    pub effect: EffectKind,
    /// The changed parameter index (0 = macro / type).
    pub parameter: u8,
    /// The new value.
    pub value: i32,
}

/// The effect kind for an `effectChange` event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectKind {
    Reverb,
    Chorus,
    Delay,
    Insertion,
}

/// All synthesizer processor events (discriminated union).
/// Equivalent to: SynthProcessorEvent
#[derive(Clone, Debug)]
pub enum SynthProcessorEvent {
    NoteOn(NoteOnCallback),
    NoteOff(NoteOffCallback),
    PitchWheel(PitchWheelCallback),
    ControllerChange(ControllerChangeCallback),
    ProgramChange(ProgramChangeCallback),
    ChannelPressure(ChannelPressureCallback),
    PolyPressure(PolyPressureCallback),
    DrumChange(DrumChangeCallback),
    StopAll(StopAllCallback),
    /// Equivalent to: channelAdded (renamed from newChannel in TS 4.3.0)
    ChannelAdded,
    MuteChannel(MuteChannelCallback),
    PresetListChange(PresetList),
    /// Equivalent to: reset (renamed from allControllerReset in TS 4.3.0; now carries the
    /// MIDI system the synthesizer was reset to)
    Reset(MIDISystem),
    SoundBankError(SoundBankErrorCallback),
    /// Equivalent to: displayMessage (renamed from synthDisplay in TS 4.3.0)
    DisplayMessage(SynthDisplayCallback),
    /// Equivalent to: globalParamChange (replaces masterParameterChange in TS 4.3.0; fired
    /// only for global MIDI parameters — system-parameter changes fire no event)
    GlobalParamChange(GlobalMIDIParameterChangeCallback),
    /// Equivalent to: effectChange
    EffectChange(EffectChangeCallback),
    /// Kept from 4.2.0 until Task 21 (TS 4.3.0: channelParamChange) — see module doc.
    ChannelPropertyChange(ChannelPropertyChangeCallback),
}

/// Synthesizer method scheduling options.
/// Equivalent to: SynthMethodOptions
#[derive(Clone, Copy, Debug, Default)]
pub struct SynthMethodOptions {
    /// Audio context time in seconds when the event should execute.
    pub time: f64,
}

/// Sample looping mode.
/// 0 = no loop, 1 = loop, 2 = start on release (unofficial), 3 = loop then play when released.
/// Equivalent to: SampleLoopingMode
pub type SampleLoopingMode = u8;

/// A list of voices for a given key:velocity.
/// Equivalent to: CachedVoiceList
pub type CachedVoiceList =
    Vec<crate::synthesizer::audio_engine::voice::voice_cache::CachedVoice>;

/// Synthesizer processor options.
/// Equivalent to: SynthProcessorOptions
///
/// Note: TS 4.3.0's optional `reverbProcessor`/`chorusProcessor`/`delayProcessor` fields are
/// not ported — the Rust core always constructs the default effect processors internally
/// (equivalent to TS's `options.reverbProcessor ?? new SpessaSynthReverb(...)` with the
/// option always absent).
#[derive(Clone, Debug)]
pub struct SynthProcessorOptions {
    /// The maximum buffer size the synthesizer can render at once.
    /// Attempting to render more samples than this will result in a panic.
    /// Defaults to 128.
    /// Equivalent to: maxBufferSize
    pub max_buffer_size: usize,
    /// If the synthesizer processes the audio effects. This can be changed later.
    /// Equivalent to: effectsEnabled (renamed from enableEffects in TS 4.3.0)
    pub effects_enabled: bool,
    /// If the event system is enabled. This can be changed later.
    /// Equivalent to: eventsEnabled (renamed from enableEventSystem in TS 4.3.0)
    pub events_enabled: bool,
    /// The initial time of the synth, in seconds.
    /// Equivalent to: initialTime
    pub initial_time: f64,
}

impl Default for SynthProcessorOptions {
    fn default() -> Self {
        crate::synthesizer::audio_engine::synth_processor_options::DEFAULT_SYNTH_OPTIONS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;

    // --- MIDISystem ---

    #[test]
    fn test_synth_system_default_is_gs() {
        assert_eq!(MIDISystem::default(), MIDISystem::Gs);
    }

    #[test]
    fn test_synth_system_variants_distinct() {
        assert_ne!(MIDISystem::Gm, MIDISystem::Gm2);
        assert_ne!(MIDISystem::Gs, MIDISystem::Xg);
        assert_ne!(MIDISystem::Gm, MIDISystem::Gs);
    }

    // --- SynthMethodOptions ---

    #[test]
    fn test_synth_method_options_default_time_is_zero() {
        let opts = SynthMethodOptions::default();
        assert_eq!(opts.time, 0.0);
    }

    #[test]
    fn test_synth_method_options_set_time() {
        let opts = SynthMethodOptions { time: 1.5 };
        assert_eq!(opts.time, 1.5);
    }

    // --- SynthProcessorOptions ---

    #[test]
    fn test_synth_processor_options_default() {
        let opts = SynthProcessorOptions::default();
        assert!(opts.events_enabled);
        assert_eq!(opts.initial_time, 0.0);
        assert!(opts.effects_enabled);
        assert_eq!(opts.max_buffer_size, 128);
    }

    #[test]
    fn test_synth_processor_options_custom() {
        let opts = SynthProcessorOptions {
            events_enabled: false,
            initial_time: 2.0,
            effects_enabled: false,
            max_buffer_size: 256,
        };
        assert!(!opts.events_enabled);
        assert_eq!(opts.initial_time, 2.0);
        assert!(!opts.effects_enabled);
        assert_eq!(opts.max_buffer_size, 256);
    }

    // --- SampleLoopingMode ---

    #[test]
    fn test_sample_looping_mode_values() {
        let no_loop: SampleLoopingMode = 0;
        let loop_mode: SampleLoopingMode = 1;
        let start_on_release: SampleLoopingMode = 2;
        let loop_then_play: SampleLoopingMode = 3;
        assert_eq!(no_loop, 0);
        assert_eq!(loop_mode, 1);
        assert_eq!(start_on_release, 2);
        assert_eq!(loop_then_play, 3);
    }

    // --- NoteOnCallback ---

    #[test]
    fn test_note_on_callback_fields() {
        let cb = NoteOnCallback {
            midi_note: 60,
            channel: 0,
            velocity: 100,
        };
        assert_eq!(cb.midi_note, 60);
        assert_eq!(cb.channel, 0);
        assert_eq!(cb.velocity, 100);
    }

    // --- NoteOffCallback ---

    #[test]
    fn test_note_off_callback_fields() {
        let cb = NoteOffCallback {
            midi_note: 60,
            channel: 1,
        };
        assert_eq!(cb.midi_note, 60);
        assert_eq!(cb.channel, 1);
    }

    // --- PitchWheelCallback ---

    #[test]
    fn test_pitch_wheel_callback_non_note_specific() {
        let cb = PitchWheelCallback {
            channel: 2,
            pitch: 8192,
            midi_note: -1,
        };
        assert_eq!(cb.pitch, 8192);
        assert_eq!(cb.midi_note, -1);
    }

    // --- ControllerChangeCallback ---

    #[test]
    fn test_controller_change_callback_fields() {
        use crate::midi::enums::midi_controllers;
        let cb = ControllerChangeCallback {
            channel: 0,
            controller_number: midi_controllers::MAIN_VOLUME,
            controller_value: 100,
        };
        assert_eq!(cb.controller_number, 7);
        assert_eq!(cb.controller_value, 100);
    }

    // --- ProgramChangeCallback ---

    #[test]
    fn test_program_change_callback_fields() {
        let patch = MidiPatch {
            program: 10,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        };
        let cb = ProgramChangeCallback { patch, channel: 3 };
        assert_eq!(cb.patch.program, 10);
        assert_eq!(cb.channel, 3);
    }

    // --- GlobalMIDIParameterChangeCallback ---

    #[test]
    fn test_global_midi_parameter_change_callback_variants() {
        let v1 = GlobalMIDIParameterChangeCallback::Volume(1.5);
        let v2 = GlobalMIDIParameterChangeCallback::Pan(-0.5);
        let v3 = GlobalMIDIParameterChangeCallback::System(MIDISystem::Xg);
        let v4 = GlobalMIDIParameterChangeCallback::KeyShift(2.0);
        let v5 = GlobalMIDIParameterChangeCallback::FineTune(-10.0);

        assert!(matches!(v1, GlobalMIDIParameterChangeCallback::Volume(_)));
        assert!(matches!(v2, GlobalMIDIParameterChangeCallback::Pan(_)));
        assert!(matches!(v3, GlobalMIDIParameterChangeCallback::System(_)));
        assert!(matches!(v4, GlobalMIDIParameterChangeCallback::KeyShift(_)));
        assert!(matches!(v5, GlobalMIDIParameterChangeCallback::FineTune(_)));
    }

    // --- ChannelProperty ---

    #[test]
    fn test_channel_property_fields() {
        let cp = ChannelProperty {
            voices_amount: 5,
            pitch_wheel: 8192,
            pitch_wheel_range: 2.0,
            is_muted: false,
            is_drum: false,
            transposition: 0.0,
        };
        assert_eq!(cp.voices_amount, 5);
        assert_eq!(cp.pitch_wheel, 8192);
        assert!(!cp.is_drum);
    }

    // --- SynthProcessorEvent variants ---

    #[test]
    fn test_synth_processor_event_channel_added() {
        let ev = SynthProcessorEvent::ChannelAdded;
        assert!(matches!(ev, SynthProcessorEvent::ChannelAdded));
    }

    #[test]
    fn test_synth_processor_event_reset_carries_system() {
        let ev = SynthProcessorEvent::Reset(MIDISystem::Gs);
        assert!(matches!(ev, SynthProcessorEvent::Reset(MIDISystem::Gs)));
    }

    #[test]
    fn test_synth_processor_event_note_on() {
        let ev = SynthProcessorEvent::NoteOn(NoteOnCallback {
            midi_note: 69,
            channel: 0,
            velocity: 90,
        });
        if let SynthProcessorEvent::NoteOn(cb) = ev {
            assert_eq!(cb.midi_note, 69);
            assert_eq!(cb.velocity, 90);
        } else {
            panic!("expected NoteOn");
        }
    }

    #[test]
    fn test_synth_processor_event_stop_all() {
        let ev = SynthProcessorEvent::StopAll(StopAllCallback {
            channel: 1,
            force: true,
        });
        if let SynthProcessorEvent::StopAll(cb) = ev {
            assert!(cb.force);
        } else {
            panic!("expected StopAll");
        }
    }

    #[test]
    fn test_synth_processor_event_sound_bank_error() {
        let ev = SynthProcessorEvent::SoundBankError("parse error".to_string());
        if let SynthProcessorEvent::SoundBankError(msg) = ev {
            assert_eq!(msg, "parse error");
        } else {
            panic!("expected SoundBankError");
        }
    }
}
