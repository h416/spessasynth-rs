/// volume_envelope.rs
/// purpose: applies a volume envelope for a given voice
/// Ported from: src/synthesizer/audio_engine/engine_components/dsp_chain/volume_envelope.ts
use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
use crate::synthesizer::audio_engine::engine_components::unit_converter::{
    cb_attenuation_to_gain, cb_attenuation_to_gain_f64, timecents_to_seconds,
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

/// Gain smoothing factor. Must be run EVERY SAMPLE.
/// Equivalent to: GAIN_SMOOTHING_FACTOR
const GAIN_SMOOTHING_FACTOR: f64 = 0.01;

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
    /// The sample rate in Hz.
    /// Equivalent to: sampleRate
    pub sample_rate: f64,
    /// The current attenuation of the envelope in cB.
    /// Equivalent to: attenuationCb
    pub attenuation_cb: f64,
    /// The current stage of the volume envelope (0–4).
    /// Equivalent to: state
    pub state: u8,

    /// The envelope's current time in samples.
    /// Uses f64 to match TS behavior (JS number is f64).
    /// Equivalent to: sampleTime
    pub(crate) sample_time: f64,
    /// The attenuation in cB when the voice entered the release stage.
    /// Equivalent to: releaseStartCb
    release_start_cb: f64,
    /// Sample time when release was triggered.
    /// Uses f64 to match TS behavior.
    /// Equivalent to: releaseStartTimeSamples
    release_start_time_samples: f64,
    /// Attack duration in samples.
    /// Uses f64 to match TS behavior.
    /// Equivalent to: attackDuration
    attack_duration: f64,
    /// Decay duration in samples (already scaled by the sustain fraction).
    /// Uses f64 to match TS behavior (can be fractional after multiply by fraction).
    /// Equivalent to: decayDuration
    decay_duration: f64,
    /// Release duration in samples.
    /// Uses f64 to match TS behavior.
    /// Equivalent to: releaseDuration
    release_duration: f64,
    /// Sustain level in cB.
    /// Equivalent to: sustainCb
    pub(crate) sustain_cb: f64,
    /// Sample index where the delay phase ends.
    /// Uses f64 to match TS behavior.
    /// Equivalent to: delayEnd
    delay_end: f64,
    /// Sample index where the attack phase ends.
    /// Uses f64 to match TS behavior.
    /// Equivalent to: attackEnd
    pub(crate) attack_end: f64,
    /// Sample index where the hold phase ends.
    /// Uses f64 to match TS behavior.
    /// Equivalent to: holdEnd
    pub(crate) hold_end: f64,
    /// Sample index where the decay phase ends.
    /// Uses f64 to match TS behavior.
    /// Equivalent to: decayEnd
    pub(crate) decay_end: f64,
    /// Whether the envelope has entered the release phase.
    /// Equivalent to: enteredRelease
    entered_release: bool,
    /// If sustain is silent, the voice can end when it reaches silence.
    /// Equivalent to: canEndOnSilentSustain
    can_end_on_silent_sustain: bool,
    /// Gain smoothing factor (adjusted for sample rate).
    /// Equivalent to: gainSmoothing
    gain_smoothing: f64,
    /// Current smoothed gain value.
    /// Equivalent to: currentGain
    pub(crate) current_gain: f64,

}

impl VolumeEnvelope {
    /// Creates a new VolumeEnvelope.
    /// Equivalent to: new VolumeEnvelope(sampleRate)
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            attenuation_cb: CB_SILENCE,
            state: 0,
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
            gain_smoothing: GAIN_SMOOTHING_FACTOR * (44_100.0 / sample_rate),
            current_gain: 0.0,
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
        self.can_end_on_silent_sustain =
            modulated_generators[gt::SUSTAIN_VOL_ENV as usize] as f64 >= PERCEIVED_CB_SILENCE;

        // Set the initial gain from the initialAttenuation generator.
        self.current_gain =
            cb_attenuation_to_gain(modulated_generators[gt::INITIAL_ATTENUATION as usize] as i32) as f64;

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

        // If there is no delay/attack, jump directly to the hold/peak stage.
        // TS: if (this.attackEnd === 0)
        if self.attack_end == 0.0 {
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
                    if self.decay_duration == 0.0 {
                        sustain_cb
                    } else {
                        (1.0 - (self.decay_end - self.release_start_time_samples)
                            / self.decay_duration)
                            * sustain_cb
                    }
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

    /// Applies the volume envelope to `buffer[..sample_count]`.
    ///
    /// `gain_target`     – the external gain target (for smoothing)
    /// `centibel_offset` – additional centibel offset (LFO / modulation)
    ///
    /// Returns `true` if the voice is still active after this block.
    ///
    /// Equivalent to: process(sampleCount, buffer, gainTarget, centibelOffset)
    pub fn process(
        &mut self,
        sample_count: usize,
        buffer: &mut [f32],
        gain_target: f64,
        centibel_offset: f64,
    ) -> bool {
        if self.entered_release {
            return self.release_phase(sample_count, buffer, gain_target, centibel_offset);
        }

        match self.state {
            0 => self.delay_phase(sample_count, buffer, gain_target, centibel_offset, 0),
            1 => self.attack_phase(sample_count, buffer, gain_target, centibel_offset, 0),
            2 => self.hold_phase(sample_count, buffer, gain_target, centibel_offset, 0),
            3 => self.decay_phase(sample_count, buffer, gain_target, centibel_offset, 0),
            4 => self.sustain_phase(sample_count, buffer, gain_target, centibel_offset, 0),
            _ => false,
        }
    }

    // -----------------------------------------------------------------------
    // Private phase helpers
    // -----------------------------------------------------------------------

    /// Release phase: linearly ramp attenuation from releaseStartCb to CB_SILENCE.
    /// Equivalent to: releasePhase(...)
    fn release_phase(
        &mut self,
        sample_count: usize,
        buffer: &mut [f32],
        gain_target: f64,
        centibel_offset: f64,
    ) -> bool {
        let mut sample_time = self.sample_time;
        let mut current_gain = self.current_gain;
        let mut attenuation_cb = self.attenuation_cb;

        let release_start_cb = self.release_start_cb;
        let release_duration = self.release_duration;
        let gain_smoothing = self.gain_smoothing;
        let mut elapsed_release = sample_time - self.release_start_time_samples;
        let cb_difference = CB_SILENCE - release_start_cb;

        let smooth = current_gain != gain_target;

        for sample in buffer[..sample_count].iter_mut() {
            if smooth {
                current_gain += (gain_target - current_gain) * gain_smoothing;
            }

            // Linear ramp of attenuation from releaseStartCb to CB_SILENCE.
            attenuation_cb = if release_duration == 0.0 {
                CB_SILENCE
            } else {
                (elapsed_release / release_duration) * cb_difference
                    + release_start_cb
            };

            // Use f64 version to match TS: cbAttenuationToGain(attenuationCb + centibelOffset)
            // where the f64 sum is passed directly and truncated inside the function.
            // Clamp attenuation_cb to CB_SILENCE to prevent out-of-bounds during long releases.
            let cb_combined_f64 = attenuation_cb.min(CB_SILENCE) + centibel_offset;
            // Emulate JS Float32Array *= semantics:
            // f32 → f64, multiply in f64, store back as f32
            *sample = (*sample as f64 * (cb_attenuation_to_gain_f64(cb_combined_f64) as f64 * current_gain)) as f32;
            sample_time += 1.0;
            elapsed_release += 1.0;
        }

        self.sample_time = sample_time;
        self.current_gain = current_gain;
        self.attenuation_cb = attenuation_cb;

        attenuation_cb < PERCEIVED_CB_SILENCE
    }

    /// Delay phase: output silence until the delay end time.
    /// Equivalent to: delayPhase(...)
    fn delay_phase(
        &mut self,
        sample_count: usize,
        buffer: &mut [f32],
        gain_target: f64,
        centibel_offset: f64,
        filled_buffer: usize,
    ) -> bool {
        let delay_end = self.delay_end;
        let mut sample_time = self.sample_time;
        let mut filled_buffer = filled_buffer;

        if sample_time < delay_end {
            self.attenuation_cb = CB_SILENCE;

            // How many silence samples to write?
            let delay_samples =
                ((delay_end - sample_time) as usize).min(sample_count - filled_buffer);
            for s in buffer[filled_buffer..filled_buffer + delay_samples].iter_mut() {
                *s = 0.0;
            }
            filled_buffer += delay_samples;
            sample_time += delay_samples as f64;

            if filled_buffer >= sample_count {
                self.sample_time = sample_time;
                return true;
            }
        }

        self.sample_time = sample_time;
        self.state += 1;

        self.attack_phase(
            sample_count,
            buffer,
            gain_target,
            centibel_offset,
            filled_buffer,
        )
    }

    /// Attack phase: linear gain ramp from 0 to full.
    /// Equivalent to: attackPhase(...)
    fn attack_phase(
        &mut self,
        sample_count: usize,
        buffer: &mut [f32],
        gain_target: f64,
        centibel_offset: f64,
        mut filled_buffer: usize,
    ) -> bool {
        let attack_end = self.attack_end;
        let attack_duration = self.attack_duration;
        let gain_smoothing = self.gain_smoothing;
        let mut sample_time = self.sample_time;
        let mut current_gain = self.current_gain;
        let smooth = current_gain != gain_target;

        if sample_time < attack_end {
            // During attack, attenuation is 0 (peak) for accounting purposes.
            self.attenuation_cb = 0.0;

            // Apply gain offset (e.g. from LFO volume modulation) during attack phase.
            let gain_offset = cb_attenuation_to_gain_f64(centibel_offset) as f64;
            while sample_time < attack_end {
                if smooth {
                    current_gain += (gain_target - current_gain) * gain_smoothing;
                }

                // Special case: linear gain ramp (not linear dB).
                let linear_gain = if attack_duration == 0.0 {
                    1.0
                } else {
                    1.0 - (attack_end - sample_time) / attack_duration
                };

                // Emulate JS Float32Array *= semantics:
                // JS: buffer[i] *= linearGain * currentGain * gainOffset
                buffer[filled_buffer] = (buffer[filled_buffer] as f64 * (linear_gain * current_gain * gain_offset)) as f32;

                sample_time += 1.0;
                filled_buffer += 1;
                if filled_buffer >= sample_count {
                    self.sample_time = sample_time;
                    self.current_gain = current_gain;
                    return true;
                }
            }
        }

        self.sample_time = sample_time;
        self.current_gain = current_gain;
        self.state += 1;

        self.hold_phase(
            sample_count,
            buffer,
            gain_target,
            centibel_offset,
            filled_buffer,
        )
    }

    /// Hold/peak phase: full volume.
    /// Equivalent to: holdPhase(...)
    fn hold_phase(
        &mut self,
        sample_count: usize,
        buffer: &mut [f32],
        gain_target: f64,
        centibel_offset: f64,
        mut filled_buffer: usize,
    ) -> bool {
        let hold_end = self.hold_end;
        let gain_smoothing = self.gain_smoothing;
        let mut sample_time = self.sample_time;
        let mut current_gain = self.current_gain;
        let smooth = current_gain != gain_target;

        if sample_time < hold_end {
            // Peak: zero attenuation.
            self.attenuation_cb = 0.0;

            // Use f64 version to match TS: cbAttenuationToGain(centibelOffset)
            let gain_offset = cb_attenuation_to_gain_f64(centibel_offset) as f64;
            while sample_time < hold_end {
                if smooth {
                    current_gain += (gain_target - current_gain) * gain_smoothing;
                }

                // Emulate JS Float32Array *= semantics:
                // JS: buffer[i] *= currentGain * gainOffset  → buffer[i] * (currentGain * gainOffset)
                buffer[filled_buffer] = (buffer[filled_buffer] as f64 * (current_gain * gain_offset)) as f32;

                sample_time += 1.0;
                filled_buffer += 1;
                if filled_buffer >= sample_count {
                    self.sample_time = sample_time;
                    self.current_gain = current_gain;
                    return true;
                }
            }
        }

        self.sample_time = sample_time;
        self.current_gain = current_gain;
        self.state += 1;

        self.decay_phase(
            sample_count,
            buffer,
            gain_target,
            centibel_offset,
            filled_buffer,
        )
    }

    /// Decay phase: linear attenuation ramp from 0 cB to sustainCb.
    /// Equivalent to: decayPhase(...)
    fn decay_phase(
        &mut self,
        sample_count: usize,
        buffer: &mut [f32],
        gain_target: f64,
        centibel_offset: f64,
        mut filled_buffer: usize,
    ) -> bool {
        let decay_duration = self.decay_duration;
        let decay_end = self.decay_end;
        let gain_smoothing = self.gain_smoothing;
        let sustain_cb = self.sustain_cb;
        let mut sample_time = self.sample_time;
        let mut current_gain = self.current_gain;
        let mut attenuation_cb = self.attenuation_cb;
        let smooth = current_gain != gain_target;

        if sample_time < decay_end {
            while sample_time < decay_end {
                if smooth {
                    current_gain += (gain_target - current_gain) * gain_smoothing;
                }

                // Linear ramp from 0 to sustainCb.
                attenuation_cb = if decay_duration == 0.0 {
                    sustain_cb
                } else {
                    (1.0 - (decay_end - sample_time) / decay_duration) * sustain_cb
                };

                // Emulate JS Float32Array *= semantics:
                // JS: buffer[i] *= currentGain * cbAttenuationToGain(...)  → buffer[i] * (currentGain * cbAtten)
                buffer[filled_buffer] = (buffer[filled_buffer] as f64
                    * (current_gain
                    * cb_attenuation_to_gain_f64(attenuation_cb + centibel_offset) as f64)) as f32;

                sample_time += 1.0;
                filled_buffer += 1;
                if filled_buffer >= sample_count {
                    self.sample_time = sample_time;
                    self.current_gain = current_gain;
                    self.attenuation_cb = attenuation_cb;
                    return true;
                }
            }
        }

        self.sample_time = sample_time;
        self.current_gain = current_gain;
        self.attenuation_cb = attenuation_cb;
        self.state += 1;

        self.sustain_phase(
            sample_count,
            buffer,
            gain_target,
            centibel_offset,
            filled_buffer,
        )
    }

    /// Sustain phase: hold at the sustain level.
    /// Equivalent to: sustainPhase(...)
    fn sustain_phase(
        &mut self,
        sample_count: usize,
        buffer: &mut [f32],
        gain_target: f64,
        centibel_offset: f64,
        mut filled_buffer: usize,
    ) -> bool {
        let sustain_cb = self.sustain_cb;
        let gain_smoothing = self.gain_smoothing;

        // If sustain is at or past perceived silence, fill with zeros and signal end.
        if self.can_end_on_silent_sustain && sustain_cb >= PERCEIVED_CB_SILENCE {
            for s in buffer[filled_buffer..sample_count].iter_mut() {
                *s = 0.0;
            }
            return false;
        }

        let mut sample_time = self.sample_time;
        let mut current_gain = self.current_gain;
        let smooth = current_gain != gain_target;

        if filled_buffer < sample_count {
            self.attenuation_cb = sustain_cb;

            // Pre-compute the sustain gain (constant for the whole block).
            // Use f64 version to match TS: cbAttenuationToGain(sustainCb + centibelOffset)
            let sustain_gain = cb_attenuation_to_gain_f64(sustain_cb + centibel_offset) as f64;

            while filled_buffer < sample_count {
                if smooth {
                    current_gain += (gain_target - current_gain) * gain_smoothing;
                }

                // Emulate JS Float32Array *= semantics:
                // JS: buffer[i] *= currentGain * sustainGain  → buffer[i] * (currentGain * sustainGain)
                buffer[filled_buffer] = (buffer[filled_buffer] as f64 * (current_gain * sustain_gain)) as f32;
                sample_time += 1.0;
                filled_buffer += 1;
            }
        }

        self.sample_time = sample_time;
        self.current_gain = current_gain;
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

    const SAMPLE_RATE: f64 = 44_100.0;
    const EPS: f32 = 1e-5;
    const EPS64: f64 = 1e-5;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < EPS
    }

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
    /// Without this, the defaults (delayVolEnv = -12000, attackVolEnv = -12000) each
    /// give ~43 delay/attack samples, so attack_end > 0 and state stays at 0.
    /// i16::MIN = -32768 satisfies `timecents <= -32767` → timecents_to_seconds returns 0.
    fn gens_no_delay_no_attack() -> Vec<i16> {
        let g = default_gens();
        let g = gens_with(g, gt::DELAY_VOL_ENV, i16::MIN);
        let g = gens_with(g, gt::ATTACK_VOL_ENV, i16::MIN);
        g
    }

    // -----------------------------------------------------------------------
    // new()
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_initial_attenuation_is_cb_silence() {
        let env = VolumeEnvelope::new(SAMPLE_RATE);
        assert!(approx_eq64(env.attenuation_cb, 960.0));
    }

    #[test]
    fn test_new_state_is_zero() {
        let env = VolumeEnvelope::new(SAMPLE_RATE);
        assert_eq!(env.state, 0);
    }

    #[test]
    fn test_new_entered_release_is_false() {
        let env = VolumeEnvelope::new(SAMPLE_RATE);
        assert!(!env.entered_release);
    }

    #[test]
    fn test_new_current_gain_is_zero() {
        let env = VolumeEnvelope::new(SAMPLE_RATE);
        assert!(approx_eq64(env.current_gain, 0.0));
    }

    #[test]
    fn test_new_sample_rate_stored() {
        let env = VolumeEnvelope::new(SAMPLE_RATE);
        assert!(approx_eq64(env.sample_rate, SAMPLE_RATE));
    }

    #[test]
    fn test_new_gain_smoothing_at_44100() {
        let env = VolumeEnvelope::new(44_100.0);
        // gainSmoothing = GAIN_SMOOTHING_FACTOR * (44100 / sampleRate) = 0.01 * 1 = 0.01
        assert!(approx_eq64(env.gain_smoothing, 0.01));
    }

    #[test]
    fn test_new_gain_smoothing_scaled_for_different_rate() {
        let env = VolumeEnvelope::new(22_050.0);
        // gainSmoothing = 0.01 * (44100 / 22050) = 0.02
        assert!(approx_eq64(env.gain_smoothing, 0.02));
    }

    // -----------------------------------------------------------------------
    // init()
    // -----------------------------------------------------------------------

    #[test]
    fn test_init_resets_state_to_zero() {
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.state = 3;
        env.init(&default_gens(), 60);
        assert_eq!(env.state, 0);
    }

    #[test]
    fn test_init_resets_entered_release() {
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.entered_release = true;
        env.init(&default_gens(), 60);
        assert!(!env.entered_release);
    }

    #[test]
    fn test_init_attack_end_zero_jumps_to_hold() {
        // When both delay and attack resolve to 0 samples, attack_end == 0
        // and init() must jump directly to state 2 (hold/peak).
        // Note: default delayVolEnv = -12000 gives ~43 delay samples, so we
        // must explicitly zero the delay generator too.
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.init(&gens_no_delay_no_attack(), 60);
        assert_eq!(env.state, 2);
    }

    #[test]
    fn test_init_with_attack_stays_in_delay_state() {
        // Set attackVolEnv to 0 timecents (1 second ≈ 44100 samples); delay defaults to
        // -12000 timecents (~43 samples). Since attack_end = 43 + 44100 > 0 → state stays 0.
        let gens = gens_with(default_gens(), gt::ATTACK_VOL_ENV, 0);
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.init(&gens, 60);
        assert_eq!(env.state, 0);
    }

    #[test]
    fn test_init_attack_end_with_nonzero_attack_timecents() {
        // attackVolEnv = 0 timecents → 44100 samples; delay zeroed → delay_end = 0.
        // attack_end = 0 (delay) + 44100 (attack) = 44100.
        let g = gens_with(gens_no_delay_no_attack(), gt::ATTACK_VOL_ENV, 0);
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.init(&g, 60);
        assert_eq!(env.attack_end, 44_100.0);
    }

    #[test]
    fn test_init_sustain_cb_clamped_to_cb_silence() {
        // sustainVolEnv clamped to min(CB_SILENCE=960, value).
        // Default sustainVolEnv = 0.
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.init(&default_gens(), 60);
        assert!(env.sustain_cb <= 960.0);
    }

    #[test]
    fn test_init_sustain_cb_zero_from_default() {
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.init(&default_gens(), 60);
        assert!(approx_eq64(env.sustain_cb, 0.0));
    }

    // -----------------------------------------------------------------------
    // process() – delay stage
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_delay_fills_zeros() {
        // Use a short delay: delayVolEnv = 0 timecents → 1 second delay.
        let gens = gens_with(default_gens(), gt::DELAY_VOL_ENV, 0);
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.init(&gens, 60);
        // state should be 0 (no attack set).
        assert_eq!(env.state, 0);

        let mut buf = vec![1.0_f32; 128];
        env.process(128, &mut buf, 1.0, 0.0);

        // All samples must be silenced during the delay phase.
        for &s in &buf {
            assert!(approx_eq(s, 0.0));
        }
    }

    #[test]
    fn test_process_delay_returns_true_while_active() {
        let gens = gens_with(default_gens(), gt::DELAY_VOL_ENV, 0);
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.init(&gens, 60);

        let mut buf = vec![1.0_f32; 128];
        let active = env.process(128, &mut buf, 1.0, 0.0);
        assert!(active);
    }

    // -----------------------------------------------------------------------
    // process() – attack stage
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_attack_ramps_from_zero() {
        // No delay, 1-second attack.
        let gens = gens_with(default_gens(), gt::ATTACK_VOL_ENV, 0);
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.init(&gens, 60);

        // state 0; fill the delay (0 samples), then enter attack.
        let mut buf = vec![1.0_f32; 128];
        env.process(128, &mut buf, 1.0, 0.0);

        // Attack ramps linearly: first sample is ≈ 0, last of 128 is small.
        // All samples should be in [0, 1].
        for &s in &buf {
            assert!(s >= 0.0 && s <= 1.0, "attack sample out of [0,1]: {s}");
        }
    }

    #[test]
    fn test_process_attack_monotonically_increasing() {
        let gens = gens_with(default_gens(), gt::ATTACK_VOL_ENV, 0);
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.init(&gens, 60);

        // use current_gain = gain_target to avoid gain smoothing
        let mut buf = vec![1.0_f32; 1024];
        env.process(1024, &mut buf, 1.0, 0.0);

        for i in 1..1024 {
            assert!(
                buf[i] >= buf[i - 1] - 1e-6,
                "attack not monotonic at i={i}: {} < {}",
                buf[i],
                buf[i - 1]
            );
        }
    }

    // -----------------------------------------------------------------------
    // process() – hold stage (state 2)
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_hold_keeps_unity_gain() {
        // Zero delay + zero attack so attack_end == 0 → init() sets state = 2.
        // holdVolEnv = 0 timecents → 1 second hold (44100 samples).
        let gens = gens_with(gens_no_delay_no_attack(), gt::HOLD_VOL_ENV, 0);
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.init(&gens, 60);
        // state = 2 (hold/peak). attack_end = 0, hold_end = 44100.
        assert_eq!(env.state, 2);

        // Gain target = 1.0, current_gain starts at 0 → needs smoothing.
        // Let's pre-set current_gain to 1.0 to avoid smoothing effect.
        env.current_gain = 1.0;

        let mut buf = vec![0.5_f32; 128];
        env.process(128, &mut buf, 1.0, 0.0);

        // Each sample should be multiplied by 1.0 * cb_attenuation_to_gain(0) = 1.0 * 1.0 = 1.0.
        // Wait, it is: buf[i] *= current_gain * gain_offset
        // current_gain = 1.0, gain_offset = cb_attenuation_to_gain(0) = 1.0.
        // So buf[i] = 0.5 * 1.0 * 1.0 = 0.5.
        for &s in &buf {
            assert!(approx_eq(s, 0.5), "hold sample not 0.5: {s}");
        }
    }

    // -----------------------------------------------------------------------
    // process() – decay stage
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_decay_ends_at_sustain_level() {
        // No delay, no attack, hold=0 (immediate hold then decay).
        // decayVolEnv = 0 timecents → 1 sec = 44100 samples.
        // sustainVolEnv = 480 (half of CB_SILENCE = 960).
        let gens = {
            let mut g = default_gens();
            g[gt::DECAY_VOL_ENV as usize] = 0; // 1 second decay
            g[gt::SUSTAIN_VOL_ENV as usize] = 480; // half silence
            g
        };
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.init(&gens, 60);
        env.current_gain = 1.0;

        // After a full decay (44100 samples), the attenuation should be at sustainCb.
        let n = 44_100;
        let mut buf = vec![1.0_f32; n];
        env.process(n, &mut buf, 1.0, 0.0);

        // The last sample should be near cb_attenuation_to_gain(480) ≈ 0.1585.
        let expected = cb_attenuation_to_gain(480);
        let last = *buf.last().unwrap();
        assert!(
            (last - expected).abs() < 0.01,
            "decay end sample {last} not near expected sustain gain {expected}"
        );
    }

    // -----------------------------------------------------------------------
    // process() – sustain stage
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_sustain_is_constant() {
        // sustainVolEnv = 200 → cb_attenuation_to_gain(200) ≈ 0.1.
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.state = 4; // Force sustain stage.
        env.sustain_cb = 200.0;
        env.current_gain = 1.0;

        let expected_gain = cb_attenuation_to_gain(200);
        let mut buf = vec![1.0_f32; 64];
        env.process(64, &mut buf, 1.0, 0.0);

        for &s in &buf {
            assert!(
                (s - expected_gain).abs() < 1e-4,
                "sustain sample {s} not near expected {expected_gain}"
            );
        }
    }

    #[test]
    fn test_process_sustain_returns_true() {
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.state = 4;
        env.sustain_cb = 0.0;
        env.current_gain = 1.0;
        let mut buf = vec![1.0_f32; 32];
        let active = env.process(32, &mut buf, 1.0, 0.0);
        assert!(active);
    }

    #[test]
    fn test_process_sustain_silent_can_end() {
        // If can_end_on_silent_sustain and sustain_cb >= PERCEIVED_CB_SILENCE,
        // the voice should end and the buffer should be zeroed.
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.state = 4;
        env.sustain_cb = 960.0; // CB_SILENCE
        env.can_end_on_silent_sustain = true;
        env.current_gain = 1.0;

        let mut buf = vec![1.0_f32; 32];
        let active = env.process(32, &mut buf, 1.0, 0.0);

        assert!(!active);
        for &s in &buf {
            assert!(approx_eq(s, 0.0));
        }
    }

    // -----------------------------------------------------------------------
    // process() – release stage
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_release_attenuates_to_silence() {
        // Set up a release from full volume (releaseStartCb = 0).
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.state = 4;
        env.sustain_cb = 0.0;
        env.current_gain = 1.0;
        env.entered_release = true;
        env.release_start_cb = 0.0;
        env.release_start_time_samples = 0.0;
        env.sample_time = 0.0;
        // Release duration: 1 second = 44100 samples.
        env.release_duration = 44_100.0;

        // Process the full release.
        let n = 44_100;
        let mut buf = vec![1.0_f32; n];
        let active = env.process(n, &mut buf, 1.0, 0.0);

        // At the end of the release the attenuation_cb should be CB_SILENCE.
        assert!(
            !active || env.attenuation_cb >= PERCEIVED_CB_SILENCE,
            "expected voice inactive or perceived silence, attenuation_cb={}",
            env.attenuation_cb
        );

        // First sample should be near unity gain (release just started).
        let first = buf[0];
        assert!(first > 0.9, "first release sample too attenuated: {first}");
    }

    #[test]
    fn test_process_release_returns_false_at_silence() {
        // release_start_cb = 0, release_duration = 128 samples.
        // After processing all 128 samples, the final elapsed_release = 127:
        //   attenuation_cb = (127/128) * 960 ≈ 952.5 > PERCEIVED_CB_SILENCE (900)
        // → process() returns false.
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.entered_release = true;
        env.release_start_cb = 0.0;
        env.release_start_time_samples = 0.0;
        env.sample_time = 0.0;
        env.current_gain = 1.0;
        env.release_duration = 128.0;

        let mut buf = vec![1.0_f32; 128];
        let active = env.process(128, &mut buf, 1.0, 0.0);

        assert!(!active);
    }

    // -----------------------------------------------------------------------
    // start_release()
    // -----------------------------------------------------------------------

    #[test]
    fn test_start_release_sets_entered_release() {
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.state = 4;
        env.sustain_cb = 0.0;
        env.init(&default_gens(), 60);

        let gens = gens_with(default_gens(), gt::RELEASE_VOL_ENV, 0);
        env.start_release(&gens, 60, 0);
        assert!(env.entered_release);
    }

    #[test]
    fn test_start_release_immediate_deactivation_when_silent() {
        // If release_start_cb >= PERCEIVED_CB_SILENCE (900), should return true.
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        // State 0 (delay): release_start_cb will be CB_SILENCE (960 >= 900).
        env.state = 0;
        env.sustain_cb = 0.0;
        env.sample_time = 0.0;

        let gens = gens_with(default_gens(), gt::RELEASE_VOL_ENV, 0);
        let deactivate = env.start_release(&gens, 60, 0);
        assert!(deactivate);
    }

    #[test]
    fn test_start_release_from_hold_stage_not_immediately_silent() {
        // State 2 (hold): release_start_cb = 0, which is well below PERCEIVED_CB_SILENCE.
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
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
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.state = 2;
        env.sustain_cb = 0.0;
        env.sample_time = 0.0;
        env.decay_end = 0.0;
        env.decay_duration = 0.0;

        // Override release to -7200 (minimum allowed).
        let gens = default_gens();
        env.start_release(&gens, 60, -2320); // exclusive class override
        // Duration should be based on -2320 (clamped to max(-7200, -2320) = -2320).
        // timecents_to_seconds(-2320) ~ 2^(-2320/1200) ~ 0.0133 seconds.
        // 0.0133 * 44100 ~ 587 samples; * releaseFraction (1.0).
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
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.state = 2;
        env.sustain_cb = 0.0;
        env.sample_time = 0.0;
        env.decay_end = 0.0;
        env.decay_duration = 0.0;

        // First release.
        let gens = gens_with(default_gens(), gt::RELEASE_VOL_ENV, 0);
        env.start_release(&gens, 60, 0);
        assert!(env.entered_release);

        // Simulate some processing: advance attenuation.
        env.attenuation_cb = 200.0;

        // Second release (exclusive class scenario): must use current attenuation.
        env.start_release(&gens, 60, 0);
        assert!(approx_eq64(env.release_start_cb, 200.0));
    }

    // -----------------------------------------------------------------------
    // centibel_offset
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_centibel_offset_affects_output() {
        // Sustain at 0 cB (full volume). Apply a 200 cB offset.
        let mut env_no_offset = VolumeEnvelope::new(SAMPLE_RATE);
        env_no_offset.state = 4;
        env_no_offset.sustain_cb = 0.0;
        env_no_offset.current_gain = 1.0;

        let mut env_with_offset = VolumeEnvelope::new(SAMPLE_RATE);
        env_with_offset.state = 4;
        env_with_offset.sustain_cb = 0.0;
        env_with_offset.current_gain = 1.0;

        let mut buf_no = vec![1.0_f32; 8];
        let mut buf_with = vec![1.0_f32; 8];

        env_no_offset.process(8, &mut buf_no, 1.0, 0.0);
        env_with_offset.process(8, &mut buf_with, 1.0, 200.0);

        // 200 cB offset reduces gain by a factor of 0.1.
        let all_same = buf_no
            .iter()
            .zip(buf_with.iter())
            .all(|(a, b)| approx_eq(*a, *b));
        assert!(!all_same, "centibel_offset must affect output");
    }

    // -----------------------------------------------------------------------
    // gain_target smoothing
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_gain_smoothing_applied() {
        // current_gain starts at 0, target is 1.0.
        // With smoothing, the output should be less than without.
        let mut env = VolumeEnvelope::new(SAMPLE_RATE);
        env.state = 4;
        env.sustain_cb = 0.0;
        env.current_gain = 0.0; // starts at 0

        let mut buf = vec![1.0_f32; 32];
        env.process(32, &mut buf, 1.0, 0.0);

        // First sample is multiplied by current_gain which starts near 0.
        assert!(
            buf[0] < 0.1,
            "first smoothed sample should be near 0, got {}",
            buf[0]
        );
        // Last sample should be closer to 1 after smoothing.
        assert!(buf[31] > buf[0], "smoothing should increase gain over time");
    }
}
