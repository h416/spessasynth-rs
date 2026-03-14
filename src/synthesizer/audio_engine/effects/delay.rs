use std::f64::consts::PI;

use super::delay_line::DelayLine;

/// SC-8850 delay time segments (manual p.236).
struct DelayTimeSegment {
    start: u8,
    end: u8,
    time_start: f64,
    resolution: f64,
}

const DELAY_TIME_SEGMENTS: &[DelayTimeSegment] = &[
    DelayTimeSegment { start: 0x01, end: 0x14, time_start: 0.1, resolution: 0.1 },
    DelayTimeSegment { start: 0x14, end: 0x23, time_start: 2.0, resolution: 0.2 },
    DelayTimeSegment { start: 0x23, end: 0x2d, time_start: 5.0, resolution: 0.5 },
    DelayTimeSegment { start: 0x2d, end: 0x37, time_start: 10.0, resolution: 1.0 },
    DelayTimeSegment { start: 0x37, end: 0x46, time_start: 20.0, resolution: 2.0 },
    DelayTimeSegment { start: 0x46, end: 0x50, time_start: 50.0, resolution: 5.0 },
    DelayTimeSegment { start: 0x50, end: 0x5a, time_start: 100.0, resolution: 10.0 },
    DelayTimeSegment { start: 0x5a, end: 0x69, time_start: 200.0, resolution: 20.0 },
    DelayTimeSegment { start: 0x69, end: 0x74, time_start: 500.0, resolution: 50.0 },
];

#[derive(Clone, Debug)]
pub struct DelaySnapshot {
    pub level: u8,
    pub pre_lowpass: u8,
    pub time_center: u8,
    pub time_ratio_right: u8,
    pub time_ratio_left: u8,
    pub level_center: u8,
    pub level_left: u8,
    pub level_right: u8,
    pub feedback: u8,
    pub send_level_to_reverb: u8,
}

pub struct SpessaSynthDelay {
    delay_left: DelayLine,
    delay_right: DelayLine,
    delay_center: DelayLine,
    sample_rate: f64,
    delay_center_output: Vec<f32>,
    delay_pre_lpf_buf: Vec<f32>,
    delay_center_time: f64,
    delay_left_multiplier: f64,
    delay_right_multiplier: f64,
    gain: f64,
    reverb_gain: f64,

    // Pre-LPF state
    pre_lpf_a: f64,
    pre_lpf_z: f64,

    // Stored parameter values
    send_level_to_reverb: u8,
    pre_lowpass: u8,
    level_right: u8,
    level: u8,
    level_center: u8,
    level_left: u8,
    feedback: u8,
    time_ratio_right: u8,
    time_ratio_left: u8,
    time_center: u8,
}

impl SpessaSynthDelay {
    pub fn new(sample_rate: f64) -> Self {
        let sr = sample_rate as usize;
        let buf_size = 128;
        Self {
            delay_left: DelayLine::new(sr),
            delay_right: DelayLine::new(sr),
            delay_center: DelayLine::new(sr),
            sample_rate,
            delay_center_output: vec![0.0; buf_size],
            delay_pre_lpf_buf: vec![0.0; buf_size],
            delay_center_time: 0.34 * sample_rate,
            delay_left_multiplier: 0.04,
            delay_right_multiplier: 0.04,
            gain: 0.0,
            reverb_gain: 0.0,
            pre_lpf_a: 0.0,
            pre_lpf_z: 0.0,
            send_level_to_reverb: 0,
            pre_lowpass: 0,
            level_right: 0,
            level: 64,
            level_center: 127,
            level_left: 0,
            feedback: 16,
            time_ratio_right: 0,
            time_ratio_left: 0,
            time_center: 12,
        }
    }

    pub fn set_send_level_to_reverb(&mut self, value: u8) {
        self.send_level_to_reverb = value;
        self.reverb_gain = value as f64 / 127.0;
    }

    pub fn set_pre_lowpass(&mut self, value: u8) {
        self.pre_lowpass = value;
        let fc = 8000.0 * 0.63_f64.powi(value as i32);
        let decay_val = (-2.0 * PI * fc / self.sample_rate).exp();
        self.pre_lpf_a = 1.0 - decay_val;
    }

    pub fn set_level_right(&mut self, value: u8) {
        self.level_right = value;
        self.update_gain();
    }

    pub fn set_level(&mut self, value: u8) {
        self.level = value;
        self.gain = value as f64 / 127.0;
    }

    pub fn set_level_center(&mut self, value: u8) {
        self.level_center = value;
        self.update_gain();
    }

    pub fn set_level_left(&mut self, value: u8) {
        self.level_left = value;
        self.update_gain();
    }

    pub fn set_feedback(&mut self, value: u8) {
        self.feedback = value;
        self.delay_left.feedback = 0.0;
        self.delay_right.feedback = 0.0;
        self.delay_center.feedback = (value as f64 - 64.0) / 66.0;
    }

    pub fn set_time_ratio_right(&mut self, value: u8) {
        self.time_ratio_right = value;
        self.delay_right_multiplier = value as f64 * (100.0 / 2400.0);
        self.delay_right
            .set_time((self.delay_center_time * self.delay_right_multiplier) as usize);
    }

    pub fn set_time_ratio_left(&mut self, value: u8) {
        self.time_ratio_left = value;
        self.delay_left_multiplier = value as f64 * (100.0 / 2400.0);
        self.delay_left
            .set_time((self.delay_center_time * self.delay_left_multiplier) as usize);
    }

    pub fn set_time_center(&mut self, value: u8) {
        self.time_center = value;

        let mut delay_ms: f64 = 0.1;
        for seg in DELAY_TIME_SEGMENTS {
            if value >= seg.start && value < seg.end {
                delay_ms =
                    seg.time_start + (value - seg.start) as f64 * seg.resolution;
                break;
            }
        }
        self.delay_center_time = (self.sample_rate * (delay_ms / 1000.0)).max(2.0);
        self.delay_center.set_time(self.delay_center_time as usize);
        self.delay_left
            .set_time((self.delay_center_time * self.delay_left_multiplier) as usize);
        self.delay_right
            .set_time((self.delay_center_time * self.delay_right_multiplier) as usize);
    }

    /// Process delay effect.
    /// - input: 0-based mono input
    /// - output_left/right: start_index-based stereo output (ADDS)
    /// - output_reverb: 0-based mono reverb send (ADDS)
    pub fn process(
        &mut self,
        input: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        output_reverb: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        // Ensure buffers
        if self.delay_center_output.len() < sample_count {
            self.delay_center_output.resize(sample_count, 0.0);
            self.delay_pre_lpf_buf.resize(sample_count, 0.0);
        }

        // Process pre-lowpass
        let use_lpf = self.pre_lowpass > 0;
        if use_lpf {
            let a = self.pre_lpf_a;
            let mut z = self.pre_lpf_z;
            for i in 0..sample_count {
                z += a * (input[i] as f64 - z);
                self.delay_pre_lpf_buf[i] = z as f32;
            }
            self.pre_lpf_z = z;
        }

        let gain = self.gain;
        let reverb_gain = self.reverb_gain;

        // Process center delay first
        {
            let delay_in = if use_lpf {
                &self.delay_pre_lpf_buf[..sample_count]
            } else {
                &input[..sample_count]
            };
            let mut temp_in = vec![0.0f32; sample_count];
            temp_in.copy_from_slice(delay_in);
            self.delay_center.process(&temp_in, &mut self.delay_center_output, sample_count);
        }

        // Mix center into output
        for i in 0..sample_count {
            let sample = self.delay_center_output[i] as f64;
            output_reverb[i] += (sample * reverb_gain) as f32;
            let out_sample = sample * gain;
            let o = i + start_index;
            output_left[o] += out_sample as f32;
            output_right[o] += out_sample as f32;
        }

        // Add dry input into center output (stereo delays take from both)
        for i in 0..sample_count {
            self.delay_center_output[i] += input[i];
        }

        // Process stereo delays (reuse delay_pre_lpf_buf as temp since DelayLine overwrites)
        // Left
        {
            let mut temp_in = vec![0.0f32; sample_count];
            temp_in.copy_from_slice(&self.delay_center_output[..sample_count]);
            self.delay_left.process(&temp_in, &mut self.delay_pre_lpf_buf, sample_count);
        }
        for i in 0..sample_count {
            let sample = self.delay_pre_lpf_buf[i] as f64;
            let o = i + start_index;
            output_left[o] += (sample * gain) as f32;
            output_reverb[i] += (sample * reverb_gain) as f32;
        }

        // Right
        {
            let mut temp_in = vec![0.0f32; sample_count];
            temp_in.copy_from_slice(&self.delay_center_output[..sample_count]);
            let mut temp_out = vec![0.0f32; sample_count];
            self.delay_right.process(&temp_in, &mut temp_out, sample_count);
            // Copy back for mixing
            self.delay_pre_lpf_buf[..sample_count].copy_from_slice(&temp_out[..sample_count]);
        }
        for i in 0..sample_count {
            let sample = self.delay_pre_lpf_buf[i] as f64;
            let o = i + start_index;
            output_right[o] += (sample * gain) as f32;
            output_reverb[i] += (sample * reverb_gain) as f32;
        }
    }

    pub fn get_snapshot(&self) -> DelaySnapshot {
        DelaySnapshot {
            level: self.level,
            pre_lowpass: self.pre_lowpass,
            time_center: self.time_center,
            time_ratio_right: self.time_ratio_right,
            time_ratio_left: self.time_ratio_left,
            level_center: self.level_center,
            level_left: self.level_left,
            level_right: self.level_right,
            feedback: self.feedback,
            send_level_to_reverb: self.send_level_to_reverb,
        }
    }

    fn update_gain(&mut self) {
        self.delay_center.gain = self.level_center as f64 / 127.0;
        self.delay_left.gain = self.level_left as f64 / 127.0;
        self.delay_right.gain = self.level_right as f64 / 127.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 44100.0;
    const EPS: f64 = 1e-6;

    // ---- DELAY_TIME_SEGMENTS tests ----

    #[test]
    fn delay_time_segment_first_entry() {
        // value=0x01: 0.1 + (1-1)*0.1 = 0.1 ms
        let mut d = SpessaSynthDelay::new(SR);
        d.set_time_center(0x01);
        let expected_samples = (SR * 0.1 / 1000.0).max(2.0);
        assert!(
            (d.delay_center_time - expected_samples).abs() < 1.0,
            "value=0x01: expected {} samples, got {}",
            expected_samples,
            d.delay_center_time
        );
    }

    #[test]
    fn delay_time_segment_boundary_0x14() {
        // value=0x14: boundary of seg0/seg1.
        // seg0: start=0x01, end=0x14 → 0x14 is NOT in seg0 (value < end).
        // seg1: start=0x14, end=0x23 → 0x14 is in seg1.
        // delay_ms = 2.0 + (0x14-0x14)*0.2 = 2.0 ms
        let mut d = SpessaSynthDelay::new(SR);
        d.set_time_center(0x14);
        let expected = (SR * 2.0 / 1000.0).max(2.0);
        assert!(
            (d.delay_center_time - expected).abs() < 1.0,
            "value=0x14: expected {} samples, got {}",
            expected,
            d.delay_center_time
        );
    }

    #[test]
    fn delay_time_segment_mid_range() {
        // value=0x37: seg4 (start=0x37, end=0x46), time_start=20.0, res=2.0
        // delay_ms = 20.0 + (0x37-0x37)*2.0 = 20.0 ms
        let mut d = SpessaSynthDelay::new(SR);
        d.set_time_center(0x37);
        let expected = (SR * 20.0 / 1000.0).max(2.0);
        assert!(
            (d.delay_center_time - expected).abs() < 1.0,
            "value=0x37: expected {} samples, got {}",
            expected,
            d.delay_center_time
        );
    }

    #[test]
    fn delay_time_segment_last() {
        // value=0x73 (last valid in seg8): seg8 start=0x69, time_start=500.0, res=50.0
        // delay_ms = 500.0 + (0x73-0x69)*50.0 = 500 + 10*50 = 1000 ms
        let mut d = SpessaSynthDelay::new(SR);
        d.set_time_center(0x73);
        let expected = SR * 1000.0 / 1000.0; // 44100 samples
        assert!(
            (d.delay_center_time - expected).abs() < 1.0,
            "value=0x73: expected {} samples, got {}",
            expected,
            d.delay_center_time
        );
    }

    #[test]
    fn delay_time_minimum_is_2_samples() {
        let mut d = SpessaSynthDelay::new(SR);
        d.set_time_center(0x01);
        assert!(d.delay_center_time >= 2.0);
    }

    // ---- Parameter setter tests ----

    #[test]
    fn set_level_calculates_gain() {
        let mut d = SpessaSynthDelay::new(SR);
        d.set_level(127);
        assert!((d.gain - 1.0).abs() < EPS);
        d.set_level(0);
        assert!(d.gain.abs() < EPS);
    }

    #[test]
    fn set_feedback_calculates_center_feedback() {
        let mut d = SpessaSynthDelay::new(SR);
        d.set_feedback(64);
        // (64 - 64) / 66 = 0
        assert!(d.delay_center.feedback.abs() < EPS);
        d.set_feedback(127);
        // (127 - 64) / 66 ≈ 0.9545
        assert!((d.delay_center.feedback - 63.0 / 66.0).abs() < EPS);
        d.set_feedback(0);
        // (0 - 64) / 66 ≈ -0.9697 (negative feedback)
        assert!((d.delay_center.feedback - (-64.0 / 66.0)).abs() < EPS);
    }

    #[test]
    fn set_feedback_zeroes_stereo_delays() {
        let mut d = SpessaSynthDelay::new(SR);
        d.delay_left.feedback = 1.0;
        d.delay_right.feedback = 1.0;
        d.set_feedback(64);
        assert!(d.delay_left.feedback.abs() < EPS);
        assert!(d.delay_right.feedback.abs() < EPS);
    }

    #[test]
    fn set_time_ratio_left_calculates_multiplier() {
        let mut d = SpessaSynthDelay::new(SR);
        d.set_time_ratio_left(24);
        // 24 * (100/2400) = 1.0
        assert!((d.delay_left_multiplier - 1.0).abs() < EPS);
    }

    #[test]
    fn set_time_ratio_right_calculates_multiplier() {
        let mut d = SpessaSynthDelay::new(SR);
        d.set_time_ratio_right(12);
        // 12 * (100/2400) = 0.5
        assert!((d.delay_right_multiplier - 0.5).abs() < EPS);
    }

    #[test]
    fn set_send_level_to_reverb_gain() {
        let mut d = SpessaSynthDelay::new(SR);
        d.set_send_level_to_reverb(127);
        assert!((d.reverb_gain - 1.0).abs() < EPS);
    }

    #[test]
    fn set_pre_lowpass_coefficient() {
        let mut d = SpessaSynthDelay::new(SR);
        d.set_pre_lowpass(1);
        let fc = 8000.0 * 0.63_f64.powi(1);
        let decay = (-2.0 * PI * fc / SR).exp();
        let expected = 1.0 - decay;
        assert!((d.pre_lpf_a - expected).abs() < EPS);
    }

    #[test]
    fn snapshot_captures_parameters() {
        let mut d = SpessaSynthDelay::new(SR);
        d.set_level(100);
        d.set_feedback(80);
        d.set_time_center(0x20);
        let snap = d.get_snapshot();
        assert_eq!(snap.level, 100);
        assert_eq!(snap.feedback, 80);
        assert_eq!(snap.time_center, 0x20);
    }
}
