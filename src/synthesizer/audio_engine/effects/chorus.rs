use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct ChorusSnapshot {
    pub level: u8,
    pub pre_lowpass: u8,
    pub depth: u8,
    pub delay: u8,
    pub send_level_to_delay: u8,
    pub send_level_to_reverb: u8,
    pub rate: u8,
    pub feedback: u8,
}

pub struct SpessaSynthChorus {
    left_delay_buffer: Vec<f32>,
    right_delay_buffer: Vec<f32>,
    sample_rate: f64,
    phase: f64,
    write: usize,
    gain: f64,
    reverb_gain: f64,
    delay_gain_value: f64,
    depth_samples: f64,
    delay_samples: f64,
    rate_inc: f64,
    feedback_gain: f64,

    // Pre-LPF state
    pre_lpf_a: f64,
    pre_lpf_z: f64,

    // Stored parameter values
    level: u8,
    pre_lowpass: u8,
    depth: u8,
    delay: u8,
    send_level_to_reverb: u8,
    send_level_to_delay: u8,
    rate: u8,
    feedback: u8,
}

impl SpessaSynthChorus {
    pub fn new(sample_rate: f64) -> Self {
        let buf_len = sample_rate as usize;
        let mut chorus = Self {
            left_delay_buffer: vec![0.0; buf_len],
            right_delay_buffer: vec![0.0; buf_len],
            sample_rate,
            phase: 0.0,
            write: 0,
            gain: 1.0,
            reverb_gain: 0.0,
            delay_gain_value: 0.0,
            depth_samples: 0.0,
            delay_samples: 1.0,
            rate_inc: 0.0,
            feedback_gain: 0.0,
            pre_lpf_a: 0.0,
            pre_lpf_z: 0.0,
            level: 64,
            pre_lowpass: 0,
            depth: 0,
            delay: 0,
            send_level_to_reverb: 0,
            send_level_to_delay: 0,
            rate: 0,
            feedback: 0,
        };
        chorus.set_pre_lowpass(0);
        chorus
    }

    pub fn set_send_level_to_reverb(&mut self, value: u8) {
        self.send_level_to_reverb = value;
        self.reverb_gain = value as f64 / 127.0;
    }

    pub fn set_send_level_to_delay(&mut self, value: u8) {
        self.send_level_to_delay = value;
        self.delay_gain_value = value as f64 / 127.0;
    }

    pub fn set_pre_lowpass(&mut self, value: u8) {
        self.pre_lowpass = value;
        let fc = 8000.0 * 0.63_f64.powi(value as i32);
        let decay_val = (-2.0 * PI * fc / self.sample_rate).exp();
        self.pre_lpf_a = 1.0 - decay_val;
    }

    pub fn set_depth(&mut self, value: u8) {
        self.depth = value;
        self.depth_samples = (value as f64 / 127.0) * 0.025 * self.sample_rate;
    }

    pub fn set_delay(&mut self, value: u8) {
        self.delay = value;
        self.delay_samples = ((value as f64 / 127.0) * 0.025 * self.sample_rate).max(1.0);
    }

    pub fn set_feedback(&mut self, value: u8) {
        self.feedback = value;
        self.feedback_gain = value as f64 / 128.0;
    }

    pub fn set_rate(&mut self, value: u8) {
        self.rate = value;
        let rate_hz = 15.5 * (value as f64 / 127.0);
        self.rate_inc = rate_hz / self.sample_rate;
    }

    pub fn set_level(&mut self, value: u8) {
        self.level = value;
        self.gain = value as f64 / 127.0;
    }

    /// Process chorus effect.
    /// - input: 0-based mono input
    /// - output_left/right: start_index-based stereo output (ADDS)
    /// - output_reverb/delay: 0-based mono send outputs (ADDS)
    pub fn process(
        &mut self,
        input: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        output_reverb: &mut [f32],
        output_delay: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        let rate_inc = self.rate_inc;
        let buf_len = self.left_delay_buffer.len();
        let depth = self.depth_samples;
        let delay = self.delay_samples;
        let gain = self.gain;
        let reverb_gain = self.reverb_gain;
        let delay_gain = self.delay_gain_value;
        let feedback = self.feedback_gain;
        let use_pre_lpf = self.pre_lowpass > 0;

        let mut phase = self.phase;
        let mut write = self.write;
        let mut z = self.pre_lpf_z;
        let a = self.pre_lpf_a;

        for i in 0..sample_count {
            let mut input_sample = input[i] as f64;

            // Pre lowpass filter
            if use_pre_lpf {
                z += a * (input_sample - z);
                input_sample = z;
            }

            // Triangle LFO
            let lfo = 2.0 * (phase - 0.5).abs();

            // Left channel
            let d_l = (delay + lfo * depth).clamp(1.0, buf_len as f64);
            let mut read_pos_l = write as f64 - d_l;
            if read_pos_l < 0.0 {
                read_pos_l += buf_len as f64;
            }

            // Linear interpolation
            let x0 = read_pos_l as usize;
            let mut x1 = x0 + 1;
            if x1 >= buf_len {
                x1 -= buf_len;
            }
            let frac = read_pos_l - x0 as f64;
            let out_l = self.left_delay_buffer[x0] as f64 * (1.0 - frac)
                + self.left_delay_buffer[x1] as f64 * frac;

            // Write left
            self.left_delay_buffer[write] = (input_sample + out_l * feedback) as f32;

            // Right channel (inverted LFO)
            let d_r = (delay + (1.0 - lfo) * depth).clamp(1.0, buf_len as f64);
            let mut read_pos_r = write as f64 - d_r;
            if read_pos_r < 0.0 {
                read_pos_r += buf_len as f64;
            }

            let x0 = read_pos_r as usize;
            let mut x1 = x0 + 1;
            if x1 >= buf_len {
                x1 -= buf_len;
            }
            let frac = read_pos_r - x0 as f64;
            let out_r = self.right_delay_buffer[x0] as f64 * (1.0 - frac)
                + self.right_delay_buffer[x1] as f64 * frac;

            // Mix to output
            let o = i + start_index;
            output_left[o] += (out_l * gain) as f32;
            output_right[o] += (out_r * gain) as f32;
            let mono = (out_l + out_r) / 2.0;
            output_reverb[i] += (mono * reverb_gain) as f32;
            output_delay[i] += (mono * delay_gain) as f32;

            // Write right and advance
            self.right_delay_buffer[write] = (input_sample + out_r * feedback) as f32;

            write += 1;
            if write >= buf_len {
                write = 0;
            }

            phase += rate_inc;
            if phase >= 1.0 {
                phase -= 1.0;
            }
        }
        self.write = write;
        self.phase = phase;
        self.pre_lpf_z = z;
    }

    pub fn get_snapshot(&self) -> ChorusSnapshot {
        ChorusSnapshot {
            level: self.level,
            pre_lowpass: self.pre_lowpass,
            depth: self.depth,
            delay: self.delay,
            send_level_to_delay: self.send_level_to_delay,
            send_level_to_reverb: self.send_level_to_reverb,
            rate: self.rate,
            feedback: self.feedback,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 44100.0;
    const EPS: f64 = 1e-6;

    #[test]
    fn new_creates_valid_chorus() {
        let c = SpessaSynthChorus::new(SR);
        assert_eq!(c.sample_rate, SR);
        assert_eq!(c.left_delay_buffer.len(), SR as usize);
    }

    #[test]
    fn set_level_calculates_gain() {
        let mut c = SpessaSynthChorus::new(SR);
        c.set_level(127);
        assert!((c.gain - 1.0).abs() < EPS);
        c.set_level(0);
        assert!(c.gain.abs() < EPS);
        c.set_level(64);
        assert!((c.gain - 64.0 / 127.0).abs() < EPS);
    }

    #[test]
    fn set_depth_calculates_samples() {
        let mut c = SpessaSynthChorus::new(SR);
        c.set_depth(127);
        let expected = 0.025 * SR; // 1102.5
        assert!((c.depth_samples - expected).abs() < 1.0);
        c.set_depth(0);
        assert!(c.depth_samples.abs() < EPS);
    }

    #[test]
    fn set_delay_minimum_is_one() {
        let mut c = SpessaSynthChorus::new(SR);
        c.set_delay(0);
        assert!((c.delay_samples - 1.0).abs() < EPS, "Min delay should be 1.0, got {}", c.delay_samples);
    }

    #[test]
    fn set_delay_calculates_samples() {
        let mut c = SpessaSynthChorus::new(SR);
        c.set_delay(127);
        let expected = 0.025 * SR; // 1102.5
        assert!((c.delay_samples - expected).abs() < 1.0);
    }

    #[test]
    fn set_rate_calculates_increment() {
        let mut c = SpessaSynthChorus::new(SR);
        c.set_rate(127);
        // rate_hz = 15.5, rate_inc = 15.5 / 44100
        let expected = 15.5 / SR;
        assert!((c.rate_inc - expected).abs() < EPS);
        c.set_rate(0);
        assert!(c.rate_inc.abs() < EPS);
    }

    #[test]
    fn set_feedback_calculates_gain() {
        let mut c = SpessaSynthChorus::new(SR);
        c.set_feedback(128);
        assert!((c.feedback_gain - 1.0).abs() < EPS);
        c.set_feedback(0);
        assert!(c.feedback_gain.abs() < EPS);
    }

    #[test]
    fn set_send_level_to_reverb() {
        let mut c = SpessaSynthChorus::new(SR);
        c.set_send_level_to_reverb(127);
        assert!((c.reverb_gain - 1.0).abs() < EPS);
    }

    #[test]
    fn set_send_level_to_delay() {
        let mut c = SpessaSynthChorus::new(SR);
        c.set_send_level_to_delay(127);
        assert!((c.delay_gain_value - 1.0).abs() < EPS);
    }

    #[test]
    fn set_pre_lowpass_coefficient() {
        let mut c = SpessaSynthChorus::new(SR);
        c.set_pre_lowpass(0);
        // fc = 8000, decay = exp(-2*PI*8000/44100), a = 1-decay
        let fc = 8000.0;
        let decay = (-2.0 * PI * fc / SR).exp();
        let expected_a = 1.0 - decay;
        assert!((c.pre_lpf_a - expected_a).abs() < EPS);
    }

    #[test]
    fn process_zero_input_produces_zero_output() {
        let mut c = SpessaSynthChorus::new(SR);
        c.set_level(127);
        c.set_depth(64);
        c.set_delay(64);
        c.set_rate(64);

        let input = vec![0.0f32; 128];
        let mut out_l = vec![0.0f32; 256];
        let mut out_r = vec![0.0f32; 256];
        let mut out_rev = vec![0.0f32; 128];
        let mut out_del = vec![0.0f32; 128];
        c.process(&input, &mut out_l, &mut out_r, &mut out_rev, &mut out_del, 0, 128);

        // With zero input and empty buffers, output should be zero
        for i in 0..128 {
            assert!((out_l[i] as f64).abs() < EPS);
            assert!((out_r[i] as f64).abs() < EPS);
        }
    }

    #[test]
    fn snapshot_captures_parameters() {
        let mut c = SpessaSynthChorus::new(SR);
        c.set_level(100);
        c.set_depth(50);
        c.set_delay(30);
        c.set_rate(80);
        c.set_feedback(64);
        let snap = c.get_snapshot();
        assert_eq!(snap.level, 100);
        assert_eq!(snap.depth, 50);
        assert_eq!(snap.delay, 30);
        assert_eq!(snap.rate, 80);
        assert_eq!(snap.feedback, 64);
    }
}
