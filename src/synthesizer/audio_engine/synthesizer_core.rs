/// synthesizer_core.rs
/// purpose: MidiChannel and SynthesizerCore structs, integrated to avoid circular dependencies.
/// Ported from:
///   - src/synthesizer/audio_engine/engine_components/midi_channel.ts
///   - src/synthesizer/audio_engine/synthesizer_core.ts
///
/// # Design note
/// TypeScript has a circular dependency: SynthesizerCore owns MidiChannel[], and MidiChannel
/// holds a back-reference `synthCore: SynthesizerCore`. Rust does not allow this.
/// Solution: abolish midi_channel.ts as a separate module and integrate both structs here.
/// Methods on MidiChannel that previously accessed `this.synthCore` now receive the needed
/// data as function parameters instead.
use std::collections::HashMap;

use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::generator_types::{
    GENERATOR_LIMITS, GENERATORS_AMOUNT, GeneratorType,
};
use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
use crate::soundbank::enums::modulator_sources;
use crate::synthesizer::audio_engine::engine_components::compute_modulator::{
    ChannelContext, SourceFilter, compute_modulators,
};
use crate::synthesizer::audio_engine::engine_components::controller_tables::{
    CONTROLLER_TABLE_SIZE, CUSTOM_CONTROLLER_TABLE_SIZE, CUSTOM_RESET_ARRAY,
    DEFAULT_MIDI_CONTROLLER_VALUES, NON_CC_INDEX_OFFSET,
};
use crate::synthesizer::audio_engine::engine_components::dsp_chain::lowpass_filter::LowpassFilter;
use crate::synthesizer::audio_engine::engine_components::dynamic_modulator_system::DynamicModulatorSystem;
use crate::synthesizer::audio_engine::engine_components::key_modifier_manager::KeyModifierManager;
use crate::synthesizer::audio_engine::engine_components::master_parameters::DEFAULT_MASTER_PARAMETERS;
use crate::synthesizer::audio_engine::engine_components::sound_bank_manager::SoundBankManager;
use crate::synthesizer::audio_engine::engine_components::synth_constants::{
    DEFAULT_PERCUSSION, GENERATOR_OVERRIDE_NO_CHANGE_VALUE, MIN_NOTE_LENGTH,
};
use crate::synthesizer::audio_engine::engine_components::voice::Voice;
use crate::synthesizer::enums::{DataEntryState, custom_controllers, data_entry_states};
use crate::synthesizer::types::{
    CachedVoiceList, ChannelProperty, ChannelPropertyChangeCallback, MasterParameterType,
    SynthProcessorEvent, SynthProcessorOptions, SynthSystem,
};
use crate::utils::loggin::{spessa_synth_info, spessa_synth_warn};
use crate::utils::midi_hacks::BankSelectHacks;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Gain smoothing factor for rapid volume changes. Must be run every sample.
/// Equivalent to: GAIN_SMOOTHING_FACTOR
const GAIN_SMOOTHING_FACTOR: f64 = 0.01;

/// Pan smoothing factor for rapid pan changes.
/// Equivalent to: PAN_SMOOTHING_FACTOR
const PAN_SMOOTHING_FACTOR: f64 = 0.05;

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

    /// True if random panning is enabled for every note played.
    /// Equivalent to: randomPan
    pub random_pan: bool,

    /// Current MIDI data entry state (RPN/NRPN).
    /// Equivalent to: dataEntryState: DataEntryState
    pub data_entry_state: DataEntryState,

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
    /// Equivalent to: lockedSystem: SynthSystem
    pub locked_system: SynthSystem,

    /// True if GS NRPN parameters are locked.
    /// Equivalent to: lockGSNRPNParams
    pub lock_gs_nrpn_params: bool,

    /// Custom vibrato settings for this channel (GS NRPN).
    /// Equivalent to: channelVibrato
    pub channel_vibrato: ChannelVibrato,

    /// True = polyphonic (POLY ON), False = monophonic (MONO ON).
    /// Equivalent to: polyMode
    pub poly_mode: bool,

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
            random_pan: false,
            data_entry_state: data_entry_states::IDLE,
            patch: MidiPatch {
                program: 0,
                bank_msb: 0,
                bank_lsb: 0,
                is_gm_gs_drum: false,
            },
            preset,
            preset_bank_idx,
            lock_preset: false,
            locked_system: SynthSystem::Gs,
            lock_gs_nrpn_params: false,
            channel_vibrato: ChannelVibrato::default(),
            poly_mode: true,
            voice_count: 0,
            channel,
            per_note_pitch: false,
            channel_tuning_cents: 0.0,
            generator_offsets: [0i16; GENERATORS_AMOUNT],
            generator_offsets_enabled: false,
            generator_overrides,
            generator_overrides_enabled: false,
            is_muted: false,
            previous_voice_count: 0,
        };
        ch.update_channel_tuning();
        ch
    }

    /// Returns the effective MIDI system for this channel.
    /// When the preset is locked, returns the system it was locked under;
    /// otherwise returns the supplied current system.
    /// Equivalent to: get channelSystem()
    pub fn channel_system(&self, current_system: SynthSystem) -> SynthSystem {
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
            ((semitones - key_shift as f64) * 100.0) as f32,
        );
        self.build_channel_property_event(enable_event_system)
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
    pub fn set_modulation_depth(&mut self, cents: f32) {
        let cents = cents.round();
        spessa_synth_info(&format!(
            "Channel {} modulation depth. Cents: {}",
            self.channel, cents
        ));
        self.set_custom_controller(custom_controllers::MODULATION_MULTIPLIER, cents / 50.0);
    }

    /// Sets the channel's fine tuning in cents.
    /// Equivalent to: setTuning(cents, log = true)
    pub fn set_tuning(&mut self, cents: f32, log: bool) {
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
    pub fn set_custom_controller(&mut self, controller_type: u8, value: f32) {
        self.custom_controllers[controller_type as usize] = value;
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
    pub fn set_preset_lock(&mut self, locked: bool, current_system: SynthSystem) {
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
        sound_bank_manager: &crate::synthesizer::audio_engine::engine_components::sound_bank_manager::SoundBankManager,
        current_system: SynthSystem,
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
    // Stubs for methods ported from external files
    // -----------------------------------------------------------------------

    // note_off is implemented in engine_methods/stopping_notes/note_off.rs
    // program_change is implemented in engine_methods/program_change.rs

    // controller_change is implemented in engine_methods/controller_control/controller_change.rs
    // reset_controllers, reset_preset, reset_controllers_rp15, reset_parameters
    //   are implemented in engine_methods/controller_control/reset_controllers.rs
    // data_entry_coarse is implemented in engine_methods/controller_control/data_entry/data_entry_coarse.rs
    // data_entry_fine is implemented in engine_methods/controller_control/data_entry/data_entry_fine.rs

    // render_voice is implemented in engine_components/dsp_chain/render_voice.rs
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

// ---------------------------------------------------------------------------
// ScheduledEvent
// ---------------------------------------------------------------------------

/// A MIDI event scheduled for a future time.
/// Equivalent to: { callback: () => unknown; time: number }
struct ScheduledEvent {
    callback: Box<dyn FnOnce(&mut SynthesizerCore)>,
    time: f64,
}

// ---------------------------------------------------------------------------
// SynthesizerCore
// ---------------------------------------------------------------------------

/// The core synthesis engine which interacts with channels and holds all synth parameters.
/// Equivalent to: class SynthesizerCore
pub struct SynthesizerCore {
    /// Voice pool. All voices are pre-allocated.
    /// Equivalent to: voices: Voice[]
    pub voices: Vec<Voice>,

    /// All MIDI channels.
    /// Equivalent to: midiChannels: MIDIChannel[]
    pub midi_channels: Vec<MidiChannel>,

    /// Sound bank manager.
    /// Equivalent to: soundBankManager: SoundBankManager
    pub sound_bank_manager: SoundBankManager,

    /// Key modifier manager for custom key overrides.
    /// Equivalent to: keyModifierManager: KeyModifierManager
    pub key_modifier_manager: KeyModifierManager,

    /// Audio sample rate in Hz.
    /// Equivalent to: sampleRate
    pub sample_rate: f64,

    /// MIDI Tuning Standard table: tunings[program * 128 + key] = note.cents
    /// -1.0 means no change.
    /// Equivalent to: tunings: Float32Array(128 * 128).fill(-1)
    pub tunings: Vec<f32>,

    /// Master synthesizer parameters.
    /// Equivalent to: masterParameters
    pub master_parameters: MasterParameterType,

    /// Current synthesizer time in seconds.
    /// Equivalent to: currentTime
    pub current_time: f64,

    /// Overall MIDI volume gain (0.0–1.0, set by SysEx).
    /// Equivalent to: midiVolume
    pub midi_volume: f64,

    /// Reverb send level (set via SysEx, reset on system reset).
    /// Equivalent to: reverbSend
    pub reverb_send: f64,

    /// True if chorus and reverb effects are enabled.
    /// Equivalent to: enableEffects
    pub enable_effects: bool,

    /// True if the event system is enabled.
    /// Equivalent to: enableEventSystem
    pub enable_event_system: bool,

    /// Chorus send level (set via SysEx, reset on system reset).
    /// Equivalent to: chorusSend
    pub chorus_send: f64,

    /// Left channel pan (0.0–1.0).
    /// Equivalent to: panLeft
    pub pan_left: f64,

    /// Right channel pan (0.0–1.0).
    /// Equivalent to: panRight
    pub pan_right: f64,

    /// Gain smoothing factor adjusted to the sample rate.
    /// Equivalent to: gainSmoothingFactor
    pub gain_smoothing_factor: f64,

    /// Pan smoothing factor adjusted to the sample rate.
    /// Equivalent to: panSmoothingFactor
    pub pan_smoothing_factor: f64,

    /// Callback invoked for synthesizer events.
    /// Equivalent to: eventCallbackHandler
    pub event_callback: Box<dyn Fn(SynthProcessorEvent)>,

    /// Cache of computed voice lists, keyed by get_cached_voice_index output.
    /// Equivalent to: cachedVoices: Map<number, CachedVoiceList>
    pub cached_voices: HashMap<u64, CachedVoiceList>,

    /// Total active voice count.
    /// Equivalent to: voiceCount
    pub voice_count: u32,

    /// Last time voice priorities were assigned (avoids redundant work in a quantum).
    /// Equivalent to: lastPriorityAssignmentTime (private)
    last_priority_assignment_time: f64,

    /// Event queue for future-scheduled messages.
    /// Equivalent to: eventQueue (private)
    event_queue: Vec<ScheduledEvent>,

    /// Duration of a single sample in seconds.
    /// Equivalent to: sampleTime (private)
    sample_time: f64,
}

impl SynthesizerCore {
    /// Creates a new SynthesizerCore.
    /// Equivalent to: constructor(eventCallbackHandler, missingPresetHandler, sampleRate, options)
    pub fn new(
        event_callback: impl Fn(SynthProcessorEvent) + 'static,
        sample_rate: f64,
        options: SynthProcessorOptions,
    ) -> Self {
        let gain_smoothing_factor = GAIN_SMOOTHING_FACTOR * (44_100.0 / sample_rate);
        let pan_smoothing_factor = PAN_SMOOTHING_FACTOR * (44_100.0 / sample_rate);
        LowpassFilter::init_cache(sample_rate);

        // Initialize voice pool
        let voice_cap = DEFAULT_MASTER_PARAMETERS.voice_cap as usize;
        let mut voices = Vec::with_capacity(voice_cap);
        for _ in 0..voice_cap {
            voices.push(Voice::new(sample_rate));
        }

        let tunings = vec![-1.0f32; 128 * 128];

        Self {
            voices,
            midi_channels: Vec::new(),
            sound_bank_manager: SoundBankManager::new(|| {}),
            key_modifier_manager: KeyModifierManager::new(),
            sample_rate,
            tunings,
            master_parameters: DEFAULT_MASTER_PARAMETERS,
            current_time: options.initial_time,
            midi_volume: 1.0,
            reverb_send: 1.0,
            enable_effects: options.enable_effects,
            enable_event_system: options.enable_event_system,
            chorus_send: 1.0,
            pan_left: 0.5,
            pan_right: 0.5,
            gain_smoothing_factor,
            pan_smoothing_factor,
            event_callback: Box::new(event_callback),
            cached_voices: HashMap::new(),
            voice_count: 0,
            last_priority_assignment_time: 0.0,
            event_queue: Vec::new(),
            sample_time: 1.0 / sample_rate,
        }
    }

    /// Dispatches an event through the event callback.
    /// Equivalent to: callEvent(eventName, eventData)
    pub fn call_event(&self, event: SynthProcessorEvent) {
        if self.enable_event_system {
            (self.event_callback)(event);
        }
    }

    /// Assigns the first available (inactive) voice, stealing the lowest-priority one if needed.
    /// Equivalent to: assignVoice()
    pub fn assign_voice(&mut self) -> &mut Voice {
        let voice_cap = self.master_parameters.voice_cap as usize;
        // Find an inactive voice
        for i in 0..voice_cap {
            if !self.voices[i].is_active {
                self.voices[i].priority = i32::MAX;
                return &mut self.voices[i];
            }
        }
        // All voices active — assign priorities and steal the lowest
        self.assign_voice_priorities();
        let mut lowest_idx = 0;
        for i in 1..voice_cap {
            if self.voices[i].priority < self.voices[lowest_idx].priority {
                lowest_idx = i;
            }
        }
        self.voices[lowest_idx].priority = i32::MAX;
        &mut self.voices[lowest_idx]
    }

    /// Like `assign_voice()` but returns the voice index instead of a reference.
    /// Used by `note_on` to allow simultaneous borrows of `voices` and `midi_channels`.
    pub(crate) fn assign_voice_idx(&mut self) -> usize {
        let voice_cap = self.master_parameters.voice_cap as usize;
        for i in 0..voice_cap {
            if !self.voices[i].is_active {
                self.voices[i].priority = i32::MAX;
                return i;
            }
        }
        self.assign_voice_priorities();
        let mut lowest_idx = 0;
        for i in 1..voice_cap {
            if self.voices[i].priority < self.voices[lowest_idx].priority {
                lowest_idx = i;
            }
        }
        self.voices[lowest_idx].priority = i32::MAX;
        lowest_idx
    }

    /// Stops all notes on all channels.
    /// Equivalent to: stopAllChannels(force)
    pub fn stop_all_channels(&mut self, force: bool) {
        spessa_synth_info("Stop all received!");
        let current_time = self.current_time;
        let voices = &mut self.voices;
        let mut events = Vec::new();
        for channel in self.midi_channels.iter_mut() {
            let evs = channel.stop_all_notes(voices, current_time, force);
            events.extend(evs);
        }
        for event in events {
            self.call_event(event);
        }
    }

    /// Creates a new MIDI channel and optionally fires events.
    /// Equivalent to: createMIDIChannel(sendEvent)
    pub fn create_midi_channel(&mut self, send_event: bool) {
        let channel_number = self.midi_channels.len() as u8;
        let (preset, bank_idx) = self.get_default_preset_and_idx();
        let mut channel = MidiChannel::new(preset, bank_idx, channel_number);

        // Channel 9 (0-based) is the default percussion channel.
        if channel_number % 16 == DEFAULT_PERCUSSION {
            channel.drum_channel = true;
        }

        self.midi_channels.push(channel);

        if send_event {
            self.call_event(SynthProcessorEvent::NewChannel);
            let ch = self.midi_channels.last().unwrap();
            if let Some(ev) = ch.build_channel_property_event(self.enable_event_system) {
                self.call_event(ev);
            }
        }
    }

    /// Returns the default preset clone and its bank index (if loaded).
    fn get_default_preset_and_idx(&self) -> (Option<BasicPreset>, Option<usize>) {
        let patch = MidiPatch {
            program: 0,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        };
        if let Some((preset, bank_idx)) = self
            .sound_bank_manager
            .get_preset_and_bank_idx(patch, SynthSystem::Xg)
        {
            (Some(preset.clone()), Some(bank_idx))
        } else {
            (None, None)
        }
    }

    /// Resets all controllers on all channels.
    /// Equivalent to: resetAllControllers(system = DEFAULT_SYNTH_MODE)
    pub fn reset_all_controllers(&mut self, system: SynthSystem) {
        self.call_event(SynthProcessorEvent::AllControllerReset);
        self.master_parameters.midi_system = system;
        // Reset private fields
        self.tunings.fill(-1.0);
        self.set_midi_volume(1.0);
        self.reverb_send = 1.0;
        self.chorus_send = 1.0;

        let enable_event_system = self.enable_event_system;
        let current_time = self.current_time;
        let mut events = Vec::new();

        // Reset controllers and preset for each channel (TS: ch.resetControllers(false); ch.resetPreset())
        for ch_idx in 0..self.midi_channels.len() {
            let mut sub = self.midi_channels[ch_idx].reset_controllers(
                false, // do not send CC events
                &mut self.voices,
                current_time,
                system,
                enable_event_system,
            );
            events.append(&mut sub);

            let mut sub = self.midi_channels[ch_idx].reset_preset(
                &self.sound_bank_manager,
                system,
                enable_event_system,
            );
            events.append(&mut sub);
        }

        for ch_idx in 0..self.midi_channels.len() {
            // Restore locked controller events.
            for cc_num in 0..128usize {
                if self.midi_channels[ch_idx].locked_controllers[cc_num] {
                    use crate::midi::enums::MidiController;
                    use crate::synthesizer::types::ControllerChangeCallback;
                    events.push(SynthProcessorEvent::ControllerChange(
                        ControllerChangeCallback {
                            channel: ch_idx as u8,
                            controller_number: cc_num as MidiController,
                            controller_value: (self.midi_channels[ch_idx].midi_controllers[cc_num]
                                >> 7) as u8,
                        },
                    ));
                }
            }

            // Restore pitch wheel event.
            let pitch_lock_idx = NON_CC_INDEX_OFFSET + modulator_sources::PITCH_WHEEL as usize;
            if !self.midi_channels[ch_idx].locked_controllers[pitch_lock_idx] {
                use crate::synthesizer::types::PitchWheelCallback;
                let val = self.midi_channels[ch_idx].midi_controllers[pitch_lock_idx];
                events.push(SynthProcessorEvent::PitchWheel(PitchWheelCallback {
                    channel: ch_idx as u8,
                    pitch: val as u16,
                    midi_note: -1,
                }));
            }

            // Restore channel pressure event.
            let cp_lock_idx = NON_CC_INDEX_OFFSET + modulator_sources::CHANNEL_PRESSURE as usize;
            if !self.midi_channels[ch_idx].locked_controllers[cp_lock_idx] {
                use crate::synthesizer::types::ChannelPressureCallback;
                let val = self.midi_channels[ch_idx].midi_controllers[cp_lock_idx] >> 7;
                events.push(SynthProcessorEvent::ChannelPressure(
                    ChannelPressureCallback {
                        channel: ch_idx as u8,
                        pressure: val as u8,
                    },
                ));
            }
        }

        for event in events {
            self.call_event(event);
        }
    }

    /// Renders audio for the current quantum.
    /// Equivalent to: renderAudio(outputs, reverb, chorus, startIndex, sampleCount)
    pub fn render_audio(
        &mut self,
        outputs: &mut [Vec<f32>],
        reverb: &mut [Vec<f32>],
        chorus: &mut [Vec<f32>],
        start_index: usize,
        sample_count: usize,
    ) {
        // Process scheduled events.
        self.process_event_queue();

        let quantum_size = if sample_count > 0 {
            sample_count
        } else {
            outputs[0].len().saturating_sub(start_index)
        };

        // Clear voice counts.
        for ch in self.midi_channels.iter_mut() {
            ch.clear_voice_count();
        }
        self.voice_count = 0;

        let enable_effects = self.enable_effects;
        let master_gain = self.master_parameters.master_gain;
        let reverb_gain = self.master_parameters.reverb_gain;
        let chorus_gain = self.master_parameters.chorus_gain;
        let midi_volume = self.midi_volume;
        let pan_left = self.pan_left;
        let pan_right = self.pan_right;
        let pan_smoothing_factor = self.pan_smoothing_factor;
        let current_time = self.current_time;
        let reverb_send = self.reverb_send;
        let chorus_send = self.chorus_send;
        let out_len = outputs[0].len();

        // Handle potentially empty reverb/chorus arrays.
        // In TypeScript, empty arrays are passed when effects are disabled;
        // accessing [0] on an empty array returns undefined (no crash).
        // In Rust, we use a dummy buffer so that raw pointer creation is safe.
        let has_reverb = reverb.len() >= 2;
        let has_chorus = chorus.len() >= 2;
        let mut dummy = [0.0f32; 0];
        let rev_len = if has_reverb { reverb[0].len() } else { 0 };
        let chr_len = if has_chorus { chorus[0].len() } else { 0 };

        // Render active voices.
        // SAFETY: voices and midi_channels are separate Vec fields — no aliasing.
        // We use raw pointers for the output slices to avoid borrow conflicts while
        // also mutably borrowing self.midi_channels[ch_idx].
        let out_l_ptr = outputs[0].as_mut_ptr();
        let out_r_ptr = outputs[1].as_mut_ptr();
        let rev_l_ptr = if has_reverb { reverb[0].as_mut_ptr() } else { dummy.as_mut_ptr() };
        let rev_r_ptr = if has_reverb { reverb[1].as_mut_ptr() } else { dummy.as_mut_ptr() };
        let chr_l_ptr = if has_chorus { chorus[0].as_mut_ptr() } else { dummy.as_mut_ptr() };
        let chr_r_ptr = if has_chorus { chorus[1].as_mut_ptr() } else { dummy.as_mut_ptr() };

        for v_idx in 0..self.voices.len() {
            if !self.voices[v_idx].is_active {
                continue;
            }
            let ch_idx = self.voices[v_idx].channel as usize;
            if self.midi_channels[ch_idx].is_muted {
                continue;
            }

            self.midi_channels[ch_idx].voice_count += 1;
            self.voice_count += 1;

            let out_l_slice = unsafe { std::slice::from_raw_parts_mut(out_l_ptr, out_len) };
            let out_r_slice = unsafe { std::slice::from_raw_parts_mut(out_r_ptr, out_len) };
            let rev_l_slice = unsafe { std::slice::from_raw_parts_mut(rev_l_ptr, rev_len) };
            let rev_r_slice = unsafe { std::slice::from_raw_parts_mut(rev_r_ptr, rev_len) };
            let chr_l_slice = unsafe { std::slice::from_raw_parts_mut(chr_l_ptr, chr_len) };
            let chr_r_slice = unsafe { std::slice::from_raw_parts_mut(chr_r_ptr, chr_len) };

            self.midi_channels[ch_idx].render_voice(
                &mut self.voices[v_idx],
                current_time,
                out_l_slice,
                out_r_slice,
                rev_l_slice,
                rev_r_slice,
                chr_l_slice,
                chr_r_slice,
                start_index,
                quantum_size,
                master_gain,
                reverb_gain,
                chorus_gain,
                midi_volume,
                pan_left,
                pan_right,
                reverb_send,
                chorus_send,
                enable_effects,
                pan_smoothing_factor,
                &self.tunings,
            );

        }

        // Fire voice count change events.
        let enable_event_system = self.enable_event_system;
        let mut events = Vec::new();
        for ch in self.midi_channels.iter() {
            if let Some(ev) = ch.update_voice_count(enable_event_system) {
                events.push(ev);
            }
        }
        for event in events {
            self.call_event(event);
        }

        // Advance time.
        self.current_time += quantum_size as f64 * self.sample_time;
    }

    /// Gets voices for a channel+note+velocity, applying key modifier overrides.
    /// Equivalent to: getVoices(channel, midiNote, velocity)
    pub fn get_voices(&self, channel: u8, midi_note: u8, velocity: u8) -> CachedVoiceList {
        let channel_obj = &self.midi_channels[channel as usize];

        let override_patch = self
            .key_modifier_manager
            .has_override_patch(channel, midi_note);

        if override_patch {
            let patch = match self.key_modifier_manager.get_patch(channel, midi_note) {
                Ok(p) => p,
                Err(_) => return Vec::new(),
            };
            if let Some((preset, bank_idx)) = self
                .sound_bank_manager
                .get_preset_and_bank_idx(patch, self.master_parameters.midi_system)
            {
                let bank = &self.sound_bank_manager.sound_bank_list[bank_idx].sound_bank;
                return self.get_voices_for_preset(preset, bank, midi_note, velocity);
            }
            return Vec::new();
        }

        // Use channel's stored preset.
        if let (Some(preset), Some(bank_idx)) = (&channel_obj.preset, channel_obj.preset_bank_idx) {
            let bank = &self.sound_bank_manager.sound_bank_list[bank_idx].sound_bank;
            return self.get_voices_for_preset(preset, bank, midi_note, velocity);
        }
        Vec::new()
    }

    /// Gets voices for a given preset+note+velocity from the sound bank (no cache).
    ///
    /// Access audio data via `sample.audio_data` directly (SF2 data is always pre-loaded;
    /// SF3 vorbis data must be pre-decoded before synthesis starts).
    ///
    /// Equivalent to: getVoicesForPreset(preset, midiNote, velocity)
    pub fn get_voices_for_preset(
        &self,
        preset: &BasicPreset,
        bank: &crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank,
        midi_note: u8,
        velocity: u8,
    ) -> CachedVoiceList {
        use crate::synthesizer::audio_engine::engine_components::voice_cache::CachedVoice;

        let voice_params = preset.get_voice_parameters(
            midi_note,
            velocity,
            &bank.instruments,
            &bank.default_modulators,
        );

        let mut voices = CachedVoiceList::new();
        for vp in voice_params {
            let sample = match bank.samples.get(vp.sample_idx) {
                Some(s) => s,
                None => {
                    spessa_synth_warn(&format!(
                        "get_voices_for_preset: invalid sample index {}",
                        vp.sample_idx
                    ));
                    continue;
                }
            };

            // Audio data must be pre-loaded (SF2) or pre-decoded (SF3 vorbis).
            let audio_data = match &sample.audio_data {
                Some(data) => data.clone(),
                None => {
                    spessa_synth_warn(&format!(
                        "Discarding invalid sample: {}",
                        sample.name
                    ));
                    continue;
                }
            };

            let cv = CachedVoice::from_bank_params(
                vp,
                audio_data,
                sample.original_key as i16,
                sample.loop_start,
                sample.loop_end,
                sample.sample_rate as f64,
                sample.pitch_correction as f64,
                midi_note,
                velocity,
                self.sample_rate,
            );
            voices.push(cv);
        }
        voices
    }

    /// Clears the voice cache.
    /// Equivalent to: clearCache()
    pub fn clear_cache(&mut self) {
        self.cached_voices.clear();
    }

    /// Sets the MIDI volume (raised to e as per GM2 spec).
    /// Equivalent to: setMIDIVolume(volume) (protected)
    pub fn set_midi_volume(&mut self, volume: f64) {
        self.midi_volume = volume.powf(std::f64::consts::E);
    }

    /// Sets the master tuning for all channels.
    /// Equivalent to: setMasterTuning(cents) (protected)
    pub fn set_master_tuning(&mut self, cents: f64) {
        let cents = cents.round() as f32;
        for ch in self.midi_channels.iter_mut() {
            ch.set_custom_controller(custom_controllers::MASTER_TUNING, cents);
        }
    }

    /// Destroys the synthesizer, releasing all resources.
    /// Equivalent to: destroySynthProcessor()
    pub fn destroy(&mut self) {
        self.voices.clear();
        for ch in self.midi_channels.iter_mut() {
            ch.locked_controllers.clear();
            ch.preset = None;
        }
        self.clear_cache();
        self.midi_channels.clear();
        self.sound_bank_manager.destroy();
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Processes all scheduled events whose time has arrived.
    /// Equivalent to: event queue processing in renderAudio
    fn process_event_queue(&mut self) {
        if self.event_queue.is_empty() {
            return;
        }
        let time = self.current_time;
        while !self.event_queue.is_empty() {
            if self.event_queue[0].time > time {
                break;
            }
            let event = self.event_queue.remove(0);
            (event.callback)(self);
        }
    }

    /// Assigns priorities to all voices for voice-stealing decisions.
    /// Equivalent to: assignVoicePriorities() (private)
    fn assign_voice_priorities(&mut self) {
        if (self.last_priority_assignment_time - self.current_time).abs() < f64::EPSILON {
            return;
        }
        self.last_priority_assignment_time = self.current_time;
        for voice in self.voices.iter_mut() {
            voice.priority = 0;
            let ch_idx = voice.channel as usize;
            if ch_idx < self.midi_channels.len() && self.midi_channels[ch_idx].drum_channel {
                voice.priority += 5;
            }
            if voice.is_in_release {
                voice.priority -= 5;
            }
            voice.priority += (voice.velocity as i32) / 25;
            voice.priority -= voice.vol_env.state as i32;
            if voice.is_in_release {
                voice.priority -= 5;
            }
            voice.priority -= (voice.vol_env.attenuation_cb / 200.0) as i32;
        }
    }

    /// Computes the cache key for a given patch+note+velocity.
    /// Equivalent to: getCachedVoiceIndex(patch, midiNote, velocity) (private)
    pub(crate) fn get_cached_voice_index(&self, patch: &MidiPatch, midi_note: u8, velocity: u8) -> u64 {
        let (bank_msb, bank_lsb) = if patch.is_gm_gs_drum {
            (128u64, 0u64)
        } else {
            (patch.bank_msb as u64, patch.bank_lsb as u64)
        };
        let program = patch.program as u64;
        let note = midi_note as u64;
        let vel = velocity as u64;

        bank_msb + bank_lsb * 128 + program * 16_384 + 2_097_152 * note + 268_435_456 * vel
    }

    /// Pushes a callback to the event queue to be called at the given time.
    /// Used by process_message to schedule future MIDI events.
    pub(crate) fn schedule_event(
        &mut self,
        callback: impl FnOnce(&mut SynthesizerCore) + 'static,
        time: f64,
    ) {
        self.event_queue
            .push(ScheduledEvent { callback: Box::new(callback), time });
    }
}
