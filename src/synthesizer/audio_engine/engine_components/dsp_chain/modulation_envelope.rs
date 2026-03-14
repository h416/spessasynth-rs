/// modulation_envelope.rs
/// purpose: calculates the modulation envelope for the given voice
/// Ported from: src/synthesizer/audio_engine/engine_components/dsp_chain/modulation_envelope.ts
///
/// # Design differences (resolving circular dependency from TypeScript → Rust)
///
/// In the TypeScript version, `process(voice, currentTime)` / `startRelease(voice)` / `init(voice)`
/// received a `Voice` instance. However, since `Voice` owns `ModulationEnvelope` as a field,
/// this would create a circular dependency in Rust.
///
/// In the Rust version, instead of passing the `Voice` object directly, the caller extracts
/// the required values and passes them individually:
///   - `process(release_start_time, current_time)`
///   - `start_release(modulated_generators)`
///   - `init(modulated_generators, start_time, midi_note)`
use std::sync::LazyLock;

use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
use crate::soundbank::enums::modulator_curve_types;
use crate::synthesizer::audio_engine::engine_components::modulator_curves::get_modulator_curve_value;
use crate::synthesizer::audio_engine::engine_components::unit_converter::timecents_to_seconds;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Peak value of the modulation envelope (full level).
/// Equivalent to: MODENV_PEAK
const MODENV_PEAK: f64 = 1.0;

/// Size of the CONVEX_ATTACK lookup table.
const CONVEX_ATTACK_SIZE: usize = 1000;

// ---------------------------------------------------------------------------
// CONVEX_ATTACK lookup table
// Equivalent to: const CONVEX_ATTACK = new Float32Array(1000)
// ---------------------------------------------------------------------------

static CONVEX_ATTACK: LazyLock<Vec<f32>> = LazyLock::new(|| {
    (0..CONVEX_ATTACK_SIZE)
        .map(|i| {
            get_modulator_curve_value(
                0,
                modulator_curve_types::CONVEX,
                i as f64 / CONVEX_ATTACK_SIZE as f64,
            ) as f32
        })
        .collect()
});

// ---------------------------------------------------------------------------
// ModulationEnvelope
// ---------------------------------------------------------------------------

/// Calculates the modulation envelope for a voice.
/// Equivalent to: class ModulationEnvelope
pub struct ModulationEnvelope {
    /// Attack duration in seconds.
    /// Equivalent to: attackDuration
    attack_duration: f64,
    /// Decay duration in seconds.
    /// Equivalent to: decayDuration
    decay_duration: f64,
    /// Hold duration in seconds.
    /// Equivalent to: holdDuration
    hold_duration: f64,
    /// Release duration in seconds.
    /// Equivalent to: releaseDuration
    release_duration: f64,
    /// Sustain level, 0–1.
    /// Equivalent to: sustainLevel
    sustain_level: f64,
    /// Delay phase end time in seconds (absolute).
    /// Equivalent to: delayEnd
    delay_end: f64,
    /// Attack phase end time in seconds (absolute).
    /// Equivalent to: attackEnd
    attack_end: f64,
    /// Hold phase end time in seconds (absolute).
    /// Equivalent to: holdEnd
    hold_end: f64,
    /// Envelope level when the release phase began.
    /// Equivalent to: releaseStartLevel
    release_start_level: f64,
    /// The current modulation envelope value.
    /// Equivalent to: currentValue
    current_value: f64,
    /// Whether the envelope has entered the release phase.
    /// Equivalent to: enteredRelease
    entered_release: bool,
    /// Decay phase end time in seconds (absolute).
    /// Equivalent to: decayEnd
    decay_end: f64,
}

impl ModulationEnvelope {
    /// Converts timecents to seconds, clamping values ≤ -10114 to 0.
    /// This prevents clicks from extremely short envelope phases.
    /// Equivalent to: tc2Sec(timecents)
    fn tc2sec(timecents: i32) -> f64 {
        if timecents <= -10114 {
            return 0.0;
        }
        timecents_to_seconds(timecents) as f64
    }

    /// Creates a new ModulationEnvelope with all fields at zero / false.
    /// Equivalent to: new ModulationEnvelope()
    pub fn new() -> Self {
        Self {
            attack_duration: 0.0,
            decay_duration: 0.0,
            hold_duration: 0.0,
            release_duration: 0.0,
            sustain_level: 0.0,
            delay_end: 0.0,
            attack_end: 0.0,
            hold_end: 0.0,
            release_start_level: 0.0,
            current_value: 0.0,
            entered_release: false,
            decay_end: 0.0,
        }
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Calculates the current modulation envelope value.
    ///
    /// `release_start_time` – the absolute time (seconds) when the voice entered release.
    ///   Pass `voice.release_start_time`.
    /// `current_time` – the current absolute time in seconds.
    ///
    /// Returns the envelope value in the range `[0, 1]`.
    ///
    /// Equivalent to: process(voice: Voice, currentTime: number)
    pub fn process(&mut self, release_start_time: f64, current_time: f64) -> f64 {
        if self.entered_release {
            // If voice is still in delay, release_start_level == 0 → avoid divide-by-zero
            if self.release_start_level == 0.0 {
                return 0.0;
            }
            let elapsed = current_time - release_start_time;
            let remaining = if self.release_duration == 0.0 {
                0.0_f64
            } else {
                1.0 - elapsed / self.release_duration
            };
            return (remaining * self.release_start_level).max(0.0);
        }

        if current_time < self.delay_end {
            // Delay phase: silence
            self.current_value = 0.0;
        } else if current_time < self.attack_end {
            // Attack phase: convex curve
            let progress = if self.attack_duration == 0.0 {
                1.0_f64
            } else {
                1.0 - (self.attack_end - current_time) / self.attack_duration
            };
            // Equivalent to JS `~~(progress * 1000)` (truncate to int)
            let idx = (progress * CONVEX_ATTACK_SIZE as f64) as usize;
            let idx = idx.min(CONVEX_ATTACK_SIZE - 1);
            self.current_value = CONVEX_ATTACK[idx] as f64;
        } else if current_time < self.hold_end {
            // Hold phase: stay at peak
            self.current_value = MODENV_PEAK;
        } else if current_time < self.decay_end {
            // Decay phase: linear ramp from MODENV_PEAK to sustain_level
            let t = if self.decay_duration == 0.0 {
                1.0_f64
            } else {
                1.0 - (self.decay_end - current_time) / self.decay_duration
            };
            self.current_value = t * (self.sustain_level - MODENV_PEAK) + MODENV_PEAK;
        } else {
            // Sustain phase
            self.current_value = self.sustain_level;
        }
        self.current_value
    }

    /// Debug accessor for the current envelope value.
    pub fn debug_current_value(&self) -> f64 {
        self.current_value
    }

    /// Starts the release phase of the modulation envelope.
    ///
    /// `modulated_generators` – `voice.modulated_generators` slice.
    ///
    /// Equivalent to: startRelease(voice: Voice)
    pub fn start_release(&mut self, modulated_generators: &[i16]) {
        self.release_start_level = self.current_value;
        self.entered_release = true;

        // Min is set to -7200 to prevent lowpass clicks
        let release_tc = (modulated_generators[gt::RELEASE_MOD_ENV as usize] as i32).max(-7200);
        let release_time = Self::tc2sec(release_tc);

        // Release time is from full level to 0%; scale by the actual start level
        self.release_duration = release_time * self.release_start_level;
    }

    /// Initializes the modulation envelope for a new note-on event.
    ///
    /// `modulated_generators` – `voice.modulated_generators` slice.
    /// `start_time` – `voice.start_time` (absolute seconds).
    /// `midi_note` – `voice.midi_note` (0–127).
    ///
    /// Equivalent to: init(voice: Voice)
    pub fn init(&mut self, modulated_generators: &[i16], start_time: f64, midi_note: i16) {
        self.entered_release = false;
        self.sustain_level =
            1.0 - modulated_generators[gt::SUSTAIN_MOD_ENV as usize] as f64 / 1000.0;

        self.attack_duration =
            Self::tc2sec(modulated_generators[gt::ATTACK_MOD_ENV as usize] as i32);

        // Decay time with key excursion
        let decay_key_excursion_cents = (60 - midi_note as i32) as f64
            * modulated_generators[gt::KEY_NUM_TO_MOD_ENV_DECAY as usize] as f64;
        let decay_time = Self::tc2sec(
            (modulated_generators[gt::DECAY_MOD_ENV as usize] as f64 + decay_key_excursion_cents)
                as i32,
        );
        // Decay time is from 100% to 0%; scale to reach the actual sustain level
        self.decay_duration = decay_time * (1.0 - self.sustain_level);

        // Hold time with key excursion
        let hold_key_excursion_cents = (60 - midi_note as i32) as f64
            * modulated_generators[gt::KEY_NUM_TO_MOD_ENV_HOLD as usize] as f64;
        self.hold_duration = Self::tc2sec(
            (hold_key_excursion_cents + modulated_generators[gt::HOLD_MOD_ENV as usize] as f64)
                as i32,
        );

        // Compute absolute phase end times
        self.delay_end = start_time
            + Self::tc2sec(modulated_generators[gt::DELAY_MOD_ENV as usize] as i32);
        self.attack_end = self.delay_end + self.attack_duration;
        self.hold_end = self.attack_end + self.hold_duration;
        self.decay_end = self.hold_end + self.decay_duration;
    }
}

impl Default for ModulationEnvelope {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator_types::{
        DEFAULT_GENERATOR_VALUES, GENERATORS_AMOUNT, generator_types as gt,
    };

    const EPS: f64 = 1e-5;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    /// Returns a default generator slice (all SF2 defaults).
    fn default_gens() -> Vec<i16> {
        DEFAULT_GENERATOR_VALUES.to_vec()
    }

    /// Returns a generator slice with one value overridden.
    fn gens_with(mut g: Vec<i16>, index: i16, value: i16) -> Vec<i16> {
        g[index as usize] = value;
        g
    }

    /// Builds generators with very short (zero-duration) DAHDSR phases for testing.
    /// Uses i16::MIN to force timecents_to_seconds → 0.
    fn gens_instant() -> Vec<i16> {
        let g = default_gens();
        let g = gens_with(g, gt::DELAY_MOD_ENV, i16::MIN);
        let g = gens_with(g, gt::ATTACK_MOD_ENV, i16::MIN);
        let g = gens_with(g, gt::HOLD_MOD_ENV, i16::MIN);
        gens_with(g, gt::DECAY_MOD_ENV, i16::MIN)
    }

    // -----------------------------------------------------------------------
    // new() / Default
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_default_values() {
        let env = ModulationEnvelope::new();
        assert!(approx_eq(env.sustain_level, 0.0));
        assert!(approx_eq(env.current_value, 0.0));
        assert!(!env.entered_release);
        assert!((env.delay_end - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_default_trait_equals_new() {
        let a = ModulationEnvelope::new();
        let b = ModulationEnvelope::default();
        assert!(approx_eq(a.sustain_level, b.sustain_level));
        assert!(!a.entered_release && !b.entered_release);
    }

    // -----------------------------------------------------------------------
    // CONVEX_ATTACK table
    // -----------------------------------------------------------------------

    #[test]
    fn test_convex_attack_first_entry_is_zero() {
        // index 0 → value at i/1000 = 0 → convex(0) = 0
        assert!(approx_eq(CONVEX_ATTACK[0] as f64, 0.0));
    }

    #[test]
    fn test_convex_attack_ends_near_one() {
        // The CONVEX curve approaches 1.0 at the high end of the table.
        // index 999 → input = 999/1000 = 0.999 → CONVEX[(0.999 * 16384)] ≈ 1.0
        let last = CONVEX_ATTACK[CONVEX_ATTACK_SIZE - 1];
        assert!(
            (last - 1.0).abs() < 0.05,
            "last CONVEX_ATTACK entry should be near 1.0, got {last}"
        );
    }

    #[test]
    fn test_convex_attack_all_finite() {
        for (i, &v) in CONVEX_ATTACK.iter().enumerate() {
            assert!(v.is_finite(), "CONVEX_ATTACK[{i}] is not finite: {v}");
        }
    }

    #[test]
    fn test_convex_attack_size_is_1000() {
        assert_eq!(CONVEX_ATTACK.len(), CONVEX_ATTACK_SIZE);
    }

    // -----------------------------------------------------------------------
    // init()
    // -----------------------------------------------------------------------

    #[test]
    fn test_init_resets_entered_release() {
        let mut env = ModulationEnvelope::new();
        env.entered_release = true;
        env.init(&default_gens(), 0.0, 60);
        assert!(!env.entered_release);
    }

    #[test]
    fn test_init_sustain_level_from_generator() {
        // sustainModEnv = 500 → sustain_level = 1 - 500/1000 = 0.5
        let gens = gens_with(default_gens(), gt::SUSTAIN_MOD_ENV, 500);
        let mut env = ModulationEnvelope::new();
        env.init(&gens, 0.0, 60);
        assert!(
            (env.sustain_level - 0.5).abs() < 0.001,
            "sustain_level = {}",
            env.sustain_level
        );
    }

    #[test]
    fn test_init_sustain_level_default_is_one() {
        // Default sustainModEnv = 0 → sustain_level = 1 - 0/1000 = 1.0
        let mut env = ModulationEnvelope::new();
        env.init(&default_gens(), 0.0, 60);
        assert!(
            (env.sustain_level - 1.0).abs() < 0.001,
            "sustain_level = {}",
            env.sustain_level
        );
    }

    #[test]
    fn test_init_delay_end_includes_start_time() {
        // delayModEnv = i16::MIN → timecents_to_seconds → 0, so delay_end = start_time
        let gens = gens_with(default_gens(), gt::DELAY_MOD_ENV, i16::MIN);
        let mut env = ModulationEnvelope::new();
        let start_time = 2.5;
        env.init(&gens, start_time, 60);
        assert!((env.delay_end - start_time).abs() < 0.001);
    }

    #[test]
    fn test_init_attack_end_after_delay_end() {
        let mut env = ModulationEnvelope::new();
        env.init(&default_gens(), 0.0, 60);
        assert!(env.attack_end >= env.delay_end);
    }

    #[test]
    fn test_init_hold_end_after_attack_end() {
        let mut env = ModulationEnvelope::new();
        env.init(&default_gens(), 0.0, 60);
        assert!(env.hold_end >= env.attack_end);
    }

    #[test]
    fn test_init_decay_end_after_hold_end() {
        let mut env = ModulationEnvelope::new();
        env.init(&default_gens(), 0.0, 60);
        assert!(env.decay_end >= env.hold_end);
    }

    #[test]
    fn test_init_nonzero_attack_timecents_gives_positive_duration() {
        // attackModEnv = 0 timecents → 2^(0/1200) = 1.0 second
        let gens = gens_with(default_gens(), gt::ATTACK_MOD_ENV, 0);
        let mut env = ModulationEnvelope::new();
        env.init(&gens, 0.0, 60);
        assert!(env.attack_duration > 0.9 && env.attack_duration < 1.1);
    }

    // -----------------------------------------------------------------------
    // process() – delay phase
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_delay_returns_zero() {
        // Large delay: delayModEnv = 0 tc → 1-second delay.
        let gens = gens_with(default_gens(), gt::DELAY_MOD_ENV, 0);
        let mut env = ModulationEnvelope::new();
        env.init(&gens, 0.0, 60);
        // current_time = 0 < delay_end (≈1s) → delay phase
        let val = env.process(f64::INFINITY, 0.0);
        assert!(approx_eq(val, 0.0), "delay should be 0, got {val}");
    }

    // -----------------------------------------------------------------------
    // process() – hold phase
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_hold_returns_one() {
        // Instant DAHDSR, then set hold manually to verify
        let gens = gens_instant();
        let mut env = ModulationEnvelope::new();
        env.init(&gens, 0.0, 60);
        // After delay=0, attack=0, we're in hold phase immediately.
        // Set hold_end to future to stay in hold.
        env.hold_end = 100.0;
        let val = env.process(f64::INFINITY, 1.0);
        assert!(
            (val - MODENV_PEAK).abs() < EPS,
            "hold should be 1.0, got {val}"
        );
    }

    // -----------------------------------------------------------------------
    // process() – sustain phase
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_sustain_returns_sustain_level() {
        let gens = gens_with(gens_instant(), gt::SUSTAIN_MOD_ENV, 600);
        let mut env = ModulationEnvelope::new();
        env.init(&gens, 0.0, 60);
        // Past decay_end → sustain
        let val = env.process(f64::INFINITY, 10_000.0);
        let expected = 1.0 - 600.0 / 1000.0; // = 0.4
        assert!(
            (val - expected).abs() < 0.01,
            "sustain should be ~{expected}, got {val}"
        );
    }

    // -----------------------------------------------------------------------
    // process() – decay phase
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_decay_starts_at_one_ends_at_sustain() {
        // 1-second decay, sustain = 0.5
        let gens = {
            let g = gens_instant();
            let g = gens_with(g, gt::DECAY_MOD_ENV, 0); // 1 second decay
            gens_with(g, gt::SUSTAIN_MOD_ENV, 500) // sustain = 0.5
        };
        let mut env = ModulationEnvelope::new();
        env.init(&gens, 0.0, 60);
        // At decay start (hold_end) → should be near MODENV_PEAK
        let val_start = env.process(f64::INFINITY, env.hold_end);
        // At decay end → should be near sustain_level
        let val_end = env.process(f64::INFINITY, env.decay_end);
        assert!(
            val_start >= 0.9,
            "decay start should be near 1, got {val_start}"
        );
        assert!(
            (val_end - 0.5).abs() < 0.05,
            "decay end should be near 0.5, got {val_end}"
        );
    }

    // -----------------------------------------------------------------------
    // process() – attack phase
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_attack_uses_convex_curve() {
        // 1-second attack, no delay
        let gens = {
            let g = gens_with(default_gens(), gt::DELAY_MOD_ENV, i16::MIN);
            gens_with(g, gt::ATTACK_MOD_ENV, 0) // 1 second
        };
        let mut env = ModulationEnvelope::new();
        env.init(&gens, 0.0, 60);
        // At the midpoint of the attack → use CONVEX_ATTACK[500]
        let midpoint = env.attack_end / 2.0;
        let val = env.process(f64::INFINITY, midpoint);
        // Should be between 0 and 1
        assert!(val >= 0.0 && val <= 1.0, "attack value out of [0,1]: {val}");
    }

    // -----------------------------------------------------------------------
    // start_release()
    // -----------------------------------------------------------------------

    #[test]
    fn test_start_release_sets_entered_release() {
        let mut env = ModulationEnvelope::new();
        env.init(&default_gens(), 0.0, 60);
        env.start_release(&default_gens());
        assert!(env.entered_release);
    }

    #[test]
    fn test_start_release_captures_current_value_as_start_level() {
        let mut env = ModulationEnvelope::new();
        env.current_value = 0.75;
        env.start_release(&default_gens());
        assert!(
            (env.release_start_level - 0.75).abs() < EPS,
            "release_start_level should be 0.75, got {}",
            env.release_start_level
        );
    }

    #[test]
    fn test_start_release_duration_zero_for_zero_current_value() {
        // If current_value = 0 (still in delay), release_duration should be 0
        let mut env = ModulationEnvelope::new();
        env.current_value = 0.0;
        env.start_release(&default_gens());
        assert!(approx_eq(env.release_duration, 0.0));
    }

    #[test]
    fn test_start_release_uses_release_mod_env_generator() {
        // releaseModEnv = 0 tc → 1 second; release_duration = 1.0 * start_level
        let gens = gens_with(default_gens(), gt::RELEASE_MOD_ENV, 0);
        let mut env = ModulationEnvelope::new();
        env.current_value = 1.0;
        env.start_release(&gens);
        // release_duration ≈ 1.0 second
        assert!(
            (env.release_duration - 1.0).abs() < 0.05,
            "release_duration should be ~1s, got {}",
            env.release_duration
        );
    }

    #[test]
    fn test_start_release_clamps_to_minus_7200() {
        // releaseModEnv = -32768 (i16::MIN) → clamped to -7200 timecents
        let gens = gens_with(default_gens(), gt::RELEASE_MOD_ENV, i16::MIN);
        let mut env = ModulationEnvelope::new();
        env.current_value = 1.0;
        env.start_release(&gens);
        // timecents_to_seconds(-7200) = 2^(-7200/1200) = 2^(-6) ≈ 0.015625
        let expected = 2f64.powf(-7200.0 / 1200.0);
        assert!(
            (env.release_duration - expected).abs() < 0.001,
            "release_duration should be ≈{expected}, got {}",
            env.release_duration
        );
    }

    // -----------------------------------------------------------------------
    // process() – release phase
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_release_returns_zero_when_start_level_is_zero() {
        let mut env = ModulationEnvelope::new();
        env.entered_release = true;
        env.release_start_level = 0.0;
        let val = env.process(0.0, 1.0);
        assert!(approx_eq(val, 0.0));
    }

    #[test]
    fn test_process_release_decreases_over_time() {
        let mut env = ModulationEnvelope::new();
        env.entered_release = true;
        env.release_start_level = 1.0;
        env.release_duration = 2.0; // 2-second release

        let val_early = env.process(0.0, 0.5); // 25% through
        let val_late = env.process(0.0, 1.5); // 75% through

        assert!(
            val_early > val_late,
            "release should decrease: early={val_early} late={val_late}"
        );
    }

    #[test]
    fn test_process_release_clamps_to_zero() {
        let mut env = ModulationEnvelope::new();
        env.entered_release = true;
        env.release_start_level = 1.0;
        env.release_duration = 1.0;

        // Well past release end
        let val = env.process(0.0, 5.0);
        assert!(val >= 0.0, "release should not go below 0, got {val}");
        assert!(approx_eq(val, 0.0), "fully released should be 0, got {val}");
    }

    #[test]
    fn test_process_release_at_start_returns_full_level() {
        let mut env = ModulationEnvelope::new();
        env.entered_release = true;
        env.release_start_level = 0.8;
        env.release_duration = 2.0;

        // release_start_time = 1.0, current_time = 1.0 → elapsed = 0 → full level
        let val = env.process(1.0, 1.0);
        assert!(
            (val - 0.8).abs() < EPS,
            "at release start should be 0.8, got {val}"
        );
    }
}
