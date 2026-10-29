//! midi_channel.rs
//! purpose: MidiChannel struct — a single MIDI channel within the synthesizer.
//! Ported from: src/synthesizer/audio_engine/channel/midi_channel.ts
//!
//! # Design note
//! TypeScript's MIDIChannel holds a back-reference `synthCore: SynthesizerCore`, which would
//! require a Rust ownership cycle (SynthesizerCore owns MidiChannel[] too). To avoid this,
//! MidiChannel does not store a reference to SynthesizerCore; methods that previously accessed
//! `this.synthCore` now receive the needed data as function parameters instead.

use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::generator_types::{
    GENERATOR_LIMITS, GENERATORS_AMOUNT, GeneratorType,
};
use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
use crate::soundbank::enums::modulator_sources;
use crate::midi::drum_parameters::{DEFAULT_DRUM_REVERB, DrumParameter, DrumParameterUtils};
use crate::synthesizer::audio_engine::channel::dynamic_modulator_system::DynamicModulatorSystem;
use crate::synthesizer::audio_engine::channel::render_voice::VoiceSendBuffer;
use crate::synthesizer::audio_engine::parameters::midi::GlobalMIDIParameter;
use crate::synthesizer::audio_engine::parameters::system::{
    DEFAULT_GLOBAL_SYSTEM_PARAMETERS, GlobalSystemParameter,
};
use crate::synthesizer::audio_engine::channel::parameters::midi::{
    CHANNEL_MIDI_PARAMETER_COUNT, ChannelMidiParameter, ChannelMidiParameterKey,
    ChannelMidiParameterValue, DEFAULT_CHANNEL_MIDI_PARAMETERS,
};
use crate::synthesizer::audio_engine::channel::reset::DEFAULT_MIDI_CONTROLLERS;
use crate::synthesizer::audio_engine::synth_constants::CONTROLLER_TABLE_SIZE;
use crate::synthesizer::audio_engine::channel::parameters::system::{
    ChannelSystemParameter, ChannelSystemParameterValue, DEFAULT_CHANNEL_SYSTEM_PARAMETERS,
};
use crate::synthesizer::audio_engine::voice::compute_modulator::{
    ChannelContext, SourceFilter, compute_modulators,
};
use crate::synthesizer::audio_engine::synth_constants::{
    GENERATOR_OVERRIDE_NO_CHANGE_VALUE, MIDI_DRUM_CHANNEL, MIN_NOTE_LENGTH,
    SPESSASYNTH_GAIN_FACTOR,
};
use crate::synthesizer::audio_engine::voice::voice::Voice;
use crate::synthesizer::types::{ChannelProperty, ChannelPropertyChangeCallback, SynthProcessorEvent};
use crate::soundbank::types::MIDISystem;
use crate::utils::loggin::{SpessaLog, spessa_synth_info};
use crate::utils::midi_hacks::BankSelectHacks;

// ---------------------------------------------------------------------------
// ChannelVibrato
// ---------------------------------------------------------------------------

/// Per-channel vibrato parameters used for GS NRPN custom vibrato.
/// Equivalent to: CustomChannelVibrato
#[derive(Clone, Debug, Default)]
pub struct ChannelVibrato {
    /// Vibrato delay in seconds.
    pub delay: f64,
    /// Vibrato depth in cents.
    pub depth: f64,
    /// Vibrato rate in Hz.
    pub rate: f64,
}

/// The drum-related system parameter locks a program change needs.
/// TS reads them from `this.synthCore.systemParameters`; a Rust channel has no core
/// reference, so the callers pass them in.
#[derive(Clone, Copy, Debug, Default)]
pub struct DrumLocks {
    /// Equivalent to: systemParameters.drumLock
    pub drum: bool,
    /// Equivalent to: systemParameters.userDrumLock
    pub user_drum: bool,
}

impl DrumLocks {
    pub fn of(sys: &GlobalSystemParameter) -> Self {
        Self {
            drum: sys.drum_lock,
            user_drum: sys.user_drum_lock,
        }
    }
}

/// The synthesizer system parameters the channel reads.
/// TS reads them from `this.synthCore.systemParameters`; the core keeps this copy
/// up to date (on channel creation and on every system parameter change).
#[derive(Clone, Copy, Debug)]
pub struct SynthSystemView {
    /// Equivalent to: systemParameters.drumLock
    pub drum_lock: bool,
    /// Equivalent to: systemParameters.nrpnParamLock
    pub nrpn_param_lock: bool,
    /// Equivalent to: systemParameters.customVibrato
    pub custom_vibrato: bool,
    /// Equivalent to: systemParameters.gain
    pub gain: f64,
    /// Equivalent to: systemParameters.pan
    pub pan: f64,
    /// Equivalent to: systemParameters.keyShift
    pub key_shift: f64,
    /// Equivalent to: systemParameters.fineTune
    pub fine_tune: f64,
    /// Equivalent to: systemParameters.pitchBasedKeyShift
    pub pitch_based_key_shift: bool,
}

impl SynthSystemView {
    pub fn of(sys: &GlobalSystemParameter) -> Self {
        Self {
            drum_lock: sys.drum_lock,
            nrpn_param_lock: sys.nrpn_param_lock,
            custom_vibrato: sys.custom_vibrato,
            gain: sys.gain,
            pan: sys.pan,
            key_shift: sys.key_shift,
            fine_tune: sys.fine_tune,
            pitch_based_key_shift: sys.pitch_based_key_shift,
        }
    }
}

impl Default for SynthSystemView {
    fn default() -> Self {
        Self::of(&DEFAULT_GLOBAL_SYSTEM_PARAMETERS)
    }
}

// ---------------------------------------------------------------------------
// MidiChannel
// ---------------------------------------------------------------------------

/// A single MIDI channel within the synthesizer.
/// Equivalent to: class MIDIChannel
pub struct MidiChannel {
    /// MIDI controller table (14-bit values, size = CONTROLLER_TABLE_SIZE).
    /// Equivalent to: midiControllers: Int16Array
    pub midi_controllers: [i16; CONTROLLER_TABLE_SIZE],

    /// Per-note pitch wheels (MIDI 2.0 per-note pitch wheel).
    /// Default value 8192 = center (no pitch bend).
    /// Equivalent to: pitchWheels: Int16Array(128).fill(8192)
    pub pitch_wheels: [i16; 128],

    /// Locked controller flags. True = locked (not allowed to change).
    /// Equivalent to: lockedControllers: boolean[]
    pub locked_controllers: Vec<bool>,

    /// The SF2 NRPN generator selector (SF2 spec section 9.6).
    /// Equivalent to: sf2NRPNGeneratorLSB
    pub sf2_nrpn_generator_lsb: i32,

    /// Per-note octave tuning (repeated every 12 notes, size 128).
    /// Equivalent to: octaveTuning: Int8Array(128)
    pub channel_octave_tuning: [i8; 128],

    /// Dynamic modulator system for advanced SysEx handling.
    /// Equivalent to: dynamicModulators: DynamicModulatorManager
    pub sys_ex_modulators: DynamicModulatorSystem,

    /// True if this is a percussion/drum channel.
    /// Equivalent to: drumChannel
    pub drum_channel: bool,

    /// An array of poly pressure values for the channel.
    /// Poly pressure persists across notes, much like per-note pitch wheels.
    /// Equivalent to: polyPressures
    pub poly_pressures: [u8; 128],

    /// Per-drum-note parameters (128 entries, one per MIDI note).
    pub drum_params: Vec<DrumParameter>,

    /// All MIDI parameters of this channel (state driven by MIDI/SysEx).
    /// Source of truth for values such as `poly_mode` and `random_pan`.
    /// Writes that must refresh the cached `current_*` values should go through
    /// `set_midi_parameter`.
    /// Equivalent to: _midiParameters (with the `midiParameters` getter)
    pub midi_parameters: ChannelMidiParameter,

    /// If a Channel MIDI parameter is locked (not allowed to change), indexed by
    /// [`ChannelMidiParameterKey`].
    /// Equivalent to: lockedMIDIParameters
    pub locked_midi_parameters: [bool; CHANNEL_MIDI_PARAMETER_COUNT],

    /// All system parameters of this channel (API-controlled).
    /// Equivalent to: _systemParameters (with the `systemParameters` getter)
    pub system_parameters: ChannelSystemParameter,

    /// If the last selected parameter was Registered (RPN). If false, the last
    /// parameter was Non-Registered (NRPN).
    /// Equivalent to: lastParameterIsRegistered
    pub last_parameter_is_registered: bool,

    /// The last pressed note on this channel, for portamento tracking. -1 means none.
    /// Strictly internal (not a MIDI parameter); set by Portamento Control CC
    /// and by the sequencer for accurate portamento recreation.
    /// Equivalent to: lastPortamentoNote
    pub last_portamento_note: i32,

    /// If the portamento should be executed once regardless of Portamento on/off.
    /// Per the MIDI spec, CC#84 (Portamento Control) ignores on/off.
    /// Equivalent to: portamentoForce
    pub portamento_force: bool,

    /// The currently selected MIDI patch (program/bank).
    /// Equivalent to: patch: MIDIPatch
    pub patch: MidiPatch,

    /// The preset currently assigned to this channel (None if not loaded).
    /// In TypeScript this is just `preset?: BasicPreset`.
    /// In Rust we store a clone since we cannot hold a reference into SoundBankManager.
    /// Equivalent to: preset?: BasicPreset
    pub preset: Option<BasicPreset>,

    /// Index into SoundBankManager.sound_bank_list for the current preset's source bank.
    /// Rust-specific: needed because BasicPreset.get_voice_parameters requires the source bank.
    pub preset_bank_idx: Option<usize>,

    /// The MIDI system when the preset was locked.
    /// Equivalent to: lockedSystem: MIDISystem
    pub locked_system: MIDISystem,

    /// Custom vibrato settings for this channel (GS NRPN).
    /// Equivalent to: customVibrato
    pub channel_vibrato: ChannelVibrato,

    /// The synthesizer system parameters (see [`SynthSystemView`]).
    pub synth_system: SynthSystemView,

    /// Current pan in range [-500;500], cached from `update_internal_params`.
    /// Equivalent to: currentPan (protected)
    current_pan: f64,

    /// Current tuning in cents, cached from `update_internal_params`.
    /// Equivalent to: currentTuning (protected)
    current_tuning: f64,

    /// Current key-shift, cached from `update_internal_params`.
    /// Equivalent to: currentKeyShift (protected)
    current_key_shift: i32,

    /// The synthesizer MIDI parameters (TS reads `this.synthCore.midiParameters`).
    /// The core keeps this copy up to date, like `synth_system`.
    pub synth_midi: GlobalMIDIParameter,

    /// Current gain, cached from `update_internal_params`.
    /// Equivalent to: currentGain (protected)
    current_gain: f64,

    /// Current voice count for this channel.
    /// Equivalent to: voiceCount
    pub voice_count: u32,

    /// If true, this skips sends as they all go through insertion.
    /// Set by the processor.
    /// Equivalent to: xgInsertionAssigned
    pub xg_insertion_assigned: bool,

    /// Current left PCM output of this channel. Will be routed to either EFX or EQ if needed.
    /// Always 0-based index.
    /// Double precision (Float64Array), as voices accumulate into it.
    /// Sized to the processor's max buffer size when the channel is created.
    /// Equivalent to: outputLeft
    pub output_left: Vec<f64>,

    /// Current right PCM output of this channel. See `output_left`.
    /// Equivalent to: outputRight
    pub output_right: Vec<f64>,

    /// If the output buffers may contain audio.
    /// If not, clearing and mixing them can be skipped.
    /// Set by the processor.
    /// Equivalent to: outputDirty
    pub output_dirty: bool,

    /// The voice send buffers of this channel.
    /// Used for grouping sends for better performance.
    /// Equivalent to: voiceSendBuffers
    pub voice_send_buffers: Vec<VoiceSendBuffer>,

    /// The number of voice send buffers used in the current rendering block.
    /// Equivalent to: voiceSendBufferCount
    pub voice_send_buffer_count: usize,

    /// This channel's 0-based index.
    /// Equivalent to: channel: number
    pub channel: u8,

    /// True if per-note pitch mode is active (MIDI 2.0).
    /// Equivalent to: perNotePitch (protected)
    pub per_note_pitch: bool,

    /// Generator offset values for SF2 NRPN support (0 = no change).
    /// Equivalent to: generators.offsets: Int16Array
    pub generator_offsets: [i16; GENERATORS_AMOUNT],

    /// True when at least one generator offset has been set.
    /// Equivalent to: generators.offsetsEnabled
    pub generator_offsets_enabled: bool,

    /// Generator override values for AWE32 support (i16::MAX = no override).
    /// Equivalent to: generators.overrides: Int16Array
    pub generator_overrides: [i16; GENERATORS_AMOUNT],

    /// True when at least one generator override has been set.
    /// Equivalent to: generators.overridesEnabled
    pub generator_overrides_enabled: bool,

    /// For Mono Mode restoring notes: `playing_notes[midi_note]` is true while that note
    /// is sounding on this channel.
    /// Equivalent to: playingNotes
    pub playing_notes: [bool; 128],

    /// Groups voices for a specific Note On message, indexed by MIDI note.
    /// Incremented on every non-internal Note On for that note (overlapping notes).
    /// Equivalent to: noteOnID
    pub note_on_id: [i32; 128],

    /// Tracks the last Note Off's note ID, indexed by MIDI note.
    /// Only advances to noteOnID once a matching Note Off is processed, so repeated
    /// Note Offs for the same instance do not release a newer overlapping instance.
    /// Equivalent to: noteOffID
    pub note_off_id: [i32; 128],

    /// The last note played in mono mode (-1 = none currently held).
    /// Strictly internal (not a MIDI parameter).
    /// Equivalent to: lastMonoNote
    pub last_mono_note: i32,

    /// The velocity of the last note played in mono mode.
    /// Strictly internal (not a MIDI parameter).
    /// Equivalent to: lastMonoVelocity
    pub last_mono_velocity: u8,
}

impl MidiChannel {
    /// Creates a new MIDI channel.
    /// Equivalent to: constructor(synthProps, preset, channelNumber)
    pub fn new(preset: Option<BasicPreset>, preset_bank_idx: Option<usize>, channel: u8) -> Self {
        let midi_controllers = DEFAULT_MIDI_CONTROLLERS;
        let mut generator_overrides = [0i16; GENERATORS_AMOUNT];
        generator_overrides.fill(GENERATOR_OVERRIDE_NO_CHANGE_VALUE);
        let mut pitch_wheels = [0i16; 128];
        pitch_wheels.fill(8192);

        let mut ch = Self {
            midi_controllers,
            pitch_wheels,
            locked_controllers: vec![false; CONTROLLER_TABLE_SIZE],
            locked_midi_parameters: [false; CHANNEL_MIDI_PARAMETER_COUNT],
            sf2_nrpn_generator_lsb: 0,
            channel_octave_tuning: [0i8; 128],
            sys_ex_modulators: DynamicModulatorSystem::new(channel as usize),
            drum_channel: false,
            midi_parameters: ChannelMidiParameter {
                // Rx Channel is initialized to the channel number.
                rx_channel: channel as i32,
                ..DEFAULT_CHANNEL_MIDI_PARAMETERS
            },
            system_parameters: DEFAULT_CHANNEL_SYSTEM_PARAMETERS,
            current_pan: 0.0,
            current_tuning: 0.0,
            current_key_shift: 0,
            synth_midi: GlobalMIDIParameter::default(),
            current_gain: 0.0,
            last_parameter_is_registered: true,
            last_portamento_note: -1,
            portamento_force: false,
            patch: MidiPatch {
                program: 0,
                bank_msb: 0,
                bank_lsb: 0,
                is_gm_gs_drum: false,
            },
            preset,
            preset_bank_idx,
            locked_system: MIDISystem::Gs,
            channel_vibrato: ChannelVibrato::default(),
            synth_system: SynthSystemView::default(),
            voice_count: 0,
            xg_insertion_assigned: false,
            output_left: Vec::new(),
            output_right: Vec::new(),
            output_dirty: true,
            voice_send_buffers: Vec::new(),
            voice_send_buffer_count: 0,
            channel,
            per_note_pitch: false,
            generator_offsets: [0i16; GENERATORS_AMOUNT],
            generator_offsets_enabled: false,
            generator_overrides,
            generator_overrides_enabled: false,
            poly_pressures: [0; 128],
            drum_params: (0..128).map(DrumParameterUtils::default_data).collect(),
            playing_notes: [false; 128],
            note_on_id: [0; 128],
            note_off_id: [0; 128],
            last_mono_note: -1,
            last_mono_velocity: 0,
        };
        ch.update_internal_params();
        ch
    }

    /// Returns the effective MIDI system for this channel.
    /// When the preset is locked, returns the system it was locked under;
    /// otherwise returns the supplied current system.
    /// Equivalent to: get channelSystem()
    pub fn channel_system(&self, current_system: MIDISystem) -> MIDISystem {
        if self.system_parameters.preset_lock {
            self.locked_system
        } else {
            current_system
        }
    }

    /// Resets the voice count. Call once per render quantum before rendering.
    /// Equivalent to: clearVoiceCount()
    pub fn clear_voice_count(&mut self) {
        self.voice_count = 0;
    }

    /// Sends the channel property as an event.
    /// Returns Some(event) if the event system is enabled.
    /// Not in TS (it has no `channelPropertyChange` event; API only, no effect on the
    /// MIDI-to-WAV output). Kept for the Rust event consumers.
    pub fn build_channel_property_event(
        &self,
        enable_event_system: bool,
    ) -> Option<SynthProcessorEvent> {
        if !enable_event_system {
            return None;
        }
        let pitch_wheel = self.midi_parameters.pitch_wheel as u16;
        let pitch_wheel_range = self.midi_parameters.pitch_wheel_range;
        let transposition =
            self.system_parameters.key_shift + self.system_parameters.fine_tune / 100.0;
        Some(SynthProcessorEvent::ChannelPropertyChange(
            ChannelPropertyChangeCallback {
                channel: self.channel,
                property: ChannelProperty {
                    voices_amount: self.voice_count,
                    pitch_wheel,
                    pitch_wheel_range,
                    is_muted: self.system_parameters.is_muted,
                    is_drum: self.drum_channel,
                    transposition,
                },
            },
        ))
    }

    /// Sets the last pressed note. Strictly internal, used by the sequencer for
    /// very accurate portamento recreation.
    /// Equivalent to: setLastNote(midiNote)
    pub fn set_last_note(&mut self, midi_note: i32) {
        self.last_portamento_note = midi_note;
    }

    /// Sets the octave tuning for all 128 notes (repeated from 12-element array).
    /// Equivalent to: setOctaveTuning(tuning: Int8Array)
    pub fn set_octave_tuning(&mut self, tuning: &[i8; 12]) {
        for i in 0..128usize {
            self.channel_octave_tuning[i] = tuning[i % 12];
        }
    }

    /// Sets the modulation depth in cents and logs it.
    /// Equivalent to: setMIDIParameter("modulationDepth", cents) followed by the
    /// `Modulation depth for ...` log (RPN 5 in data_entry.ts, GS LFO1 pitch depth in roland.ts).
    pub fn set_modulation_depth(&mut self, cents: f64) {
        // Stored in cents and converted to a multiplier (dividing by 50) at the point of use
        // in `compute_single_modulator`.
        self.set_midi_parameter(ChannelMidiParameterValue::ModulationDepth(cents));
        spessa_synth_info(&format!(
            "Channel {} modulation depth. Cents: {}",
            self.channel,
            cents.round()
        ));
    }

    /// Sets the pitch wheel for this channel (or per-note if midi_note >= 0).
    /// Returns events to dispatch (pitch wheel event, optionally channel property).
    /// Equivalent to: pitchWheel(pitch, midiNote = -1)
    pub fn pitch_wheel(
        &mut self,
        voices: &mut [Voice],
        pitch: i16,
        midi_note: i32,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        let mut events = Vec::new();

        if midi_note == -1 {
            // Disable the per note pitch mode
            self.per_note_pitch = false;
            // setMIDIParameter("pitchWheel", pitch): the lock check, then the recompute
            if self.locked_midi_parameters[ChannelMidiParameterKey::PitchWheel as usize] {
                return events;
            }
            self.set_midi_parameter(ChannelMidiParameterValue::PitchWheel(pitch as i32));
            self.compute_modulators_all_impl(voices, 0, modulator_sources::PITCH_WHEEL as usize);
            if let Some(ev) = self.build_channel_property_event(enable_event_system) {
                events.push(ev);
            }
        } else {
            if !self.per_note_pitch {
                // Enable the per-note pitch (fill the pitches with the current CC value)
                self.pitch_wheels.fill(self.midi_parameters.pitch_wheel as i16);
            }
            self.per_note_pitch = true;

            self.pitch_wheels[midi_note as usize] = pitch;

            // Recompute only specific modulators
            self.compute_modulators_all_impl(voices, 0, modulator_sources::PITCH_WHEEL as usize);
        }

        use crate::synthesizer::types::PitchWheelCallback;
        events.push(SynthProcessorEvent::PitchWheel(PitchWheelCallback {
            channel: self.channel,
            pitch: pitch as u16,
            midi_note,
        }));
        events
    }

    /// Sets the channel pressure (aftertouch).
    /// Returns events to dispatch.
    /// Equivalent to: channelPressure(pressure)
    pub fn channel_pressure(
        &mut self,
        voices: &mut [Voice],
        pressure: u8,
    ) -> Vec<SynthProcessorEvent> {
        // setMIDIParameter("pressure", pressure): the lock check, then the recompute
        if self.locked_midi_parameters[ChannelMidiParameterKey::Pressure as usize] {
            return Vec::new();
        }
        self.set_midi_parameter(ChannelMidiParameterValue::Pressure(pressure as i32));
        self.compute_modulators_all_impl(voices, 0, modulator_sources::CHANNEL_PRESSURE as usize);

        use crate::synthesizer::types::ChannelPressureCallback;
        vec![SynthProcessorEvent::ChannelPressure(
            ChannelPressureCallback {
                channel: self.channel,
                pressure,
            },
        )]
    }

    /// Sets polyphonic key pressure on a specific note.
    /// Returns events to dispatch.
    /// Equivalent to: polyPressure(midiNote, pressure)
    pub fn poly_pressure(
        &mut self,
        voices: &mut [Voice],
        midi_note: u8,
        pressure: u8,
    ) -> Vec<SynthProcessorEvent> {
        // Note to self: Store the pressure first, the modulators read it
        self.poly_pressures[midi_note as usize] = pressure;
        self.compute_modulators_all_impl(voices, 0, modulator_sources::POLY_PRESSURE as usize);
        use crate::synthesizer::types::PolyPressureCallback;
        vec![SynthProcessorEvent::PolyPressure(PolyPressureCallback {
            channel: self.channel,
            midi_note,
            pressure,
        })]
    }

    /// Equivalent to: resetVibratoParams()
    pub(crate) fn reset_vibrato_params(&mut self) {
        if !self.synth_system.custom_vibrato {
            return;
        }
        self.channel_vibrato.rate = 0.0;
        self.channel_vibrato.depth = 0.0;
        self.channel_vibrato.delay = 0.0;
    }

    /// Resets the drum parameters of a drum channel to the defaults.
    /// Mirrors reverb as the chorus and variation send on XG only (GS has none by default).
    /// `drum_lock` is `systemParameters.drumLock`.
    /// Equivalent to: resetDrumParams()
    pub fn reset_drum_params(&mut self, drum_lock: bool, current_system: MIDISystem) {
        if drum_lock || !self.drum_channel {
            return;
        }
        let is_xg = self.channel_system(current_system) == MIDISystem::Xg;
        for index in 0..128 {
            let p = &mut self.drum_params[index];
            DrumParameterUtils::copy_into(&DrumParameterUtils::default_data(index), p);
            // Mirror reverb on XG only, GS has no chorus by default
            p.chorus_send = if is_xg { DEFAULT_DRUM_REVERB[index] } else { 0 };
            p.variation_send = if is_xg { DEFAULT_DRUM_REVERB[index] } else { 0 };
        }
    }

    /// Resets all generator overrides to the "no override" sentinel value.
    /// Equivalent to: resetGeneratorOverrides()
    pub fn reset_generator_overrides(&mut self) {
        self.generator_overrides
            .fill(GENERATOR_OVERRIDE_NO_CHANGE_VALUE);
        self.generator_overrides_enabled = false;
    }

    /// Sets a generator override (AWE32 support).
    /// If `realtime`, immediately applies to all active voices on this channel.
    /// Equivalent to: setGeneratorOverride(gen, value, realtime = false)
    pub fn set_generator_override(
        &mut self,
        r#gen: GeneratorType,
        value: i16,
        realtime: bool,
        voices: &mut [Voice],
    ) {
        self.generator_overrides[r#gen as usize] = value;
        self.generator_overrides_enabled = true;
        if realtime {
            let mut vc = 0u32;
            if self.voice_count > 0 {
                for v in voices.iter_mut() {
                    if v.channel == self.channel && v.is_active {
                        v.generators[r#gen as usize] = value;
                        let mut modulated = v.modulated_generators;
                        compute_modulators(self, v, &mut modulated, SourceFilter::All, 0);
                        v.modulated_generators = modulated;
                        vc += 1;
                        if vc >= self.voice_count {
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Resets all generator offsets to zero.
    /// Equivalent to: resetGeneratorOffsets()
    pub fn reset_generator_offsets(&mut self) {
        self.generator_offsets.fill(0);
        self.generator_offsets_enabled = false;
    }

    /// Sets a generator offset (SF2 NRPN support).
    /// Immediately applies to all active voices on this channel.
    /// Equivalent to: setGeneratorOffset(gen, value)
    pub fn set_generator_offset(&mut self, r#gen: GeneratorType, value: i16, voices: &mut [Voice]) {
        self.generator_offsets[r#gen as usize] =
            (value as f64 * GENERATOR_LIMITS[r#gen as usize].map_or(0.0, |l| l.nrpn as f64)) as i16;
        self.generator_offsets_enabled = true;
        let mut vc = 0u32;
        if self.voice_count > 0 {
            for v in voices.iter_mut() {
                if v.channel == self.channel && v.is_active {
                    let mut modulated = v.modulated_generators;
                    compute_modulators(self, v, &mut modulated, SourceFilter::All, 0);
                    v.modulated_generators = modulated;
                    vc += 1;
                    if vc >= self.voice_count {
                        break;
                    }
                }
            }
        }
    }

    /// Stops a note nearly instantly by setting a very short release.
    /// Equivalent to: killNote(midiNote, releaseTime = -12000)
    pub fn kill_note(
        &mut self,
        midi_note: u8,
        release_time: i32,
        voices: &mut [Voice],
        current_time: f64,
    ) {
        // Clear the note-on/off IDs for this note so the next Note On starts a fresh group.
        self.note_off_id[midi_note as usize] = 0;
        self.note_on_id[midi_note as usize] = 0;
        let mut vc = 0u32;
        if self.voice_count > 0 {
            for v in voices.iter_mut() {
                if v.channel == self.channel && v.is_active && v.midi_note == midi_note {
                    v.override_release_vol_env = release_time;
                    v.is_in_release = false;
                    v.release_voice(current_time, MIN_NOTE_LENGTH);
                    vc += 1;
                    if vc >= self.voice_count {
                        break;
                    }
                }
            }
        }
    }

    /// Stops all notes on this channel.
    /// Returns events to dispatch.
    /// Equivalent to: stopAllNotes(force = false)
    pub fn stop_all_notes(
        &mut self,
        voices: &mut [Voice],
        current_time: f64,
        force: bool,
    ) -> Vec<SynthProcessorEvent> {
        self.stop_all_notes_impl(voices, current_time, force);
        use crate::synthesizer::types::StopAllCallback;
        vec![SynthProcessorEvent::StopAll(StopAllCallback {
            channel: self.channel,
            force,
        })]
    }

    /// Internal helper that modifies voices without returning events.
    fn stop_all_notes_impl(&mut self, voices: &mut [Voice], current_time: f64, force: bool) {
        // Clear IDs.
        self.note_on_id.fill(0);
        self.note_off_id.fill(0);
        self.playing_notes.fill(false);
        if force {
            let mut vc = 0u32;
            if self.voice_count > 0 {
                for v in voices.iter_mut() {
                    if v.channel == self.channel && v.is_active {
                        v.is_active = false;
                        vc += 1;
                        if vc >= self.voice_count {
                            break;
                        }
                    }
                }
            }
            self.clear_voice_count();
        } else {
            let mut vc = 0u32;
            if self.voice_count > 0 {
                for v in voices.iter_mut() {
                    if v.channel == self.channel && v.is_active {
                        v.release_voice(current_time, MIN_NOTE_LENGTH);
                        vc += 1;
                        if vc >= self.voice_count {
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Recomputes modulators for all active voices on this channel triggered by a given source.
    /// Equivalent to: computeModulatorsAll(sourceUsesCC, sourceIndex) (protected)
    pub fn compute_modulators_all_impl(
        &mut self,
        voices: &mut [Voice],
        source_uses_cc: i8,
        source_index: usize,
    ) {
        let filter = match source_uses_cc {
            0 => SourceFilter::NonCC,
            1 => SourceFilter::CC,
            _ => SourceFilter::All,
        };

        let mut vc = 0u32;
        if self.voice_count > 0 {
            for v in voices.iter_mut() {
                if v.channel == self.channel && v.is_active {
                    let mut modulated = v.modulated_generators;
                    compute_modulators(self, v, &mut modulated, filter, source_index);
                    v.modulated_generators = modulated;
                    vc += 1;
                    if vc >= self.voice_count {
                        break;
                    }
                }
            }
        }
    }

    /// Sets bank MSB unless the preset is locked.
    /// Equivalent to: setBankMSB(bankMSB) (protected)
    pub fn set_bank_msb(&mut self, bank_msb: u8) {
        if !self.system_parameters.preset_lock {
            self.patch.bank_msb = bank_msb;
        }
    }

    /// Sets bank LSB unless the preset is locked.
    /// Equivalent to: setBankLSB(bankLSB) (protected)
    pub fn set_bank_lsb(&mut self, bank_lsb: u8) {
        if !self.system_parameters.preset_lock {
            self.patch.bank_lsb = bank_lsb;
        }
    }

    /// Sets the GM/GS drum flag of the patch, unless the preset is locked.
    /// Equivalent to: setIsGMGSDrum(isGMGSDrum) (protected)
    pub fn set_is_gm_gs_drum(&mut self, is_gm_gs_drum: bool) {
        if self.system_parameters.preset_lock {
            return;
        }
        self.patch.is_gm_gs_drum = is_gm_gs_drum;
    }

    /// Sets drums on channel.
    /// Does not require a preset, emits no event, and refreshes the cached
    /// internal parameters (drum channels ignore the key shift and tuning).
    /// Equivalent to: setDrumFlag(isDrum) (protected)
    pub fn set_drum_flag(&mut self, is_drum: bool) {
        if self.system_parameters.preset_lock || self.drum_channel == is_drum {
            return;
        }

        self.drum_channel = is_drum;
        self.update_internal_params();
    }

    /// Toggles drums on the channel and keeps the current program number.
    /// Executes a program change so the change is immediately audible.
    /// This does _not_ bypass `presetLock`.
    /// Equivalent to: setDrums(isDrum)
    pub fn set_drums(
        &mut self,
        is_drum: bool,
        sound_bank_manager: &crate::synthesizer::audio_engine::sound_bank_manager::SoundBankManager,
        current_system: MIDISystem,
        locks: DrumLocks,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        if self.system_parameters.preset_lock {
            return Vec::new();
        }

        let ch_system = self.channel_system(current_system);
        if BankSelectHacks::is_system_xg(ch_system) {
            if is_drum {
                if BankSelectHacks::is_xg_drum(self.patch.bank_msb) {
                    return Vec::new();
                }
                if let Some(drum_bank) = BankSelectHacks::get_drum_bank(ch_system) {
                    self.set_bank_msb(drum_bank);
                }
            } else {
                if self.channel % 16 == MIDI_DRUM_CHANNEL {
                    SpessaLog::warn(&format!(
                        "Cannot disable drums on channel {} for XG.",
                        self.channel
                    ));
                    return Vec::new();
                }
                self.set_bank_msb(BankSelectHacks::get_default_bank(ch_system));
            }

            // Commit the changes and return
            let program = self.patch.program;
            return self.program_change(
                program,
                sound_bank_manager,
                current_system,
                locks,
                enable_event_system,
            );
        }
        if is_drum == self.drum_channel {
            return Vec::new();
        }
        // Flip the drums for GS
        self.set_is_gm_gs_drum(is_drum);
        let program = self.patch.program;
        let events =
            self.program_change(program, sound_bank_manager, current_system, locks, enable_event_system);
        // Fallback if no preset matched and the flag didn't sync
        self.set_drum_flag(is_drum);
        events
    }

    // -----------------------------------------------------------------------
    // Channel parameter infrastructure
    // -----------------------------------------------------------------------

    /// Locks or unlocks a given controller.
    /// Equivalent to: lockController(controller, isLocked)
    pub fn lock_controller(&mut self, controller: usize, is_locked: bool) {
        self.locked_controllers[controller] = is_locked;
    }

    /// Locks or unlocks a given Channel MIDI Parameter.
    /// This prevents any changes to it until it's unlocked.
    /// Equivalent to: lockMIDIParameter(parameter, isLocked)
    pub fn lock_midi_parameter(&mut self, parameter: ChannelMidiParameterKey, is_locked: bool) {
        self.locked_midi_parameters[parameter as usize] = is_locked;
    }

    /// Sets a channel MIDI parameter and refreshes the cached `current_*` values.
    /// Equivalent to: setMIDIParameter(parameter, value)
    ///
    /// # Note
    /// The TS version recomputes the modulators for `pitchWheel`/`pressure` here. Rust has
    /// no voices in this call, so those two go through `pitch_wheel`/`channel_pressure`,
    /// which check the lock, set the value with this method and recompute. The
    /// `channelParamChange` event is not ported (API only, no effect on the output).
    pub fn set_midi_parameter(&mut self, value: ChannelMidiParameterValue) {
        use ChannelMidiParameterValue::*;
        if self.locked_midi_parameters[value.key() as usize] {
            return;
        }
        match value {
            Pressure(v) => self.midi_parameters.pressure = v,
            PitchWheel(v) => self.midi_parameters.pitch_wheel = v,
            PitchWheelRange(v) => self.midi_parameters.pitch_wheel_range = v,
            ModulationDepth(v) => self.midi_parameters.modulation_depth = v,
            RxChannel(v) => self.midi_parameters.rx_channel = v,
            PolyMode(v) => self.midi_parameters.poly_mode = v,
            KeyShift(v) => self.midi_parameters.key_shift = v,
            FineTune(v) => self.midi_parameters.fine_tune = v,
            RandomPan(v) => self.midi_parameters.random_pan = v,
            AssignMode(v) => self.midi_parameters.assign_mode = v,
            EfxAssign(v) => {
                self.midi_parameters.efx_assign = v;
            }
            Cc1(v) => self.midi_parameters.cc1 = v,
            Cc2(v) => self.midi_parameters.cc2 = v,
            DrumMap(v) => self.midi_parameters.drum_map = v,
            DryLevel(v) => self.midi_parameters.dry_level = v,
            VelocitySenseDepth(v) => self.midi_parameters.velocity_sense_depth = v,
            VelocitySenseOffset(v) => self.midi_parameters.velocity_sense_offset = v,
        }
        self.update_internal_params();
    }

    /// Sets a channel system parameter and refreshes the cached `current_*` values.
    /// `voices`/`current_time` are needed by the notes stopped on `isMuted` and `keyShift`.
    /// Equivalent to: setSystemParameterInternal(parameter, value)
    pub fn set_system_parameter(
        &mut self,
        value: ChannelSystemParameterValue,
        voices: &mut [Voice],
        current_time: f64,
    ) {
        use ChannelSystemParameterValue::*;
        let prev = self.system_parameters.clone();
        match value {
            PresetLock(v) => self.system_parameters.preset_lock = v,
            IsMuted(v) => self.system_parameters.is_muted = v,
            Gain(v) => self.system_parameters.gain = v,
            Pan(v) => self.system_parameters.pan = v,
            KeyShift(v) => self.system_parameters.key_shift = v,
            FineTune(v) => self.system_parameters.fine_tune = v,
            InterpolationType(v) => self.system_parameters.interpolation_type = v,
            NrpnParamLock(v) => self.system_parameters.nrpn_param_lock = v,
            MonophonicRetrigger(v) => self.system_parameters.monophonic_retrigger = v,
        }
        // if (this._systemParameters[parameter] === value) return;
        if self.system_parameters == prev {
            return;
        }
        self.update_internal_params();
        // Additional handling for specific parameters
        match value {
            PresetLock(true) => {
                self.locked_system = self.synth_midi.system;
            }
            IsMuted(true) => {
                self.stop_all_notes_impl(voices, current_time, true);
            }
            KeyShift(v) if !self.drum_channel && prev.key_shift != v => {
                self.stop_all_notes_impl(voices, current_time, true);
            }
            _ => {}
        }
    }

    /// Recomputes the cached `current_pan/current_gain/current_tuning/current_key_shift`
    /// from the synthesizer's and the channel's system and MIDI parameters.
    /// Equivalent to: updateInternalParams()
    pub fn update_internal_params(&mut self) {
        let global_system = &self.synth_system;
        let channel_system = &self.system_parameters;
        let global_midi = &self.synth_midi;
        let channel_midi = &self.midi_parameters;
        // Note:
        // - System -> System Parameter
        // - MIDI -> MIDI Parameter

        // Global MIDI + Channel MIDI
        let midi_key_shift = global_midi.key_shift + channel_midi.key_shift;
        let pitch_based = global_system.pitch_based_key_shift;

        // Only Channel System is processed for drum channels.
        // Drum channels ignore key shift
        // Testcase: th07_19_user_gm.mid
        let current_key_shift = if self.drum_channel {
            channel_system.key_shift
        } else {
            // Global System
            global_system.key_shift
                // Global MIDI + Channel MIDI (unless pitch based)
                + (if pitch_based { 0.0 } else { midi_key_shift })
                // Channel System
                + channel_system.key_shift
        };
        // Ensure integer
        self.current_key_shift = current_key_shift.trunc() as i32;

        // Only Channel System is processed for drum channels
        self.current_tuning = if self.drum_channel {
            channel_system.fine_tune
        } else {
            // Global System
            global_system.fine_tune
                // Global MIDI
                + global_midi.fine_tune
                // Channel System
                + channel_system.fine_tune
                // Channel MIDI
                + channel_midi.fine_tune
                // MIDI key shift as semitone pitch change
                + (if pitch_based { midi_key_shift * 100.0 } else { 0.0 })
        };

        // [-1;1] normalized
        let current_pan_normalized =
            // Global System
            global_system.pan
                // Global MIDI
                + global_midi.pan
                // Channel System
                + channel_system.pan;
        // Channel MIDI is the pan controller

        // For faster renderVoice calculation
        self.current_pan = current_pan_normalized * 500.0;

        self.current_gain =
            // Global forced
            SPESSASYNTH_GAIN_FACTOR
                // Global System
                * global_system.gain
                // Global MIDI
                // It corresponds to CC volume, so volume is squared.
                * global_midi.volume.powi(2)
                // Channel System
                * channel_system.gain;
        // Channel MIDI are the volume/expression controllers
    }

    /// Current pan in range [-500;500]. Equivalent to: currentPan
    pub fn current_pan(&self) -> f64 {
        self.current_pan
    }

    /// Current tuning in cents. Equivalent to: currentTuning
    pub fn current_tuning(&self) -> f64 {
        self.current_tuning
    }

    /// Current key-shift. Equivalent to: currentKeyShift
    pub fn current_key_shift(&self) -> i32 {
        self.current_key_shift
    }

    /// Current gain. Equivalent to: currentGain
    pub fn current_gain(&self) -> f64 {
        self.current_gain
    }

    // -----------------------------------------------------------------------
    // Stubs for methods ported from external files
    // -----------------------------------------------------------------------

    // note_off is implemented in channel/note_off.rs
    // program_change is implemented in channel/program_change.rs

    // controller_change is implemented in channel/controller_change.rs
    // reset_controllers, reset_preset, reset_controllers_rp15, reset_parameters
    //   are implemented in channel/reset.rs
    // data_entry is implemented in channel/data_entry.rs

    // render_voice is implemented in channel/render_voice.rs
}

// ---------------------------------------------------------------------------
// ChannelContext trait implementation for MidiChannel
// ---------------------------------------------------------------------------

impl ChannelContext for MidiChannel {
    fn generator_offsets_enabled(&self) -> bool {
        self.generator_offsets_enabled
    }

    fn generator_offsets(&self) -> &[i16] {
        &self.generator_offsets
    }

    fn per_note_pitch(&self) -> bool {
        self.per_note_pitch
    }

    fn pitch_wheels(&self) -> &[i16] {
        &self.pitch_wheels
    }

    fn midi_controllers(&self) -> &[i16] {
        &self.midi_controllers
    }

    fn pitch_wheel(&self) -> i16 {
        // The pitch wheel is a 14-bit value (0..=16383), so it always fits in i16.
        self.midi_parameters.pitch_wheel as i16
    }

    fn modulation_depth(&self) -> f64 {
        self.midi_parameters.modulation_depth
    }

    fn channel_pressure(&self) -> i32 {
        self.midi_parameters.pressure
    }

    fn pitch_wheel_range(&self) -> f64 {
        self.midi_parameters.pitch_wheel_range
    }

    fn poly_pressure(&self, midi_note: usize) -> u8 {
        self.poly_pressures[midi_note]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// `updateInternalParams` (non-drum branch):
    /// `currentKeyShift = globalSystem.keyShift + globalMIDI.keyShift +
    /// channelSystem.keyShift + channelMIDI.keyShift`, truncated to an integer.
    /// With global MIDI key shift = 6 and channel MIDI key shift = 3, the
    /// sum is 0 + 6 + 0 + 3 = 9.
    #[test]
    fn test_current_key_shift_sums_global_and_channel_for_non_drum_channel() {
        let mut channel = MidiChannel::new(None, None, 0);
        assert!(!channel.drum_channel);

        channel.synth_midi.key_shift = 6.0;
        channel.set_midi_parameter(ChannelMidiParameterValue::KeyShift(3.0));

        assert_eq!(channel.current_key_shift(), 9);
    }

    /// `updateInternalParams` (drum branch): drum channels ignore the
    /// global and channel MIDI key shifts entirely and use only
    /// `channelSystem.keyShift`, which defaults to 0 for GS. So even with the same
    /// global/channel-MIDI key shifts as the non-drum test, a drum channel's
    /// `current_key_shift()` stays 0.
    #[test]
    fn test_current_key_shift_ignores_global_and_channel_midi_for_drum_channel() {
        let mut channel = MidiChannel::new(None, None, MIDI_DRUM_CHANNEL);
        channel.drum_channel = true;

        channel.synth_midi.key_shift = 6.0;
        channel.set_midi_parameter(ChannelMidiParameterValue::KeyShift(3.0));

        assert_eq!(channel.current_key_shift(), 0);
    }

    /// `updateInternalParams`: drum channels use only the channel system fine tune,
    /// so the global and the channel MIDI fine tunes (RPN fine tune, GS part fine tune,
    /// universal master fine tune) do not reach them.
    #[test]
    fn test_current_tuning_ignores_global_and_channel_midi_for_drum_channel() {
        let mut melodic = MidiChannel::new(None, None, 0);
        let mut drum = MidiChannel::new(None, None, MIDI_DRUM_CHANNEL);
        drum.drum_channel = true;
        for ch in [&mut melodic, &mut drum] {
            ch.synth_system.fine_tune = 5.0;
            ch.synth_midi.fine_tune = 30.0;
            ch.set_system_parameter(ChannelSystemParameterValue::FineTune(2.0), &mut [], 0.0);
            ch.set_midi_parameter(ChannelMidiParameterValue::FineTune(40.0));
        }
        assert!((melodic.current_tuning() - 77.0).abs() < 1e-12);
        assert!((drum.current_tuning() - 2.0).abs() < 1e-12);
    }

    /// `updateInternalParams`: the gain folds in the system gain, the squared global
    /// MIDI volume and the channel system gain; the pan sums the global system,
    /// global MIDI and channel system pans.
    #[test]
    fn test_current_gain_and_pan_fold_global_parameters() {
        let mut channel = MidiChannel::new(None, None, 0);
        channel.synth_system.gain = 2.0;
        channel.synth_system.pan = 0.25;
        channel.synth_midi.volume = 0.5;
        channel.synth_midi.pan = -0.5;
        channel.set_system_parameter(ChannelSystemParameterValue::Gain(0.5), &mut [], 0.0);
        assert!((channel.current_gain() - SPESSASYNTH_GAIN_FACTOR * 2.0 * 0.25 * 0.5).abs() < 1e-12);
        assert!((channel.current_pan() - (0.25 - 0.5) * 500.0).abs() < 1e-12);
    }

    /// `presetLock` is the channel system parameter: locking records the current system
    /// as the channel system and blocks bank changes (TS `channelSystem`, `setBankMSB`).
    #[test]
    fn test_preset_lock_system_parameter_locks_channel_system() {
        let mut ch = MidiChannel::new(None, None, 0);
        ch.synth_midi.system = MIDISystem::Xg;
        ch.set_system_parameter(ChannelSystemParameterValue::PresetLock(true), &mut [], 0.0);
        ch.synth_midi.system = MIDISystem::Gs;
        assert_eq!(ch.channel_system(MIDISystem::Gs), MIDISystem::Xg);
        ch.set_bank_msb(8);
        assert_eq!(ch.patch.bank_msb, 0);
        ch.set_system_parameter(ChannelSystemParameterValue::PresetLock(false), &mut [], 0.0);
        assert_eq!(ch.channel_system(MIDISystem::Gs), MIDISystem::Gs);
    }

    /// `isMuted` and a changed `keyShift` (melodic channels only) force-stop the notes.
    #[test]
    fn test_mute_and_key_shift_system_parameters_stop_notes() {
        let mut voices = vec![Voice::new(44_100.0, 128), Voice::new(44_100.0, 128)];
        let start = |ch: &mut MidiChannel, voices: &mut Vec<Voice>| {
            for v in voices.iter_mut() {
                v.channel = ch.channel as _;
                v.is_active = true;
            }
            ch.voice_count = voices.len() as u32;
        };
        let mut ch = MidiChannel::new(None, None, 0);
        start(&mut ch, &mut voices);
        ch.set_system_parameter(ChannelSystemParameterValue::IsMuted(true), &mut voices, 0.0);
        assert!(voices.iter().all(|v| !v.is_active));

        start(&mut ch, &mut voices);
        ch.set_system_parameter(ChannelSystemParameterValue::KeyShift(2.0), &mut voices, 0.0);
        assert!(voices.iter().all(|v| !v.is_active));

        // Same value: no change, no stop
        start(&mut ch, &mut voices);
        ch.set_system_parameter(ChannelSystemParameterValue::KeyShift(2.0), &mut voices, 0.0);
        assert!(voices.iter().all(|v| v.is_active));

        // Drum channels keep their notes on a key shift change
        let mut drum = MidiChannel::new(None, None, MIDI_DRUM_CHANNEL);
        drum.drum_channel = true;
        start(&mut drum, &mut voices);
        drum.set_system_parameter(ChannelSystemParameterValue::KeyShift(3.0), &mut voices, 0.0);
        assert!(voices.iter().all(|v| v.is_active));
    }

    /// `setMIDIParameterInternal` returns early for a locked parameter, so the pitch wheel
    /// and channel pressure MIDI messages leave a locked value alone.
    #[test]
    fn test_locked_midi_parameters_ignore_changes() {
        let mut channel = MidiChannel::new(None, None, 0);
        let mut voices: Vec<Voice> = Vec::new();
        MidiChannel::pitch_wheel(&mut channel, &mut voices, 1000, -1, false);
        MidiChannel::channel_pressure(&mut channel, &mut voices, 30);
        channel.lock_midi_parameter(ChannelMidiParameterKey::PitchWheel, true);
        channel.lock_midi_parameter(ChannelMidiParameterKey::Pressure, true);
        MidiChannel::pitch_wheel(&mut channel, &mut voices, 9000, -1, false);
        MidiChannel::channel_pressure(&mut channel, &mut voices, 100);
        assert_eq!(channel.midi_parameters.pitch_wheel, 1000);
        assert_eq!(channel.midi_parameters.pressure, 30);
        // The per-note pitch mode is still disabled by a locked global pitch wheel.
        MidiChannel::pitch_wheel(&mut channel, &mut voices, 500, 60, false);
        assert!(channel.per_note_pitch);
        assert_eq!(channel.pitch_wheels[61], 1000);
        MidiChannel::pitch_wheel(&mut channel, &mut voices, 9000, -1, false);
        assert!(!channel.per_note_pitch);

        channel.lock_midi_parameter(ChannelMidiParameterKey::PitchWheel, false);
        MidiChannel::pitch_wheel(&mut channel, &mut voices, 9000, -1, false);
        assert_eq!(channel.midi_parameters.pitch_wheel, 9000);
    }
}
