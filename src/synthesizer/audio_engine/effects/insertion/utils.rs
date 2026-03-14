/// utils.rs
/// purpose: DSP utility types and functions for insertion effects.
/// Ported from: src/synthesizer/audio_engine/effects/insertion/utils.ts

use std::f64::consts::PI;
use std::sync::OnceLock;

const HALF_PI: f64 = PI / 2.0;
const MIN_PAN: i32 = -64;
const MAX_PAN: i32 = 63;
const PAN_RESOLUTION: i32 = MAX_PAN - MIN_PAN; // 127

/// Pre-computed pan table for left channel (cos law), 128 entries.
static PAN_TABLE_LEFT: OnceLock<[f32; 128]> = OnceLock::new();

/// Pre-computed pan table for right channel (sin law), 128 entries.
static PAN_TABLE_RIGHT: OnceLock<[f32; 128]> = OnceLock::new();

pub fn get_pan_table_left() -> &'static [f32; 128] {
    PAN_TABLE_LEFT.get_or_init(|| {
        let mut table = [0f32; 128];
        for pan in MIN_PAN..=MAX_PAN {
            let real_pan = (pan - MIN_PAN) as f64 / PAN_RESOLUTION as f64;
            let idx = (pan - MIN_PAN) as usize;
            table[idx] = (HALF_PI * real_pan).cos() as f32;
        }
        table
    })
}

pub fn get_pan_table_right() -> &'static [f32; 128] {
    PAN_TABLE_RIGHT.get_or_init(|| {
        let mut table = [0f32; 128];
        for pan in MIN_PAN..=MAX_PAN {
            let real_pan = (pan - MIN_PAN) as f64 / PAN_RESOLUTION as f64;
            let idx = (pan - MIN_PAN) as usize;
            table[idx] = (HALF_PI * real_pan).sin() as f32;
        }
        table
    })
}

#[derive(Clone, Debug)]
pub struct BiquadCoeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a0: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Default for BiquadCoeffs {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a0: 1.0,
            a1: 0.0,
            a2: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct BiquadState {
    pub x1: f64,
    pub x2: f64,
    pub y1: f64,
    pub y2: f64,
}

impl BiquadState {
    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// Apply cascaded low shelf + high shelf (Direct Form I, inlined).
#[inline]
pub fn apply_shelves(
    x: f64,
    low_c: &BiquadCoeffs,
    high_c: &BiquadCoeffs,
    low_s: &mut BiquadState,
    high_s: &mut BiquadState,
) -> f64 {
    // Low shelf
    let l = low_c.b0 * x + low_c.b1 * low_s.x1 + low_c.b2 * low_s.x2
        - low_c.a1 * low_s.y1
        - low_c.a2 * low_s.y2;
    low_s.x2 = low_s.x1;
    low_s.x1 = x;
    low_s.y2 = low_s.y1;
    low_s.y1 = l;
    // High shelf
    let h = high_c.b0 * l + high_c.b1 * high_s.x1 + high_c.b2 * high_s.x2
        - high_c.a1 * high_s.y1
        - high_c.a2 * high_s.y2;
    high_s.x2 = high_s.x1;
    high_s.x1 = l;
    high_s.y2 = high_s.y1;
    high_s.y1 = h;
    h
}

/// Direct Form I biquad filter.
#[inline]
pub fn process_biquad(x: f64, coeffs: &BiquadCoeffs, state: &mut BiquadState) -> f64 {
    let y = coeffs.b0 * x + coeffs.b1 * state.x1 + coeffs.b2 * state.x2
        - coeffs.a1 * state.y1
        - coeffs.a2 * state.y2;
    state.x2 = state.x1;
    state.x1 = x;
    state.y2 = state.y1;
    state.y1 = y;
    y
}

/// Robert Bristow-Johnson cookbook shelf filter coefficients.
pub fn compute_shelf_coeffs(
    coeffs: &mut BiquadCoeffs,
    db_gain: f64,
    f0: f64,
    fs: f64,
    is_low: bool,
) {
    let a = 10.0_f64.powf(db_gain / 40.0);
    let w0 = (2.0 * PI * f0) / fs;
    let cosw0 = w0.cos();
    let sinw0 = w0.sin();
    let s = 1.0;
    let alpha = (sinw0 / 2.0) * ((a + 1.0 / a) * (1.0 / s - 1.0) + 2.0).sqrt();

    let (b0, b1, b2, a0, a1, a2);
    let sqrt_a = a.sqrt();

    if is_low {
        b0 = a * (a + 1.0 - (a - 1.0) * cosw0 + 2.0 * sqrt_a * alpha);
        b1 = 2.0 * a * (a - 1.0 - (a + 1.0) * cosw0);
        b2 = a * (a + 1.0 - (a - 1.0) * cosw0 - 2.0 * sqrt_a * alpha);
        a0 = a + 1.0 + (a - 1.0) * cosw0 + 2.0 * sqrt_a * alpha;
        a1 = -2.0 * (a - 1.0 + (a + 1.0) * cosw0);
        a2 = a + 1.0 + (a - 1.0) * cosw0 - 2.0 * sqrt_a * alpha;
    } else {
        b0 = a * (a + 1.0 + (a - 1.0) * cosw0 + 2.0 * sqrt_a * alpha);
        b1 = -2.0 * a * (a - 1.0 + (a + 1.0) * cosw0);
        b2 = a * (a + 1.0 + (a - 1.0) * cosw0 - 2.0 * sqrt_a * alpha);
        a0 = a + 1.0 - (a - 1.0) * cosw0 + 2.0 * sqrt_a * alpha;
        a1 = 2.0 * (a - 1.0 - (a + 1.0) * cosw0);
        a2 = a + 1.0 - (a - 1.0) * cosw0 - 2.0 * sqrt_a * alpha;
    }

    coeffs.b0 = b0 / a0;
    coeffs.b1 = b1 / a0;
    coeffs.b2 = b2 / a0;
    coeffs.a0 = 1.0;
    coeffs.a1 = a1 / a0;
    coeffs.a2 = a2 / a0;
}

/// Compute peaking EQ (parametric) biquad coefficients.
pub fn compute_peaking_eq_coeffs(
    coeffs: &mut BiquadCoeffs,
    freq: f64,
    gain_db: f64,
    q: f64,
    sample_rate: f64,
) {
    let a = 10.0_f64.powf(gain_db / 40.0);
    let w0 = (2.0 * PI * freq) / sample_rate;
    let cosw0 = w0.cos();
    let sinw0 = w0.sin();
    let alpha = sinw0 / (2.0 * q);

    let b0 = 1.0 + alpha * a;
    let b1 = -2.0 * cosw0;
    let b2 = 1.0 - alpha * a;
    let a0 = 1.0 + alpha / a;
    let a1 = -2.0 * cosw0;
    let a2 = 1.0 - alpha / a;

    coeffs.a0 = 1.0;
    coeffs.a1 = a1 / a0;
    coeffs.a2 = a2 / a0;
    coeffs.b0 = b0 / a0;
    coeffs.b1 = b1 / a0;
    coeffs.b2 = b2 / a0;
}

/// Compute lowpass biquad coefficients.
pub fn compute_lowpass_coeffs(
    coeffs: &mut BiquadCoeffs,
    freq: f64,
    q: f64,
    sample_rate: f64,
) {
    let w0 = (2.0 * PI * freq) / sample_rate;
    let cosw0 = w0.cos();
    let sinw0 = w0.sin();
    let alpha = sinw0 / (2.0 * q);

    let b1 = 1.0 - cosw0;
    let b0 = b1 / 2.0;
    let b2 = b0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cosw0;
    let a2 = 1.0 - alpha;

    coeffs.a0 = 1.0;
    coeffs.a1 = a1 / a0;
    coeffs.a2 = a2 / a0;
    coeffs.b0 = b0 / a0;
    coeffs.b1 = b1 / a0;
    coeffs.b2 = b2 / a0;
}

/// Compute highpass biquad coefficients.
pub fn compute_highpass_coeffs(
    coeffs: &mut BiquadCoeffs,
    freq: f64,
    q: f64,
    sample_rate: f64,
) {
    let w0 = (2.0 * PI * freq) / sample_rate;
    let cosw0 = w0.cos();
    let sinw0 = w0.sin();
    let alpha = sinw0 / (2.0 * q);

    let b0 = (1.0 + cosw0) / 2.0;
    let b1 = -(1.0 + cosw0);
    let b2 = b0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cosw0;
    let a2 = 1.0 - alpha;

    coeffs.a0 = 1.0;
    coeffs.a1 = a1 / a0;
    coeffs.a2 = a2 / a0;
    coeffs.b0 = b0 / a0;
    coeffs.b1 = b1 / a0;
    coeffs.b2 = b2 / a0;
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;
    const EPS_F32: f32 = 1e-6;
    const SAMPLE_RATE: f64 = 44100.0;

    // ---- Pan table tests ----

    #[test]
    fn pan_table_left_boundaries() {
        let table = get_pan_table_left();
        // index 0 = full left: cos(0) = 1.0
        assert!((table[0] - 1.0).abs() < EPS_F32);
        // index 127 = full right: cos(PI/2) = 0.0
        assert!(table[127].abs() < EPS_F32);
    }

    #[test]
    fn pan_table_right_boundaries() {
        let table = get_pan_table_right();
        // index 0 = full left: sin(0) = 0.0
        assert!(table[0].abs() < EPS_F32);
        // index 127 = full right: sin(PI/2) = 1.0
        assert!((table[127] - 1.0).abs() < EPS_F32);
    }

    #[test]
    fn pan_table_center_value() {
        let left = get_pan_table_left();
        let right = get_pan_table_right();
        // center pan index = 64
        let l = left[64] as f64;
        let r = right[64] as f64;
        assert!((l - r).abs() < 0.01, "Center pan L={} R={} should be equal", l, r);
    }

    #[test]
    fn pan_table_power_complementary() {
        let left = get_pan_table_left();
        let right = get_pan_table_right();
        // cos²(x) + sin²(x) = 1 for all entries
        for i in 0..128 {
            let l = left[i] as f64;
            let r = right[i] as f64;
            let sum = l * l + r * r;
            assert!(
                (sum - 1.0).abs() < 0.001,
                "Power sum at index {}: {} (expected 1.0)",
                i,
                sum
            );
        }
    }

    #[test]
    fn pan_table_monotonic() {
        let left = get_pan_table_left();
        let right = get_pan_table_right();
        for i in 1..128 {
            assert!(left[i] <= left[i - 1], "Left table not decreasing at index {}", i);
        }
        for i in 1..128 {
            assert!(right[i] >= right[i - 1], "Right table not increasing at index {}", i);
        }
    }

    // ---- Biquad process tests ----

    #[test]
    fn biquad_passthrough() {
        let coeffs = BiquadCoeffs::default();
        let mut state = BiquadState::default();
        for &x in &[1.0, -0.5, 0.25, 0.0, -1.0] {
            let y = process_biquad(x, &coeffs, &mut state);
            assert!((y - x).abs() < EPS, "Passthrough failed: input={}, output={}", x, y);
        }
    }

    #[test]
    fn biquad_state_update() {
        let coeffs = BiquadCoeffs::default();
        let mut state = BiquadState::default();
        process_biquad(1.0, &coeffs, &mut state);
        assert!((state.x1 - 1.0).abs() < EPS);
        assert!(state.x2.abs() < EPS);
        process_biquad(2.0, &coeffs, &mut state);
        assert!((state.x1 - 2.0).abs() < EPS);
        assert!((state.x2 - 1.0).abs() < EPS);
    }

    #[test]
    fn biquad_dc_response_lowpass() {
        let mut coeffs = BiquadCoeffs::default();
        compute_lowpass_coeffs(&mut coeffs, 1000.0, 0.707, SAMPLE_RATE);
        let mut state = BiquadState::default();
        let mut y = 0.0;
        for _ in 0..10000 {
            y = process_biquad(1.0, &coeffs, &mut state);
        }
        assert!((y - 1.0).abs() < 0.001, "Lowpass DC gain should be ~1.0, got {}", y);
    }

    // ---- apply_shelves tests ----

    #[test]
    fn shelves_passthrough() {
        let low_c = BiquadCoeffs::default();
        let high_c = BiquadCoeffs::default();
        let mut low_s = BiquadState::default();
        let mut high_s = BiquadState::default();
        for &x in &[1.0, -0.5, 0.25] {
            let y = apply_shelves(x, &low_c, &high_c, &mut low_s, &mut high_s);
            assert!((y - x).abs() < EPS);
        }
    }

    // ---- Shelf coefficients tests ----

    #[test]
    fn shelf_zero_gain_passthrough() {
        let mut coeffs = BiquadCoeffs::default();
        compute_shelf_coeffs(&mut coeffs, 0.0, 1000.0, SAMPLE_RATE, true);
        assert!((coeffs.a0 - 1.0).abs() < EPS);
        let mut state = BiquadState::default();
        let mut y = 0.0;
        for _ in 0..10000 {
            y = process_biquad(1.0, &coeffs, &mut state);
        }
        assert!((y - 1.0).abs() < 0.001, "0dB low shelf DC gain should be ~1.0, got {}", y);
    }

    #[test]
    fn shelf_low_boost_dc() {
        let mut coeffs = BiquadCoeffs::default();
        compute_shelf_coeffs(&mut coeffs, 6.0, 500.0, SAMPLE_RATE, true);
        let mut state = BiquadState::default();
        let mut y = 0.0;
        for _ in 0..10000 {
            y = process_biquad(1.0, &coeffs, &mut state);
        }
        assert!(y > 1.0, "Low shelf +6dB should boost DC, got {}", y);
    }

    #[test]
    fn shelf_high_boost_preserves_dc() {
        let mut coeffs = BiquadCoeffs::default();
        compute_shelf_coeffs(&mut coeffs, 6.0, 5000.0, SAMPLE_RATE, false);
        let mut state = BiquadState::default();
        let mut y = 0.0;
        for _ in 0..10000 {
            y = process_biquad(1.0, &coeffs, &mut state);
        }
        assert!((y - 1.0).abs() < 0.01, "High shelf +6dB DC gain should be ~1.0, got {}", y);
    }

    // ---- Peaking EQ tests ----

    #[test]
    fn peaking_eq_zero_gain_passthrough() {
        let mut coeffs = BiquadCoeffs::default();
        compute_peaking_eq_coeffs(&mut coeffs, 1000.0, 0.0, 1.0, SAMPLE_RATE);
        assert!((coeffs.b0 - 1.0).abs() < EPS, "Peaking EQ 0dB b0 should be 1.0, got {}", coeffs.b0);
        assert!((coeffs.b1 - coeffs.a1).abs() < EPS, "Peaking EQ 0dB: b1 should equal a1");
        assert!((coeffs.b2 - coeffs.a2).abs() < EPS, "Peaking EQ 0dB: b2 should equal a2");
    }

    // ---- Lowpass tests ----

    #[test]
    fn lowpass_coeffs_symmetry() {
        let mut coeffs = BiquadCoeffs::default();
        compute_lowpass_coeffs(&mut coeffs, 2000.0, 0.707, SAMPLE_RATE);
        assert!((coeffs.b0 - coeffs.b2).abs() < EPS, "Lowpass b0 should equal b2");
        assert!((coeffs.b1 - 2.0 * coeffs.b0).abs() < EPS, "Lowpass b1 should equal 2*b0");
    }

    #[test]
    fn lowpass_blocks_nyquist() {
        let mut coeffs = BiquadCoeffs::default();
        compute_lowpass_coeffs(&mut coeffs, 100.0, 0.707, SAMPLE_RATE);
        let mut state = BiquadState::default();
        let mut y = 0.0;
        for i in 0..10000 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            y = process_biquad(x, &coeffs, &mut state);
        }
        assert!(y.abs() < 0.01, "Lowpass should block Nyquist, got {}", y);
    }

    // ---- Highpass tests ----

    #[test]
    fn highpass_coeffs_symmetry() {
        let mut coeffs = BiquadCoeffs::default();
        compute_highpass_coeffs(&mut coeffs, 2000.0, 0.707, SAMPLE_RATE);
        assert!((coeffs.b0 - coeffs.b2).abs() < EPS, "Highpass b0 should equal b2");
        assert!((coeffs.b1 - (-2.0 * coeffs.b0)).abs() < EPS, "Highpass b1 should equal -2*b0");
    }

    #[test]
    fn highpass_blocks_dc() {
        let mut coeffs = BiquadCoeffs::default();
        compute_highpass_coeffs(&mut coeffs, 1000.0, 0.707, SAMPLE_RATE);
        let mut state = BiquadState::default();
        let mut y = 0.0;
        for _ in 0..10000 {
            y = process_biquad(1.0, &coeffs, &mut state);
        }
        assert!(y.abs() < 0.001, "Highpass should block DC, got {}", y);
    }

    #[test]
    fn highpass_passes_nyquist() {
        let mut coeffs = BiquadCoeffs::default();
        compute_highpass_coeffs(&mut coeffs, 100.0, 0.707, SAMPLE_RATE);
        let mut state = BiquadState::default();
        let mut y = 0.0;
        for i in 0..10000 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            y = process_biquad(x, &coeffs, &mut state);
        }
        assert!(y.abs() > 0.9, "Highpass should pass Nyquist, got {}", y);
    }
}
