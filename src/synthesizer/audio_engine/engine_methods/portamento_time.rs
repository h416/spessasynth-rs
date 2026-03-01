/// portamento_time.rs
/// purpose: Converts MIDI portamento time (CC#5) to seconds using a PCHIP cubic spline.
/// Ported from: src/synthesizer/audio_engine/engine_methods/portamento_time.ts
///
/// Reference measurements by John Novak:
/// https://github.com/dosbox-staging/dosbox-staging/pull/2705
/// PCHIP function by Benjamin Rosseaux (Sobanth SF2 synthesizer).
const PORTA_DIVISION_CONSTANT: f64 = 40.0;

/// Left endpoints of each spline segment.
const X0: [f64; 12] = [
    1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 80.0, 96.0, 112.0, 120.0, 124.0,
];

/// Reciprocals of segment widths (1 / (x0[i+1] - x0[i])).
const IH: [f64; 12] = [
    1.0,
    0.5,
    0.25,
    0.125,
    0.0625,
    0.031_25,
    0.0625,
    0.0625,
    0.0625,
    0.125,
    0.25,
    1.0 / 3.0,
];

/// PCHIP cubic coefficients – degree-3 term.
const A: [f64; 12] = [
    -0.166_531_273_825_012_15,
    0.118_638_752_182_994_08,
    0.029_479_047_361_245_264,
    -0.005_442_312_089_231_738,
    0.145_152_087_597_303_7,
    -0.005_056_281_449_558_275,
    -0.005_095_486_882_876_532,
    0.033_340_095_511_115_44,
    -0.093_613_686_780_204_32,
    0.141_325_697_024_518_22,
    -0.158_055_653_010_113_82,
    -0.099_188_569_558_819_27,
];

/// PCHIP cubic coefficients – degree-2 term.
const B: [f64; 12] = [
    0.028_212_773_333_433_472,
    -0.338_850_206_499_284_7,
    -0.158_395_298_909_297_13,
    -0.123_981_317_667_754_83,
    -0.287_484_855_268_511_1,
    0.012_254_866_302_537_692,
    0.005_957_797_193_345_771,
    -0.037_458_993_303_473_74,
    0.129_117_818_698_101_96,
    -0.158_671_932_241_625_68,
    0.504_406_322_732_748,
    0.378_684_513_187_545_8,
];

/// PCHIP cubic coefficients – degree-1 term.
const C: [f64; 12] = [
    0.721_895_086_125_528_3,
    0.557_453_622_634_716_8,
    0.471_338_932_370_258_26,
    0.485_970_953_270_799_14,
    0.443_362_763_335_188_54,
    0.607_698_631_180_155_1,
    0.308_519_759_718_277_94,
    0.305_148_893_456_339_55,
    0.330_251_193_382_738_4,
    0.153_822_885_219_165,
    0.130_228_055_904_733_7,
    0.498_655_306_754_916_87,
];

/// PCHIP cubic coefficients – degree-0 term (log10 of rate at left endpoint).
const D: [f64; 12] = [
    -2.221_848_749_616_356_6,
    -1.638_272_163_982_407_2,
    -1.301_029_995_663_981_3,
    -0.958_607_314_841_775,
    -0.602_059_991_327_962_4,
    -std::f64::consts::LOG10_2,
    0.313_867_220_369_153_43,
    0.623_249_290_397_900_4,
    0.924_279_286_061_881_7,
    1.290_034_611_362_518,
    1.426_511_261_364_575_2,
    1.903_089_986_991_943_5,
];

/// Thresholds used to select the spline segment.
/// Equivalent to: thresholds = [2, 4, 8, 16, 32, 64, 80, 96, 112, 120, 124]
const THRESHOLDS: [u8; 11] = [2, 4, 8, 16, 32, 64, 80, 96, 112, 120, 124];

/// Returns the portamento rate (seconds per semitone) for a given CC#5 value.
/// Equivalent to: portaTimeToRate (private)
fn porta_time_to_rate(cc: u8) -> f64 {
    if cc < 1 {
        return 0.0;
    }

    // Find the spline segment index.
    // Equivalent to: thresholds.findLastIndex(t => t < cc) + 1
    let s = THRESHOLDS
        .iter()
        .enumerate()
        .rev()
        .find(|&(_, &t)| t < cc)
        .map(|(i, _)| i + 1)
        .unwrap_or(0);

    // Normalised position within the segment [0, 1].
    let t = (cc as f64 - X0[s]) * IH[s];

    // Evaluate the cubic polynomial in log10 space, then exponentiate.
    // 2.302_585_092_994_046 = ln(10), so exp(ln(10) * x) = 10^x.
    (std::f64::consts::LN_10 * (((A[s] * t + B[s]) * t + C[s]) * t + D[s])).exp()
        / PORTA_DIVISION_CONSTANT
}

/// Converts portamento time to seconds.
/// `time`: MIDI CC#5 value (0–127, integer).
/// `distance`: pitch distance in semitones to slide over.
/// Equivalent to: portamentoTimeToSeconds
pub fn portamento_time_to_seconds(time: u8, distance: f64) -> f64 {
    porta_time_to_rate(time) * distance
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- cc = 0: rate is 0 ---

    #[test]
    fn test_cc_zero_any_distance_is_zero() {
        assert_eq!(portamento_time_to_seconds(0, 1.0), 0.0);
        assert_eq!(portamento_time_to_seconds(0, 100.0), 0.0);
    }

    // --- reference values at knot endpoints ---
    // At knot left-endpoints (t=0), the spline evaluates to 10^(D[s]) / 40.
    // At cc=1 (s=0, t=0):

    #[test]
    fn test_cc_1_matches_d0() {
        let expected = (2.302_585_092_994_046 * D[0]).exp() / PORTA_DIVISION_CONSTANT;
        let got = portamento_time_to_seconds(1, 1.0);
        assert!(
            (got - expected).abs() < 1e-12,
            "got={got}, expected={expected}"
        );
    }

    // At knot right-endpoints (t=1), poly = A+B+C+D.
    // cc=32 is the right end of segment s=4:

    #[test]
    fn test_cc_32_matches_poly_at_t1() {
        let poly = A[4] + B[4] + C[4] + D[4];
        let expected = (2.302_585_092_994_046 * poly).exp() / PORTA_DIVISION_CONSTANT;
        let got = portamento_time_to_seconds(32, 1.0);
        assert!(
            (got - expected).abs() < 1e-12,
            "got={got}, expected={expected}"
        );
    }

    // --- approximate agreement with the reference table (tolerance 1%) ---
    // The table lists portaTimeToRate(cc) * 40 ≈ table_value.
    // So portamentoTimeToSeconds(cc, 40.0) ≈ table_value.

    #[test]
    fn test_approx_table_cc1() {
        // table: 0.006 s
        assert_close(portamento_time_to_seconds(1, 40.0), 0.006, 0.01);
    }

    #[test]
    fn test_approx_table_cc8() {
        // table: 0.110 s
        assert_close(portamento_time_to_seconds(8, 40.0), 0.110, 0.01);
    }

    #[test]
    fn test_approx_table_cc16() {
        // table: 0.250 s
        assert_close(portamento_time_to_seconds(16, 40.0), 0.250, 0.01);
    }

    #[test]
    fn test_approx_table_cc32() {
        // table: 0.500 s (knot — exact)
        assert_close(portamento_time_to_seconds(32, 40.0), 0.500, 1e-4);
    }

    #[test]
    fn test_approx_table_cc64() {
        // table: 2.060 s
        assert_close(portamento_time_to_seconds(64, 40.0), 2.060, 0.01);
    }

    #[test]
    fn test_approx_table_cc96() {
        // table: 8.400 s
        assert_close(portamento_time_to_seconds(96, 40.0), 8.400, 0.01);
    }

    #[test]
    fn test_approx_table_cc127() {
        // table: 480 s
        assert_close(portamento_time_to_seconds(127, 40.0), 480.0, 0.01);
    }

    // --- distance proportionality ---

    #[test]
    fn test_proportional_to_distance() {
        for cc in [1u8, 16, 32, 64, 96, 127] {
            let t1 = portamento_time_to_seconds(cc, 1.0);
            let t2 = portamento_time_to_seconds(cc, 3.0);
            assert!((t2 - 3.0 * t1).abs() < 1e-12, "cc={cc}: t1={t1}, t2={t2}");
        }
    }

    // --- monotonically non-decreasing across all 128 CC values ---

    #[test]
    fn test_monotonically_non_decreasing() {
        let mut prev = 0.0f64;
        for cc in 0u8..=127 {
            let rate = portamento_time_to_seconds(cc, 1.0);
            assert!(
                rate >= prev - 1e-15,
                "rate decreased at cc={cc}: {prev} → {rate}"
            );
            prev = rate;
        }
    }

    // --- all outputs are non-negative ---

    #[test]
    fn test_all_non_negative() {
        for cc in 0u8..=127 {
            let rate = portamento_time_to_seconds(cc, 1.0);
            assert!(rate >= 0.0, "negative rate at cc={cc}: {rate}");
        }
    }

    // --- segment boundary C0 continuity ---
    // PCHIP guarantees: A[i]+B[i]+C[i]+D[i] == D[i+1] at each internal knot.
    // We verify the stored coefficients satisfy this property.

    #[test]
    fn test_spline_c0_continuity_at_knots() {
        for i in 0..11 {
            let left_val = A[i] + B[i] + C[i] + D[i];
            let right_val = D[i + 1];
            assert!(
                (left_val - right_val).abs() < 1e-10,
                "C0 discontinuity at knot {i}: left={left_val:.15}, right={right_val:.15}"
            );
        }
    }

    // Helper: assert relative closeness.
    fn assert_close(got: f64, expected: f64, rel_tol: f64) {
        let err = (got - expected).abs() / expected.abs().max(1e-15);
        assert!(
            err <= rel_tol,
            "got={got}, expected={expected}, relative_error={err:.4}"
        );
    }
}
