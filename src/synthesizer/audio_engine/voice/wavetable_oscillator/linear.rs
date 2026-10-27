//! Linear interpolation oscillator.
//! Ported from: src/synthesizer/audio_engine/voice/wavetable_oscillator/linear.ts
use super::wavetable_oscillator::WavetableOscillator;

impl WavetableOscillator {
    /// Equivalent to: LinearOscillator.process
    pub(super) fn process_linear(
        &mut self,
        sample_count: usize,
        tuning_ratio: f64,
        output_buffer: &mut [f64],
    ) -> bool {
        let step = tuning_ratio * self.playback_step;
        let data = self.sample_data.as_ref().unwrap();
        let loop_end = self.loop_end;
        let loop_length = self.loop_length;
        let end = self.end;
        let mut cursor = self.cursor;

        if self.is_looping {
            // Below this cursor, both points are inside the loop, so no wrapping is needed
            let safe_end = loop_end - 1.0;
            let mut i = 0;
            while i < sample_count {
                // Fast path, this is where almost all samples are rendered
                while i < sample_count && cursor < safe_end {
                    // TS: `(cursor | 0) & 0x3f_ff_ff_ff` — cursor is non-negative here
                    let floor = cursor as usize;
                    let fraction = cursor - floor as f64;
                    let lower = data[floor] as f64;
                    output_buffer[i] = lower + (data[floor + 1] as f64 - lower) * fraction;
                    cursor += step;
                    i += 1;
                }
                if i >= sample_count {
                    break;
                }

                // Slow path, a single sample near the loop end
                // Check for loop first (end is exclusive so it's `<=` and not `<`)
                // Extreme playback rates:
                // Sample can loop more than once per frame! (that's why this 'while' is here)
                while cursor >= loop_end {
                    cursor -= loop_length;
                }

                // Grab the 2 nearest points
                let floor = cursor as usize;
                let mut ceil = floor + 1;

                // Ensure that the point above does not go over the loop
                if ceil as f64 >= loop_end {
                    ceil = (ceil as f64 - loop_length) as usize;
                }

                let fraction = cursor - floor as f64;

                // Grab the samples and interpolate
                let upper = data[ceil] as f64;
                let lower = data[floor] as f64;
                output_buffer[i] = lower + (upper - lower) * fraction;

                cursor += step;
                i += 1;
            }
        } else {
            for i in 0..sample_count {
                // Linear interpolation
                let floor = cursor as usize;
                let ceil = floor + 1;

                // Flag the voice as finished if needed
                if ceil as f64 >= end {
                    // Fill the rest with zeros (testcase drum_spam_test.mid)
                    output_buffer[i..sample_count].fill(0.0);
                    return false;
                }

                let fraction = cursor - floor as f64;

                // Grab the samples and interpolate
                let upper = data[ceil] as f64;
                let lower = data[floor] as f64;
                output_buffer[i] = lower + (upper - lower) * fraction;

                cursor += step;
            }
        }
        self.cursor = cursor;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthesizer::enums::interpolation_types;

    const EPS: f64 = 1e-5;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    /// Builds a simple ramp sample: [0.0, 1.0, 2.0, ..., (len-1).0] / (len-1)
    fn ramp_sample(len: usize) -> Vec<f32> {
        (0..len).map(|i| i as f32 / (len - 1) as f32).collect()
    }

    fn osc(data: Vec<f32>) -> WavetableOscillator {
        let mut osc = WavetableOscillator::new(interpolation_types::LINEAR);
        osc.end = data.len() as f64;
        osc.sample_data = Some(data);
        osc.playback_step = 1.0;
        osc
    }

    #[test]
    fn linear_no_loop_returns_false_at_end() {
        let mut osc = osc(ramp_sample(8));
        let mut buf = vec![0.0f64; 16];
        // step=1, tuning_ratio=1 → will exhaust 8-sample buffer before 16 outputs
        assert!(!osc.process(16, 1.0, &mut buf));
    }

    #[test]
    fn linear_no_loop_interpolates_midpoint() {
        let mut osc = osc(vec![0.0f32, 1.0, 1.0]);
        osc.playback_step = 0.5;
        osc.cursor = 0.5;
        let mut buf = vec![0.0f64; 1];
        assert!(osc.process(1, 1.0, &mut buf));
        assert!(approx(buf[0], 0.5), "expected 0.5, got {}", buf[0]);
    }

    #[test]
    fn linear_no_loop_full_pass_step1() {
        let data = vec![0.0f32, 0.25, 0.5, 0.75, 1.0, 1.0];
        let mut osc = osc(data.clone());
        let mut buf = vec![0.0f64; 4];
        assert!(osc.process(4, 1.0, &mut buf));
        for (i, &v) in buf.iter().enumerate() {
            assert!(approx(v, data[i] as f64), "sample {i}: expected {}, got {v}", data[i]);
        }
    }

    #[test]
    fn linear_loop_wraps_cursor() {
        let mut osc = osc(vec![0.0f32, 1.0, 2.0, 3.0, 4.0]);
        osc.is_looping = true;
        osc.loop_start = 0.0;
        osc.loop_end = 4.0;
        osc.loop_length = 4.0;
        let mut buf = vec![0.0f64; 8];
        assert!(osc.process(8, 1.0, &mut buf));
        assert_eq!(buf, vec![0.0, 1.0, 2.0, 3.0, 0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn linear_loop_ceil_wraps_to_loop_start() {
        // cursor 3.5 in a [0, 4) loop interpolates between data[3] and data[0]
        let mut osc = osc(vec![0.0f32, 1.0, 2.0, 4.0, 9.0]);
        osc.is_looping = true;
        osc.loop_end = 4.0;
        osc.loop_length = 4.0;
        osc.cursor = 3.5;
        let mut buf = vec![0.0f64; 1];
        osc.process(1, 1.0, &mut buf);
        assert!(approx(buf[0], 2.0), "got {}", buf[0]);
    }

    #[test]
    fn linear_tuning_ratio_doubles_step() {
        let mut osc = osc(vec![0.0f32, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        let mut buf = vec![0.0f64; 3];
        osc.process(3, 2.0, &mut buf);
        assert_eq!(buf, vec![0.0, 2.0, 4.0]);
    }
}
