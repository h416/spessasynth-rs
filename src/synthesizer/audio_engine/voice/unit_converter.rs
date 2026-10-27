//! Converts sound bank units into more usable values, using lookup tables to improve performance.
//! Ported from: src/synthesizer/audio_engine/voice/unit_converter.ts
//!
//! The tables are Float32Array in TS, so a table hit is an f32 widened to f64, while the
//! out-of-table fallbacks are full f64 `Math.pow` results. All functions return f64 so
//! that the fallback keeps its precision.
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Timecent lookup table
// ---------------------------------------------------------------------------

const MIN_TIMECENT: i32 = -15_000;
const MAX_TIMECENT: i32 = 15_000;

static TIMECENT_LOOKUP_TABLE: LazyLock<Vec<f32>> = LazyLock::new(|| {
    let len = (MAX_TIMECENT - MIN_TIMECENT + 1) as usize;
    let mut table = Vec::with_capacity(len);
    for i in 0..len {
        let timecents = MIN_TIMECENT + i as i32;
        // Compute in f64 then truncate to f32, matching TS: Math.pow(2, t/1200) → Float32Array
        table.push(2f64.powf(timecents as f64 / 1200.0) as f32);
    }
    table
});

/// Converts timecents to seconds.
/// Equivalent to: timecentsToSeconds
pub fn timecents_to_seconds(timecents: i32) -> f64 {
    if timecents <= -32_767 {
        return 0.0;
    }
    if timecents < MIN_TIMECENT || timecents > MAX_TIMECENT {
        // Key scaling can push the value outside the table
        return 2f64.powf(timecents as f64 / 1200.0);
    }
    TIMECENT_LOOKUP_TABLE[(timecents - MIN_TIMECENT) as usize] as f64
}

// ---------------------------------------------------------------------------
// Absolute cent lookup table
// ---------------------------------------------------------------------------

const MIN_ABS_CENT: i32 = -20_000;
const MAX_ABS_CENT: i32 = 16_500;

static ABSOLUTE_CENT_LOOKUP_TABLE: LazyLock<Vec<f32>> = LazyLock::new(|| {
    let len = (MAX_ABS_CENT - MIN_ABS_CENT + 1) as usize;
    let mut table = Vec::with_capacity(len);
    for i in 0..len {
        let cents = MIN_ABS_CENT + i as i32;
        // Compute in f64 then truncate to f32, matching TS: 440 * Math.pow(2, (c-6900)/1200) → Float32Array
        table.push((440.0f64 * 2f64.powf((cents as f64 - 6900.0) / 1200.0)) as f32);
    }
    table
});

/// Converts absolute cents to frequency in Hz.
/// Equivalent to: absCentsToHz
pub fn abs_cents_to_hz(cents: i32) -> f64 {
    if !(MIN_ABS_CENT..=MAX_ABS_CENT).contains(&cents) {
        return 440.0f64 * 2f64.powf((cents as f64 - 6900.0) / 1200.0);
    }
    ABSOLUTE_CENT_LOOKUP_TABLE[(cents - MIN_ABS_CENT) as usize] as f64
}

// ---------------------------------------------------------------------------
// Centibel lookup table
// ---------------------------------------------------------------------------

const MIN_CENTIBELS: i32 = -16_600;
const MAX_CENTIBELS: i32 = 16_000;

static CENTIBEL_LOOKUP_TABLE: LazyLock<Vec<f32>> = LazyLock::new(|| {
    let len = (MAX_CENTIBELS - MIN_CENTIBELS + 1) as usize;
    let mut table = Vec::with_capacity(len);
    for i in 0..len {
        let centibels = MIN_CENTIBELS + i as i32;
        // Compute in f64 then truncate to f32, matching TS: Math.pow(10, -cb/200) → Float32Array
        table.push(10f64.powf(-centibels as f64 / 200.0) as f32);
    }
    table
});

/// Converts centibel attenuation to linear gain (integer argument).
/// Equivalent to: cbAttenuationToGain when called with an integer argument.
pub fn cb_attenuation_to_gain(centibels: i32) -> f64 {
    cb_attenuation_to_gain_f64(centibels as f64)
}

/// Converts centibel attenuation to linear gain.
///
/// `centibels` is a JS number, so the table index is `(centibels - MIN_CENTIBELS) | 0`:
/// the subtraction happens in f64 and is then truncated, which is not the same as
/// truncating `centibels` first (`-11.27 - (-16600)` = `16588.73` → `16588`,
/// vs. `-11 - (-16600)` = `16589`).
/// Equivalent to: cbAttenuationToGain
pub fn cb_attenuation_to_gain_f64(centibels: f64) -> f64 {
    if centibels >= MIN_CENTIBELS as f64 && centibels <= MAX_CENTIBELS as f64 {
        return CENTIBEL_LOOKUP_TABLE[(centibels - MIN_CENTIBELS as f64) as i32 as usize] as f64;
    }
    // Clamp to the table, NaN is treated as silence
    if centibels < MIN_CENTIBELS as f64 {
        CENTIBEL_LOOKUP_TABLE[0] as f64
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-4;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    // --- timecents_to_seconds ---

    #[test]
    fn test_timecents_below_threshold_returns_zero() {
        // timecents <= -32767 → 0
        assert_eq!(timecents_to_seconds(-32_767), 0.0);
        assert_eq!(timecents_to_seconds(-32_768), 0.0);
        assert_eq!(timecents_to_seconds(i32::MIN), 0.0);
    }

    #[test]
    fn test_timecents_zero_is_one_second() {
        // 2^(0/1200) = 1.0
        assert!(approx_eq(timecents_to_seconds(0), 1.0));
    }

    #[test]
    fn test_timecents_1200_is_two_seconds() {
        // 2^(1200/1200) = 2.0
        assert!(approx_eq(timecents_to_seconds(1200), 2.0));
    }

    #[test]
    fn test_timecents_minus_1200_is_half_second() {
        // 2^(-1200/1200) = 0.5
        assert!(approx_eq(timecents_to_seconds(-1200), 0.5));
    }

    #[test]
    fn test_timecents_2400_is_four_seconds() {
        // 2^(2400/1200) = 4.0
        assert!(approx_eq(timecents_to_seconds(2400), 4.0));
    }

    #[test]
    fn test_timecents_max_in_table() {
        // Should not panic at table boundary
        let v = timecents_to_seconds(MAX_TIMECENT);
        assert!(v > 0.0);
    }

    #[test]
    fn test_timecents_min_in_table() {
        let v = timecents_to_seconds(MIN_TIMECENT);
        assert!(v > 0.0);
    }

    #[test]
    fn test_timecents_out_of_range_uses_pow() {
        // 4.4.0: key scaling can push the value outside the table, computed directly.
        assert_eq!(timecents_to_seconds(-15_001), 2f64.powf(-15_001.0 / 1200.0));
        assert_eq!(timecents_to_seconds(15_001), 2f64.powf(15_001.0 / 1200.0));
    }

    #[test]
    fn test_cb_out_of_range_clamps() {
        // Below the table: the first entry. Above it (and NaN): silence.
        assert_eq!(cb_attenuation_to_gain(-20_000), cb_attenuation_to_gain(-16_600));
        assert_eq!(cb_attenuation_to_gain(20_000), 0.0);
        assert_eq!(cb_attenuation_to_gain_f64(f64::NAN), 0.0);
    }

    #[test]
    fn test_nan_max_returns_zero_in_rust() {
        // Rust: f64::NAN.max(0.0) = 0.0 (IEEE 754-2008 maxNum: NaN is ignored)
        // JS: Math.max(0, NaN) = NaN (NaN propagates)
        // This difference affects volume_envelope's timecents_to_samples.
        assert_eq!(f64::NAN.max(0.0), 0.0);
        assert_eq!(0.0f64.max(f64::NAN), 0.0);
    }

    // --- abs_cents_to_hz ---

    #[test]
    fn test_abs_cents_a4_is_440hz() {
        // 6900 abs cents = A4 = 440 Hz
        assert!(approx_eq(abs_cents_to_hz(6900), 440.0));
    }

    #[test]
    fn test_abs_cents_a5_is_880hz() {
        // 8100 abs cents = A5 = 880 Hz  (+1200 cents = one octave)
        assert!(approx_eq(abs_cents_to_hz(8100), 880.0));
    }

    #[test]
    fn test_abs_cents_a3_is_220hz() {
        // 5700 abs cents = A3 = 220 Hz  (-1200 cents = one octave)
        assert!(approx_eq(abs_cents_to_hz(5700), 220.0));
    }

    #[test]
    fn test_abs_cents_out_of_range_high() {
        // Above MAX_ABS_CENT falls back to direct calculation
        let direct = 440.0f64 * 2f64.powf((20_000.0 - 6900.0) / 1200.0);
        assert!(approx_eq(abs_cents_to_hz(20_000), direct));
    }

    #[test]
    fn test_abs_cents_out_of_range_low() {
        // Below MIN_ABS_CENT falls back to direct calculation
        let direct = 440.0f64 * 2f64.powf((-25_000.0 - 6900.0) / 1200.0);
        assert!(approx_eq(abs_cents_to_hz(-25_000), direct));
    }

    #[test]
    fn test_abs_cents_table_boundary_max() {
        let v = abs_cents_to_hz(MAX_ABS_CENT);
        assert!(v > 0.0);
    }

    #[test]
    fn test_abs_cents_table_boundary_min() {
        let v = abs_cents_to_hz(MIN_ABS_CENT);
        assert!(v > 0.0);
    }

    // --- cb_attenuation_to_gain ---

    #[test]
    fn test_cb_zero_attenuation_is_unity_gain() {
        // 10^(-0/200) = 1.0
        assert!(approx_eq(cb_attenuation_to_gain(0), 1.0));
    }

    #[test]
    fn test_cb_200_is_tenth_gain() {
        // 10^(-200/200) = 10^-1 = 0.1
        assert!(approx_eq(cb_attenuation_to_gain(200), 0.1));
    }

    #[test]
    fn test_cb_400_is_hundredth_gain() {
        // 10^(-400/200) = 10^-2 = 0.01
        assert!(approx_eq(cb_attenuation_to_gain(400), 0.01));
    }

    #[test]
    fn test_cb_negative_boosts_gain() {
        // 10^(100/200) = 10^0.5 ≈ 3.162
        let expected = 10f64.powf(100.0 / 200.0);
        assert!(approx_eq(cb_attenuation_to_gain(-100), expected));
    }

    #[test]
    fn test_cb_table_boundary_max() {
        let v = cb_attenuation_to_gain(MAX_CENTIBELS);
        assert!(v >= 0.0);
    }

    #[test]
    fn test_cb_table_boundary_min() {
        let v = cb_attenuation_to_gain(MIN_CENTIBELS);
        assert!(v > 0.0);
    }
}
