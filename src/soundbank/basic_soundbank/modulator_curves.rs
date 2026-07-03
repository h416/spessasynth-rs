/// modulator_curves.rs
/// purpose: Precomputes modulator concave and convex curves and calculates
///          a curve value for a given polarity, direction and type.
/// Ported from: src/synthesizer/audio_engine/engine_components/modulator_curves.ts
use std::sync::LazyLock;

use crate::soundbank::enums::{ModulatorCurveType, modulator_curve_types};

/// The length of the precomputed curve tables.
/// Equivalent to: MODULATOR_RESOLUTION
pub const MODULATOR_RESOLUTION: usize = 16_384;

/// Number of distinct modulator curve types (linear, concave, convex, switch).
/// Equivalent to: MOD_CURVE_TYPES_AMOUNT
pub const MOD_CURVE_TYPES_AMOUNT: usize = 4;

/// Number of source-transform possibilities:
///   unipolar positive, unipolar negative, bipolar positive, bipolar negative.
/// Equivalent to: MOD_SOURCE_TRANSFORM_POSSIBILITIES
pub const MOD_SOURCE_TRANSFORM_POSSIBILITIES: usize = 4;

// ---------------------------------------------------------------------------
// Precomputed lookup tables
// The equation is taken from FluidSynth (gen_conv.c) as the SoundFont standard.
// ---------------------------------------------------------------------------

static CONCAVE: LazyLock<Vec<f32>> = LazyLock::new(|| {
    let len = MODULATOR_RESOLUTION + 1; // 16_385 entries
    let mut concave = vec![0.0f32; len];
    concave[len - 1] = 1.0;
    for i in 1..(MODULATOR_RESOLUTION - 1) {
        // x = ((-200*2) / 960) * log10(i / (len - 1))
        let x = ((-400.0_f64) / 960.0) * (i as f64 / (len - 1) as f64).log10();
        concave[len - 1 - i] = x as f32;
    }
    concave
});

static CONVEX: LazyLock<Vec<f32>> = LazyLock::new(|| {
    let len = MODULATOR_RESOLUTION + 1; // 16_385 entries
    let mut convex = vec![0.0f32; len];
    convex[len - 1] = 1.0;
    for (i, val) in convex.iter_mut().enumerate().skip(1).take(MODULATOR_RESOLUTION - 2) {
        let x = ((-400.0_f64) / 960.0) * (i as f64 / (len - 1) as f64).log10();
        *val = (1.0 - x) as f32;
    }
    convex
});

/// Transforms a value with a given curve type.
///
/// * `transform_type` – the bipolar and negative flags as a 2-bit number:
///   `0bPD` (polarity MSB, direction LSB).
/// * `curve_type` – enumeration of curve types (see `modulator_curve_types`).
/// * `value` – the linear input value, in the range 0.0 to 1.0.
///
/// Returns the transformed value (0.0 to 1.0, or -1.0 to 1.0 for bipolar).
/// Equivalent to: getModulatorCurveValue
pub fn get_modulator_curve_value(
    transform_type: u8,
    curve_type: ModulatorCurveType,
    mut value: f64,
) -> f64 {
    let is_bipolar = (transform_type & 0b10) != 0;
    let is_negative = (transform_type & 0b01) != 0;

    // Inverse the value if needed
    if is_negative {
        value = 1.0 - value;
    }

    match curve_type {
        modulator_curve_types::LINEAR => {
            if is_bipolar {
                // Bipolar curve
                value * 2.0 - 1.0
            } else {
                value
            }
        }

        modulator_curve_types::SWITCH => {
            value = if value > 0.5 { 1.0 } else { 0.0 };
            if is_bipolar { value * 2.0 - 1.0 } else { value }
        }

        modulator_curve_types::CONCAVE => {
            if is_bipolar {
                value = value * 2.0 - 1.0;
                if value < 0.0 {
                    -(CONCAVE[(-value * MODULATOR_RESOLUTION as f64) as usize] as f64)
                } else {
                    CONCAVE[(value * MODULATOR_RESOLUTION as f64) as usize] as f64
                }
            } else {
                CONCAVE[(value * MODULATOR_RESOLUTION as f64) as usize] as f64
            }
        }

        modulator_curve_types::CONVEX => {
            if is_bipolar {
                value = value * 2.0 - 1.0;
                if value < 0.0 {
                    -(CONVEX[(-value * MODULATOR_RESOLUTION as f64) as usize] as f64)
                } else {
                    CONVEX[(value * MODULATOR_RESOLUTION as f64) as usize] as f64
                }
            } else {
                CONVEX[(value * MODULATOR_RESOLUTION as f64) as usize] as f64
            }
        }

        _ => value,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::enums::modulator_curve_types as mct;

    // Helper: transform_type bits
    // 0b00 = unipolar positive, 0b01 = unipolar negative,
    // 0b10 = bipolar positive, 0b11 = bipolar negative
    const UNIPOLAR_POS: u8 = 0b00;
    const UNIPOLAR_NEG: u8 = 0b01;
    const BIPOLAR_POS: u8 = 0b10;
    const BIPOLAR_NEG: u8 = 0b11;

    const EPS: f64 = 1e-4;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    // --- Constants ---

    #[test]
    fn test_modulator_resolution() {
        assert_eq!(MODULATOR_RESOLUTION, 16_384);
    }

    #[test]
    fn test_mod_curve_types_amount() {
        assert_eq!(MOD_CURVE_TYPES_AMOUNT, 4);
    }

    #[test]
    fn test_mod_source_transform_possibilities() {
        assert_eq!(MOD_SOURCE_TRANSFORM_POSSIBILITIES, 4);
    }

    // --- Linear unipolar positive ---

    #[test]
    fn test_linear_unipolar_pos_zero() {
        assert!(approx_eq(
            get_modulator_curve_value(UNIPOLAR_POS, mct::LINEAR, 0.0),
            0.0
        ));
    }

    #[test]
    fn test_linear_unipolar_pos_half() {
        assert!(approx_eq(
            get_modulator_curve_value(UNIPOLAR_POS, mct::LINEAR, 0.5),
            0.5
        ));
    }

    #[test]
    fn test_linear_unipolar_pos_one() {
        assert!(approx_eq(
            get_modulator_curve_value(UNIPOLAR_POS, mct::LINEAR, 1.0),
            1.0
        ));
    }

    // --- Linear unipolar negative (value is inverted first) ---

    #[test]
    fn test_linear_unipolar_neg_zero() {
        // 1 - 0 = 1
        assert!(approx_eq(
            get_modulator_curve_value(UNIPOLAR_NEG, mct::LINEAR, 0.0),
            1.0
        ));
    }

    #[test]
    fn test_linear_unipolar_neg_one() {
        // 1 - 1 = 0
        assert!(approx_eq(
            get_modulator_curve_value(UNIPOLAR_NEG, mct::LINEAR, 1.0),
            0.0
        ));
    }

    #[test]
    fn test_linear_unipolar_neg_half() {
        // 1 - 0.5 = 0.5
        assert!(approx_eq(
            get_modulator_curve_value(UNIPOLAR_NEG, mct::LINEAR, 0.5),
            0.5
        ));
    }

    // --- Linear bipolar positive ---

    #[test]
    fn test_linear_bipolar_pos_zero() {
        // 0 * 2 - 1 = -1
        assert!(approx_eq(
            get_modulator_curve_value(BIPOLAR_POS, mct::LINEAR, 0.0),
            -1.0
        ));
    }

    #[test]
    fn test_linear_bipolar_pos_half() {
        // 0.5 * 2 - 1 = 0
        assert!(approx_eq(
            get_modulator_curve_value(BIPOLAR_POS, mct::LINEAR, 0.5),
            0.0
        ));
    }

    #[test]
    fn test_linear_bipolar_pos_one() {
        // 1 * 2 - 1 = 1
        assert!(approx_eq(
            get_modulator_curve_value(BIPOLAR_POS, mct::LINEAR, 1.0),
            1.0
        ));
    }

    // --- Linear bipolar negative ---

    #[test]
    fn test_linear_bipolar_neg_zero() {
        // invert: 1 - 0 = 1; bipolar: 1 * 2 - 1 = 1
        assert!(approx_eq(
            get_modulator_curve_value(BIPOLAR_NEG, mct::LINEAR, 0.0),
            1.0
        ));
    }

    #[test]
    fn test_linear_bipolar_neg_one() {
        // invert: 1 - 1 = 0; bipolar: 0 * 2 - 1 = -1
        assert!(approx_eq(
            get_modulator_curve_value(BIPOLAR_NEG, mct::LINEAR, 1.0),
            -1.0
        ));
    }

    // --- Switch unipolar positive ---

    #[test]
    fn test_switch_unipolar_pos_below_threshold() {
        // value = 0.4 <= 0.5 → 0
        assert!(approx_eq(
            get_modulator_curve_value(UNIPOLAR_POS, mct::SWITCH, 0.4),
            0.0
        ));
    }

    #[test]
    fn test_switch_unipolar_pos_exactly_half() {
        // value = 0.5, NOT > 0.5, so → 0
        assert!(approx_eq(
            get_modulator_curve_value(UNIPOLAR_POS, mct::SWITCH, 0.5),
            0.0
        ));
    }

    #[test]
    fn test_switch_unipolar_pos_above_threshold() {
        // value = 0.6 > 0.5 → 1
        assert!(approx_eq(
            get_modulator_curve_value(UNIPOLAR_POS, mct::SWITCH, 0.6),
            1.0
        ));
    }

    #[test]
    fn test_switch_unipolar_pos_one() {
        assert!(approx_eq(
            get_modulator_curve_value(UNIPOLAR_POS, mct::SWITCH, 1.0),
            1.0
        ));
    }

    // --- Switch bipolar positive ---

    #[test]
    fn test_switch_bipolar_pos_below_threshold() {
        // switch → 0; bipolar: 0 * 2 - 1 = -1
        assert!(approx_eq(
            get_modulator_curve_value(BIPOLAR_POS, mct::SWITCH, 0.3),
            -1.0
        ));
    }

    #[test]
    fn test_switch_bipolar_pos_above_threshold() {
        // switch → 1; bipolar: 1 * 2 - 1 = 1
        assert!(approx_eq(
            get_modulator_curve_value(BIPOLAR_POS, mct::SWITCH, 0.7),
            1.0
        ));
    }

    // --- Concave unipolar positive ---

    #[test]
    fn test_concave_unipolar_pos_zero() {
        // concave[0] = 0
        assert!(approx_eq(
            get_modulator_curve_value(UNIPOLAR_POS, mct::CONCAVE, 0.0),
            0.0
        ));
    }

    #[test]
    fn test_concave_unipolar_pos_one() {
        // concave[16384] = 1
        assert!(approx_eq(
            get_modulator_curve_value(UNIPOLAR_POS, mct::CONCAVE, 1.0),
            1.0
        ));
    }

    #[test]
    fn test_concave_unipolar_pos_monotone() {
        // The concave curve should be monotonically increasing
        let v1 = get_modulator_curve_value(UNIPOLAR_POS, mct::CONCAVE, 0.3);
        let v2 = get_modulator_curve_value(UNIPOLAR_POS, mct::CONCAVE, 0.7);
        assert!(v1 < v2);
    }

    #[test]
    fn test_concave_unipolar_pos_midpoint_below_half() {
        // Concave curve: at 0.5 input, output should be less than 0.5 (curves down)
        let v = get_modulator_curve_value(UNIPOLAR_POS, mct::CONCAVE, 0.5);
        assert!(v < 0.5);
    }

    // --- Convex unipolar positive ---

    #[test]
    fn test_convex_unipolar_pos_zero() {
        // convex[0] = 0
        assert!(approx_eq(
            get_modulator_curve_value(UNIPOLAR_POS, mct::CONVEX, 0.0),
            0.0
        ));
    }

    #[test]
    fn test_convex_unipolar_pos_one() {
        // convex[16384] = 1
        assert!(approx_eq(
            get_modulator_curve_value(UNIPOLAR_POS, mct::CONVEX, 1.0),
            1.0
        ));
    }

    #[test]
    fn test_convex_unipolar_pos_monotone() {
        // The convex curve should be monotonically increasing
        let v1 = get_modulator_curve_value(UNIPOLAR_POS, mct::CONVEX, 0.3);
        let v2 = get_modulator_curve_value(UNIPOLAR_POS, mct::CONVEX, 0.7);
        assert!(v1 < v2);
    }

    #[test]
    fn test_convex_unipolar_pos_midpoint_above_half() {
        // Convex curve: at 0.5 input, output should be greater than 0.5 (curves up)
        let v = get_modulator_curve_value(UNIPOLAR_POS, mct::CONVEX, 0.5);
        assert!(v > 0.5);
    }

    // --- Concave vs Convex symmetry ---

    #[test]
    fn test_concave_convex_complementary() {
        // concave(x) + convex(x) ≈ 1 for the same input
        // (by construction: convex[i] = 1 - x, concave[len-1-i] = x)
        // At any given sampled point both curves are built from the same x value,
        // so concave(v) + convex(1 - v) should be approximately 1.
        let v = 0.3_f64;
        let cv = get_modulator_curve_value(UNIPOLAR_POS, mct::CONCAVE, v);
        let cc = get_modulator_curve_value(UNIPOLAR_POS, mct::CONVEX, 1.0 - v);
        assert!(approx_eq(cv + cc, 1.0));
    }

    // --- Concave bipolar positive ---

    #[test]
    fn test_concave_bipolar_pos_zero() {
        // value=0 → bipolar: 0*2-1 = -1 → negative branch → -concave[16384] = -1
        assert!(approx_eq(
            get_modulator_curve_value(BIPOLAR_POS, mct::CONCAVE, 0.0),
            -1.0
        ));
    }

    #[test]
    fn test_concave_bipolar_pos_one() {
        // value=1 → bipolar: 1*2-1 = 1 → positive branch → concave[16384] = 1
        assert!(approx_eq(
            get_modulator_curve_value(BIPOLAR_POS, mct::CONCAVE, 1.0),
            1.0
        ));
    }

    // --- Convex bipolar positive ---

    #[test]
    fn test_convex_bipolar_pos_zero() {
        // value=0 → bipolar: -1 → negative branch → -convex[16384] = -1
        assert!(approx_eq(
            get_modulator_curve_value(BIPOLAR_POS, mct::CONVEX, 0.0),
            -1.0
        ));
    }

    #[test]
    fn test_convex_bipolar_pos_one() {
        // value=1 → bipolar: 1 → positive branch → convex[16384] = 1
        assert!(approx_eq(
            get_modulator_curve_value(BIPOLAR_POS, mct::CONVEX, 1.0),
            1.0
        ));
    }

    // --- Table boundary: no panic ---

    #[test]
    fn test_no_panic_concave_boundary() {
        // Ensure no out-of-bounds panic at table boundaries
        let _ = get_modulator_curve_value(UNIPOLAR_POS, mct::CONCAVE, 0.0);
        let _ = get_modulator_curve_value(UNIPOLAR_POS, mct::CONCAVE, 1.0);
        let _ = get_modulator_curve_value(BIPOLAR_POS, mct::CONCAVE, 0.0);
        let _ = get_modulator_curve_value(BIPOLAR_POS, mct::CONCAVE, 1.0);
    }

    #[test]
    fn test_no_panic_convex_boundary() {
        let _ = get_modulator_curve_value(UNIPOLAR_POS, mct::CONVEX, 0.0);
        let _ = get_modulator_curve_value(UNIPOLAR_POS, mct::CONVEX, 1.0);
        let _ = get_modulator_curve_value(BIPOLAR_POS, mct::CONVEX, 0.0);
        let _ = get_modulator_curve_value(BIPOLAR_POS, mct::CONVEX, 1.0);
    }

    // --- Unknown curve type falls through to identity ---

    #[test]
    fn test_unknown_curve_type_returns_value() {
        // curve_type=255 is not a known type → should return value unchanged
        let v = get_modulator_curve_value(UNIPOLAR_POS, 255, 0.42);
        assert!(approx_eq(v, 0.42));
    }
}
