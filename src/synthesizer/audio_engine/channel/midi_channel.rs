/// midi_channel.rs
/// purpose: MidiChannel struct — a single MIDI channel within the synthesizer.
/// Ported from: src/synthesizer/audio_engine/engine_components/midi_channel.ts
///
/// # Design note
/// TypeScript's MIDIChannel holds a back-reference `synthCore: SynthesizerCore`, which would
/// require a Rust ownership cycle (SynthesizerCore owns MidiChannel[] too). To avoid this,
/// MidiChannel does not store a reference to SynthesizerCore; methods that previously accessed
/// `this.synthCore` now receive the needed data as function parameters instead.
use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::generator_types::{
    GENERATOR_LIMITS, GENERATORS_AMOUNT, GeneratorType,
};
use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
use crate::soundbank::enums::modulator_sources;
use crate::synthesizer::audio_engine::channel::drum_parameters::DrumParameters;
use crate::synthesizer::audio_engine::channel::dynamic_modulator_system::DynamicModulatorSystem;
use crate::synthesizer::audio_engine::channel::parameters::midi::{
    CONTROLLER_TABLE_SIZE, CUSTOM_CONTROLLER_TABLE_SIZE, CUSTOM_RESET_ARRAY,
    ChannelMidiParameter, ChannelMidiParameterValue, DEFAULT_CHANNEL_MIDI_PARAMETERS,
    DEFAULT_MIDI_CONTROLLER_VALUES, NON_CC_INDEX_OFFSET,
};
use crate::synthesizer::audio_engine::channel::parameters::system::{
    ChannelSystemParameter, ChannelSystemParameterValue, DEFAULT_CHANNEL_SYSTEM_PARAMETERS,
};
use crate::synthesizer::audio_engine::voice::compute_modulator::{
    ChannelContext, SourceFilter, compute_modulators,
};
use crate::synthesizer::audio_engine::synth_constants::{
    GENERATOR_OVERRIDE_NO_CHANGE_VALUE, MIN_NOTE_LENGTH, SPESSASYNTH_GAIN_FACTOR,
};
use crate::synthesizer::audio_engine::voice::voice::Voice;
use crate::synthesizer::enums::custom_controllers;
use crate::synthesizer::types::{ChannelProperty, ChannelPropertyChangeCallback, SynthProcessorEvent};
use crate::soundbank::types::MIDISystem;
use crate::utils::loggin::spessa_synth_info;
use crate::utils::midi_hacks::BankSelectHacks;

// ---------------------------------------------------------------------------
// ChannelVibrato
// ---------------------------------------------------------------------------

/// Per-channel vibrato parameters used for GS NRPN custom vibrato.
/// Equivalent to: channelVibrato inline object in MIDIChannel
#[derive(Clone, Debug, Default)]
pub struct ChannelVibrato {
    /// Vibrato delay in seconds.
    pub delay: f64,
    /// Vibrato depth in cents.
    pub depth: f64,
    /// Vibrato rate in Hz.
    pub rate: f64,
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

    /// Custom (non-SF2) controller values: tuning, modulation depth, etc.
    /// Equivalent to: customControllers: Float32Array
    pub custom_controllers: [f32; CUSTOM_CONTROLLER_TABLE_SIZE],

    /// Key shift of the channel in semitones.
    /// Equivalent to: channelTransposeKeyShift
    pub channel_transpose_key_shift: i16,

    /// Per-note octave tuning (repeated every 12 notes, size 128).
    /// Equivalent to: channelOctaveTuning: Int8Array(128)
    pub channel_octave_tuning: [i8; 128],

    /// Dynamic modulator system for advanced SysEx handling.
    /// Equivalent to: sysExModulators: DynamicModulatorSystem
    pub sys_ex_modulators: DynamicModulatorSystem,

    /// True if this is a percussion/drum channel.
    /// Equivalent to: drumChannel
    pub drum_channel: bool,

    /// Per-drum-note parameters (128 entries, one per MIDI note).
    pub drum_params: Vec<DrumParameters>,

    /// All MIDI parameters of this channel (state driven by MIDI/SysEx).
    /// Source of truth for values such as `poly_mode` and `random_pan`.
    /// Writes that must refresh the cached `current_*` values should go through
    /// `set_midi_parameter`.
    /// Equivalent to: _midiParameters (with the `midiParameters` getter)
    pub midi_parameters: ChannelMidiParameter,

    /// All system parameters of this channel (API-controlled).
    /// Equivalent to: _systemParameters (with the `systemParameters` getter)
    pub system_parameters: ChannelSystemParameter,

    /// If the last selected parameter was Registered (RPN). If false, the last
    /// parameter was Non-Registered (NRPN). Replaces the pre-4.3.0 `dataEntryState`.
    /// Equivalent to: lastParameterIsRegistered
    pub last_parameter_is_registered: bool,

    /// The last pressed note on this channel. -1 means none.
    /// Strictly internal (not a MIDI parameter); set by Portamento Control CC
    /// and by the sequencer for accurate portamento recreation.
    /// Equivalent to: lastNote
    pub last_note: i32,

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

    /// True if the program on this channel is locked.
    /// Equivalent to: lockPreset
    pub lock_preset: bool,

    /// The MIDI system when the preset was locked.
    /// Equivalent to: lockedSystem: MIDISystem
    pub locked_system: MIDISystem,

    /// True if GS NRPN parameters are locked.
    /// Equivalent to: lockGSNRPNParams
    pub lock_gs_nrpn_params: bool,

    /// Custom vibrato settings for this channel (GS NRPN).
    /// Equivalent to: channelVibrato
    pub channel_vibrato: ChannelVibrato,

    /// Current pan in range [-500;500], cached from `update_internal_params`.
    /// Equivalent to: currentPan (protected)
    current_pan: f64,

    /// Current tuning in cents, cached from `update_internal_params`.
    /// Equivalent to: currentTuning (protected)
    current_tuning: f64,

    /// Current key-shift, cached from `update_internal_params`.
    /// Equivalent to: currentKeyShift (protected)
    current_key_shift: i32,

    /// Current gain, cached from `update_internal_params`.
    /// Equivalent to: currentGain (protected)
    current_gain: f64,

    /// Current voice count for this channel.
    /// Equivalent to: voiceCount
    pub voice_count: u32,

    /// This channel's 0-based index.
    /// Equivalent to: channel: number
    pub channel: u8,

    /// True if per-note pitch mode is active (MIDI 2.0).
    /// Equivalent to: perNotePitch (protected)
    pub per_note_pitch: bool,

    /// Pre-computed channel tuning in cents (sum of all tuning sources).
    /// Equivalent to: channelTuningCents (protected)
    pub channel_tuning_cents: f64,

    /// Generator offset values for SF2 NRPN support (0 = no change).
    /// Equivalent to: generatorOffsets: Int16Array
    pub generator_offsets: [i16; GENERATORS_AMOUNT],

    /// True when at least one generator offset has been set.
    /// Equivalent to: generatorOffsetsEnabled (protected)
    pub generator_offsets_enabled: bool,

    /// Generator override values for AWE32 support (i16::MAX = no override).
    /// Equivalent to: generatorOverrides: Int16Array
    pub generator_overrides: [i16; GENERATORS_AMOUNT],

    /// True when at least one generator override has been set.
    /// Equivalent to: generatorOverridesEnabled (protected)
    pub generator_overrides_enabled: bool,

    /// True if this channel is muted.
    /// Equivalent to: _isMuted (protected)
    pub is_muted: bool,

    /// Previous voice count, used to detect voice count changes for events.
    /// Equivalent to: previousVoiceCount (private)
    previous_voice_count: u32,

    /// True if insertion effect routing is enabled for this channel.
    /// Equivalent to: insertionEnabled
    pub insertion_enabled: bool,

}

impl MidiChannel {
    /// Creates a new MIDI channel.
    /// Equivalent to: constructor(synthProps, preset, channelNumber)
    pub fn new(preset: Option<BasicPreset>, preset_bank_idx: Option<usize>, channel: u8) -> Self {
        let midi_controllers = DEFAULT_MIDI_CONTROLLER_VALUES;
        let mut generator_overrides = [0i16; GENERATORS_AMOUNT];
        generator_overrides.fill(GENERATOR_OVERRIDE_NO_CHANGE_VALUE);
        let mut pitch_wheels = [0i16; 128];
        pitch_wheels.fill(8192);

        let mut ch = Self {
            midi_controllers,
            pitch_wheels,
            locked_controllers: vec![false; CONTROLLER_TABLE_SIZE],
            custom_controllers: CUSTOM_RESET_ARRAY,
            channel_transpose_key_shift: 0,
            channel_octave_tuning: [0i8; 128],
            sys_ex_modulators: DynamicModulatorSystem::new(),
            drum_channel: false,
            midi_parameters: ChannelMidiParameter {
                // Rx Channel is initialized to the channel number.
                rx_channel: channel,
                ..DEFAULT_CHANNEL_MIDI_PARAMETERS
            },
            system_parameters: DEFAULT_CHANNEL_SYSTEM_PARAMETERS,
            current_pan: 0.0,
            current_tuning: 0.0,
            current_key_shift: 0,
            current_gain: 0.0,
            last_parameter_is_registered: true,
            last_note: -1,
            portamento_force: false,
            patch: MidiPatch {
                program: 0,
                bank_msb: 0,
                bank_lsb: 0,
                is_gm_gs_drum: false,
            },
            preset,
            preset_bank_idx,
            lock_preset: false,
            locked_system: MIDISystem::Gs,
            lock_gs_nrpn_params: false,
            channel_vibrato: ChannelVibrato::default(),
            voice_count: 0,
            channel,
            per_note_pitch: false,
            channel_tuning_cents: 0.0,
            generator_offsets: [0i16; GENERATORS_AMOUNT],
            generator_offsets_enabled: false,
            generator_overrides,
            generator_overrides_enabled: false,
            drum_params: (0..128).map(|_| DrumParameters::default()).collect(),
            is_muted: false,
            previous_voice_count: 0,
            insertion_enabled: false,
        };
        ch.update_channel_tuning();
        ch.update_internal_params();
        ch
    }

    /// Returns the effective MIDI system for this channel.
    /// When the preset is locked, returns the system it was locked under;
    /// otherwise returns the supplied current system.
    /// Equivalent to: get channelSystem()
    pub fn channel_system(&self, current_system: MIDISystem) -> MIDISystem {
        if self.lock_preset {
            self.locked_system
        } else {
            current_system
        }
    }

    /// Saves and resets the voice count. Call once per render quantum before rendering.
    /// Equivalent to: clearVoiceCount()
    pub fn clear_voice_count(&mut self) {
        self.previous_voice_count = self.voice_count;
        self.voice_count = 0;
    }

    /// Sends a channelPropertyChange event if the voice count has changed.
    /// Returns Some(event) if the count changed and event system is active.
    /// Equivalent to: updateVoiceCount()
    pub fn update_voice_count(&self, enable_event_system: bool) -> Option<SynthProcessorEvent> {
        if self.voice_count != self.previous_voice_count {
            self.build_channel_property_event(enable_event_system)
        } else {
            None
        }
    }

    /// Sends the channel property as an event.
    /// Returns Some(event) if the event system is enabled.
    /// Equivalent to: sendChannelProperty()
    pub fn build_channel_property_event(
        &self,
        enable_event_system: bool,
    ) -> Option<SynthProcessorEvent> {
        if !enable_event_system {
            return None;
        }
        let pitch_wheel = self.midi_controllers
            [NON_CC_INDEX_OFFSET + modulator_sources::PITCH_WHEEL as usize]
            as u16;
        let pitch_wheel_range = self.midi_controllers
            [NON_CC_INDEX_OFFSET + modulator_sources::PITCH_WHEEL_RANGE as usize]
            as f64
            / 128.0;
        let transposition = self.channel_transpose_key_shift as f64
            + self.custom_controllers[custom_controllers::CHANNEL_TRANSPOSE_FINE as usize] as f64
                / 100.0;
        Some(SynthProcessorEvent::ChannelPropertyChange(
            ChannelPropertyChangeCallback {
                channel: self.channel,
                property: ChannelProperty {
                    voices_amount: self.voice_count,
                    pitch_wheel,
                    pitch_wheel_range,
                    is_muted: self.is_muted,
                    is_drum: self.drum_channel,
                    transposition,
                },
            },
        ))
    }

    /// Transposes the channel by `semitones`.
    /// Equivalent to: transposeChannel(semitones, force = false)
    pub fn transpose_channel(
        &mut self,
        mut semitones: f64,
        force: bool,
        master_transposition: f64,
        voices: &mut [Voice],
        current_time: f64,
        enable_event_system: bool,
    ) -> Option<SynthProcessorEvent> {
        if !self.drum_channel {
            semitones += master_transposition;
        }
        let key_shift = semitones.trunc() as i16;
        let current_transpose = self.channel_transpose_key_shift as f64
            + self.custom_controllers[custom_controllers::CHANNEL_TRANSPOSE_FINE as usize] as f64
                / 100.0;
        if (self.drum_channel && !force) || (semitones - current_transpose).abs() < f64::EPSILON {
            return None;
        }
        if key_shift != self.channel_transpose_key_shift {
            self.stop_all_notes_impl(voices, current_time, false);
        }
        self.channel_transpose_key_shift = key_shift;
        self.set_custom_controller(
            custom_controllers::CHANNEL_TRANSPOSE_FINE,
            (semitones - key_shift as f64) * 100.0,
        );
        self.build_channel_property_event(enable_event_system)
    }

    /// Sets the last pressed note. Strictly internal, used by the sequencer for
    /// very accurate portamento recreation.
    /// Equivalent to: setLastNote(midiNote)
    pub fn set_last_note(&mut self, midi_note: i32) {
        self.last_note = midi_note;
    }

    /// Sets the octave tuning for all 128 notes (repeated from 12-element array).
    /// Equivalent to: setOctaveTuning(tuning: Int8Array)
    pub fn set_octave_tuning(&mut self, tuning: &[i8; 12]) {
        for i in 0..128usize {
            self.channel_octave_tuning[i] = tuning[i % 12];
        }
    }

    /// Sets the modulation depth in cents.
    /// Equivalent to: setModulationDepth(cents)
    pub fn set_modulation_depth(&mut self, cents: f64) {
        let cents = cents.round();
        spessa_synth_info(&format!(
            "Channel {} modulation depth. Cents: {}",
            self.channel, cents
        ));
        self.set_custom_controller(custom_controllers::MODULATION_MULTIPLIER, cents / 50.0);
    }

    /// Sets the channel's fine tuning in cents.
    /// Equivalent to: setTuning(cents, log = true)
    pub fn set_tuning(&mut self, cents: f64, log: bool) {
        let cents = cents.round();
        self.set_custom_controller(custom_controllers::CHANNEL_TUNING, cents);
        if log {
            spessa_synth_info(&format!(
                "Fine tuning for channel {} is now set to {} cents.",
                self.channel, cents
            ));
        }
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
        let lock_idx = NON_CC_INDEX_OFFSET + modulator_sources::PITCH_WHEEL as usize;
        if self.locked_controllers[lock_idx] {
            return Vec::new();
        }

        let mut events = Vec::new();

        if midi_note == -1 {
            self.per_note_pitch = false;
            self.midi_controllers[lock_idx] = pitch;
            self.compute_modulators_all_impl(voices, 0, modulator_sources::PITCH_WHEEL as usize);
            if let Some(ev) = self.build_channel_property_event(enable_event_system) {
                events.push(ev);
            }
        } else {
            if !self.per_note_pitch {
                let current = self.midi_controllers[lock_idx];
                self.pitch_wheels.fill(current);
            }
            self.per_note_pitch = true;
            self.pitch_wheels[midi_note as usize] = pitch;
            // Recompute only voices with this specific note.
            let mut vc = 0u32;
            if self.voice_count > 0 {
                for v in voices.iter_mut() {
                    if v.is_active && v.channel == self.channel && v.midi_note == midi_note as u8 {
                        let mut modulated = v.modulated_generators;
                        compute_modulators(self, v, &mut modulated, SourceFilter::NonCC, modulator_sources::POLY_PRESSURE as usize);
                        v.modulated_generators = modulated;
                        vc += 1;
                        if vc >= self.voice_count {
                            break;
                        }
                    }
                }
            }
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
        self.midi_controllers[NON_CC_INDEX_OFFSET + modulator_sources::CHANNEL_PRESSURE as usize] =
            (pressure as i16) << 7;
        self.update_channel_tuning();
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
        let mut vc = 0u32;
        if self.voice_count > 0 {
            for v in voices.iter_mut() {
                if v.is_active && v.channel == self.channel && v.midi_note == midi_note {
                    v.pressure = pressure;
                    let mut modulated = v.modulated_generators;
                    compute_modulators(self, v, &mut modulated, SourceFilter::NonCC, modulator_sources::POLY_PRESSURE as usize);
                    v.modulated_generators = modulated;
                    vc += 1;
                    if vc >= self.voice_count {
                        break;
                    }
                }
            }
        }

        use crate::synthesizer::types::PolyPressureCallback;
        vec![SynthProcessorEvent::PolyPressure(PolyPressureCallback {
            channel: self.channel,
            midi_note,
            pressure,
        })]
    }

    /// Sets a custom controller value and updates channel tuning.
    /// Equivalent to: setCustomController(type, value)
    pub fn set_custom_controller(&mut self, controller_type: u8, value: f64) {
        self.custom_controllers[controller_type as usize] = value as f32;
        self.update_channel_tuning();
    }

    /// Recomputes the pre-cached channel tuning from all tuning sources.
    /// Equivalent to: updateChannelTuning()
    pub fn update_channel_tuning(&mut self) {
        // Promote to f64 before arithmetic to match TS (Float32Array reads → f64 arithmetic)
        self.channel_tuning_cents = self.custom_controllers
            [custom_controllers::CHANNEL_TUNING as usize] as f64
            + self.custom_controllers[custom_controllers::CHANNEL_TRANSPOSE_FINE as usize] as f64
            + self.custom_controllers[custom_controllers::MASTER_TUNING as usize] as f64
            + self.custom_controllers[custom_controllers::CHANNEL_TUNING_SEMITONES as usize] as f64
                * 100.0;
    }

    /// Locks or unlocks the preset from MIDI program changes.
    /// Equivalent to: setPresetLock(locked)
    pub fn set_preset_lock(&mut self, locked: bool, current_system: MIDISystem) {
        if self.lock_preset == locked {
            return;
        }
        self.lock_preset = locked;
        if locked {
            self.locked_system = current_system;
        }
    }

    /// Sets the GM/GS drum flag (updates patch.is_gm_gs_drum).
    /// Equivalent to: setGSDrums(drums)
    pub fn set_gs_drums(&mut self, drums: bool) {
        if drums == self.patch.is_gm_gs_drum {
            return;
        }
        self.set_bank_lsb(0);
        self.set_bank_msb(0);
        self.patch.is_gm_gs_drum = drums;
    }

    /// Sets the custom vibrato.
    /// Equivalent to: setVibrato(depth, rate, delay)
    pub fn set_vibrato(&mut self, depth: f64, rate: f64, delay: f64) {
        if self.lock_gs_nrpn_params {
            return;
        }
        self.channel_vibrato.rate = rate;
        self.channel_vibrato.delay = delay;
        self.channel_vibrato.depth = depth;
    }

    /// Disables and locks all GS NRPN parameters including custom vibrato.
    /// Equivalent to: disableAndLockGSNRPN()
    pub fn disable_and_lock_gs_nrpn(&mut self) {
        self.lock_gs_nrpn_params = true;
        self.channel_vibrato.rate = 0.0;
        self.channel_vibrato.delay = 0.0;
        self.channel_vibrato.depth = 0.0;
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
        let adjusted_note = (midi_note as i32
            + self.custom_controllers[custom_controllers::CHANNEL_KEY_SHIFT as usize] as i32)
            as u8;
        let mut vc = 0u32;
        if self.voice_count > 0 {
            for v in voices.iter_mut() {
                if v.channel == self.channel && v.is_active && v.real_key == adjusted_note {
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

    /// Mutes or unmutes this channel.
    /// Returns events to dispatch.
    /// Equivalent to: muteChannel(isMuted)
    pub fn mute_channel(
        &mut self,
        voices: &mut [Voice],
        current_time: f64,
        is_muted: bool,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        if is_muted {
            self.stop_all_notes_impl(voices, current_time, true);
        }
        self.is_muted = is_muted;
        let mut events = Vec::new();
        if let Some(ev) = self.build_channel_property_event(enable_event_system) {
            events.push(ev);
        }
        use crate::synthesizer::types::MuteChannelCallback;
        events.push(SynthProcessorEvent::MuteChannel(MuteChannelCallback {
            channel: self.channel,
            is_muted,
        }));
        events
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
        if !self.lock_preset {
            self.patch.bank_msb = bank_msb;
        }
    }

    /// Sets bank LSB unless the preset is locked.
    /// Equivalent to: setBankLSB(bankLSB) (protected)
    pub fn set_bank_lsb(&mut self, bank_lsb: u8) {
        if !self.lock_preset {
            self.patch.bank_lsb = bank_lsb;
        }
    }

    /// Sets the drum flag on the channel.
    /// Returns a drum-change event if the drum state changed.
    /// Equivalent to: setDrumFlag(isDrum) (protected)
    pub fn set_drum_flag(&mut self, is_drum: bool) -> Option<SynthProcessorEvent> {
        if self.lock_preset || self.preset.is_none() {
            return None;
        }
        if self.drum_channel == is_drum {
            return None;
        }
        if is_drum {
            self.channel_transpose_key_shift = 0;
            self.drum_channel = true;
        } else {
            self.drum_channel = false;
        }
        use crate::synthesizer::types::DrumChangeCallback;
        Some(SynthProcessorEvent::DrumChange(DrumChangeCallback {
            channel: self.channel,
            is_drum_channel: self.drum_channel,
        }))
    }

    /// Changes the preset to, or from drums.
    /// Sets up the proper bank selection for drum channels and executes a program change.
    /// Equivalent to: setDrums(isDrum)
    pub fn set_drums(
        &mut self,
        is_drum: bool,
        sound_bank_manager: &crate::synthesizer::audio_engine::sound_bank_manager::SoundBankManager,
        current_system: MIDISystem,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        let ch_system = self.channel_system(current_system);
        if BankSelectHacks::is_system_xg(ch_system) {
            if is_drum {
                if let Some(drum_bank) = BankSelectHacks::get_drum_bank(ch_system) {
                    self.set_bank_msb(drum_bank);
                    self.set_bank_lsb(0);
                }
            } else {
                self.set_bank_msb(0);
                self.set_bank_lsb(0);
            }
        } else {
            self.set_gs_drums(is_drum);
        }
        self.set_drum_flag(is_drum);
        let program = self.patch.program;
        self.program_change(program, sound_bank_manager, current_system, enable_event_system)
    }

    // -----------------------------------------------------------------------
    // Channel parameter infrastructure (TS 4.3.0)
    // -----------------------------------------------------------------------

    /// Sets a channel MIDI parameter and refreshes the cached `current_*` values.
    /// Equivalent to: setMIDIParameter(parameter, value)
    ///
    /// # Note
    /// The TS version additionally recomputes modulators for `pitchWheel`/`pressure`
    /// and fires a `channelParamChange` event. In this stage those two parameters are
    /// still handled by the dedicated `pitch_wheel`/`channel_pressure` methods (which
    /// keep their storage in the `midi_controllers` table and recompute modulators),
    /// so no voice recompute is done here. The event has no effect on rendering.
    pub fn set_midi_parameter(&mut self, value: ChannelMidiParameterValue) {
        use ChannelMidiParameterValue::*;
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
            EfxAssign(v) => self.midi_parameters.efx_assign = v,
            Cc1(v) => self.midi_parameters.cc1 = v,
            Cc2(v) => self.midi_parameters.cc2 = v,
            DrumMap(v) => self.midi_parameters.drum_map = v,
        }
        self.update_internal_params();
    }

    /// Sets a channel system parameter and refreshes the cached `current_*` values.
    /// Equivalent to: setSystemParameterInternal(parameter, value)
    ///
    /// # Note
    /// The TS version has additional per-parameter side effects (e.g. `presetLock`
    /// records the locked system, `isMuted`/`keyShift` stop all notes). Those side
    /// effects require access to voices / the current system and their callers are
    /// migrated in a later stage; this stage keeps them on the existing dedicated
    /// paths (`set_preset_lock`, `mute_channel`, `transpose_channel`).
    pub fn set_system_parameter(&mut self, value: ChannelSystemParameterValue) {
        use ChannelSystemParameterValue::*;
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
        self.update_internal_params();
    }

    /// Recomputes the cached `current_pan/current_gain/current_tuning/current_key_shift`
    /// from the channel's system and MIDI parameters.
    /// Equivalent to: updateInternalParams()
    ///
    /// # Note
    /// The global (synth-wide) system/MIDI parameters are not yet plumbed into the
    /// channel. They default to identity (keyShift 0, fineTune 0, gain 1, pan 0) in
    /// the current engine, so they are omitted from the sums below — this makes the
    /// cached values equal to the 4.2.0 behavior. The consumers in `render_voice`/
    /// `compute_modulator` are still on the old fields and are migrated in a later
    /// stage (see module docs); these cached values are exposed via getters for that.
    pub fn update_internal_params(&mut self) {
        let channel_system = &self.system_parameters;
        let channel_midi = &self.midi_parameters;

        // Only Channel System is processed for drum channels.
        let current_key_shift = if self.drum_channel {
            channel_system.key_shift
        } else {
            channel_system.key_shift + channel_midi.key_shift
        };
        // Ensure integer.
        self.current_key_shift = current_key_shift.trunc() as i32;

        // Only Channel System is processed for drum channels.
        self.current_tuning = if self.drum_channel {
            channel_system.fine_tune
        } else {
            channel_system.fine_tune + channel_midi.fine_tune
        };

        // [-1;1] normalized (Channel MIDI is the pan controller, handled elsewhere).
        let current_pan_normalized = channel_system.pan;
        // For faster render_voice calculation.
        self.current_pan = current_pan_normalized * 500.0;

        self.current_gain = SPESSASYNTH_GAIN_FACTOR * channel_system.gain;
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
}
