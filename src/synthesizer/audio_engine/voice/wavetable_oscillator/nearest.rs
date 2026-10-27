//! Nearest-neighbor (no interpolation) oscillator.
//! Ported from: src/synthesizer/audio_engine/voice/wavetable_oscillator/nearest.ts
use super::wavetable_oscillator::WavetableOscillator;

impl WavetableOscillator {
    /// Equivalent to: NearestOscillator.process
    pub(super) fn process_nearest(
        &mut self,
        sample_count: usize,
        tuning_ratio: f64,
        output_buffer: &mut [f64],
    ) -> bool {
        let step = tuning_ratio * self.playback_step;
        let sample_data = self.sample_data.as_ref().unwrap();
        let loop_end = self.loop_end;
        let loop_length = self.loop_length;
        let end = self.end;
        let mut cursor = self.cursor;

        if self.is_looping {
            for out in output_buffer.iter_mut().take(sample_count) {
                // Check for loop first (end is exclusive so it's `<=` and not `<`)
                // Extreme playback rates:
                // Sample can loop more than once per frame! (that's why this 'while' is here)
                while cursor >= loop_end {
                    cursor -= loop_length;
                }

                // Grab the nearest neighbor
                *out = sample_data[cursor as usize] as f64;
                cursor += step;
            }
        } else {
            for i in 0..sample_count {
                // Flag the voice as finished if needed
                if cursor >= end {
                    // Fill the rest with zeros (testcase drum_spam_test.mid)
                    output_buffer[i..sample_count].fill(0.0);
                    return false;
                }

                output_buffer[i] = sample_data[cursor as usize] as f64;
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

    fn osc(data: Vec<f32>) -> WavetableOscillator {
        let mut osc = WavetableOscillator::new(interpolation_types::NEAREST_NEIGHBOR);
        osc.end = data.len() as f64;
        osc.sample_data = Some(data);
        osc.playback_step = 1.0;
        osc
    }

    #[test]
    fn nearest_no_loop_returns_false_at_end() {
        let mut osc = osc(vec![0.0f32, 1.0, 2.0]);
        let mut buf = vec![0.0f64; 8];
        assert!(!osc.process(8, 1.0, &mut buf));
        assert_eq!(buf[3..], [0.0; 5]);
    }

    #[test]
    fn nearest_no_loop_reads_correct_sample() {
        let mut osc = osc(vec![0.0f32, 10.0, 20.0, 30.0]);
        osc.cursor = 1.7;
        let mut buf = vec![0.0f64; 1];
        osc.process(1, 1.0, &mut buf);
        assert_eq!(buf[0], 10.0);
    }

    #[test]
    fn nearest_loop_wraps_cursor() {
        let mut osc = osc(vec![0.0f32, 1.0, 2.0, 3.0]);
        osc.is_looping = true;
        osc.loop_end = 2.0;
        osc.loop_length = 2.0;
        let mut buf = vec![0.0f64; 4];
        assert!(osc.process(4, 1.0, &mut buf));
        assert_eq!(buf, vec![0.0, 1.0, 0.0, 1.0]);
    }
}
