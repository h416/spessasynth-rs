/// Simple circular buffer delay line with feedback.
/// Used as a building block for reverb, chorus, and delay effects.
pub struct DelayLine {
    buffer: Vec<f32>,
    buffer_length: usize,
    write_index: usize,
    time: usize,
    pub feedback: f64,
    pub gain: f64,
}

impl DelayLine {
    pub fn new(max_delay: usize) -> Self {
        Self {
            buffer: vec![0.0; max_delay],
            buffer_length: max_delay,
            write_index: 0,
            time: max_delay.saturating_sub(5),
            feedback: 0.0,
            gain: 1.0,
        }
    }

    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.write_index = 0;
    }

    pub fn time(&self) -> usize {
        self.time
    }

    pub fn set_time(&mut self, samples: usize) {
        self.time = samples.min(self.buffer_length);
    }

    /// OVERWRITES the output buffer.
    pub fn process(&mut self, input: &[f32], output: &mut [f32], sample_count: usize) {
        let mut write_index = self.write_index;
        let delay = self.time;
        let buf_len = self.buffer_length;
        let feedback = self.feedback;
        let gain = self.gain;

        for i in 0..sample_count {
            // Read
            let mut read_index = write_index as isize - delay as isize;
            if read_index < 0 {
                read_index += buf_len as isize;
            }
            let delayed = self.buffer[read_index as usize] as f64;
            output[i] = (delayed * gain) as f32;

            // Write
            self.buffer[write_index] = (input[i] as f64 + delayed * feedback) as f32;

            // Advance and wrap
            write_index += 1;
            if write_index >= buf_len {
                write_index = 0;
            }
        }
        self.write_index = write_index;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_correct_buffer_size() {
        let dl = DelayLine::new(100);
        assert_eq!(dl.buffer_length, 100);
        assert_eq!(dl.buffer.len(), 100);
    }

    #[test]
    fn new_default_time_is_near_max() {
        let dl = DelayLine::new(100);
        assert_eq!(dl.time, 95); // max_delay - 5
    }

    #[test]
    fn set_time_clamps_to_buffer_length() {
        let mut dl = DelayLine::new(100);
        dl.set_time(200);
        assert_eq!(dl.time, 100);
    }

    #[test]
    fn set_time_accepts_valid_value() {
        let mut dl = DelayLine::new(100);
        dl.set_time(50);
        assert_eq!(dl.time, 50);
    }

    #[test]
    fn clear_resets_buffer_and_write_index() {
        let mut dl = DelayLine::new(100);
        dl.buffer[10] = 1.0;
        dl.write_index = 42;
        dl.clear();
        assert_eq!(dl.write_index, 0);
        assert!(dl.buffer.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn process_delays_signal() {
        let mut dl = DelayLine::new(100);
        dl.set_time(5);
        dl.feedback = 0.0;
        dl.gain = 1.0;

        // Write an impulse
        let input = vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let mut output = vec![0.0; 10];
        dl.process(&input, &mut output, 10);

        // First 5 samples should be 0 (delay=5), sample 5 should have the impulse
        for i in 0..5 {
            assert_eq!(output[i], 0.0, "Expected 0 at index {}, got {}", i, output[i]);
        }
        assert!(
            (output[5] - 1.0).abs() < 1e-6,
            "Expected delayed impulse at index 5, got {}",
            output[5]
        );
    }

    #[test]
    fn process_applies_gain() {
        let mut dl = DelayLine::new(100);
        dl.set_time(2);
        dl.feedback = 0.0;
        dl.gain = 0.5;

        let input = vec![1.0, 0.0, 0.0, 0.0, 0.0];
        let mut output = vec![0.0; 5];
        dl.process(&input, &mut output, 5);

        assert!((output[2] - 0.5).abs() < 1e-6, "Gain not applied: {}", output[2]);
    }

    #[test]
    fn process_feedback_creates_echo() {
        let mut dl = DelayLine::new(100);
        dl.set_time(3);
        dl.feedback = 0.5;
        dl.gain = 1.0;

        let mut input = vec![0.0; 10];
        input[0] = 1.0;
        let mut output = vec![0.0; 10];
        dl.process(&input, &mut output, 10);

        // First echo at index 3
        assert!((output[3] - 1.0).abs() < 1e-6);
        // Second echo at index 6 (feedback * first echo = 0.5)
        assert!((output[6] - 0.5).abs() < 1e-6);
        // Third echo at index 9 (0.5 * 0.5 = 0.25)
        assert!((output[9] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn process_wraps_around_buffer() {
        let mut dl = DelayLine::new(8);
        dl.set_time(3);
        dl.feedback = 0.0;
        dl.gain = 1.0;

        // Process more samples than buffer size
        let mut input = vec![0.0; 20];
        input[0] = 1.0;
        input[10] = 2.0;
        let mut output = vec![0.0; 20];
        dl.process(&input, &mut output, 20);

        assert!((output[3] - 1.0).abs() < 1e-6, "First impulse delay");
        assert!((output[13] - 2.0).abs() < 1e-6, "Second impulse delay after wrap");
    }
}
