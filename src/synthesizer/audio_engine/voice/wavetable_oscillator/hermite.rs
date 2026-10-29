//! Hermite cubic spline interpolation oscillator.
//! Ported from: src/synthesizer/audio_engine/voice/wavetable_oscillator/hermite.ts
//!
//! 4.4.0 moved the interpolated span: it is now between floor (x0) and floor + 1 (x1),
//! so the 4 points used are floor - 1, floor, floor + 1 and floor + 2.
//! (Up to 4.3.22 the points were floor .. floor + 3, i.e. one sample later.)
use super::wavetable_oscillator::WavetableOscillator;

/// Hermite interpolation of 4 points at the distance `t` from `x0`.
/// https://www.musicdsp.org/en/latest/Other/93-hermite-interpollation.html
#[inline(always)]
fn hermite(xm1: f64, x0: f64, x1: f64, x2: f64, t: f64) -> f64 {
    let c = (x1 - xm1) * 0.5;
    let v = x0 - x1;
    let w = c + v;
    let a = w + v + (x2 - x0) * 0.5;
    let b = w + a;
    ((a * t - b) * t + c) * t + x0
}

impl WavetableOscillator {
    /// Equivalent to: HermiteOscillator.process
    pub(super) fn process_hermite(
        &mut self,
        sample_count: usize,
        tuning_ratio: f64,
        output_buffer: &mut [f64],
    ) -> bool {
        let step = tuning_ratio * self.playback_step;
        let sample_data = self.sample_data.as_ref().unwrap();
        let loop_start = self.loop_start;
        let loop_end = self.loop_end;
        let loop_length = self.loop_length;
        let end = self.end;
        let mut cursor = self.cursor;
        let at = |idx: usize| sample_data[idx] as f64;

        if self.is_looping {
            let mut has_looped = self.has_looped;
            // Below this cursor, the points after the cursor are inside the loop, so no wrapping is needed
            let safe_end = loop_end - 2.0;
            let mut i = 0;
            while i < sample_count {
                // Above this cursor, the point before the cursor is valid without wrapping
                // (after the first loop it must be inside the loop)
                if cursor >= (if has_looped { loop_start + 1.0 } else { 1.0 }) {
                    // Fast path, this is where almost all samples are rendered
                    while i < sample_count && cursor < safe_end {
                        // TS: `(cursor | 0) & 0x3f_ff_ff_ff` — cursor is positive here
                        let y0 = cursor as usize;
                        let t = cursor - y0 as f64;
                        output_buffer[i] =
                            hermite(at(y0 - 1), at(y0), at(y0 + 1), at(y0 + 2), t);
                        cursor += step;
                        i += 1;
                    }
                    if i >= sample_count {
                        break;
                    }
                }

                // Slow path, a single sample near the loop points or the sample start
                // Check for loop first (end is exclusive so it's `<=` and not `<`)
                // Extreme playback rates:
                // Sample can loop more than once per frame! (that's why this 'while' is here)
                if cursor >= loop_end {
                    has_looped = true;
                    while cursor >= loop_end {
                        cursor -= loop_length;
                    }
                }

                // Grab the 4 points
                let y0 = cursor as i64; // Point before the cursor.
                let mut ym1 = y0 - 1; // Point before y0
                let mut y1 = y0 + 1; // Point after the cursor
                let mut y2 = y0 + 2; // Point after y1
                let t = cursor - y0 as f64; // The distance from y0 to cursor [0;1]

                // The point before y0:
                // After looping, the point before the loop start is the last point of the loop.
                // Before that, the sample data before the loop start is played.
                // There is no point before the first one, so it is duplicated.
                if has_looped {
                    if (ym1 as f64) < loop_start {
                        ym1 = (ym1 as f64 + loop_length) as i64;
                    }
                } else if ym1 < 0 {
                    ym1 = 0;
                }

                // Ensure that the points above do not go over the loop.
                // Use while here:
                // When the loop is very short (or the cursor wraps near the loop end),
                // A single loop is not enough to bring
                // The points back into the loop range.
                // Testcase: OmegaGMGS2.sf2 drum kit,
                // Crash Cymbal (loop length 1, shortest valid)
                while y1 as f64 >= loop_end {
                    y1 = (y1 as f64 - loop_length) as i64;
                }
                while y2 as f64 >= loop_end {
                    y2 = (y2 as f64 - loop_length) as i64;
                }

                output_buffer[i] = hermite(
                    at(ym1 as usize),
                    at(y0 as usize),
                    at(y1 as usize),
                    at(y2 as usize),
                    t,
                );

                cursor += step;
                i += 1;
            }
            self.has_looped = has_looped;
        } else {
            // Below this cursor, the last point is inside the sample
            let safe_end = end - 2.0;
            let mut i = 0;
            while i < sample_count {
                // Above this cursor, the point before the cursor exists
                if cursor >= 1.0 {
                    // Fast path, this is where almost all samples are rendered
                    while i < sample_count && cursor < safe_end {
                        let y0 = cursor as usize;
                        let t = cursor - y0 as f64;
                        output_buffer[i] =
                            hermite(at(y0 - 1), at(y0), at(y0 + 1), at(y0 + 2), t);
                        cursor += step;
                        i += 1;
                    }
                    if i >= sample_count {
                        break;
                    }
                }

                // Slow path, a single sample near the sample start or end
                let y0 = cursor as usize; // Point before the cursor
                let t = cursor - y0 as f64; // The distance from y0 to cursor [0;1]

                // Flag as finished if needed
                if (y0 + 1) as f64 >= end {
                    // Fill the rest with zeros (testcase drum_spam_test.mid)
                    output_buffer[i..sample_count].fill(0.0);
                    return false;
                }

                // Grab the samples
                // There is no point before the first one or after the last one, so they are duplicated
                let xm1 = at(if y0 > 0 { y0 - 1 } else { 0 });
                let x0 = at(y0);
                let x1 = at(y0 + 1);
                let x2 = at(if ((y0 + 2) as f64) < end {
                    y0 + 2
                } else {
                    (end - 1.0) as usize
                });
                output_buffer[i] = hermite(xm1, x0, x1, x2, t);

                cursor += step;
                i += 1;
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

    const EPS: f64 = 1e-9;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    fn osc(data: Vec<f32>) -> WavetableOscillator {
        let mut osc = WavetableOscillator::new(interpolation_types::HERMITE);
        osc.end = data.len() as f64;
        osc.sample_data = Some(data);
        osc.playback_step = 1.0;
        osc
    }

    #[test]
    fn hermite_no_loop_returns_false_near_end() {
        let mut osc = osc(vec![0.0f32, 1.0, 2.0, 3.0, 4.0]);
        osc.playback_step = 1.5;
        let mut buf = vec![0.0f64; 4];
        assert!(!osc.process(4, 1.0, &mut buf));
    }

    #[test]
    fn hermite_at_integer_cursor_returns_that_sample() {
        // 4.4.0: x0 is the point at the cursor, so t=0 yields data[cursor]
        let mut osc = osc(vec![0.0f32, 1.0, 4.0, 9.0, 16.0, 25.0]);
        let mut buf = vec![0.0f64; 4];
        osc.process(4, 1.0, &mut buf);
        assert_eq!(buf, vec![0.0, 1.0, 4.0, 9.0]);
    }

    #[test]
    fn hermite_reproduces_a_line() {
        // Cubic hermite is exact on a straight line, including the duplicated edge points
        // only where the slope is preserved (interior points).
        let data: Vec<f32> = (0..16).map(|i| i as f32).collect();
        let mut osc = osc(data);
        osc.cursor = 1.25;
        let mut buf = vec![0.0f64; 8];
        osc.process(8, 1.0, &mut buf);
        for (i, v) in buf.iter().enumerate() {
            assert!(approx(*v, 1.25 + i as f64), "sample {i}: {v}");
        }
    }

    #[test]
    fn hermite_loop_length_one_is_flat() {
        let mut osc = osc(vec![10.0f32, 20.0, 30.0]);
        osc.is_looping = true;
        osc.loop_start = 0.0;
        osc.loop_end = 1.0;
        osc.loop_length = 1.0;
        osc.playback_step = 0.0;
        osc.cursor = 0.5;
        osc.has_looped = true;
        let mut buf = vec![0.0f64; 1];
        osc.process(1, 1.0, &mut buf);
        assert!(approx(buf[0], 10.0), "got {}", buf[0]);
    }

    #[test]
    fn hermite_loop_sets_has_looped() {
        let data: Vec<f32> = (0..8).map(|i| i as f32).collect();
        let mut osc = osc(data);
        osc.is_looping = true;
        osc.loop_start = 2.0;
        osc.loop_end = 6.0;
        osc.loop_length = 4.0;
        let mut buf = vec![0.0f64; 8];
        assert!(osc.process(8, 1.0, &mut buf));
        assert!(osc.has_looped);
        // 0..5, then wraps to 2,3
        assert_eq!(buf, vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 2.0, 3.0]);
    }
}
