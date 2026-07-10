/// volume_envelope.rs
/// purpose: applies a volume envelope for a given voice
/// Ported from: src/synthesizer/audio_engine/voice/volume_envelope.ts (spessasynth_core 4.3.0)
///
/// For performance reasons, cbAttenuationToGain is inlined here (via the shared
/// CENTIBEL_LOOKUP_TABLE exposed by `unit_converter::cb_attenuation_to_gain_f64`).
///
/// # 4.3.0 rewrite
/// The volume envelope no longer processes the audio buffer sample-by-sample.
/// Instead, `process` advances the envelope by an entire render quantum and writes the
/// gain value for the LAST sample of the block into `output_gain`, returning the block's
/// activity. The caller (`render_voice`) then linearly interpolates from the previous
/// `output_gain` to the new one across the block. The per-sample gain smoothing and the
/// `centibel_offset` argument were removed; the centibel excursion (mod LFO to volume,
/// resonance) is now folded into `gain_target` by the caller before calling `process`.
use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
use crate::synthesizer::audio_engine::synth_constants::SPESSA_BUFSIZE;
use crate::synthesizer::audio_engine::voice::unit_converter::{
    cb_attenuation_to_gain_f64, timecents_to_seconds,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Per SF2 definition: silence threshold in centibels.
/// Equivalent to: CB_SILENCE
const CB_SILENCE: f64 = 960.0;

/// Perceived silence boundary: above this we consider the voice silent.
/// Equivalent to: PERCEIVED_CB_SILENCE
const PERCEIVED_CB_SILENCE: f64 = 900.0;

// ---------------------------------------------------------------------------
// VolumeEnvelope
// ---------------------------------------------------------------------------

/// Applies a volume envelope (DAHDSR) to a voice.
///
/// Stage mapping (state field):
///   0 = delay, 1 = attack, 2 = hold/peak, 3 = decay, 4 = sustain
///   Release is indicated by `entered_release`.
///
/// Equivalent to: VolumeEnvelope class
pub struct VolumeEnvelope {
    /// The target gain for the current rendering block (the last sample's gain).
    /// Equivalent to: outputGain
    pub output_gain: f64,
    /// The current attenuation of the envelope in cB.
    /// Equivalent to: attenuationCb
    pub attenuation_cb: f64,
    /// The current stage of the volume envelope (0–4).
    /// Equivalent to: state
    pub state: u8,

    /// The sample rate in Hz.
    /// Equivalent to: sampleRate
    sample_rate: f64,
    /// The sample count between updates of the volume envelope (= render buffer size).
    /// Since the envelope calculation runs once per rendering quantum, this is effectively
    /// the buffer size. For the WAV pipeline this is fixed at `SPESSA_BUFSIZE`.
    /// Equivalent to: updateInterval
    update_interval: f64,

    /// The envelope's current time in samples.
    /// Uses f64 to match TS behavior (JS number is f64).
    /// Equivalent to: sampleTime
    pub(crate) sample_time: f64,
    /// The attenuation in cB when the voice entered the release stage.
    /// Equivalent to: releaseStartCb
    release_start_cb: f64,
    /// Sample time when release was triggered.
    /// Equivalent to: releaseStartTimeSamples
    release_start_time_samples: f64,
    /// Attack duration in samples.
    /// Equivalent to: attackDuration
    attack_duration: f64,
    /// Decay duration in samples (already scaled by the sustain fraction).
    /// Equivalent to: decayDuration
    decay_duration: f64,
    /// Release duration in samples.
    /// Equivalent to: releaseDuration
    release_duration: f64,
    /// Sustain level in cB.
    /// Equivalent to: sustainCb
    pub(crate) sustain_cb: f64,
    /// Sample index where the delay phase ends.
    /// Equivalent to: delayEnd
    delay_end: f64,
    /// Sample index where the attack phase ends.
    /// Equivalent to: attackEnd
    pub(crate) attack_end: f64,
    /// Sample index where the hold phase ends.
    /// Equivalent to: holdEnd
    pub(crate) hold_end: f64,
    /// Sample index where the decay phase ends.
    /// Equivalent to: decayEnd
    pub(crate) decay_end: f64,
    /// Whether the envelope has entered the release phase.
    /// Equivalent to: enteredRelease
    entered_release: bool,
    /// If sustain is silent, the voice can end when it reaches silence.
    /// Equivalent to: canEndOnSilentSustain
    can_end_on_silent_sustain: bool,
}

impl VolumeEnvelope {
    /// Creates a new VolumeEnvelope.
    ///
    /// Equivalent to: new VolumeEnvelope(sampleRate, bufferSize)
    ///
    /// The TS constructor takes `bufferSize` (= `maxBufferSize`). For this WAV-only port the
    /// render quantum is always `SPESSA_BUFSIZE`, so `update_interval` is fixed to it here.
    /// This keeps the constructor signature unchanged and confines the 4.3.0 rewrite to
    /// `volume_envelope.rs` + `render_voice.rs`.
    pub fn new(sample_rate: f64) -> Self {
        Self {
            output_gain: 0.0,
            attenuation_cb: CB_SILENCE,
            state: 0,
            sample_rate,
            update_interval: SPESSA_BUFSIZE as f64,
            sample_time: 0.0,
            release_start_cb: CB_SILENCE,
            release_start_time_samples: 0.0,
            attack_duration: 0.0,
            decay_duration: 0.0,
            release_duration: 0.0,
            sustain_cb: 0.0,
            delay_end: 0.0,
            attack_end: 0.0,
            hold_end: 0.0,
            decay_end: 0.0,
            entered_release: false,
            can_end_on_silent_sustain: false,
        }
    }

    /// Converts timecents to a sample count (>= 0).
    /// Returns f64 to match TS behavior (JS Math.floor returns f64).
    /// Equivalent to: timecentsToSamples(tc)
    fn timecents_to_samples(&self, tc: i32) -> f64 {
        // Match TS: Math.max(0, Math.floor(timecentsToSeconds(tc) * this.sampleRate))
        // TS order: floor first, then max(0, ...).
        // JS Math.max(0, NaN) returns NaN; Rust f64::NAN.max(0.0) returns 0.0.
        // We must preserve NaN propagation to match TS behavior.
        let samples = (timecents_to_seconds(tc) as f64 * self.sample_rate).floor();
        if samples > 0.0 {
            samples
        } else if samples.is_nan() {
            f64::NAN
        } else {
            0.0
        }
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Initialises the envelope for a new note-on event.
    ///
    /// `modulated_generators` – voice.modulatedGenerators (GENERATORS_AMOUNT elements)
    /// `target_key`           – voice.targetKey
    ///
    /// Equivalent to: init(voice)
    pub fn init(&mut self, modulated_generators: &[i16], target_key: i16) {
        self.entered_release = false;
        self.state = 0;
        self.sample_time = 0.0;
        self.output_gain = 0.0;
        self.can_end_on_silent_sustain =
            modulated_generators[gt::SUSTAIN_VOL_ENV as usize] as f64 >= PERCEIVED_CB_SILENCE;

        // Sustain level (clamped to CB_SILENCE).
        self.sustain_cb =
            (modulated_generators[gt::SUSTAIN_VOL_ENV as usize] as f64).min(CB_SILENCE);

        // Attack duration.
        self.attack_duration =
            self.timecents_to_samples(modulated_generators[gt::ATTACK_VOL_ENV as usize] as i32);

        // Decay: SF2 spec section 8.1.3 -- time is for 0 dB to -100 dB, so scale by
        // the fraction representing how far sustain is from silence.
        // Keep as f64 to match TS behavior (JS number * number = number).
        let key_num_addition = (60 - target_key as i32) as f64
            * modulated_generators[gt::KEY_NUM_TO_VOL_ENV_DECAY as usize] as f64;
        let fraction = self.sustain_cb / CB_SILENCE;
        self.decay_duration = self.timecents_to_samples(
            (modulated_generators[gt::DECAY_VOL_ENV as usize] as f64 + key_num_addition) as i32,
        ) * fraction;

        // Absolute end-times.
        self.delay_end =
            self.timecents_to_samples(modulated_generators[gt::DELAY_VOL_ENV as usize] as i32);
        self.attack_end = self.attack_duration + self.delay_end;

        // Hold: also account for keyNumToVolEnvHold.
        let hold_excursion = (60 - target_key as i32) as f64
            * modulated_generators[gt::KEY_NUM_TO_VOL_ENV_HOLD as usize] as f64;
        self.hold_end = self.timecents_to_samples(
            (modulated_generators[gt::HOLD_VOL_ENV as usize] as f64 + hold_excursion) as i32,
        ) + self.attack_end;

        self.decay_end = self.decay_duration + self.hold_end;

        // If the voice has no meaningful delay/attack (within one update interval),
        // jump directly to the hold/peak stage.
        // TS: if (this.attackEnd <= this.updateInterval)
        if self.attack_end <= self.update_interval {
            self.state = 2;
        }
    }

    /// Transitions the envelope into the release phase.
    ///
    /// `modulated_generators`    – voice.modulatedGenerators
    /// `target_key`              – voice.targetKey
    /// `override_release_vol_env`– voice.overrideReleaseVolEnv (0 = use generator)
    ///
    /// Returns `true` when the voice should be deactivated immediately
    /// (i.e., the envelope is already perceived as silent).
    ///
    /// Equivalent to: startRelease(voice)  [caller sets voice.isActive = false
    /// if this returns true]
    pub fn start_release(
        &mut self,
        modulated_generators: &[i16],
        target_key: i16,
        override_release_vol_env: i32,
    ) -> bool {
        self.release_start_time_samples = self.sample_time;

        // Determine the release timecents (override or generator).
        // TypeScript: voice.overrideReleaseVolEnv || voice.modulatedGenerators[releaseVolEnv]
        let timecents = if override_release_vol_env != 0 {
            override_release_vol_env
        } else {
            modulated_generators[gt::RELEASE_VOL_ENV as usize] as i32
        };
        // SF2 spec: min −7200 timecents to prevent clicks.
        self.release_duration = self.timecents_to_samples(timecents.max(-7200));

        if self.entered_release {
            // Already in release (e.g. exclusive class update): just track the
            // current attenuation as the new release starting point.
            self.release_start_cb = self.attenuation_cb;
        } else {
            let sustain_cb = self.sustain_cb.clamp(0.0, CB_SILENCE);
            let fraction = sustain_cb / CB_SILENCE;

            // Recalculate the (already-started) decay duration so we can estimate
            // the release start level for voices in the decay stage.
            let key_num_addition = (60 - target_key as i32) as f64
                * modulated_generators[gt::KEY_NUM_TO_VOL_ENV_DECAY as usize] as f64;
            self.decay_duration = self.timecents_to_samples(
                (modulated_generators[gt::DECAY_VOL_ENV as usize] as f64 + key_num_addition) as i32,
            ) * fraction;

            // Estimate the attenuation (in cB) at the moment of release,
            // depending on which stage the envelope was in.
            self.release_start_cb = match self.state {
                0 => {
                    // Delay stage: no sound produced yet.
                    CB_SILENCE
                }
                1 => {
                    // Attack stage: linear gain -> convert to dB.
                    let elapsed =
                        1.0 - (self.attack_end - self.release_start_time_samples) / self.attack_duration;
                    // linearGain -> cB: 200 * log10(gain) * -1
                    -200.0 * elapsed.log10()
                }
                2 => {
                    // Hold/peak stage: full volume.
                    0.0
                }
                3 => {
                    // Decay stage: interpolate between 0 and sustainCb.
                    (1.0 - (self.decay_end - self.release_start_time_samples) / self.decay_duration)
                        * sustain_cb
                }
                _ => {
                    // Sustain stage (or unknown).
                    sustain_cb
                }
            };

            self.release_start_cb = self.release_start_cb.clamp(0.0, CB_SILENCE);
            self.attenuation_cb = self.release_start_cb;
        }

        self.entered_release = true;

        // Scale the release duration by the fraction still remaining to silence.
        // SF2 spec: time is from peak to -100 dB, so adjust for the actual start level.
        let release_fraction = (CB_SILENCE - self.release_start_cb) / CB_SILENCE;
        self.release_duration *= release_fraction;

        // If already at or past perceived silence, signal immediate voice end.
        self.release_start_cb >= PERCEIVED_CB_SILENCE
    }

    /// Advances the envelope by one render quantum and writes the last sample's gain into
    /// `output_gain`. Returns `true` if the voice is still active after this block.
    ///
    /// Essentially we use the approach of 100 dB is silence, 0 dB is peak.
    ///
    /// `gain_target` – the gain to apply (initial attenuation * centibel excursion gain).
    ///
    /// Equivalent to: process(sampleCount, gainTarget)
    pub fn process(&mut self, sample_count: usize, gain_target: f64) -> bool {
        // Advance time by the entire block to calculate the last sample's gain.
        self.sample_time += sample_count as f64;
        let sample_time = self.sample_time;

        if self.entered_release {
            // How much time has passed since release was started?
            let elapsed_release = sample_time - self.release_start_time_samples;
            let cb_difference = CB_SILENCE - self.release_start_cb;

            // Linearly ramp down decibels.
            self.attenuation_cb =
                (elapsed_release / self.release_duration) * cb_difference + self.release_start_cb;
            self.output_gain = cb_attenuation_to_gain_f64(self.attenuation_cb) as f64 * gain_target;
            return self.attenuation_cb < PERCEIVED_CB_SILENCE;
        }

        // Delay phase: no sound is produced.
        if self.state == 0 {
            if sample_time < self.delay_end {
                // Silence.
                self.attenuation_cb = CB_SILENCE;
                self.output_gain = 0.0;
                return true;
            }
            self.state += 1;
        }

        // Attack phase: ramp from 0 to attenuation.
        if self.state == 1 {
            if sample_time < self.attack_end {
                // Set current attenuation to peak as it's invalid during this phase.
                self.attenuation_cb = 0.0;
                // Special case: linear gain ramp instead of linear dB ramp.
                let linear_gain = 1.0 - (self.attack_end - sample_time) / self.attack_duration;
                self.output_gain = linear_gain * gain_target;
                return true;
            }
            self.state += 1;
        }

        // Hold/peak phase: stay at max volume.
        if self.state == 2 {
            if sample_time < self.hold_end {
                // Peak, no attenuation.
                self.attenuation_cb = 0.0;
                self.output_gain = gain_target;
                return true;
            }
            self.state += 1;
        }

        // Decay phase: linear centibel ramp down to sustain.
        if self.state == 3 {
            if sample_time < self.decay_end {
                self.attenuation_cb =
                    (1.0 - (self.decay_end - sample_time) / self.decay_duration) * self.sustain_cb;
                self.output_gain =
                    gain_target * cb_attenuation_to_gain_f64(self.attenuation_cb) as f64;
                return true;
            }
            self.state += 1;
        }

        // Sustain phase: stay at sustain.
        if self.can_end_on_silent_sustain && self.sustain_cb >= PERCEIVED_CB_SILENCE {
            // Make sure to end on silence.
            // https://github.com/spessasus/spessasynth_core/issues/57
            self.attenuation_cb = CB_SILENCE;
            self.output_gain = 0.0;
            return false;
        }

        self.attenuation_cb = self.sustain_cb;
        self.output_gain = gain_target * cb_attenuation_to_gain_f64(self.sustain_cb) as f64;
        true
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator_types::{
        DEFAULT_GENERATOR_VALUES, generator_types as gt,
    };
    use crate::synthesizer::audio_engine::voice::unit_converter::cb_attenuation_to_gain;

    const SAMPLE_RATE: f64 = 44_100.0;
    const EPS64: f64 = 1e-5;

    fn approx_eq64(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS64
    }

    /// Returns default generator values as a Vec.
    fn default_gens() -> Vec<i16> {
        DEFAULT_GENERATOR_VALUES.to_vec()
    }

    /// Builds a generator vec with a specific timecent value for one generator.
    fn gens_with(mut g: Vec<i16>, index: i16, value: i16) -> Vec<i16> {
        g[index as usize] = value;
        g
    }

    /// Returns generators with delay and attack both zeroed (i16::MIN → 0 samples).
    /// i16::MIN = -32768 satisfies `timecents <= -32767` → timecents_to_seconds returns 0.
    fn gens_no_delay_no_attack() -> Vec<i16> {
        let g = default_gens();
        let g = gens_with(g, gt::DELAY_VOL_ENV, i16::MIN);
        gens_with(g, gt::ATTACK_VOL_ENV, i16::MIN)
    }

    fn new_env() -> VolumeEnvelope {
        VolumeEnvelope::new(SAMPLE_RATE)
    }

    // -----------------------------------------------------------------------
    // new()
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_initial_attenuation_is_cb_silence() {
        let env = new_env();
        assert!(approx_eq64(env.attenuation_cb, 960.0));
    }

    #[test]
    fn test_new_state_is_zero() {
        let env = new_env();
        assert_eq!(env.state, 0);
    }

    #[test]
    fn test_new_entered_release_is_false() {
        let env = new_env();
        assert!(!env.entered_release);
    }

    #[test]
    fn test_new_output_gain_is_zero() {
        let env = new_env();
        assert!(approx_eq64(env.output_gain, 0.0));
    }

    #[test]
    fn test_new_update_interval_is_bufsize() {
        let env = new_env();
        assert!(approx_eq64(env.update_interval, SPESSA_BUFSIZE as f64));
    }

    // -----------------------------------------------------------------------
    // init()
    // -----------------------------------------------------------------------

    #[test]
    fn test_init_resets_state_to_zero() {
        let mut env = new_env();
        env.state = 3;
        // Give a long attack so state stays at 0 (attack_end > update_interval).
        let gens = gens_with(default_gens(), gt::ATTACK_VOL_ENV, 0);
        env.init(&gens, 60);
        assert_eq!(env.state, 0);
    }

    #[test]
    fn test_init_resets_entered_release() {
        let mut env = new_env();
        env.entered_release = true;
        env.init(&default_gens(), 60);
        assert!(!env.entered_release);
    }

    #[test]
    fn test_init_short_attack_jumps_to_hold() {
        // When both delay and attack resolve to 0 samples, attack_end == 0 <= update_interval
        // → jump directly to state 2 (hold/peak).
        let mut env = new_env();
        env.init(&gens_no_delay_no_attack(), 60);
        assert_eq!(env.state, 2);
    }

    #[test]
    fn test_init_with_long_attack_stays_in_delay_state() {
        // attackVolEnv = 0 timecents ≈ 44100 samples → attack_end >> update_interval → state 0.
        let gens = gens_with(default_gens(), gt::ATTACK_VOL_ENV, 0);
        let mut env = new_env();
        env.init(&gens, 60);
        assert_eq!(env.state, 0);
    }

    #[test]
    fn test_init_attack_end_with_nonzero_attack_timecents() {
        // attackVolEnv = 0 timecents → 44100 samples; delay zeroed → attack_end = 44100.
        let g = gens_with(gens_no_delay_no_attack(), gt::ATTACK_VOL_ENV, 0);
        let mut env = new_env();
        env.init(&g, 60);
        assert_eq!(env.attack_end, 44_100.0);
    }

    #[test]
    fn test_init_sustain_cb_zero_from_default() {
        let mut env = new_env();
        env.init(&default_gens(), 60);
        assert!(approx_eq64(env.sustain_cb, 0.0));
    }

    // -----------------------------------------------------------------------
    // process() – delay stage
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_delay_outputs_zero_gain() {
        // delayVolEnv = 0 timecents → 1 second delay. Long attack keeps state 0.
        let gens = gens_with(
            gens_with(default_gens(), gt::DELAY_VOL_ENV, 0),
            gt::ATTACK_VOL_ENV,
            0,
        );
        let mut env = new_env();
        env.init(&gens, 60);
        assert_eq!(env.state, 0);

        let active = env.process(128, 1.0);
        assert!(active);
        assert!(approx_eq64(env.output_gain, 0.0));
    }

    // -----------------------------------------------------------------------
    // process() – hold stage
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_hold_keeps_unity_gain() {
        // Zero delay + zero attack → state 2; holdVolEnv = 0 → 1 second hold.
        let gens = gens_with(gens_no_delay_no_attack(), gt::HOLD_VOL_ENV, 0);
        let mut env = new_env();
        env.init(&gens, 60);
        assert_eq!(env.state, 2);

        let active = env.process(128, 1.0);
        assert!(active);
        // Hold outputs gain_target directly.
        assert!(approx_eq64(env.output_gain, 1.0));
    }

    // -----------------------------------------------------------------------
    // process() – decay stage
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_decay_ends_at_sustain_level() {
        // No delay/attack, hold=0, decayVolEnv=0 (1 sec), sustainVolEnv=480 (half silence).
        let gens = {
            let mut g = gens_no_delay_no_attack();
            g[gt::HOLD_VOL_ENV as usize] = i16::MIN;
            g[gt::DECAY_VOL_ENV as usize] = 0;
            g[gt::SUSTAIN_VOL_ENV as usize] = 480;
            g
        };
        let mut env = new_env();
        env.init(&gens, 60);

        // Advance a full decay (44100 samples in one block).
        env.process(44_100, 1.0);

        // The last sample should be near cb_attenuation_to_gain(480).
        let expected = cb_attenuation_to_gain(480) as f64;
        assert!(
            (env.output_gain - expected).abs() < 0.02,
            "decay end gain {} not near expected sustain gain {}",
            env.output_gain,
            expected
        );
    }

    // -----------------------------------------------------------------------
    // process() – sustain stage
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_sustain_is_constant() {
        let mut env = new_env();
        env.state = 4;
        env.sustain_cb = 200.0;

        let expected_gain = cb_attenuation_to_gain(200) as f64;
        env.process(64, 1.0);
        assert!(
            (env.output_gain - expected_gain).abs() < 1e-4,
            "sustain gain {} not near expected {}",
            env.output_gain,
            expected_gain
        );
    }

    #[test]
    fn test_process_sustain_returns_true() {
        let mut env = new_env();
        env.state = 4;
        env.sustain_cb = 0.0;
        let active = env.process(32, 1.0);
        assert!(active);
    }

    #[test]
    fn test_process_sustain_silent_can_end() {
        // can_end_on_silent_sustain && sustain_cb >= PERCEIVED_CB_SILENCE → end, gain 0.
        let mut env = new_env();
        env.state = 4;
        env.sustain_cb = 960.0;
        env.can_end_on_silent_sustain = true;

        let active = env.process(32, 1.0);
        assert!(!active);
        assert!(approx_eq64(env.output_gain, 0.0));
    }

    // -----------------------------------------------------------------------
    // process() – release stage
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_release_attenuates_to_silence() {
        let mut env = new_env();
        env.state = 4;
        env.sustain_cb = 0.0;
        env.entered_release = true;
        env.release_start_cb = 0.0;
        env.release_start_time_samples = 0.0;
        env.sample_time = 0.0;
        env.release_duration = 44_100.0;

        let active = env.process(44_100, 1.0);
        // After a full release the attenuation should be at CB_SILENCE (perceived silent).
        assert!(
            !active || env.attenuation_cb >= PERCEIVED_CB_SILENCE,
            "expected voice inactive or perceived silence, attenuation_cb={}",
            env.attenuation_cb
        );
    }

    #[test]
    fn test_process_release_returns_false_at_silence() {
        // release_start_cb=0, release_duration=128; after 128 samples elapsed=128:
        //   attenuation_cb = (128/128)*960 = 960 > PERCEIVED (900) → returns false.
        let mut env = new_env();
        env.entered_release = true;
        env.release_start_cb = 0.0;
        env.release_start_time_samples = 0.0;
        env.sample_time = 0.0;
        env.release_duration = 128.0;

        let active = env.process(128, 1.0);
        assert!(!active);
    }

    // -----------------------------------------------------------------------
    // start_release()
    // -----------------------------------------------------------------------

    #[test]
    fn test_start_release_sets_entered_release() {
        let mut env = new_env();
        env.state = 4;
        env.sustain_cb = 0.0;
        env.init(&default_gens(), 60);

        let gens = gens_with(default_gens(), gt::RELEASE_VOL_ENV, 0);
        env.start_release(&gens, 60, 0);
        assert!(env.entered_release);
    }

    #[test]
    fn test_start_release_immediate_deactivation_when_silent() {
        // State 0 (delay): release_start_cb = CB_SILENCE (960 >= 900) → deactivate.
        let mut env = new_env();
        env.state = 0;
        env.sustain_cb = 0.0;
        env.sample_time = 0.0;

        let gens = gens_with(default_gens(), gt::RELEASE_VOL_ENV, 0);
        let deactivate = env.start_release(&gens, 60, 0);
        assert!(deactivate);
    }

    #[test]
    fn test_start_release_from_hold_stage_not_immediately_silent() {
        // State 2 (hold): release_start_cb = 0 → not silent.
        let mut env = new_env();
        env.state = 2;
        env.sustain_cb = 0.0;
        env.attenuation_cb = 0.0;
        env.sample_time = 100.0;
        env.decay_end = 200.0;
        env.decay_duration = 100.0;

        let gens = gens_with(default_gens(), gt::RELEASE_VOL_ENV, 0);
        let deactivate = env.start_release(&gens, 60, 0);
        assert!(!deactivate);
        assert!(approx_eq64(env.release_start_cb, 0.0));
    }

    #[test]
    fn test_start_release_uses_override_when_nonzero() {
        let mut env = new_env();
        env.state = 2;
        env.sustain_cb = 0.0;
        env.sample_time = 0.0;
        env.decay_end = 0.0;
        env.decay_duration = 0.0;

        let gens = default_gens();
        env.start_release(&gens, 60, -2320);
        // Duration is based on max(-7200, -2320) = -2320; releaseFraction = 1.0 (releaseStartCb=0).
        let expected_secs = 2f64.powf(-2320.0 / 1200.0) * SAMPLE_RATE;
        assert!(
            (env.release_duration - expected_secs).abs() < 2.0,
            "release_duration {} not near expected {}",
            env.release_duration,
            expected_secs
        );
    }

    #[test]
    fn test_start_release_twice_updates_from_current_attenuation() {
        let mut env = new_env();
        env.state = 2;
        env.sustain_cb = 0.0;
        env.sample_time = 0.0;
        env.decay_end = 0.0;
        env.decay_duration = 0.0;

        let gens = gens_with(default_gens(), gt::RELEASE_VOL_ENV, 0);
        env.start_release(&gens, 60, 0);
        assert!(env.entered_release);

        env.attenuation_cb = 200.0;
        env.start_release(&gens, 60, 0);
        assert!(approx_eq64(env.release_start_cb, 200.0));
    }
}
