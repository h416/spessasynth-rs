/// lowpass_filter.rs
/// purpose: applies a low pass filter to a voice
/// Ported from: src/synthesizer/audio_engine/engine_components/dsp_chain/lowpass_filter.ts
///
/// Note: a lot of tricks come from fluidsynth.
/// They are the real smart guys.
/// Shoutout to them!
/// https://github.com/FluidSynth/fluidsynth
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
use crate::synthesizer::audio_engine::engine_components::unit_converter::{
    abs_cents_to_hz, cb_attenuation_to_gain,
};

/// Lowpass filter frequency smoothing factor.
/// Equivalent to: FILTER_SMOOTHING_FACTOR
pub const FILTER_SMOOTHING_FACTOR: f64 = 0.03;

// ---------------------------------------------------------------------------
// Global static state (equivalent to TypeScript static class fields)
// ---------------------------------------------------------------------------

/// Global coefficient cache.  Key = resonanceCb + cutoffCentsI32 * 961.
/// Equivalent to: LowpassFilter.cachedCoefficients
static CACHED_COEFFICIENTS: LazyLock<Mutex<HashMap<i32, CachedCoefficient>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Global smoothing constant, stored as f64 bits in an AtomicU64.
/// Initial value 1.0.
/// Equivalent to: LowpassFilter.smoothingConstant
static SMOOTHING_CONSTANT: AtomicU64 = AtomicU64::new(0x3FF0_0000_0000_0000);

fn get_smoothing_constant() -> f64 {
    f64::from_bits(SMOOTHING_CONSTANT.load(Ordering::Relaxed))
}

fn set_smoothing_constant(v: f64) {
    SMOOTHING_CONSTANT.store(v.to_bits(), Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// CachedCoefficient
// ---------------------------------------------------------------------------

/// Cached biquad filter coefficients.
/// Equivalent to: CachedCoefficient interface
#[derive(Clone, Copy)]
struct CachedCoefficient {
    a0: f64,
    a1: f64,
    a2: f64,
    a3: f64,
    a4: f64,
}

// ---------------------------------------------------------------------------
// LowpassFilter
// ---------------------------------------------------------------------------

/// Biquad lowpass filter applied per voice.
/// Equivalent to: LowpassFilter class
pub struct LowpassFilter {
    /// Resonance in centibels (from initialFilterQ generator).
    /// Equivalent to: resonanceCb
    pub resonance_cb: i16,
    /// Current (smoothed) cutoff frequency in absolute cents.
    /// Equivalent to: currentInitialFc
    /// f64 to match TS (JS number is f64).
    pub current_initial_fc: f64,
    /// Filter coefficient 1. Equivalent to: a0
    a0: f64,
    /// Filter coefficient 2. Equivalent to: a1
    a1: f64,
    /// Filter coefficient 3. Equivalent to: a2
    a2: f64,
    /// Filter coefficient 4. Equivalent to: a3
    a3: f64,
    /// Filter coefficient 5. Equivalent to: a4
    a4: f64,
    /// Input history 1. Equivalent to: x1
    x1: f64,
    /// Input history 2. Equivalent to: x2
    x2: f64,
    /// Output history 1. Equivalent to: y1
    y1: f64,
    /// Output history 2. Equivalent to: y2
    y2: f64,
    /// Last cutoff used (Infinity forces recalculation on first call).
    /// Equivalent to: lastTargetCutoff
    /// f64 to match TS (JS number is f64).
    last_target_cutoff: f64,
    /// Whether the filter has been initialised for the current note.
    /// Equivalent to: initialized
    initialized: bool,
    /// Audio engine sample rate in Hz.
    /// Equivalent to: sampleRate
    sample_rate: f64,
    /// Maximum allowed cutoff Hz (= sampleRate * 0.45).
    /// Equivalent to: maxCutoff
    max_cutoff: f64,
}

impl LowpassFilter {
    /// Creates a new LowpassFilter instance.
    /// Equivalent to: new LowpassFilter(sampleRate)
    pub fn new(sample_rate: f64) -> Self {
        Self {
            resonance_cb: 0,
            current_initial_fc: 13_500.0_f64,
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

    /// Initialises the global smoothing constant and pre-populates the coefficient cache
    /// for all integer cutoffs in the SF spec range [1500, 13500) with Q = 0.
    /// Equivalent to: LowpassFilter.initCache(sampleRate)
    pub fn init_cache(sample_rate: f64) {
        set_smoothing_constant(FILTER_SMOOTHING_FACTOR * (44_100.0 / sample_rate));
        let mut dummy = LowpassFilter::new(sample_rate);
        dummy.resonance_cb = 0;
        // SF spec §8.1.3: initialFilterFc ranges 1500 – 13 499 cents
        for i in 1500..13_500_i32 {
            dummy.current_initial_fc = i as f64;
            dummy.calculate_coefficients(i as f64);
        }
    }

    /// Resets all filter state for a new note.
    /// Equivalent to: init()
    pub fn init(&mut self) {
        self.last_target_cutoff = f64::INFINITY;
        self.resonance_cb = 0;
        self.current_initial_fc = 13_500.0_f64;
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

    /// Applies the lowpass filter to the first `sample_count` samples in `output_buffer`.
    ///
    /// `modulated_generators` is the voice's real-time generator slice (size GENERATORS_AMOUNT).
    /// `fc_offset` is the modulation/LFO frequency excursion in cents.
    ///
    /// Equivalent to: process(sampleCount, voice, outputBuffer, fcOffset)
    pub fn process(
        &mut self,
        sample_count: usize,
        modulated_generators: &[i16],
        output_buffer: &mut [f32],
        fc_offset: f64,
    ) {
        // Use f64 for filter cutoff arithmetic to match TS (JS number is f64).
        let initial_fc = modulated_generators[gt::INITIAL_FILTER_FC as usize] as f64;

        if self.initialized {
            // Smooth only the base cutoff; modulation offsets are not smoothed.
            self.current_initial_fc +=
                (initial_fc - self.current_initial_fc) * get_smoothing_constant();
        } else {
            // First call: snap directly to target.
            self.initialized = true;
            self.current_initial_fc = initial_fc;
        }

        let target_cutoff = self.current_initial_fc + fc_offset;
        let modulated_resonance = modulated_generators[gt::INITIAL_FILTER_Q as usize];

        // Filter bypass: fully open + no resonance.
        if self.current_initial_fc > 13_499.0
            && target_cutoff > 13_499.0
            && modulated_resonance == 0
        {
            self.current_initial_fc = 13_500.0;
            return;
        }

        // Recalculate coefficients if cutoff or resonance changed.
        if (self.last_target_cutoff - target_cutoff).abs() > 1.0
            || self.resonance_cb != modulated_resonance
        {
            self.last_target_cutoff = target_cutoff;
            self.resonance_cb = modulated_resonance;
            self.calculate_coefficients(target_cutoff);
        }

        // IIR biquad filter loop (Direct Form I).
        // Initial filtering code was ported from meltysynth created by sinshu.
        for sample in output_buffer[..sample_count].iter_mut() {
            let input = *sample as f64;
            let filtered = self.a0 * input + self.a1 * self.x1 + self.a2 * self.x2
                - self.a3 * self.y1
                - self.y2 * self.a4;

            self.x2 = self.x1;
            self.x1 = input;
            self.y2 = self.y1;
            self.y1 = filtered;

            *sample = filtered as f32;
        }
    }

    /// Computes and caches biquad coefficients for the given cutoff frequency in cents.
    /// Equivalent to: calculateCoefficients(cutoffCents)
    pub fn calculate_coefficients(&mut self, cutoff_cents: f64) {
        // Truncate to integer, matching JS `| 0`.
        let cutoff_cents_i = cutoff_cents as i32;
        let q_cb = self.resonance_cb;

        // Cache key: resonanceCb + cutoffCentsInt * 961
        let cache_key = q_cb as i32 + cutoff_cents_i * 961;

        // Try cache hit first.
        {
            let cache = CACHED_COEFFICIENTS.lock().unwrap();
            if let Some(cached) = cache.get(&cache_key) {
                self.a0 = cached.a0;
                self.a1 = cached.a1;
                self.a2 = cached.a2;
                self.a3 = cached.a3;
                self.a4 = cached.a4;
                return;
            }
        }

        // Compute cutoff Hz and clamp to max_cutoff.
        let cutoff_hz = (abs_cents_to_hz(cutoff_cents_i) as f64).min(self.max_cutoff);

        // resonanceGain = cbAttenuationToGain(-(qCb - 3.01))
        // Since qCb is an integer, (3.01 - qCb) truncates to (3 - qCb).
        // -3.01 gives a non-resonant peak; -1 because it's attenuation (we want gain).
        let resonance_gain = cb_attenuation_to_gain(3 - q_cb as i32) as f64;

        // qGain = 1 / sqrt(cbAttenuationToGain(-qCb))
        // This reduces the overall output gain based on Q.
        let q_gain = 1.0_f64 / (cb_attenuation_to_gain(-(q_cb as i32)) as f64).sqrt();

        // Standard biquad lowpass coefficient formula.
        // The coefficient calculation code was originally ported from meltysynth by sinshu.
        let w = (2.0 * std::f64::consts::PI * cutoff_hz) / self.sample_rate;
        let cos_w = w.cos();
        let alpha = w.sin() / (2.0 * resonance_gain);

        let b1 = (1.0 - cos_w) * q_gain;
        let b0 = b1 / 2.0;
        let b2 = b0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w;
        let a2 = 1.0 - alpha;

        let coeff = CachedCoefficient {
            a0: b0 / a0,
            a1: b1 / a0,
            a2: b2 / a0,
            a3: a1 / a0,
            a4: a2 / a0,
        };

        self.a0 = coeff.a0;
        self.a1 = coeff.a1;
        self.a2 = coeff.a2;
        self.a3 = coeff.a3;
        self.a4 = coeff.a4;

        CACHED_COEFFICIENTS.lock().unwrap().insert(cache_key, coeff);
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
    const EPS64: f64 = 1e-10;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < EPS
    }

    fn approx_eq64(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS64
    }

    /// Returns a generator slice with all defaults.
    fn default_gens() -> Vec<i16> {
        DEFAULT_GENERATOR_VALUES.to_vec()
    }

    /// Returns a generator slice with initialFilterFc set to `fc_cents`.
    fn gens_with_fc(fc_cents: i16) -> Vec<i16> {
        let mut g = default_gens();
        g[gt::INITIAL_FILTER_FC as usize] = fc_cents;
        g
    }

    /// Returns a generator slice with both initialFilterFc and initialFilterQ set.
    fn gens_with_fc_and_q(fc_cents: i16, q_cb: i16) -> Vec<i16> {
        let mut g = gens_with_fc(fc_cents);
        g[gt::INITIAL_FILTER_Q as usize] = q_cb;
        g
    }

    // -----------------------------------------------------------------------
    // new()
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_sample_rate_stored() {
        let f = LowpassFilter::new(SAMPLE_RATE);
        assert!(approx_eq64(f.sample_rate, SAMPLE_RATE));
    }

    #[test]
    fn test_new_max_cutoff_is_45_percent_of_sample_rate() {
        let f = LowpassFilter::new(SAMPLE_RATE);
        assert!(approx_eq64(f.max_cutoff, SAMPLE_RATE * 0.45));
    }

    #[test]
    fn test_new_initialized_is_false() {
        let f = LowpassFilter::new(SAMPLE_RATE);
        assert!(!f.initialized);
    }

    #[test]
    fn test_new_current_initial_fc_is_13500() {
        let f = LowpassFilter::new(SAMPLE_RATE);
        assert!(approx_eq64(f.current_initial_fc, 13_500.0));
    }

    #[test]
    fn test_new_resonance_cb_is_zero() {
        let f = LowpassFilter::new(SAMPLE_RATE);
        assert_eq!(f.resonance_cb, 0);
    }

    #[test]
    fn test_new_last_target_cutoff_is_infinity() {
        let f = LowpassFilter::new(SAMPLE_RATE);
        assert!(f.last_target_cutoff.is_infinite());
    }

    // -----------------------------------------------------------------------
    // init()
    // -----------------------------------------------------------------------

    #[test]
    fn test_init_resets_initialized_flag() {
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        f.initialized = true;
        f.init();
        assert!(!f.initialized);
    }

    #[test]
    fn test_init_resets_current_initial_fc() {
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        f.current_initial_fc = 5_000.0;
        f.init();
        assert!(approx_eq64(f.current_initial_fc, 13_500.0));
    }

    #[test]
    fn test_init_resets_resonance_cb() {
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        f.resonance_cb = 100;
        f.init();
        assert_eq!(f.resonance_cb, 0);
    }

    #[test]
    fn test_init_resets_history() {
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        f.x1 = 1.0;
        f.x2 = 2.0;
        f.y1 = 3.0;
        f.y2 = 4.0;
        f.init();
        assert!((f.x1 - 0.0).abs() < 1e-10);
        assert!((f.x2 - 0.0).abs() < 1e-10);
        assert!((f.y1 - 0.0).abs() < 1e-10);
        assert!((f.y2 - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_init_resets_last_target_cutoff_to_infinity() {
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        f.last_target_cutoff = 5_000.0;
        f.init();
        assert!(f.last_target_cutoff.is_infinite());
    }

    // -----------------------------------------------------------------------
    // calculate_coefficients()
    // -----------------------------------------------------------------------

    #[test]
    fn test_calculate_coefficients_produces_nonzero_a0() {
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        f.calculate_coefficients(5_000.0);
        assert!(f.a0 > 0.0);
    }

    #[test]
    fn test_calculate_coefficients_lowpass_symmetry_a0_equals_a2() {
        // For a standard biquad lowpass filter, b0 == b2, so a0 == a2.
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        f.calculate_coefficients(5_000.0);
        assert!(approx_eq64(f.a0, f.a2));
    }

    #[test]
    fn test_calculate_coefficients_a1_is_twice_a0() {
        // b1 == 2 * b0, so a1 == 2 * a0.
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        f.calculate_coefficients(5_000.0);
        assert!(approx_eq64(f.a1, f.a0 * 2.0));
    }

    #[test]
    fn test_calculate_coefficients_same_params_same_result() {
        let mut f1 = LowpassFilter::new(SAMPLE_RATE);
        let mut f2 = LowpassFilter::new(SAMPLE_RATE);
        f1.calculate_coefficients(6_000.0);
        f2.calculate_coefficients(6_000.0);
        assert!(approx_eq64(f1.a0, f2.a0));
        assert!(approx_eq64(f1.a3, f2.a3));
    }

    #[test]
    fn test_calculate_coefficients_different_cutoff_different_result() {
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        f.calculate_coefficients(3_000.0);
        let a0_low = f.a0;
        f.calculate_coefficients(10_000.0);
        let a0_high = f.a0;
        assert!((a0_low - a0_high).abs() > 1e-4);
    }

    #[test]
    fn test_calculate_coefficients_cache_hit_sets_same_values() {
        // Populate cache then read from cache.
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        f.resonance_cb = 0;
        f.calculate_coefficients(7_000.0); // populates cache
        let a0_first = f.a0;
        let a3_first = f.a3;

        // Second call must hit cache and produce identical values.
        f.calculate_coefficients(7_000.0);
        assert!(approx_eq64(f.a0, a0_first));
        assert!(approx_eq64(f.a3, a3_first));
    }

    #[test]
    fn test_calculate_coefficients_high_cutoff_clamped_to_max() {
        // A very high cutoff (e.g., 16000 cents) should be clamped to max_cutoff.
        // Two filters with very different raw Hz should produce the same coefficients
        // once clamped by their respective max_cutoffs (same sample rate here).
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        f.calculate_coefficients(16_000.0);
        let a0_a = f.a0;

        // Using an even higher value – both should clamp to the same max_cutoff.
        f.resonance_cb = 0; // same resonance
        f.calculate_coefficients(16_500.0);
        let a0_b = f.a0;

        // Both clamp to max_cutoff = SAMPLE_RATE * 0.45 ≈ 19845 Hz, so coefficients match.
        assert!(approx_eq64(a0_a, a0_b));
    }

    // -----------------------------------------------------------------------
    // process() – bypass
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_bypass_when_filter_is_open() {
        // Default generators have INITIAL_FILTER_FC = 13500 and INITIAL_FILTER_Q = 0,
        // so the filter should bypass and leave the buffer untouched.
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        let gens = default_gens(); // fc=13500, q=0
        let input = vec![0.1_f32, 0.2, 0.3, 0.4];
        let mut buf = input.clone();

        f.process(4, &gens, &mut buf, 0.0);

        // Buffer must be unmodified.
        for (a, b) in buf.iter().zip(input.iter()) {
            assert!(approx_eq(*a, *b));
        }
    }

    #[test]
    fn test_process_bypass_resets_current_initial_fc_to_13500() {
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        let gens = default_gens();
        let mut buf = vec![1.0_f32; 8];
        f.process(8, &gens, &mut buf, 0.0);
        assert!(approx_eq64(f.current_initial_fc, 13_500.0));
    }

    // -----------------------------------------------------------------------
    // process() – initialisation
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_first_call_sets_initialized() {
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        // Use a low fc so the filter is active.
        let gens = gens_with_fc(5_000);
        let mut buf = vec![0.0_f32; 4];
        f.process(4, &gens, &mut buf, 0.0);
        assert!(f.initialized);
    }

    #[test]
    fn test_process_first_call_snaps_fc_to_target() {
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        let gens = gens_with_fc(5_000);
        let mut buf = vec![0.0_f32; 4];
        f.process(4, &gens, &mut buf, 0.0);
        // On the first call (before smoothing), current_initial_fc must equal the generator value.
        assert!(approx_eq64(f.current_initial_fc, 5_000.0));
    }

    // -----------------------------------------------------------------------
    // process() – smoothing
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_second_call_applies_smoothing() {
        // Set smoothing constant to a fixed known value.
        set_smoothing_constant(0.5);

        let mut f = LowpassFilter::new(SAMPLE_RATE);
        let gens_low = gens_with_fc(4_000);
        let gens_high = gens_with_fc(8_000);
        let mut buf = vec![0.0_f32; 4];

        // First call snaps to 4000.
        f.process(4, &gens_low, &mut buf, 0.0);
        assert!(approx_eq64(f.current_initial_fc, 4_000.0));

        // Second call: target is 8000, smoothing factor 0.5.
        // new_fc = 4000 + (8000 - 4000) * 0.5 = 6000
        f.process(4, &gens_high, &mut buf, 0.0);
        assert!(approx_eq64(f.current_initial_fc, 6_000.0));

        // Restore default smoothing constant for other tests.
        set_smoothing_constant(1.0);
    }

    // -----------------------------------------------------------------------
    // process() – filtering
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_active_filter_modifies_buffer() {
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        let gens = gens_with_fc(3_000);
        let input = vec![1.0_f32, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
        let mut buf = input.clone();

        f.process(8, &gens, &mut buf, 0.0);

        // High-frequency (Nyquist) signal must be attenuated.
        let max_out = buf.iter().map(|x| x.abs()).fold(0.0_f32, f32::max);
        assert!(
            max_out < 0.5,
            "expected significant attenuation, got {max_out}"
        );
    }

    #[test]
    fn test_process_dc_signal_passes_lowpass() {
        // DC (all 1.0) should pass through a lowpass filter mostly intact.
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        let gens = gens_with_fc(8_000);
        let mut buf = vec![1.0_f32; 64];

        f.process(64, &gens, &mut buf, 0.0);

        // After many samples the output should converge near 1.0.
        let last = buf[63];
        assert!(
            (last - 1.0).abs() < 0.05,
            "DC should pass through, got {last}"
        );
    }

    #[test]
    fn test_process_zero_input_gives_zero_output() {
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        let gens = gens_with_fc(5_000);
        let mut buf = vec![0.0_f32; 8];

        f.process(8, &gens, &mut buf, 0.0);

        for &s in &buf {
            assert!(approx_eq(s, 0.0));
        }
    }

    #[test]
    fn test_process_respects_sample_count() {
        // Only the first `sample_count` samples should be processed.
        let mut f = LowpassFilter::new(SAMPLE_RATE);
        let gens = gens_with_fc(3_000);
        let mut buf = vec![1.0_f32; 8];
        // Process only first 4 samples.
        f.process(4, &gens, &mut buf, 0.0);
        // The last 4 samples must remain exactly 1.0.
        for &s in &buf[4..] {
            assert!(approx_eq(s, 1.0));
        }
    }

    // -----------------------------------------------------------------------
    // process() – resonance
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_resonance_changes_output() {
        let fc: i16 = 5_000;

        let mut f_no_q = LowpassFilter::new(SAMPLE_RATE);
        let gens_no_q = gens_with_fc_and_q(fc, 0);
        let mut buf_no_q = vec![1.0_f32; 16];
        f_no_q.process(16, &gens_no_q, &mut buf_no_q, 0.0);

        let mut f_with_q = LowpassFilter::new(SAMPLE_RATE);
        let gens_with_q = gens_with_fc_and_q(fc, 200);
        let mut buf_with_q = vec![1.0_f32; 16];
        f_with_q.process(16, &gens_with_q, &mut buf_with_q, 0.0);

        // Outputs must differ when resonance changes.
        let all_same = buf_no_q
            .iter()
            .zip(buf_with_q.iter())
            .all(|(a, b)| approx_eq(*a, *b));
        assert!(!all_same, "resonance change must affect output");
    }

    // -----------------------------------------------------------------------
    // process() – fc_offset
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_fc_offset_changes_output() {
        let fc: i16 = 5_000;
        let gens = gens_with_fc(fc);
        let input = vec![1.0_f32; 32];

        let mut f0 = LowpassFilter::new(SAMPLE_RATE);
        let mut buf0 = input.clone();
        f0.process(32, &gens, &mut buf0, 0.0);

        let mut f1 = LowpassFilter::new(SAMPLE_RATE);
        let mut buf1 = input.clone();
        f1.process(32, &gens, &mut buf1, -3_000.0);

        let all_same = buf0.iter().zip(buf1.iter()).all(|(a, b)| approx_eq(*a, *b));
        assert!(!all_same, "fc_offset must affect output");
    }

    // -----------------------------------------------------------------------
    // FILTER_SMOOTHING_FACTOR
    // -----------------------------------------------------------------------

    #[test]
    fn test_smoothing_factor_value() {
        assert!(approx_eq64(FILTER_SMOOTHING_FACTOR, 0.03));
    }
}
