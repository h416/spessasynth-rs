/// voice_cache.rs
/// purpose: Cached voice data for the synthesizer.
/// Ported from: src/synthesizer/audio_engine/engine_components/voice_cache.ts
use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
use crate::soundbank::basic_soundbank::generator_types::GENERATORS_AMOUNT;
pub use crate::soundbank::basic_soundbank::modulator::Modulator;
use crate::soundbank::types::VoiceParameters as BankVoiceParameters;
use crate::synthesizer::types::SampleLoopingMode;

/// Minimal sample data needed by CachedVoice construction.
/// TODO: Replace with crate::soundbank::basic_soundbank::basic_sample::BasicSample
///       when basic_sample.ts is ported.
/// Equivalent to: BasicSample (fields consumed in voice_cache.ts)
pub struct BasicSample {
    /// Original pitch as a MIDI note number.
    pub original_key: i16,
    /// Loop start position in sample points.
    pub loop_start: u32,
    /// Loop end position in sample points.
    pub loop_end: u32,
    /// Sample rate in Hz.
    pub sample_rate: f64,
    /// Pitch correction in cents.
    pub pitch_correction: f64,
    /// PCM audio data (f32 normalised).
    pub audio_data: Vec<f32>,
}

impl BasicSample {
    /// Returns a clone of the audio data.
    /// Equivalent to: sample.getAudioData()
    pub fn get_audio_data(&self) -> Vec<f32> {
        self.audio_data.clone()
    }
}

/// Input parameters for creating a CachedVoice.
/// TODO: Move to soundbank/types.rs when soundbank/types.ts is ported.
/// Equivalent to: VoiceParameters
pub struct VoiceParameters {
    /// SoundFont2 generator values (Int16Array in TypeScript).
    pub generators: Vec<i16>,
    /// Modulators for this voice.
    pub modulators: Vec<Modulator>,
    /// The sample to use.
    pub sample: BasicSample,
}

/// Represents a cached voice ready for playback.
/// Equivalent to: CachedVoice
#[derive(Clone)]
pub struct CachedVoice {
    /// Sample data of this voice.
    pub sample_data: Vec<f32>,
    /// The unmodulated (copied to) generators of the voice.
    pub generators: Vec<i16>,
    /// The voice's modulators.
    pub modulators: Vec<Modulator>,
    /// Exclusive class number for hi-hats etc.
    pub exclusive_class: i16,
    /// Target key of the voice (can be overridden by generators).
    pub target_key: i16,
    /// Target velocity of the voice (can be overridden by generators).
    pub velocity: i16,
    /// MIDI root key of the sample.
    pub root_key: i16,
    /// Start position of the loop.
    pub loop_start: u32,
    /// End position of the loop.
    pub loop_end: u32,
    /// Playback step (rate) for sample pitch correction.
    pub playback_step: f64,
    /// Sample looping mode.
    pub looping_mode: SampleLoopingMode,
}

impl CachedVoice {
    /// Creates a new CachedVoice from voice parameters.
    /// Equivalent to: new CachedVoice(voiceParams, midiNote, velocity, sampleRate)
    pub fn new(
        voice_params: VoiceParameters,
        midi_note: u8,
        velocity: u8,
        sample_rate: f64,
    ) -> Self {
        let sample = &voice_params.sample;
        let generators = &voice_params.generators;

        // Root key override
        let root_key = if generators[gt::OVERRIDING_ROOT_KEY as usize] > -1 {
            generators[gt::OVERRIDING_ROOT_KEY as usize]
        } else {
            sample.original_key
        };

        // Key override
        let target_key = if generators[gt::KEY_NUM as usize] > -1 {
            generators[gt::KEY_NUM as usize]
        } else {
            midi_note as i16
        };

        // Velocity override
        // Note: use a separate velocity to not override the cached velocity.
        // Testcase: LiveHQ Natural SoundFont GM - the Glockenspiel preset
        let effective_velocity = if generators[gt::VELOCITY as usize] > -1 {
            generators[gt::VELOCITY as usize]
        } else {
            velocity as i16
        };

        let exclusive_class = generators[gt::EXCLUSIVE_CLASS as usize];

        // Create the sample for the wavetable oscillator.
        // Offsets are calculated at note on time (to allow for modulation of them).
        let loop_start = sample.loop_start;
        let loop_end = sample.loop_end;
        let sample_data = sample.get_audio_data();
        let playback_step =
            (sample.sample_rate / sample_rate) * (2.0_f64).powf(sample.pitch_correction / 1200.0); // Cent tuning
        let looping_mode = generators[gt::SAMPLE_MODES as usize] as SampleLoopingMode;

        // Borrows of voice_params fields end here; move owned fields into CachedVoice.
        let VoiceParameters {
            generators,
            modulators,
            sample: _,
        } = voice_params;

        Self {
            sample_data,
            generators,
            modulators,
            exclusive_class,
            target_key,
            velocity: effective_velocity,
            root_key,
            loop_start,
            loop_end,
            playback_step,
            looping_mode,
        }
    }

    /// Constructs a CachedVoice from real bank-loaded VoiceParameters and sample fields.
    ///
    /// This bridges the gap between `soundbank::types::VoiceParameters` (which uses
    /// `[i16; GENERATORS_AMOUNT]` and a sample index) and the cached voice representation.
    ///
    /// # Parameters
    /// - `vp`: Voice parameters from `BasicPreset::get_voice_parameters`
    /// - `audio_data`: Pre-decoded PCM audio data for the sample
    /// - `original_key`: Sample's original MIDI key
    /// - `loop_start`, `loop_end`: Sample loop points
    /// - `sample_rate_hz`: Sample rate of the audio data
    /// - `pitch_correction_cents`: Pitch correction from the sample header
    /// - `midi_note`: MIDI note number for this note-on
    /// - `velocity`: Note velocity (0-127)
    /// - `playback_rate_hz`: Synthesizer's output sample rate
    ///
    /// Equivalent to: new CachedVoice(voiceParams, midiNote, velocity, sampleRate)
    #[allow(clippy::too_many_arguments)]
    pub fn from_bank_params(
        vp: BankVoiceParameters,
        audio_data: Vec<f32>,
        original_key: i16,
        loop_start: u32,
        loop_end: u32,
        sample_rate_hz: f64,
        pitch_correction_cents: f64,
        midi_note: u8,
        velocity: u8,
        playback_rate_hz: f64,
    ) -> Self {
        let generators = &vp.generators;

        // Root key override
        let root_key = if generators[gt::OVERRIDING_ROOT_KEY as usize] > -1 {
            generators[gt::OVERRIDING_ROOT_KEY as usize]
        } else {
            original_key
        };

        // Key override
        let target_key = if generators[gt::KEY_NUM as usize] > -1 {
            generators[gt::KEY_NUM as usize]
        } else {
            midi_note as i16
        };

        // Velocity override
        let effective_velocity = if generators[gt::VELOCITY as usize] > -1 {
            generators[gt::VELOCITY as usize]
        } else {
            velocity as i16
        };

        let exclusive_class = generators[gt::EXCLUSIVE_CLASS as usize];
        let looping_mode = generators[gt::SAMPLE_MODES as usize] as SampleLoopingMode;
        let playback_step = (sample_rate_hz / playback_rate_hz)
            * (2.0_f64).powf(pitch_correction_cents / 1200.0);

        Self {
            sample_data: audio_data,
            generators: vp.generators.to_vec(),
            modulators: vp.modulators,
            exclusive_class,
            target_key,
            velocity: effective_velocity,
            root_key,
            loop_start,
            loop_end,
            playback_step,
            looping_mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator_types::{
        DEFAULT_GENERATOR_VALUES, generator_types as gt,
    };

    /// Builds a generators Vec initialised to SF2 default values.
    fn default_generators() -> Vec<i16> {
        DEFAULT_GENERATOR_VALUES.to_vec()
    }

    /// Builds a minimal BasicSample for testing.
    fn make_sample(
        original_key: i16,
        loop_start: u32,
        loop_end: u32,
        sample_rate: f64,
        pitch_correction: f64,
    ) -> BasicSample {
        BasicSample {
            original_key,
            loop_start,
            loop_end,
            sample_rate,
            pitch_correction,
            audio_data: vec![0.0_f32; 100],
        }
    }

    /// Builds a default VoiceParameters with no generator overrides.
    fn default_voice_params(sample: BasicSample) -> VoiceParameters {
        VoiceParameters {
            generators: default_generators(),
            modulators: vec![],
            sample,
        }
    }

    // --- root_key ---

    #[test]
    fn test_root_key_uses_sample_original_key_by_default() {
        // overridingRootKey default is -1, so sample.original_key should be used.
        let vp = default_voice_params(make_sample(60, 0, 99, 44_100.0, 0.0));
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert_eq!(cv.root_key, 60);
    }

    #[test]
    fn test_root_key_overridden_by_generator() {
        let mut gens = default_generators();
        gens[gt::OVERRIDING_ROOT_KEY as usize] = 48;
        let vp = VoiceParameters {
            generators: gens,
            modulators: vec![],
            sample: make_sample(60, 0, 99, 44_100.0, 0.0),
        };
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert_eq!(cv.root_key, 48);
    }

    // --- target_key ---

    #[test]
    fn test_target_key_uses_midi_note_by_default() {
        // keyNum default is -1, so midiNote should be used.
        let vp = default_voice_params(make_sample(60, 0, 99, 44_100.0, 0.0));
        let cv = CachedVoice::new(vp, 64, 100, 44_100.0);
        assert_eq!(cv.target_key, 64);
    }

    #[test]
    fn test_target_key_overridden_by_generator() {
        let mut gens = default_generators();
        gens[gt::KEY_NUM as usize] = 72;
        let vp = VoiceParameters {
            generators: gens,
            modulators: vec![],
            sample: make_sample(60, 0, 99, 44_100.0, 0.0),
        };
        let cv = CachedVoice::new(vp, 64, 100, 44_100.0);
        assert_eq!(cv.target_key, 72);
    }

    // --- velocity ---

    #[test]
    fn test_velocity_uses_input_by_default() {
        // velocity generator default is -1, so input velocity should be used.
        let vp = default_voice_params(make_sample(60, 0, 99, 44_100.0, 0.0));
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert_eq!(cv.velocity, 100);
    }

    #[test]
    fn test_velocity_overridden_by_generator() {
        let mut gens = default_generators();
        gens[gt::VELOCITY as usize] = 80;
        let vp = VoiceParameters {
            generators: gens,
            modulators: vec![],
            sample: make_sample(60, 0, 99, 44_100.0, 0.0),
        };
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert_eq!(cv.velocity, 80);
    }

    // --- exclusive_class ---

    #[test]
    fn test_exclusive_class_default_is_zero() {
        let vp = default_voice_params(make_sample(60, 0, 99, 44_100.0, 0.0));
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert_eq!(cv.exclusive_class, 0);
    }

    #[test]
    fn test_exclusive_class_from_generator() {
        let mut gens = default_generators();
        gens[gt::EXCLUSIVE_CLASS as usize] = 5;
        let vp = VoiceParameters {
            generators: gens,
            modulators: vec![],
            sample: make_sample(60, 0, 99, 44_100.0, 0.0),
        };
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert_eq!(cv.exclusive_class, 5);
    }

    // --- loop points ---

    #[test]
    fn test_loop_points_from_sample() {
        let vp = default_voice_params(make_sample(60, 100, 500, 44_100.0, 0.0));
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert_eq!(cv.loop_start, 100);
        assert_eq!(cv.loop_end, 500);
    }

    // --- playback_step ---

    #[test]
    fn test_playback_step_matching_rates_no_correction() {
        // sample_rate == playback rate, no pitch correction → step == 1.0
        let vp = default_voice_params(make_sample(60, 0, 99, 44_100.0, 0.0));
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert!((cv.playback_step - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_playback_step_half_sample_rate() {
        // 22050 / 44100 = 0.5, no pitch correction → step == 0.5
        let vp = default_voice_params(make_sample(60, 0, 99, 22_050.0, 0.0));
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert!((cv.playback_step - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_playback_step_1200_cents_pitch_correction() {
        // 1200 cents == 1 octave == factor of 2
        let vp = default_voice_params(make_sample(60, 0, 99, 44_100.0, 1200.0));
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert!((cv.playback_step - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_playback_step_negative_pitch_correction() {
        // -1200 cents → factor of 0.5
        let vp = default_voice_params(make_sample(60, 0, 99, 44_100.0, -1200.0));
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert!((cv.playback_step - 0.5).abs() < 1e-9);
    }

    // --- looping_mode ---

    #[test]
    fn test_looping_mode_default_is_no_loop() {
        // sampleModes default is 0 (no loop)
        let vp = default_voice_params(make_sample(60, 0, 99, 44_100.0, 0.0));
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert_eq!(cv.looping_mode, 0);
    }

    #[test]
    fn test_looping_mode_loop_from_generator() {
        let mut gens = default_generators();
        gens[gt::SAMPLE_MODES as usize] = 1;
        let vp = VoiceParameters {
            generators: gens,
            modulators: vec![],
            sample: make_sample(60, 0, 99, 44_100.0, 0.0),
        };
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert_eq!(cv.looping_mode, 1);
    }

    // --- sample_data ---

    #[test]
    fn test_sample_data_is_copied_from_basic_sample() {
        let audio_data = vec![0.1_f32, 0.2, 0.3];
        let sample = BasicSample {
            original_key: 60,
            loop_start: 0,
            loop_end: 2,
            sample_rate: 44_100.0,
            pitch_correction: 0.0,
            audio_data: audio_data.clone(),
        };
        let vp = default_voice_params(sample);
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert_eq!(cv.sample_data, audio_data);
    }

    // --- generators and modulators ownership ---

    #[test]
    fn test_generators_are_stored() {
        let gens = default_generators();
        let gens_expected = gens.clone();
        let vp = VoiceParameters {
            generators: gens,
            modulators: vec![],
            sample: make_sample(60, 0, 99, 44_100.0, 0.0),
        };
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert_eq!(cv.generators, gens_expected);
    }

    #[test]
    fn test_modulators_count_is_preserved() {
        let mods = vec![Modulator::default(), Modulator::default()];
        let vp = VoiceParameters {
            generators: default_generators(),
            modulators: mods,
            sample: make_sample(60, 0, 99, 44_100.0, 0.0),
        };
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert_eq!(cv.modulators.len(), 2);
    }

    #[test]
    fn test_empty_modulators() {
        let vp = default_voice_params(make_sample(60, 0, 99, 44_100.0, 0.0));
        let cv = CachedVoice::new(vp, 60, 100, 44_100.0);
        assert!(cv.modulators.is_empty());
    }
}
