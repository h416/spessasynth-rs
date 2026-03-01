/// lfo.rs
/// purpose: Low-frequency triangle oscillator.
/// Ported from: src/synthesizer/audio_engine/engine_components/dsp_chain/lfo.ts
///
/// Gets the current value of the LFO at a given time.
/// The output is a triangle wave oscillating between -1.0 and 1.0.
/// Returns 0.0 if current_time is before start_time.
/// Equivalent to: getLFOValue
pub fn get_lfo_value(start_time: f64, frequency: f64, current_time: f64) -> f64 {
    if current_time < start_time {
        return 0.0;
    }

    // xVal = elapsed cycles + 0.25 phase offset.
    // The +0.25 ensures the LFO starts at 0 (not -1) when currentTime == startTime.
    let x_val = (current_time - start_time) * frequency + 0.25;

    // Triangle wave formula. Equivalent to: Math.abs(xVal - ~~(xVal + 0.5)) * 4 - 1
    // ~~(x) in JS truncates toward zero, identical to f64::trunc() in Rust.
    let trunc = (x_val + 0.5).trunc();
    (x_val - trunc).abs() * 4.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    // --- before start_time ---

    #[test]
    fn test_before_start_returns_zero() {
        assert_eq!(get_lfo_value(1.0, 1.0, 0.5), 0.0);
    }

    #[test]
    fn test_well_before_start_returns_zero() {
        assert_eq!(get_lfo_value(100.0, 2.0, 0.0), 0.0);
    }

    // --- at start_time: output must be 0 ---

    #[test]
    fn test_at_start_is_zero_freq1() {
        assert!(approx(get_lfo_value(0.0, 1.0, 0.0), 0.0));
    }

    #[test]
    fn test_at_start_is_zero_freq2() {
        assert!(approx(get_lfo_value(0.0, 2.0, 0.0), 0.0));
    }

    #[test]
    fn test_at_start_nonzero_start_time() {
        assert!(approx(get_lfo_value(5.0, 1.0, 5.0), 0.0));
    }

    // --- quarter period: output must be +1 (peak) ---

    #[test]
    fn test_quarter_period_is_peak_freq1() {
        // freq=1 Hz, period=1s, quarter=0.25s
        assert!(approx(get_lfo_value(0.0, 1.0, 0.25), 1.0));
    }

    #[test]
    fn test_quarter_period_is_peak_freq2() {
        // freq=2 Hz, period=0.5s, quarter=0.125s
        assert!(approx(get_lfo_value(0.0, 2.0, 0.125), 1.0));
    }

    // --- half period: output must be 0 ---

    #[test]
    fn test_half_period_is_zero_freq1() {
        assert!(approx(get_lfo_value(0.0, 1.0, 0.5), 0.0));
    }

    #[test]
    fn test_half_period_is_zero_freq4() {
        // freq=4 Hz, half period = 0.125s
        assert!(approx(get_lfo_value(0.0, 4.0, 0.125), 0.0));
    }

    // --- three-quarter period: output must be -1 (trough) ---

    #[test]
    fn test_three_quarter_period_is_trough_freq1() {
        assert!(approx(get_lfo_value(0.0, 1.0, 0.75), -1.0));
    }

    #[test]
    fn test_three_quarter_period_is_trough_freq2() {
        assert!(approx(get_lfo_value(0.0, 2.0, 0.375), -1.0));
    }

    // --- full period: back to 0 ---

    #[test]
    fn test_full_period_is_zero_freq1() {
        assert!(approx(get_lfo_value(0.0, 1.0, 1.0), 0.0));
    }

    #[test]
    fn test_full_period_is_zero_freq2() {
        assert!(approx(get_lfo_value(0.0, 2.0, 0.5), 0.0));
    }

    // --- output is always in [-1, 1] ---

    #[test]
    fn test_output_in_range() {
        for i in 0..1000 {
            let t = i as f64 * 0.001;
            let v = get_lfo_value(0.0, 3.7, t);
            assert!(v >= -1.0 && v <= 1.0, "out of range at t={t}: {v}");
        }
    }

    // --- nonzero start_time shifts the wave correctly ---

    #[test]
    fn test_nonzero_start_time_matches_zero_start() {
        let offset = 42.0;
        let freq = 1.5;
        for i in 0..100 {
            let dt = i as f64 * 0.01;
            let v0 = get_lfo_value(0.0, freq, dt);
            let v1 = get_lfo_value(offset, freq, offset + dt);
            assert!(approx(v0, v1), "mismatch at dt={dt}: {v0} vs {v1}");
        }
    }
}
