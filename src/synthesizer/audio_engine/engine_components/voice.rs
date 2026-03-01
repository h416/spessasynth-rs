/// voice.rs
/// purpose: prepares Voices from sample and generator data
/// Ported from: src/synthesizer/audio_engine/engine_components/voice.ts
use crate::soundbank::basic_soundbank::generator_types::GENERATORS_AMOUNT;
use crate::soundbank::basic_soundbank::modulator::DecodedModulator;
use crate::soundbank::basic_soundbank::modulator_source::VoiceModInputs;
use crate::synthesizer::audio_engine::engine_components::compute_modulator::VoiceContext;
use crate::synthesizer::audio_engine::engine_components::dsp_chain::lowpass_filter::LowpassFilter;
use crate::synthesizer::audio_engine::engine_components::dsp_chain::modulation_envelope::ModulationEnvelope;
use crate::synthesizer::audio_engine::engine_components::dsp_chain::volume_envelope::VolumeEnvelope;
use crate::synthesizer::audio_engine::engine_components::dsp_chain::wavetable_oscillator::WavetableOscillator;
use crate::synthesizer::audio_engine::engine_components::master_parameters::DEFAULT_MASTER_PARAMETERS;
use crate::synthesizer::audio_engine::engine_components::synth_constants::{
    MIN_EXCLUSIVE_LENGTH, MIN_NOTE_LENGTH,
};
use crate::synthesizer::enums::interpolation_types;
use crate::synthesizer::types::SampleLoopingMode;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Timecents value for an exclusive class release (nearly instant cutoff).
/// Equivalent to: const EXCLUSIVE_CUTOFF_TIME = -2320
const EXCLUSIVE_CUTOFF_TIME: i32 = -2320;

/// Multiplier applied to effect modulator (reverb/chorus send) amounts.
/// 1000 / 200 = 5.0
/// Equivalent to: const EFFECT_MODULATOR_TRANSFORM_MULTIPLIER = 1000 / 200
const EFFECT_MODULATOR_TRANSFORM_MULTIPLIER: f32 = 1000.0 / 200.0;

// ---------------------------------------------------------------------------
// Voice
// ---------------------------------------------------------------------------

/// Represents a single instance of the SoundFont2 synthesis model:
/// a wavetable oscillator, volume envelope, modulation envelope,
/// generators, modulators, and MIDI parameters.
///
/// Equivalent to: class Voice
pub struct Voice {
    /// All three wavetable oscillators: [Linear, Nearest, Hermite].
    /// Indexed by `oscillator_type`.
    /// Equivalent to: oscillators: Record<InterpolationType, WavetableOscillator>
    pub oscillators: [WavetableOscillator; 3],

    /// Index of the currently active oscillator (0 = Linear, 1 = Nearest, 2 = Hermite).
    /// Mirrors the `wavetable` reference in TypeScript: `this.wavetable = this.oscillators[type]`.
    pub oscillator_type: u8,

    /// Lowpass filter applied to the voice output.
    /// Equivalent to: filter: LowpassFilter
    pub filter: LowpassFilter,

    /// Unmodulated (base) generator values copied from the preset/instrument zone.
    /// Equivalent to: generators: Int16Array(GENERATORS_AMOUNT)
    pub generators: [i16; GENERATORS_AMOUNT],

    /// Real-time modulated generator values used during rendering.
    /// Updated by `compute_modulators` each render cycle.
    /// Equivalent to: modulatedGenerators: Int16Array(GENERATORS_AMOUNT)
    pub modulated_generators: [i16; GENERATORS_AMOUNT],

    /// Decoded modulators for this voice.
    /// Equivalent to: modulators: Array<Modulator>
    pub modulators: Vec<DecodedModulator>,

    /// Cached output values for each modulator (same order as `modulators`).
    /// Stored as i16 to match TypeScript's `Int16Array(64)`.
    /// Equivalent to: modulatorValues: Int16Array(64)
    pub modulator_values: Vec<i16>,

    /// Modulation envelope (controls pitch/filter over time).
    /// Equivalent to: modEnv: ModulationEnvelope
    pub mod_env: ModulationEnvelope,

    /// Volume envelope (controls amplitude over time).
    /// Equivalent to: volEnv: VolumeEnvelope
    pub vol_env: VolumeEnvelope,

    /// Per-voice render buffer; reused each frame to avoid allocation.
    /// Equivalent to: buffer: Float32Array(128)
    pub buffer: Vec<f32>,

    /// Current resonance offset from the default resonant modulator.
    /// Equivalent to: resonanceOffset
    pub resonance_offset: f64,

    /// Voice priority used for voice stealing.
    /// Equivalent to: priority
    pub priority: i32,

    /// Whether the voice slot is currently in use.
    /// Equivalent to: isActive
    pub is_active: bool,

    /// Whether the voice has rendered at least one buffer.
    /// Used by exclusive class to avoid killing the note just started.
    /// Equivalent to: hasRendered
    pub has_rendered: bool,

    /// Whether the voice is in the release phase.
    /// Equivalent to: isInRelease
    pub is_in_release: bool,

    /// Whether the voice is held by the sustain pedal.
    /// Equivalent to: isHeld
    pub is_held: bool,

    /// MIDI channel (0–15).
    /// Equivalent to: channel
    pub channel: u8,

    /// Note-on velocity (0–127).
    /// Equivalent to: velocity
    pub velocity: u8,

    /// MIDI note number (0–127).
    /// Equivalent to: midiNote
    pub midi_note: u8,

    /// Root key of the assigned sample.
    /// Equivalent to: rootKey
    pub root_key: i16,

    /// Target MIDI note for pitch calculation.
    /// Equivalent to: targetKey
    pub target_key: i16,

    /// Polyphonic key pressure (0–127).
    /// Equivalent to: pressure
    pub pressure: u8,

    /// Linear gain modifier (set by Key Modifiers).
    /// Equivalent to: gainModifier
    pub gain_modifier: f64,

    /// Sample looping mode (0 = no loop, 1 = loop, 2 = start-on-release, 3 = loop+play).
    /// Equivalent to: loopingMode: SampleLoopingMode
    pub looping_mode: SampleLoopingMode,

    /// Absolute note-start time in seconds.
    /// Equivalent to: startTime
    pub start_time: f64,

    /// Absolute time when the release phase began (f64::INFINITY = not yet released).
    /// Equivalent to: releaseStartTime = Infinity
    pub release_start_time: f64,

    /// Current tuning offset in cents.
    /// Equivalent to: tuningCents
    pub tuning_cents: f32,

    /// Current tuning as a frequency ratio.
    /// Equivalent to: tuningRatio
    pub tuning_ratio: f64,

    /// Current pan value (−500 to +500) used for smoothing.
    /// Equivalent to: currentPan
    pub current_pan: f64,

    /// Actual MIDI key (may differ from `midi_note` when MIDI Tuning Standard is active).
    /// Equivalent to: realKey
    pub real_key: u8,

    /// Key to glide from for portamento (−1 = portamento off).
    /// Equivalent to: portamentoFromKey
    pub portamento_from_key: i32,

    /// Duration of the linear portamento glide in seconds.
    /// Equivalent to: portamentoDuration
    pub portamento_duration: f64,

    /// Pan override (0 = use channel pan; ±500 for random pan feature).
    /// Equivalent to: overridePan
    pub override_pan: f64,

    /// Exclusive class number for hi-hats etc. (0 = none).
    /// Equivalent to: exclusiveClass
    pub exclusive_class: i32,

    /// Volume envelope release override in timecents (0 = use modulatedGenerators).
    /// Equivalent to: overrideReleaseVolEnv
    pub override_release_vol_env: i32,
}

impl Voice {
    /// Creates a new, inactive Voice for the given sample rate.
    ///
    /// The default oscillator type is `HERMITE` (= 2), matching
    /// `DEFAULT_MASTER_PARAMETERS.interpolation_type`.
    ///
    /// Equivalent to: constructor(sampleRate: number)
    pub fn new(sample_rate: f64) -> Self {
        Self {
            oscillators: [
                WavetableOscillator::new(interpolation_types::LINEAR),
                WavetableOscillator::new(interpolation_types::NEAREST_NEIGHBOR),
                WavetableOscillator::new(interpolation_types::HERMITE),
            ],
            oscillator_type: DEFAULT_MASTER_PARAMETERS.interpolation_type,
            filter: LowpassFilter::new(sample_rate),
            generators: [0; GENERATORS_AMOUNT],
            modulated_generators: [0; GENERATORS_AMOUNT],
            modulators: Vec::new(),
            modulator_values: Vec::new(),
            mod_env: ModulationEnvelope::new(),
            vol_env: VolumeEnvelope::new(sample_rate),
            buffer: vec![0.0; 128],
            resonance_offset: 0.0,
            priority: 0,
            is_active: false,
            has_rendered: false,
            is_in_release: false,
            is_held: false,
            channel: 0,
            velocity: 0,
            midi_note: 0,
            root_key: 0,
            target_key: 0,
            pressure: 0,
            gain_modifier: 1.0,
            looping_mode: 0,
            start_time: 0.0,
            release_start_time: f64::INFINITY,
            tuning_cents: 0.0,
            tuning_ratio: 1.0,
            current_pan: 0.0,
            real_key: 60,
            portamento_from_key: -1,
            portamento_duration: 0.0,
            override_pan: 0.0,
            exclusive_class: 0,
            override_release_vol_env: 0,
        }
    }

    /// Returns a shared reference to the currently active wavetable oscillator.
    ///
    /// TypeScript stores a direct object reference (`this.wavetable`); Rust returns
    /// a reference into the `oscillators` array instead.
    ///
    /// Equivalent to: this.wavetable (getter)
    pub fn wavetable(&self) -> &WavetableOscillator {
        &self.oscillators[self.oscillator_type as usize]
    }

    /// Returns a mutable reference to the currently active wavetable oscillator.
    ///
    /// Equivalent to: this.wavetable (mutable access)
    pub fn wavetable_mut(&mut self) -> &mut WavetableOscillator {
        let idx = self.oscillator_type as usize;
        &mut self.oscillators[idx]
    }

    /// Applies an exclusive class release (nearly instant cutoff) to this voice.
    ///
    /// Sets `override_release_vol_env` to `EXCLUSIVE_CUTOFF_TIME` so that the
    /// volume envelope decays in ≈ 2320 timecents → near-instant release.
    ///
    /// Equivalent to: exclusiveRelease(currentTime, minExclusiveLength)
    pub fn exclusive_release(&mut self, current_time: f64, min_exclusive_length: f64) {
        self.override_release_vol_env = EXCLUSIVE_CUTOFF_TIME;
        self.is_in_release = false;
        self.release_voice(current_time, min_exclusive_length);
    }

    /// Stops the voice at `current_time`, extending short notes to `min_note_length`.
    ///
    /// Equivalent to: releaseVoice(currentTime, minNoteLength)
    pub fn release_voice(&mut self, current_time: f64, min_note_length: f64) {
        self.release_start_time = current_time;
        if self.release_start_time - self.start_time < min_note_length {
            self.release_start_time = self.start_time + min_note_length;
        }
    }

    /// Initialises the voice for a new note-on event.
    ///
    /// Equivalent to: setup(currentTime, channel, midiNote, velocity, realKey)
    pub fn setup(
        &mut self,
        current_time: f64,
        channel: u8,
        midi_note: u8,
        velocity: u8,
        real_key: u8,
    ) {
        self.start_time = current_time;
        self.is_active = true;
        self.is_in_release = false;
        self.has_rendered = false;
        self.release_start_time = f64::INFINITY;
        self.pressure = 0;
        self.channel = channel;
        self.midi_note = midi_note;
        self.velocity = velocity;
        self.real_key = real_key;
        self.override_release_vol_env = 0;
        self.portamento_duration = 0.0;
        self.portamento_from_key = -1;
    }
}

// ---------------------------------------------------------------------------
// VoiceContext implementation
// ---------------------------------------------------------------------------

impl VoiceContext for Voice {
    /// Returns the voice's decoded modulator list.
    /// Equivalent to: voice.modulators
    fn decoded_modulators(&self) -> &[DecodedModulator] {
        &self.modulators
    }

    /// Returns the voice's base (unmodulated) generator values.
    /// Equivalent to: voice.generators
    fn generators(&self) -> &[i16] {
        &self.generators
    }

    /// Returns the real MIDI key number as usize (for per-note pitch lookup).
    /// Equivalent to: voice.realKey
    fn real_key(&self) -> usize {
        self.real_key as usize
    }

    /// Computes the modulator at `index`, caches it in `modulator_values[index]`,
    /// and returns the result.
    ///
    /// # Design note
    /// TypeScript calls `modulator.primarySource.getValue(controllerTable, pitchWheel, this)`.
    /// In Rust, we extract modulator fields first (to release the immutable borrow of
    /// `self.modulators`), then mutate `self.resonance_offset` and `self.modulator_values`.
    ///
    /// Equivalent to: computeModulator(controllerTable, pitchWheel, modulatorIndex)
    fn compute_single_modulator(
        &mut self,
        midi_controllers: &[i16],
        pitch: i16,
        index: usize,
    ) -> f64 {
        // Extract all data from the modulator before taking a mutable borrow.
        let (
            transform_amount,
            transform_type,
            is_effect_mod,
            is_resonant_mod,
            primary_src,
            secondary_src,
        ) = {
            let modulator = &self.modulators[index];
            (
                modulator.transform_amount,
                modulator.transform_type,
                modulator.is_effect_modulator,
                modulator.is_default_resonant_modulator,
                modulator.primary_source(), // returns a new ModulatorSource
                modulator.secondary_source(), // returns a new ModulatorSource
            )
        };

        // transformAmount === 0 → short-circuit to zero.
        // Equivalent to: if (modulator.transformAmount === 0) { ... return 0; }
        if transform_amount == 0.0 {
            if let Some(v) = self.modulator_values.get_mut(index) {
                *v = 0;
            }
            return 0.0;
        }

        // Build minimal voice view for source value lookup.
        let voice_inputs = VoiceModInputs {
            midi_note: self.midi_note,
            velocity: self.velocity,
            pressure: self.pressure,
        };

        // source values are f32 from the precomputed lookup table (Float32Array in TS)
        let source_value = primary_src.get_value(midi_controllers, pitch as i32, &voice_inputs);
        let second_src_value =
            secondary_src.get_value(midi_controllers, pitch as i32, &voice_inputs);

        // Use f64 for intermediate computation to match TypeScript's `number` (f64) arithmetic.
        // TS: computedValue = sourceValue * secondSrcValue * transformAmount (all f64)
        let mut amount = transform_amount;
        if is_effect_mod && amount <= 1000.0 {
            amount *= EFFECT_MODULATOR_TRANSFORM_MULTIPLIER as f64;
            amount = amount.min(1000.0);
        }

        let mut computed_value =
            source_value as f64 * second_src_value as f64 * amount;

        // Absolute-value transform type.
        // Equivalent to: if (modulator.transformType === 2) computedValue = Math.abs(computedValue);
        if transform_type == 2 {
            computed_value = computed_value.abs();
        }

        // Resonant modulator: store half the value to negate filter gain change.
        // Equivalent to: if (isDefaultResonantModulator) this.resonanceOffset = Math.max(0, computedValue / 2);
        if is_resonant_mod {
            self.resonance_offset = (computed_value / 2.0).max(0.0);
        }

        // Store as i16 to match TypeScript's Int16Array truncation behavior.
        // TS: this.modulatorValues[modulatorIndex] = computedValue; (Int16Array truncates to i16)
        if let Some(v) = self.modulator_values.get_mut(index) {
            *v = computed_value as i16;
        }
        // Return full-precision value (matches TS's `return computedValue` which is f64).
        // The caller (computeModulators All mode) uses this return value, not the stored i16.
        computed_value
    }

    /// Returns the cached modulator output values.
    /// Equivalent to: voice.modulatorValues (Int16Array)
    fn modulator_values(&self) -> &[i16] {
        &self.modulator_values
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator_types::{
        GENERATORS_AMOUNT, generator_types as gt,
    };
    use crate::soundbank::basic_soundbank::modulator::DecodedModulator;
    use crate::synthesizer::audio_engine::engine_components::master_parameters::DEFAULT_MASTER_PARAMETERS;
    use crate::synthesizer::enums::interpolation_types;

    const SAMPLE_RATE: f64 = 44_100.0;

    // -----------------------------------------------------------------------
    // new()
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_oscillator_count() {
        let v = Voice::new(SAMPLE_RATE);
        assert_eq!(v.oscillators.len(), 3);
    }

    #[test]
    fn test_new_oscillator_types_match_interpolation_constants() {
        let v = Voice::new(SAMPLE_RATE);
        assert_eq!(
            v.oscillators[0].interpolation_type,
            interpolation_types::LINEAR
        );
        assert_eq!(
            v.oscillators[1].interpolation_type,
            interpolation_types::NEAREST_NEIGHBOR
        );
        assert_eq!(
            v.oscillators[2].interpolation_type,
            interpolation_types::HERMITE
        );
    }

    #[test]
    fn test_new_default_oscillator_type_is_hermite() {
        let v = Voice::new(SAMPLE_RATE);
        assert_eq!(
            v.oscillator_type,
            DEFAULT_MASTER_PARAMETERS.interpolation_type
        );
        assert_eq!(v.oscillator_type, interpolation_types::HERMITE);
    }

    #[test]
    fn test_new_generators_are_zero() {
        let v = Voice::new(SAMPLE_RATE);
        assert_eq!(v.generators.len(), GENERATORS_AMOUNT);
        assert!(v.generators.iter().all(|&x| x == 0));
    }

    #[test]
    fn test_new_modulated_generators_are_zero() {
        let v = Voice::new(SAMPLE_RATE);
        assert!(v.modulated_generators.iter().all(|&x| x == 0));
    }

    #[test]
    fn test_new_buffer_length_128() {
        let v = Voice::new(SAMPLE_RATE);
        assert_eq!(v.buffer.len(), 128);
    }

    #[test]
    fn test_new_buffer_all_zeros() {
        let v = Voice::new(SAMPLE_RATE);
        assert!(v.buffer.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_new_inactive() {
        let v = Voice::new(SAMPLE_RATE);
        assert!(!v.is_active);
    }

    #[test]
    fn test_new_not_in_release() {
        let v = Voice::new(SAMPLE_RATE);
        assert!(!v.is_in_release);
    }

    #[test]
    fn test_new_release_start_time_is_infinity() {
        let v = Voice::new(SAMPLE_RATE);
        assert!(v.release_start_time.is_infinite() && v.release_start_time > 0.0);
    }

    #[test]
    fn test_new_gain_modifier_is_one() {
        let v = Voice::new(SAMPLE_RATE);
        assert!((v.gain_modifier - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_new_tuning_ratio_is_one() {
        let v = Voice::new(SAMPLE_RATE);
        assert!((v.tuning_ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_new_real_key_is_60() {
        let v = Voice::new(SAMPLE_RATE);
        assert_eq!(v.real_key, 60);
    }

    #[test]
    fn test_new_portamento_from_key_is_minus_one() {
        let v = Voice::new(SAMPLE_RATE);
        assert_eq!(v.portamento_from_key, -1);
    }

    #[test]
    fn test_new_resonance_offset_is_zero() {
        let v = Voice::new(SAMPLE_RATE);
        assert!((v.resonance_offset - 0.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // wavetable() / wavetable_mut()
    // -----------------------------------------------------------------------

    #[test]
    fn test_wavetable_returns_hermite_by_default() {
        let v = Voice::new(SAMPLE_RATE);
        assert_eq!(
            v.wavetable().interpolation_type,
            interpolation_types::HERMITE
        );
    }

    #[test]
    fn test_wavetable_linear_when_type_zero() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.oscillator_type = interpolation_types::LINEAR;
        assert_eq!(
            v.wavetable().interpolation_type,
            interpolation_types::LINEAR
        );
    }

    #[test]
    fn test_wavetable_mut_allows_modification() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.wavetable_mut().cursor = 42.0;
        // oscillator_type defaults to HERMITE (2)
        assert!((v.oscillators[2].cursor - 42.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // setup()
    // -----------------------------------------------------------------------

    #[test]
    fn test_setup_sets_start_time() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.setup(1.5, 0, 60, 100, 60);
        assert!((v.start_time - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_setup_activates_voice() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.setup(0.0, 0, 60, 100, 60);
        assert!(v.is_active);
    }

    #[test]
    fn test_setup_clears_release_flags() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.is_in_release = true;
        v.has_rendered = true;
        v.setup(0.0, 0, 60, 100, 60);
        assert!(!v.is_in_release);
        assert!(!v.has_rendered);
    }

    #[test]
    fn test_setup_resets_release_start_time_to_infinity() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.release_start_time = 1.0;
        v.setup(0.0, 0, 60, 100, 60);
        assert!(v.release_start_time.is_infinite() && v.release_start_time > 0.0);
    }

    #[test]
    fn test_setup_sets_channel_and_note() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.setup(0.0, 3, 64, 80, 64);
        assert_eq!(v.channel, 3);
        assert_eq!(v.midi_note, 64);
        assert_eq!(v.velocity, 80);
        assert_eq!(v.real_key, 64);
    }

    #[test]
    fn test_setup_resets_portamento() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.portamento_from_key = 50;
        v.portamento_duration = 0.5;
        v.setup(0.0, 0, 60, 100, 60);
        assert_eq!(v.portamento_from_key, -1);
        assert!((v.portamento_duration - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_setup_resets_override_release_vol_env() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.override_release_vol_env = -2320;
        v.setup(0.0, 0, 60, 100, 60);
        assert_eq!(v.override_release_vol_env, 0);
    }

    #[test]
    fn test_setup_resets_pressure() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.pressure = 64;
        v.setup(0.0, 0, 60, 100, 60);
        assert_eq!(v.pressure, 0);
    }

    // -----------------------------------------------------------------------
    // release_voice()
    // -----------------------------------------------------------------------

    #[test]
    fn test_release_voice_sets_release_time() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.setup(0.0, 0, 60, 100, 60);
        v.release_voice(1.0, MIN_NOTE_LENGTH);
        assert!((v.release_start_time - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_release_voice_extends_short_note() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.start_time = 0.0;
        // Release at 0.001 s, which is less than MIN_NOTE_LENGTH (0.03 s)
        v.release_voice(0.001, MIN_NOTE_LENGTH);
        let expected = 0.0 + MIN_NOTE_LENGTH;
        assert!((v.release_start_time - expected).abs() < 1e-9);
    }

    #[test]
    fn test_release_voice_does_not_shorten_long_note() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.start_time = 0.0;
        let release_time = 1.0; // much longer than MIN_NOTE_LENGTH
        v.release_voice(release_time, MIN_NOTE_LENGTH);
        assert!((v.release_start_time - release_time).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // exclusive_release()
    // -----------------------------------------------------------------------

    #[test]
    fn test_exclusive_release_sets_cutoff_time() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.setup(0.0, 0, 60, 100, 60);
        v.exclusive_release(0.5, MIN_EXCLUSIVE_LENGTH);
        assert_eq!(v.override_release_vol_env, EXCLUSIVE_CUTOFF_TIME);
    }

    #[test]
    fn test_exclusive_release_clears_is_in_release() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.is_in_release = true;
        v.exclusive_release(0.0, MIN_EXCLUSIVE_LENGTH);
        assert!(!v.is_in_release);
    }

    #[test]
    fn test_exclusive_release_sets_release_start_time() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.start_time = 0.0;
        v.exclusive_release(1.0, MIN_EXCLUSIVE_LENGTH);
        // 1.0 − 0.0 = 1.0 ≥ MIN_EXCLUSIVE_LENGTH → release_start_time = 1.0
        assert!((v.release_start_time - 1.0).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // VoiceContext trait – decoded_modulators / generators / real_key
    // -----------------------------------------------------------------------

    #[test]
    fn test_decoded_modulators_empty_by_default() {
        let v = Voice::new(SAMPLE_RATE);
        assert!(v.decoded_modulators().is_empty());
    }

    #[test]
    fn test_generators_returns_correct_slice() {
        let v = Voice::new(SAMPLE_RATE);
        assert_eq!(v.generators().len(), GENERATORS_AMOUNT);
    }

    #[test]
    fn test_real_key_returns_usize() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.real_key = 72;
        assert_eq!(v.real_key(), 72usize);
    }

    // -----------------------------------------------------------------------
    // VoiceContext – compute_single_modulator()
    // -----------------------------------------------------------------------

    /// Helper: build a DecodedModulator with a NO_CONTROLLER (constant 1.0) primary source
    /// and a NO_CONTROLLER secondary source.
    ///
    /// `source_enum = 0` → index 0 (NO_CONTROLLER), non-CC, linear, unipolar, positive
    ///   → get_value → raw = 16_383 → table[0][0 + 16_383] ≈ 1.0
    fn make_no_controller_mod(dest: i16, amount: i16) -> DecodedModulator {
        // source_enum = 0 → NO_CONTROLLER (returns 1.0 for linear/unipolar/positive)
        // secondary_source_enum = 0 → same
        DecodedModulator::new(0, 0, dest, amount, 0)
    }

    #[test]
    fn test_compute_single_modulator_zero_amount_returns_zero() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.modulators = vec![make_no_controller_mod(gt::PAN, 0)];
        v.modulator_values = vec![99];
        let midi_controllers = vec![0i16; 147];
        let result = v.compute_single_modulator(&midi_controllers, 0, 0);
        assert!((result - 0.0).abs() < 1e-5);
        assert_eq!(v.modulator_values[0], 0);
    }

    #[test]
    fn test_compute_single_modulator_nonzero_amount_nonzero() {
        let mut v = Voice::new(SAMPLE_RATE);
        // NO_CONTROLLER source → 1.0; secondary NO_CONTROLLER → 1.0;
        // amount = 100 → computed = 1.0 * 1.0 * 100.0 = 100.0
        v.modulators = vec![make_no_controller_mod(gt::PAN, 100)];
        v.modulator_values = vec![0];
        let midi_controllers = vec![0i16; 147];
        let result = v.compute_single_modulator(&midi_controllers, 0, 0);
        // result ≈ 100.0 (NO_CONTROLLER returns full scale)
        assert!(result.abs() > 0.0, "result should be nonzero, got {result}");
        // modulatorValues stores as i16 (matches TS Int16Array)
        assert_eq!(v.modulator_values[0], result as i16);
    }

    #[test]
    fn test_compute_single_modulator_writes_to_modulator_values() {
        let mut v = Voice::new(SAMPLE_RATE);
        v.modulators = vec![make_no_controller_mod(gt::PAN, 50)];
        v.modulator_values = vec![0];
        let midi_controllers = vec![0i16; 147];
        let result = v.compute_single_modulator(&midi_controllers, 0, 0);
        assert_eq!(v.modulator_values[0], result as i16);
    }

    #[test]
    fn test_compute_single_modulator_transform_type_2_is_abs() {
        let mut v = Voice::new(SAMPLE_RATE);
        // transform_type = 2 → abs. Use a negative amount to get negative before abs.
        // But NO_CONTROLLER source → positive, amount negative → negative → |result| > 0
        let mod_ = DecodedModulator::new(0, 0, gt::PAN, -100, 2);
        v.modulators = vec![mod_];
        v.modulator_values = vec![0];
        let midi_controllers = vec![0i16; 147];
        let result = v.compute_single_modulator(&midi_controllers, 0, 0);
        assert!(
            result >= 0.0,
            "transform_type 2 should produce abs value, got {result}"
        );
    }

    #[test]
    fn test_modulator_values_empty_by_default() {
        let v = Voice::new(SAMPLE_RATE);
        assert!(v.modulator_values().is_empty());
    }
}
