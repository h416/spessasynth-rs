//! The modulator source curves. The built-in curves precompute one table per polarity and direction,
//! so the synthesizer only reads a table when computing a modulator. Custom curves are evaluated from their points.
//! Ported from: src/soundbank/basic_soundbank/modulator_curve.ts
//!
//! TS keeps a `ModulatorCurve` object per source (`LinearCurve`, `ConcaveCurve`, `ConvexCurve`, `SwitchCurve`,
//! `UnresolvedCurve` and `CustomCurve`). Rust keeps the curve type on the source and the built-in tables here;
//! a custom curve is held by the source itself (see `ModulatorSource::curve`), and any other type outputs 0
//! like `UnresolvedCurve`.
use std::sync::LazyLock;

use crate::soundbank::enums::{ModulatorCurveType, modulator_curve_types};

/// The source value range, 0 to 16384 (SFe 4.0, section 7.2).
/// The precomputed tables have one more entry, so 16384 is a real index.
/// Equivalent to: MODULATOR_RESOLUTION
pub const MODULATOR_RESOLUTION: usize = 16_384;

/// Equivalent to: Math.round (half-up)
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// The source value of a 7-bit value (velocity, key, pressures, controllers), center-max (SFe 4.0, section 7.2).
/// 64 is exactly the center (8192) and 127 exactly the end (16384).
/// Equivalent to: SOURCE_VALUE_7_BIT (Uint16Array of 128)
pub static SOURCE_VALUE_7_BIT: LazyLock<[u16; 128]> = LazyLock::new(|| {
    let mut t = [0u16; 128];
    for (v, e) in t.iter_mut().enumerate() {
        // v <= 64 ? v * 128 : Math.round(8192 + (8192 * (v - 64)) / 63)
        *e = if v <= 64 {
            (v * 128) as u16
        } else {
            js_round(8192.0 + (8192.0 * (v as f64 - 64.0)) / 63.0) as u16
        };
    }
    t
});

/// The source value of a 14-bit value (the pitch wheel and the computed ranges), center-max (SFe 4.0, section 7.2).
/// 8192 stays the center and 16383 is exactly the end (16384).
/// Equivalent to: SOURCE_VALUE_14_BIT (Uint16Array of 16384)
pub static SOURCE_VALUE_14_BIT: LazyLock<Vec<u16>> = LazyLock::new(|| {
    (0..16_384usize)
        .map(|w| {
            // w <= 8192 ? w : Math.round(8192 + (8192 * (w - 8192)) / 8191)
            if w <= 8192 {
                w as u16
            } else {
                js_round(8192.0 + (8192.0 * (w as f64 - 8192.0)) / 8191.0) as u16
            }
        })
        .collect()
});

/// Precalculate lookup tables for concave and convex curves.
/// Equivalent to: const concave / const convex (Float32Array of MODULATOR_RESOLUTION + 1)
static CURVES: LazyLock<(Vec<f32>, Vec<f32>)> = LazyLock::new(|| {
    let len = MODULATOR_RESOLUTION + 1;
    let mut concave = vec![0.0f32; len];
    let mut convex = vec![0.0f32; len];
    // The equation is taken from FluidSynth as it's the standard for soundFonts
    // More precisely, the gen_conv.c file
    concave[0] = 0.0;
    concave[len - 1] = 1.0;

    convex[0] = 0.0;
    convex[len - 1] = 1.0;
    for i in 1..MODULATOR_RESOLUTION {
        // The formula was made for a 128-entry table.
        // At this resolution it goes above 1 near the ends, so clamp it.
        let x = ((((-200.0_f64 * 2.0) / 960.0) * (i as f64 / (len - 1) as f64).ln())
            / std::f64::consts::LN_10)
            .min(1.0);
        convex[i] = (1.0 - x) as f32;
        concave[len - 1 - i] = x as f32;
    }
    (concave, convex)
});

/// Computes the output of a built-in curve.
/// `value` is the source value, 0 to 1, already reversed for the negative direction.
/// Equivalent to: evaluate(value, isBipolar) of LinearCurve, ConcaveCurve, ConvexCurve and SwitchCurve
fn evaluate(curve_type: ModulatorCurveType, mut value: f64, is_bipolar: bool) -> f64 {
    match curve_type {
        modulator_curve_types::LINEAR => {
            if is_bipolar {
                value * 2.0 - 1.0
            } else {
                value
            }
        }
        modulator_curve_types::CONCAVE | modulator_curve_types::CONVEX => {
            let table = if curve_type == modulator_curve_types::CONCAVE {
                &CURVES.0
            } else {
                &CURVES.1
            };
            if is_bipolar {
                value = value * 2.0 - 1.0;
                if value < 0.0 {
                    return -(table[(value * -(MODULATOR_RESOLUTION as f64)).trunc() as usize] as f64);
                }
            }
            table[(value * MODULATOR_RESOLUTION as f64).trunc() as usize] as f64
        }
        modulator_curve_types::SWITCH => {
            value = if value >= 0.5 { 1.0 } else { 0.0 };
            if is_bipolar { value * 2.0 - 1.0 } else { value }
        }
        _ => 0.0,
    }
}

/// Computes the four tables of a built-in curve, one per transform (0bPD: polarity, direction),
/// indexed by the source value (0 to 16,384).
/// Equivalent to: buildTables()
fn build_tables(curve_type: ModulatorCurveType) -> [Vec<f32>; 4] {
    std::array::from_fn(|transform| {
        let is_bipolar = (transform & 0b10) != 0;
        let is_negative = (transform & 1) != 0;
        let mut table = vec![0.0f32; MODULATOR_RESOLUTION + 1];
        for (v, entry) in table.iter_mut().enumerate() {
            let value = if curve_type == modulator_curve_types::SWITCH {
                // SF2 switches on after half of the maximum, so the center (8192, CC value 64) is already on.
                // The negative direction is the exact opposite, off from the center up
                let on = (v >= MODULATOR_RESOLUTION / 2) != is_negative;
                if on { 1.0 } else { 0.0 }
            } else {
                let value = v as f64 / MODULATOR_RESOLUTION as f64;
                if is_negative { 1.0 - value } else { value }
            };
            *entry = evaluate(curve_type, value, is_bipolar) as f32;
        }
        table
    })
}

/// The built-in curves, indexed by the curve type.
/// Equivalent to: BUILT_IN_CURVES
static BUILT_IN_CURVES: LazyLock<[[Vec<f32>; 4]; 4]> = LazyLock::new(|| {
    [
        build_tables(modulator_curve_types::LINEAR),
        build_tables(modulator_curve_types::CONCAVE),
        build_tables(modulator_curve_types::CONVEX),
        build_tables(modulator_curve_types::SWITCH),
    ]
});

/// The precomputed table of a built-in curve for a transform (0bPD), or `None` for any other type
/// (a custom curve, or a type this version does not know).
/// Equivalent to: `ModulatorCurve.fromType(type).tables[transform]`
pub fn built_in_table(curve_type: ModulatorCurveType, transform: usize) -> Option<&'static [f32]> {
    BUILT_IN_CURVES
        .get(curve_type as usize)
        .map(|tables| tables[transform].as_slice())
}

/// The name of a curve type.
/// Equivalent to: ModulatorCurve.toString()
pub fn curve_type_name(curve_type: ModulatorCurveType) -> String {
    match curve_type {
        modulator_curve_types::LINEAR => "linear".to_string(),
        modulator_curve_types::CONCAVE => "concave".to_string(),
        modulator_curve_types::CONVEX => "convex".to_string(),
        modulator_curve_types::SWITCH => "switch".to_string(),
        modulator_curve_types::CUSTOM => "custom".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::enums::modulator_curve_types as mct;

    fn at(curve: ModulatorCurveType, transform: usize, v: usize) -> f32 {
        built_in_table(curve, transform).unwrap()[v]
    }

    #[test]
    fn linear_tables() {
        assert_eq!(at(mct::LINEAR, 0, 0), 0.0);
        assert_eq!(at(mct::LINEAR, 0, 8192), 0.5);
        assert_eq!(at(mct::LINEAR, 2, 8192), 0.0);
        assert_eq!(at(mct::LINEAR, 1, 0), 1.0);
    }

    #[test]
    fn switch_is_on_from_the_center() {
        assert_eq!(at(mct::SWITCH, 0, 8191), 0.0);
        assert_eq!(at(mct::SWITCH, 0, 8192), 1.0);
        assert_eq!(at(mct::SWITCH, 1, 8191), 1.0);
        assert_eq!(at(mct::SWITCH, 1, 8192), 0.0);
        assert_eq!(at(mct::SWITCH, 2, 8191), -1.0);
        assert_eq!(at(mct::SWITCH, 3, 8192), -1.0);
    }

    #[test]
    fn concave_bipolar_is_odd() {
        let lo = at(mct::CONCAVE, 2, 4096);
        let hi = at(mct::CONCAVE, 2, 16_384 - 4096);
        assert!((lo + hi).abs() < 1e-6);
    }

    #[test]
    fn unknown_types_have_no_table() {
        assert!(built_in_table(mct::CUSTOM, 0).is_none());
        assert!(built_in_table(17, 0).is_none());
    }
}
