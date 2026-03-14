/// types.rs
/// purpose: Common data types for the synthesizer.
/// Ported from: src/synthesizer/types.ts
use crate::midi::enums::MidiController;
use crate::soundbank::basic_soundbank::midi_patch::{MidiPatch, MidiPatchNamed};
use crate::synthesizer::enums::InterpolationType;

/// MIDI system mode.
/// Equivalent to: SynthSystem
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SynthSystem {
    Gm,
    Gm2,
    #[default]
    Gs,
    Xg,
}

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

/// Equivalent to: PresetListEntry (extends MIDIPatchNamed)
#[derive(Clone, Debug)]
pub struct PresetListEntry {
    pub named: MidiPatchNamed,
    pub is_any_drums: bool,
}

/// Equivalent to: PresetList
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

/// The master parameters of the synthesizer.
/// Equivalent to: MasterParameterType
#[derive(Clone, Debug)]
pub struct MasterParameterType {
    /// Master gain, from 0 to any number. 1 is 100% volume.
    pub master_gain: f64,
    /// Master pan, from -1 (left) to 1 (right). 0 is center.
    pub master_pan: f64,
    /// Maximum number of voices that can be played at once.
    pub voice_cap: u32,
    /// Interpolation type used for sample playback.
    pub interpolation_type: InterpolationType,
    /// MIDI system used for bank selects and system exclusives.
    pub midi_system: SynthSystem,
    /// Monophonic retrigger mode (emulates Microsoft GS Wavetable Synth behavior).
    pub monophonic_retrigger_mode: bool,
    /// Reverb gain, from 0 to any number. 1 is 100% reverb.
    pub reverb_gain: f64,
    /// Chorus gain, from 0 to any number. 1 is 100% chorus.
    pub chorus_gain: f64,
    /// Forces note killing instead of releasing. Improves performance in black MIDIs.
    pub black_midi_mode: bool,
    /// Global transposition in semitones (decimal for microtonal tuning).
    pub transposition: f64,
    /// Synthesizer's device ID for system exclusive messages. -1 to accept all.
    pub device_id: i32,
    /// Delay gain, from 0 to any number. 1 is 100% delay.
    pub delay_gain: f64,
}

/// Discriminated union for master parameter changes.
/// Equivalent to: MasterParameterChangeCallback
#[derive(Clone, Debug)]
pub enum MasterParameterChangeCallback {
    MasterGain(f64),
    MasterPan(f64),
    VoiceCap(u32),
    InterpolationType(InterpolationType),
    MidiSystem(SynthSystem),
    MonophonicRetriggerMode(bool),
    ReverbGain(f64),
    ChorusGain(f64),
    BlackMidiMode(bool),
    Transposition(f64),
    DeviceId(i32),
}

/// Channel property snapshot.
/// Equivalent to: ChannelProperty
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

/// Equivalent to: ChannelPropertyChangeCallback
#[derive(Clone, Copy, Debug)]
pub struct ChannelPropertyChangeCallback {
    pub channel: u8,
    pub property: ChannelProperty,
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
    NewChannel,
    MuteChannel(MuteChannelCallback),
    PresetListChange(PresetList),
    AllControllerReset,
    SoundBankError(SoundBankErrorCallback),
    SynthDisplay(SynthDisplayCallback),
    MasterParameterChange(MasterParameterChangeCallback),
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
    Vec<crate::synthesizer::audio_engine::engine_components::voice_cache::CachedVoice>;

/// Synthesizer processor options.
/// Equivalent to: SynthProcessorOptions
#[derive(Clone, Debug)]
pub struct SynthProcessorOptions {
    /// Whether the event system is enabled.
    pub enable_event_system: bool,
    /// Initial synthesizer time in seconds.
    pub initial_time: f64,
    /// Whether effects are enabled.
    pub enable_effects: bool,
}

impl Default for SynthProcessorOptions {
    fn default() -> Self {
        Self {
            enable_event_system: true,
            initial_time: 0.0,
            enable_effects: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
    use crate::synthesizer::enums::interpolation_types;

    // --- SynthSystem ---

    #[test]
    fn test_synth_system_default_is_gs() {
        assert_eq!(SynthSystem::default(), SynthSystem::Gs);
    }

    #[test]
    fn test_synth_system_variants_distinct() {
        assert_ne!(SynthSystem::Gm, SynthSystem::Gm2);
        assert_ne!(SynthSystem::Gs, SynthSystem::Xg);
        assert_ne!(SynthSystem::Gm, SynthSystem::Gs);
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
        assert!(opts.enable_event_system);
        assert_eq!(opts.initial_time, 0.0);
        assert!(opts.enable_effects);
    }

    #[test]
    fn test_synth_processor_options_custom() {
        let opts = SynthProcessorOptions {
            enable_event_system: false,
            initial_time: 2.0,
            enable_effects: false,
        };
        assert!(!opts.enable_event_system);
        assert_eq!(opts.initial_time, 2.0);
        assert!(!opts.enable_effects);
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

    // --- MasterParameterType ---

    #[test]
    fn test_master_parameter_type_fields() {
        let mp = MasterParameterType {
            master_gain: 1.0,
            master_pan: 0.0,
            voice_cap: 350,
            interpolation_type: interpolation_types::LINEAR,
            midi_system: SynthSystem::Gs,
            monophonic_retrigger_mode: false,
            reverb_gain: 1.0,
            chorus_gain: 1.0,
            black_midi_mode: false,
            transposition: 0.0,
            device_id: -1,
            delay_gain: 1.0,
        };
        assert_eq!(mp.voice_cap, 350);
        assert_eq!(mp.device_id, -1);
        assert_eq!(mp.midi_system, SynthSystem::Gs);
    }

    // --- MasterParameterChangeCallback ---

    #[test]
    fn test_master_parameter_change_callback_variants() {
        let v1 = MasterParameterChangeCallback::MasterGain(1.5);
        let v2 = MasterParameterChangeCallback::VoiceCap(256);
        let v3 = MasterParameterChangeCallback::MidiSystem(SynthSystem::Xg);
        let v4 = MasterParameterChangeCallback::DeviceId(-1);
        let v5 = MasterParameterChangeCallback::BlackMidiMode(true);

        matches!(v1, MasterParameterChangeCallback::MasterGain(_));
        matches!(v2, MasterParameterChangeCallback::VoiceCap(_));
        matches!(v3, MasterParameterChangeCallback::MidiSystem(_));
        matches!(v4, MasterParameterChangeCallback::DeviceId(_));
        matches!(v5, MasterParameterChangeCallback::BlackMidiMode(_));
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
    fn test_synth_processor_event_new_channel() {
        let ev = SynthProcessorEvent::NewChannel;
        matches!(ev, SynthProcessorEvent::NewChannel);
    }

    #[test]
    fn test_synth_processor_event_all_controller_reset() {
        let ev = SynthProcessorEvent::AllControllerReset;
        matches!(ev, SynthProcessorEvent::AllControllerReset);
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
