/// wavetable_oscillator.rs
/// purpose: plays back raw audio data at an arbitrary playback rate
/// Ported from: src/synthesizer/audio_engine/engine_components/dsp_chain/wavetable_oscillator.ts
use crate::synthesizer::enums::{InterpolationType, interpolation_types};

/// Shared state for all wavetable oscillator variants.
/// Equivalent to: WavetableOscillator (abstract class fields)
pub struct WavetableOscillator {
    /// Interpolation mode. One of `interpolation_types` constants.
    pub interpolation_type: InterpolationType,
    /// Is the loop on?
    /// Equivalent to: isLooping
    pub is_looping: bool,
    /// Sample data of the voice.
    /// Equivalent to: sampleData
    pub sample_data: Option<Vec<f32>>,
    /// Playback step (rate) for sample pitch correction.
    /// Equivalent to: playbackStep
    pub playback_step: f64,
    /// Start position of the loop.
    /// Equivalent to: loopStart
    pub loop_start: f64,
    /// End position of the loop.
    /// Equivalent to: loopEnd
    pub loop_end: f64,
    /// Length of the loop.
    /// Equivalent to: loopLength
    pub loop_length: f64,
    /// End position of the sample.
    /// Equivalent to: end
    pub end: f64,
    /// The current cursor of the sample.
    /// Equivalent to: cursor
    pub cursor: f64,
}

impl WavetableOscillator {
    /// Creates a new WavetableOscillator with default values.
    pub fn new(interpolation_type: InterpolationType) -> Self {
        Self {
            interpolation_type,
            is_looping: false,
            sample_data: None,
            playback_step: 0.0,
            loop_start: 0.0,
            loop_end: 0.0,
            loop_length: 0.0,
            end: 0.0,
            cursor: 0.0,
        }
    }

    /// Fills the output buffer with raw sample data using the configured interpolation.
    /// Returns `true` if the voice is still active, `false` if it has finished.
    /// Equivalent to: process (abstract method dispatched per subclass)
    pub fn process(
        &mut self,
        sample_count: usize,
        tuning_ratio: f64,
        output_buffer: &mut [f32],
    ) -> bool {
        match self.interpolation_type {
            interpolation_types::NEAREST_NEIGHBOR => {
                self.process_nearest(sample_count, tuning_ratio, output_buffer)
            }
            interpolation_types::HERMITE => {
                self.process_hermite(sample_count, tuning_ratio, output_buffer)
            }
            // Default: LINEAR (interpolation_types::LINEAR == 0)
            _ => self.process_linear(sample_count, tuning_ratio, output_buffer),
        }
    }

    /// Linear interpolation oscillator.
    /// Equivalent to: LinearOscillator.process
    fn process_linear(
        &mut self,
        sample_count: usize,
        tuning_ratio: f64,
        output_buffer: &mut [f32],
    ) -> bool {
        let step = tuning_ratio * self.playback_step;
        let data = match &self.sample_data {
            Some(d) => d,
            None => return false,
        };
        let loop_end = self.loop_end;
        let loop_length = self.loop_length;
        let loop_start = self.loop_start;
        let end = self.end;
        let mut cursor = self.cursor;

        if self.is_looping {
            for out in output_buffer.iter_mut().take(sample_count) {
                // Check for loop
                if cursor > loop_start {
                    cursor = loop_start + ((cursor - loop_start) % loop_length);
                }

                // Grab the 2 nearest points
                let floor = cursor as usize;
                let mut ceil = floor + 1;

                if ceil as f64 >= loop_end {
                    ceil -= loop_length as usize;
                }

                let fraction = cursor - floor as f64;

                let lower = data[floor] as f64;
                let upper = data[ceil] as f64;
                *out = (lower + (upper - lower) * fraction) as f32;

                cursor += step;
            }
        } else {
            for out in output_buffer.iter_mut().take(sample_count) {
                let floor = cursor as usize;
                let ceil = floor + 1;

                if ceil as f64 >= end {
                    self.cursor = cursor;
                    return false;
                }

                let fraction = cursor - floor as f64;

                let lower = data[floor] as f64;
                let upper = data[ceil] as f64;
                *out = (lower + (upper - lower) * fraction) as f32;

                cursor += step;
            }
        }
        self.cursor = cursor;
        true
    }

    /// Nearest-neighbor (no interpolation) oscillator.
    /// Equivalent to: NearestOscillator.process
    fn process_nearest(
        &mut self,
        sample_count: usize,
        tuning_ratio: f64,
        output_buffer: &mut [f32],
    ) -> bool {
        let step = tuning_ratio * self.playback_step;
        let data = match &self.sample_data {
            Some(d) => d,
            None => return false,
        };
        let loop_length = self.loop_length;
        let loop_start = self.loop_start;
        let end = self.end;
        let mut cursor = self.cursor;

        if self.is_looping {
            for out in output_buffer.iter_mut().take(sample_count) {
                // Check for loop
                if cursor > loop_start {
                    cursor = loop_start + ((cursor - loop_start) % loop_length);
                }

                *out = data[cursor as usize];
                cursor += step;
            }
        } else {
            for out in output_buffer.iter_mut().take(sample_count) {
                if cursor >= end {
                    self.cursor = cursor;
                    return false;
                }

                *out = data[cursor as usize];
                cursor += step;
            }
        }
        self.cursor = cursor;
        true
    }

    /// Hermite cubic spline interpolation oscillator.
    /// Equivalent to: HermiteOscillator.process
    fn process_hermite(
        &mut self,
        sample_count: usize,
        tuning_ratio: f64,
        output_buffer: &mut [f32],
    ) -> bool {
        let step = tuning_ratio * self.playback_step;
        let data = match &self.sample_data {
            Some(d) => d,
            None => return false,
        };
        let loop_end = self.loop_end;
        let loop_length = self.loop_length;
        let loop_start = self.loop_start;
        let end = self.end;
        let mut cursor = self.cursor;

        if self.is_looping {
            for out in output_buffer.iter_mut().take(sample_count) {
                // Check for loop
                if cursor > loop_start {
                    cursor = loop_start + ((cursor - loop_start) % loop_length);
                }

                // Grab the 4 points
                let y0 = cursor as usize; // Point before cursor
                let mut y1 = y0 + 1; // Point after cursor
                let mut y2 = y0 + 2; // Point 1 after cursor
                let mut y3 = y0 + 3; // Point 2 after cursor
                let t = cursor - y0 as f64; // Distance from y0 to cursor [0;1]

                let loop_end_usize = loop_end as usize;
                let loop_length_usize = loop_length as usize;
                if y1 >= loop_end_usize {
                    y1 -= loop_length_usize;
                }
                if y2 >= loop_end_usize {
                    y2 -= loop_length_usize;
                }
                if y3 >= loop_end_usize {
                    y3 -= loop_length_usize;
                }

                // Grab the samples
                let xm1 = data[y0] as f64;
                let x0 = data[y1] as f64;
                let x1 = data[y2] as f64;
                let x2 = data[y3] as f64;

                // Hermite interpolation
                // https://www.musicdsp.org/en/latest/Other/93-hermite-interpollation.html
                let c = (x1 - xm1) * 0.5;
                let v = x0 - x1;
                let w = c + v;
                let a = w + v + (x2 - x0) * 0.5;
                let b = w + a;
                *out = (((a * t - b) * t + c) * t + x0) as f32;

                cursor += step;
            }
        } else {
            for out in output_buffer.iter_mut().take(sample_count) {
                let y0 = cursor as usize;
                let y3 = y0 + 3;

                if y3 as f64 >= end {
                    self.cursor = cursor;
                    return false;
                }

                let y1 = y0 + 1;
                let y2 = y0 + 2;
                let t = cursor - y0 as f64;

                let xm1 = data[y0] as f64;
                let x0 = data[y1] as f64;
                let x1 = data[y2] as f64;
                let x2 = data[y3] as f64;

                // Hermite interpolation
                // https://www.musicdsp.org/en/latest/Other/93-hermite-interpollation.html
                let c = (x1 - xm1) * 0.5;
                let v = x0 - x1;
                let w = c + v;
                let a = w + v + (x2 - x0) * 0.5;
                let b = w + a;
                *out = (((a * t - b) * t + c) * t + x0) as f32;

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

    const EPS: f32 = 1e-5;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < EPS
    }

    /// Builds a simple ramp sample: [0.0, 1.0, 2.0, ..., (len-1).0] / (len-1)
    fn ramp_sample(len: usize) -> Vec<f32> {
        (0..len).map(|i| i as f32 / (len - 1) as f32).collect()
    }

    // ─── LinearOscillator ───────────────────────────────────────────────────

    #[test]
    fn linear_no_loop_returns_false_at_end() {
        let data = ramp_sample(8);
        let end = data.len() as f64;
        let mut osc = WavetableOscillator::new(interpolation_types::LINEAR);
        osc.sample_data = Some(data);
        osc.end = end;
        osc.playback_step = 1.0;

        let mut buf = vec![0.0f32; 16];
        // step=1, tuning_ratio=1 → will exhaust 8-sample buffer before 16 outputs
        let result = osc.process(16, 1.0, &mut buf);
        assert!(!result, "should return false when sample is exhausted");
    }

    #[test]
    fn linear_no_loop_interpolates_midpoint() {
        // 2-sample data: [0.0, 1.0], read at cursor=0.5 → expect 0.5
        let data = vec![0.0f32, 1.0, 1.0]; // 3 elements so ceil doesn't go out of bounds
        let mut osc = WavetableOscillator::new(interpolation_types::LINEAR);
        osc.sample_data = Some(data);
        osc.end = 3.0;
        osc.playback_step = 0.5; // advance 0.5 per step (tuning_ratio=1)
        osc.cursor = 0.5; // start at midpoint

        let mut buf = vec![0.0f32; 1];
        let result = osc.process(1, 1.0, &mut buf);
        assert!(result);
        assert!(approx(buf[0], 0.5), "expected 0.5, got {}", buf[0]);
    }

    #[test]
    fn linear_no_loop_full_pass_step1() {
        // step=1 reads samples at integer positions → each output equals data[i]
        let data = vec![0.0f32, 0.25, 0.5, 0.75, 1.0, 1.0];
        let mut osc = WavetableOscillator::new(interpolation_types::LINEAR);
        osc.sample_data = Some(data.clone());
        osc.end = data.len() as f64;
        osc.playback_step = 1.0;

        let mut buf = vec![0.0f32; 4];
        let result = osc.process(4, 1.0, &mut buf);
        assert!(result);
        for (i, &v) in buf.iter().enumerate() {
            assert!(
                approx(v, data[i]),
                "sample {i}: expected {}, got {v}",
                data[i]
            );
        }
    }

    #[test]
    fn linear_loop_wraps_cursor() {
        // loop: start=0, end=4, length=4, data=[0,1,2,3,...]
        // with step=1 and loop enabled, cursor should wrap every 4 samples
        let data = vec![0.0f32, 1.0, 2.0, 3.0, 4.0];
        let mut osc = WavetableOscillator::new(interpolation_types::LINEAR);
        osc.sample_data = Some(data.clone());
        osc.is_looping = true;
        osc.loop_start = 0.0;
        osc.loop_end = 4.0;
        osc.loop_length = 4.0;
        osc.end = 5.0;
        osc.playback_step = 1.0;

        let mut buf = vec![0.0f32; 8];
        let result = osc.process(8, 1.0, &mut buf);
        assert!(result, "looping oscillator should never return false");
        // First 4 outputs: 0,1,2,3; next 4 should wrap back to 0,1,2,3
        assert!(approx(buf[0], 0.0));
        assert!(approx(buf[4], 0.0), "expected wrap to 0, got {}", buf[4]);
    }

    // ─── NearestOscillator ──────────────────────────────────────────────────

    #[test]
    fn nearest_no_loop_returns_false_at_end() {
        let data = vec![0.5f32; 4];
        let end = 4.0;
        let mut osc = WavetableOscillator::new(interpolation_types::NEAREST_NEIGHBOR);
        osc.sample_data = Some(data);
        osc.end = end;
        osc.playback_step = 1.0;

        let mut buf = vec![0.0f32; 8];
        let result = osc.process(8, 1.0, &mut buf);
        assert!(!result);
    }

    #[test]
    fn nearest_no_loop_reads_correct_sample() {
        let data = vec![10.0f32, 20.0, 30.0, 40.0, 50.0];
        let mut osc = WavetableOscillator::new(interpolation_types::NEAREST_NEIGHBOR);
        osc.sample_data = Some(data);
        osc.end = 5.0;
        osc.playback_step = 1.0;

        let mut buf = vec![0.0f32; 4];
        osc.process(4, 1.0, &mut buf);
        assert!(approx(buf[0], 10.0));
        assert!(approx(buf[1], 20.0));
        assert!(approx(buf[2], 30.0));
        assert!(approx(buf[3], 40.0));
    }

    #[test]
    fn nearest_loop_wraps_cursor() {
        let data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0];
        let mut osc = WavetableOscillator::new(interpolation_types::NEAREST_NEIGHBOR);
        osc.sample_data = Some(data);
        osc.is_looping = true;
        osc.loop_start = 0.0;
        osc.loop_end = 4.0;
        osc.loop_length = 4.0;
        osc.end = 5.0;
        osc.playback_step = 1.0;

        let mut buf = vec![0.0f32; 8];
        let result = osc.process(8, 1.0, &mut buf);
        assert!(result);
        assert!(approx(buf[0], 1.0));
        assert!(approx(buf[4], 1.0), "expected wrap, got {}", buf[4]);
    }

    // ─── HermiteOscillator ──────────────────────────────────────────────────

    #[test]
    fn hermite_no_loop_returns_false_near_end() {
        // Hermite needs 4 points; with end=4 and cursor at 1, y3=4 >= end → false
        let data = vec![0.0f32, 1.0, 2.0, 3.0, 4.0];
        let mut osc = WavetableOscillator::new(interpolation_types::HERMITE);
        osc.sample_data = Some(data);
        osc.end = 4.0;
        osc.playback_step = 1.5; // large step to exhaust quickly

        let mut buf = vec![0.0f32; 4];
        let result = osc.process(4, 1.0, &mut buf);
        assert!(
            !result,
            "should return false when fewer than 4 points remain"
        );
    }

    #[test]
    fn hermite_no_loop_at_integer_cursor_matches_sample() {
        // When cursor is exactly at integer, t=0, so hermite returns x0 = data[cursor+1]
        let data = vec![0.0f32, 1.0, 2.0, 3.0, 4.0, 5.0];
        let mut osc = WavetableOscillator::new(interpolation_types::HERMITE);
        osc.sample_data = Some(data);
        osc.end = 6.0;
        osc.playback_step = 1.0;
        // cursor=0 → y0=0,y1=1,y2=2,y3=3, t=0 → result = x0 = data[1] = 1.0
        osc.cursor = 0.0;

        let mut buf = vec![0.0f32; 1];
        osc.process(1, 1.0, &mut buf);
        assert!(
            approx(buf[0], 1.0),
            "hermite at integer cursor should return data[cursor+1], got {}",
            buf[0]
        );
    }

    #[test]
    fn hermite_loop_wraps_cursor() {
        let data: Vec<f32> = (0..8).map(|i| i as f32).collect();
        let mut osc = WavetableOscillator::new(interpolation_types::HERMITE);
        osc.sample_data = Some(data);
        osc.is_looping = true;
        osc.loop_start = 0.0;
        osc.loop_end = 4.0;
        osc.loop_length = 4.0;
        osc.end = 8.0;
        osc.playback_step = 1.0;

        let mut buf = vec![0.0f32; 8];
        let result = osc.process(8, 1.0, &mut buf);
        assert!(result, "looping hermite should not return false");
    }

    // ─── sample_data = None ─────────────────────────────────────────────────

    #[test]
    fn no_sample_data_returns_false_linear() {
        let mut osc = WavetableOscillator::new(interpolation_types::LINEAR);
        let mut buf = vec![0.0f32; 4];
        assert!(!osc.process(4, 1.0, &mut buf));
    }

    #[test]
    fn no_sample_data_returns_false_nearest() {
        let mut osc = WavetableOscillator::new(interpolation_types::NEAREST_NEIGHBOR);
        let mut buf = vec![0.0f32; 4];
        assert!(!osc.process(4, 1.0, &mut buf));
    }

    #[test]
    fn no_sample_data_returns_false_hermite() {
        let mut osc = WavetableOscillator::new(interpolation_types::HERMITE);
        let mut buf = vec![0.0f32; 4];
        assert!(!osc.process(4, 1.0, &mut buf));
    }

    // ─── tuning_ratio scaling ───────────────────────────────────────────────

    #[test]
    fn linear_tuning_ratio_doubles_step() {
        // tuning_ratio=2 doubles the effective step → reads data twice as fast
        let data = vec![0.0f32, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let mut osc = WavetableOscillator::new(interpolation_types::LINEAR);
        osc.sample_data = Some(data.clone());
        osc.end = data.len() as f64;
        osc.playback_step = 1.0;

        let mut buf1 = vec![0.0f32; 3];
        // ratio=1: reads at 0,1,2 → [0,1,2]
        let mut osc1 = WavetableOscillator::new(interpolation_types::LINEAR);
        osc1.sample_data = Some(data.clone());
        osc1.end = data.len() as f64;
        osc1.playback_step = 1.0;
        osc1.process(3, 1.0, &mut buf1);

        let mut buf2 = vec![0.0f32; 3];
        // ratio=2: reads at 0,2,4 → [0,2,4]
        osc.process(3, 2.0, &mut buf2);

        assert!(approx(buf2[0], buf1[0])); // both at 0
        assert!(approx(buf2[1], buf1[2])); // ratio=2 skips one
        assert!(approx(buf2[2], 4.0));
    }
}
