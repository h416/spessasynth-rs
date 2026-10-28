//! Applies a lowpass filter to a voice.
//! Ported from: src/synthesizer/audio_engine/voice/lowpass_filter.ts
//!
//! A lot of the tricks here come from FluidSynth.
//! They are the real smart guys.
//! Shoutout to them!
//! <https://github.com/FluidSynth/fluidsynth>
//!
//! The filtering loop itself is inlined in `render_voice` (as in the TS), so this file only
//! holds the filter state and the coefficient calculation.
//!
//! Coefficient cache: TS 4.4.0 keys it by sample rate, then resonance, then cutoff cents
//! (chunked typed arrays). Up to 4.3.22 the key was `resonanceCb + cutoffCents * 961`,
//! which collides once the resonance reaches 961 cB. The coefficients are a pure function
//! of (sample rate, resonance, cutoff), so any collision-free cache is observably the same;
//! Rust keys a map by the full triple.
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::synthesizer::audio_engine::voice::unit_converter::{
    abs_cents_to_hz, cb_attenuation_to_gain, cb_attenuation_to_gain_f64,
};

/// Lowpass filter frequency smoothing factor.
/// Lowered from 0.1 to 0.03
/// Equivalent to: FILTER_SMOOTHING_FACTOR
pub const FILTER_SMOOTHING_FACTOR: f64 = 0.03;

/// The cache covers cutoffs [0, 16384) cents, anything outside is computed directly.
/// Equivalent to: CACHE_MAX_CENTS
const CACHE_MAX_CENTS: i32 = 16_384;

/// Cached biquad filter coefficients: [a0, a1, a2, a3, a4].
type Coefficients = [f64; 5];

/// Key: (sample rate bits, resonanceCb, cutoff cents).
type CacheKey = (u64, i16, i32);

/// Global coefficient caches, shared between all synthesizer instances.
/// Equivalent to: LowpassFilter.caches
static CACHES: LazyLock<Mutex<HashMap<CacheKey, Coefficients>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Biquad lowpass filter applied per voice.
/// Equivalent to: LowpassFilter class
pub struct LowpassFilter {
    /// For smoothing the filter cutoff frequency.
    /// This is per filter, as it depends on the sample rate.
    /// Equivalent to: smoothingConstant
    pub smoothing_constant: f64,
    /// Resonance in centibels.
    /// Equivalent to: resonanceCb
    pub resonance_cb: i16,
    /// Current cutoff frequency in absolute cents.
    /// Equivalent to: currentInitialFc
    pub current_initial_fc: f64,
    /// Filter coefficient 1. Equivalent to: a0
    pub a0: f64,
    /// Filter coefficient 2. Equivalent to: a1
    pub a1: f64,
    /// Filter coefficient 3. Equivalent to: a2
    pub a2: f64,
    /// Filter coefficient 4. Equivalent to: a3
    pub a3: f64,
    /// Filter coefficient 5. Equivalent to: a4
    pub a4: f64,
    /// Input history 1. Equivalent to: x1
    pub x1: f64,
    /// Input history 2. Equivalent to: x2
    pub x2: f64,
    /// Output history 1. Equivalent to: y1
    pub y1: f64,
    /// Output history 2. Equivalent to: y2
    pub y2: f64,
    /// For tracking the last cutoff frequency to avoid recalculating coefficients.
    /// Equivalent to: lastTargetCutoff
    pub last_target_cutoff: f64,
    /// Used for tracking if the filter has been initialized.
    /// Equivalent to: initialized
    pub initialized: bool,
    /// Filter's sample rate in Hz.
    /// Equivalent to: sampleRate
    sample_rate: f64,
    /// Maximum cutoff frequency (= sampleRate * 0.45).
    /// Equivalent to: maxCutoff
    max_cutoff: f64,
}

impl LowpassFilter {
    /// Initializes a new instance of the filter.
    /// Equivalent to: constructor(sampleRate)
    pub fn new(sample_rate: f64) -> Self {
        Self {
            smoothing_constant: FILTER_SMOOTHING_FACTOR * (44_100.0 / sample_rate),
            resonance_cb: 0,
            current_initial_fc: 13_500.0,
            a0: 0.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            a4: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
            last_target_cutoff: f64::INFINITY,
            initialized: false,
            sample_rate,
            max_cutoff: sample_rate * 0.45,
        }
    }

    /// Pre-populates the shared coefficient cache for a sample rate.
    /// Precomputes selected cutoffs for 0q (most common).
    /// Equivalent to: LowpassFilter.initCache(sampleRate)
    pub fn init_cache(sample_rate: f64) {
        let mut dummy = LowpassFilter::new(sample_rate);
        dummy.resonance_cb = 0;
        // 8000 is roughly 830 Hz and 13,500 is max
        for i in 8000..13_500_i32 {
            dummy.current_initial_fc = i as f64;
            dummy.calculate_coefficients(i as f64);
        }
    }

    /// Equivalent to: init()
    pub fn init(&mut self) {
        self.last_target_cutoff = f64::INFINITY;
        self.resonance_cb = 0;
        self.current_initial_fc = 13_500.0;
        self.a0 = 0.0;
        self.a1 = 0.0;
        self.a2 = 0.0;
        self.a3 = 0.0;
        self.a4 = 0.0;
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
        self.initialized = false;
    }

    /// Calculates the filter coefficients based on the current resonance and cutoff frequency.
    /// Equivalent to: calculateCoefficients(cutoffCents)
    pub fn calculate_coefficients(&mut self, cutoff_cents: f64) {
        let cutoff_cents = cutoff_cents as i32; // Math.floor
        if !(0..CACHE_MAX_CENTS).contains(&cutoff_cents) {
            // Outside the cache range, compute directly
            self.compute_coefficients(cutoff_cents);
            return;
        }

        let key = (self.sample_rate.to_bits(), self.resonance_cb, cutoff_cents);
        let mut cache = CACHES.lock().unwrap();
        if let Some(c) = cache.get(&key) {
            [self.a0, self.a1, self.a2, self.a3, self.a4] = *c;
            return;
        }
        // Compute and cache the coefficients
        self.compute_coefficients(cutoff_cents);
        cache.insert(key, [self.a0, self.a1, self.a2, self.a3, self.a4]);
    }

    /// Calculates the filter coefficients based on the current resonance and cutoff frequency, without caching.
    /// Equivalent to: computeCoefficients(cutoffCents)
    fn compute_coefficients(&mut self, cutoff_cents: i32) {
        let q_cb = self.resonance_cb as f64;
        let mut cutoff_hz = abs_cents_to_hz(cutoff_cents);

        // Fix cutoff on low sample rates
        cutoff_hz = cutoff_hz.min(self.max_cutoff);

        // The coefficient calculation code was originally ported from meltysynth by sinshu.
        // Turn resonance to gain, -3.01 so it gives a non-resonant peak
        // -1 because it's attenuation, and we don't want attenuation
        let resonance_gain = cb_attenuation_to_gain_f64(-(q_cb - 3.01));

        // The sf spec asks for a reduction in gain based on the Q value.
        // Note that we calculate it again,
        // Without the 3.01-peak offset as it only applies to the coefficients, not the gain.
        let q_gain = 1.0 / cb_attenuation_to_gain_f64(-q_cb).sqrt();

        // Note: no sin or cos tables are used here as the coefficients are cached
        let w = (2.0 * std::f64::consts::PI * cutoff_hz) / self.sample_rate;
        let cosw = w.cos();
        let alpha = w.sin() / (2.0 * resonance_gain);

        let b1 = (1.0 - cosw) * q_gain;
        let b0 = b1 / 2.0;
        let b2 = b0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cosw;
        let a2 = 1.0 - alpha;

        self.a0 = b0 / a0;
        self.a1 = b1 / a0;
        self.a2 = b2 / a0;
        self.a3 = a1 / a0;
        self.a4 = a2 / a0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coeffs(f: &LowpassFilter) -> [f64; 5] {
        [f.a0, f.a1, f.a2, f.a3, f.a4]
    }

    #[test]
    fn test_new_defaults() {
        let f = LowpassFilter::new(44_100.0);
        assert_eq!(f.max_cutoff, 44_100.0 * 0.45);
        assert!(!f.initialized);
        assert_eq!(f.current_initial_fc, 13_500.0);
        assert_eq!(f.resonance_cb, 0);
        assert!(f.last_target_cutoff.is_infinite());
    }

    #[test]
    fn test_smoothing_constant_is_per_sample_rate() {
        assert_eq!(LowpassFilter::new(44_100.0).smoothing_constant, FILTER_SMOOTHING_FACTOR);
        assert_eq!(
            LowpassFilter::new(48_000.0).smoothing_constant,
            FILTER_SMOOTHING_FACTOR * (44_100.0 / 48_000.0)
        );
    }

    #[test]
    fn test_init_resets_state() {
        let mut f = LowpassFilter::new(44_100.0);
        f.initialized = true;
        f.resonance_cb = 100;
        f.current_initial_fc = 5000.0;
        f.last_target_cutoff = 5000.0;
        f.x1 = 1.0;
        f.y2 = 1.0;
        f.init();
        assert!(!f.initialized);
        assert_eq!(f.resonance_cb, 0);
        assert_eq!(f.current_initial_fc, 13_500.0);
        assert!(f.last_target_cutoff.is_infinite());
        assert_eq!((f.x1, f.x2, f.y1, f.y2), (0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn test_lowpass_coefficient_shape() {
        let mut f = LowpassFilter::new(44_100.0);
        f.calculate_coefficients(8000.0);
        assert!(f.a0 > 0.0);
        assert_eq!(f.a0, f.a2);
        assert!((f.a1 - 2.0 * f.a0).abs() < 1e-15);
    }

    #[test]
    fn test_cached_equals_computed() {
        let mut cached = LowpassFilter::new(44_100.0);
        cached.resonance_cb = 120;
        cached.calculate_coefficients(9000.0);
        cached.calculate_coefficients(9000.0);
        let mut direct = LowpassFilter::new(44_100.0);
        direct.resonance_cb = 120;
        direct.compute_coefficients(9000);
        assert_eq!(coeffs(&cached), coeffs(&direct));
    }

    #[test]
    fn test_no_collision_between_resonances() {
        // The 4.3.22 key `q + fc * 961` mapped (q=961, fc=0) and (q=0, fc=1) together.
        let mut a = LowpassFilter::new(44_100.0);
        a.resonance_cb = 961;
        a.calculate_coefficients(0.0);
        let mut b = LowpassFilter::new(44_100.0);
        b.resonance_cb = 0;
        b.calculate_coefficients(1.0);
        assert_ne!(coeffs(&a), coeffs(&b));
    }

    #[test]
    fn test_cutoff_is_truncated() {
        let mut a = LowpassFilter::new(44_100.0);
        a.calculate_coefficients(9000.9);
        let mut b = LowpassFilter::new(44_100.0);
        b.calculate_coefficients(9000.0);
        assert_eq!(coeffs(&a), coeffs(&b));
    }

    #[test]
    fn test_high_cutoff_clamped_to_max() {
        // 16000 cents is well above 0.45 * 22050 Hz at 22050 Hz, both clamp to max_cutoff
        let mut a = LowpassFilter::new(22_050.0);
        a.calculate_coefficients(16_000.0);
        let mut b = LowpassFilter::new(22_050.0);
        b.calculate_coefficients(15_000.0);
        assert_eq!(coeffs(&a), coeffs(&b));
    }

    #[test]
    fn test_integer_gain_helper_still_matches() {
        // `cb_attenuation_to_gain(i32)` is a thin wrapper of the f64 version.
        assert_eq!(cb_attenuation_to_gain(-120), cb_attenuation_to_gain_f64(-120.0));
    }
}
