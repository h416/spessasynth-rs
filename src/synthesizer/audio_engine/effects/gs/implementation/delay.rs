//! The default GS Delay implementation.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/delay.ts
//!
//! A single shared buffer: center, left and right are read positions in it, and only the
//! center delay feeds back.
use std::f64::consts::PI;

const DELAY_GAIN: f64 = 1.66;

/// SC-8850 delay time segments (manual p.236).
/// How nice of Roland to provide the conversion values to ms!
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

/// Equivalent to: GSDelayParameter
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

/// Equivalent to: class DefaultGSDelay
pub struct DefaultGSDelay {
    /// Alpha
    pre_lpf_a: f64,
    /// Previous value
    pre_lpf_z: f64,
    buffer: Vec<f32>,
    sample_rate: f64,
    delay_pre_lpf: Vec<f32>,
    delay_left_multiplier: f64,
    delay_right_multiplier: f64,
    gain: f64,
    reverb_gain: f64,
    feedback_gain: f64,
    /// Samples
    delay_center: i64,
    /// Samples
    delay_left: i64,
    /// Samples
    delay_right: i64,
    gain_center: f64,
    gain_left: f64,
    gain_right: f64,
    write_index: i64,

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

impl DefaultGSDelay {
    /// Constructs a new default GS delay processor.
    pub fn new(sample_rate: f64, max_buffer_size: usize) -> Self {
        let delay_center = (0.34 * sample_rate).floor() as i64;
        let level = 64u8;
        Self {
            pre_lpf_a: 0.0,
            pre_lpf_z: 0.0,
            buffer: vec![0.0; sample_rate as usize],
            sample_rate,
            delay_pre_lpf: vec![0.0; max_buffer_size],
            delay_left_multiplier: 0.04,
            delay_right_multiplier: 0.04,
            gain: (level as f64 / 127.0) * DELAY_GAIN,
            reverb_gain: 0.0,
            feedback_gain: 0.0,
            delay_center,
            delay_left: (delay_center as f64 * 0.04).floor() as i64,
            delay_right: (delay_center as f64 * 0.04).floor() as i64,
            gain_center: 1.0,
            gain_left: 0.0,
            gain_right: 0.0,
            write_index: 0,
            send_level_to_reverb: 0,
            pre_lowpass: 0,
            level_right: 0,
            level,
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
        // GS sure loves weird mappings, huh?
        // Maps to around 8000-300 Hz
        let pre_lpf_fc = 8000.0 * 0.63_f64.powf(value as f64);
        let decay = ((-2.0 * PI * pre_lpf_fc) / self.sample_rate).exp();
        self.pre_lpf_a = 1.0 - decay;
    }

    pub fn set_level_right(&mut self, value: u8) {
        self.level_right = value;
        self.update_gain();
    }

    pub fn set_level(&mut self, value: u8) {
        self.level = value;
        self.gain = (value as f64 / 127.0) * DELAY_GAIN;
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
        // -64 means max at inverted phase
        // Use 66 for it to not be infinite (-1)
        self.feedback_gain = (value as f64 - 64.0) / 66.0;
        self.feedback = value;
    }

    pub fn set_time_ratio_right(&mut self, value: u8) {
        self.time_ratio_right = value;
        // The resolution is 100/24(%).
        // Turn that into multiplier
        self.delay_right_multiplier = value as f64 * (100.0 / 2400.0);
        self.update_delay_times();
    }

    pub fn set_time_ratio_left(&mut self, value: u8) {
        self.time_ratio_left = value;
        // The resolution is 100/24(%).
        // Turn that into multiplier
        self.delay_left_multiplier = value as f64 * (100.0 / 2400.0);
        self.update_delay_times();
    }

    pub fn set_time_center(&mut self, value: u8) {
        self.time_center = value;

        let mut delay_ms: f64 = 0.1;
        for seg in DELAY_TIME_SEGMENTS {
            if value >= seg.start && value < seg.end {
                delay_ms = seg.time_start + (value - seg.start) as f64 * seg.resolution;
                break;
            }
        }
        self.delay_center = ((self.buffer.len() - 1) as f64)
            .min((self.sample_rate * (delay_ms / 1000.0)).max(2.0))
            .floor() as i64;
        self.update_delay_times();
        self.buffer.fill(0.0);
    }

    /// Process the effect and ADDS it to the output.
    /// - input: 0-based mono input
    /// - output_left/right: start_index-based stereo output
    /// - output_reverb: 0-based mono reverb input
    pub fn process(
        &mut self,
        input: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        output_reverb: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        // Process pre-lowpass
        let delay_in: &[f32] = if self.pre_lowpass > 0 {
            let mut z = self.pre_lpf_z;
            let a = self.pre_lpf_a;
            for i in 0..sample_count {
                let x = input[i] as f64;
                z += a * (x - z);
                self.delay_pre_lpf[i] = z as f32;
            }
            self.pre_lpf_z = z;
            &self.delay_pre_lpf
        } else {
            input
        };

        // Connections are:
        // center delay -> stereo output (and reverb)
        // center delay -> center feedback
        // stereo delays only connect to the output.
        // Also level is separate from reverb send level,
        // i.e. level = 0 and reverb send level = 127 will still send sound to reverb.
        //
        // Center always sends to stereo, regardless of level center in hardware and latest
        // SCVA, older revisions incorrectly don't send it,
        // So level center = 0, level left = 127 will still have feedback.
        // Also feedback time is always time center, even if only left delay is playing.
        let gain = self.gain;
        let reverb_gain = self.reverb_gain;
        let delay_center = self.delay_center;
        let delay_left = self.delay_left;
        let delay_right = self.delay_right;
        let feedback_gain = self.feedback_gain;
        let buffer = &mut self.buffer;
        let mut write_index = self.write_index;
        let buffer_length = buffer.len() as i64;
        let center_gain = self.gain_center * gain;
        let left_gain = self.gain_left * gain;
        let right_gain = self.gain_right * gain;

        // Float32Array `+=`: the sum is done in f64, then stored as f32.
        fn add(dst: &mut f32, v: f64) {
            *dst = (*dst as f64 + v) as f32;
        }

        for i in 0..sample_count {
            // Read center
            let mut center_read_index = write_index - delay_center;
            if center_read_index < 0 {
                center_read_index += buffer_length;
            }

            // Read left
            let mut left_read_index = (write_index - delay_left) % buffer_length;
            if left_read_index < 0 {
                left_read_index += buffer_length;
            }

            // Read right
            let mut right_read_index = (write_index - delay_right) % buffer_length;
            if right_read_index < 0 {
                right_read_index += buffer_length;
            }

            // Write center
            let o = start_index + i;
            let delayed = buffer[center_read_index as usize] as f64;
            let c = delayed * center_gain;
            add(&mut output_left[o], c);
            add(&mut output_right[o], c);
            add(&mut output_reverb[i], c * reverb_gain);

            // Center feedback, do it first so left and right delay of 0 work fine
            // Testcase: gs_effect_send_level_test
            buffer[write_index as usize] = (delay_in[i] as f64 + delayed * feedback_gain) as f32;

            // Write left
            let l = buffer[left_read_index as usize] as f64 * left_gain;
            add(&mut output_left[o], l);
            add(&mut output_reverb[i], l * reverb_gain);

            // Write right
            let r = buffer[right_read_index as usize] as f64 * right_gain;
            add(&mut output_right[o], r);
            add(&mut output_reverb[i], r * reverb_gain);

            // Advance and wrap
            write_index += 1;
            if write_index >= buffer_length {
                write_index = 0;
            }
        }
        self.write_index = write_index;
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

    /// Restores this effect processor instance from a snapshot.
    /// Equivalent to: applySnapshot(snapshot)
    pub fn apply_snapshot(&mut self, snapshot: &DelaySnapshot) {
        self.set_level(snapshot.level);
        self.set_pre_lowpass(snapshot.pre_lowpass);
        self.set_time_center(snapshot.time_center);
        self.set_time_ratio_left(snapshot.time_ratio_left);
        self.set_time_ratio_right(snapshot.time_ratio_right);
        self.set_level_center(snapshot.level_center);
        self.set_level_left(snapshot.level_left);
        self.set_level_right(snapshot.level_right);
        self.set_feedback(snapshot.feedback);
        self.set_send_level_to_reverb(snapshot.send_level_to_reverb);
    }

    fn update_delay_times(&mut self) {
        // Recompute times
        let max_delay = self.buffer.len() as i64 - 1;
        self.delay_left =
            max_delay.min((self.delay_center as f64 * self.delay_left_multiplier).floor() as i64);
        self.delay_right =
            max_delay.min((self.delay_center as f64 * self.delay_right_multiplier).floor() as i64);
    }

    fn update_gain(&mut self) {
        // Center gain is applied in post
        self.gain_center = self.level_center as f64 / 127.0;
        self.gain_left = self.level_left as f64 / 127.0;
        self.gain_right = self.level_right as f64 / 127.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    #[test]
    fn time_center_segments() {
        let mut d = DefaultGSDelay::new(48_000.0, 128);
        // 0x01 -> 0.1 ms -> clamped to 2 samples
        d.set_time_center(0x01);
        assert_eq!(d.delay_center, 4);
        // 0x14 -> 2 ms
        d.set_time_center(0x14);
        assert_eq!(d.delay_center, 96);
        // 0x73 -> 500 + 10 * 50 = 1000 ms -> capped to buffer length - 1
        d.set_time_center(0x73);
        assert_eq!(d.delay_center, 47_999);
    }

    #[test]
    fn initial_gain_follows_default_level() {
        let d = DefaultGSDelay::new(44_100.0, 128);
        assert!((d.gain - (64.0 / 127.0) * DELAY_GAIN).abs() < EPS);
        assert_eq!(d.delay_center, (0.34f64 * 44_100.0).floor() as i64);
    }

    #[test]
    fn feedback_gain() {
        let mut d = DefaultGSDelay::new(44_100.0, 128);
        d.set_feedback(64);
        assert!(d.feedback_gain.abs() < EPS);
        d.set_feedback(127);
        assert!((d.feedback_gain - 63.0 / 66.0).abs() < EPS);
        d.set_feedback(0);
        assert!((d.feedback_gain + 64.0 / 66.0).abs() < EPS);
    }

    #[test]
    fn time_ratios_scale_from_center() {
        let mut d = DefaultGSDelay::new(44_100.0, 128);
        d.set_time_ratio_left(24);
        d.set_time_ratio_right(12);
        assert_eq!(d.delay_left, d.delay_center);
        assert_eq!(d.delay_right, (d.delay_center as f64 * 0.5).floor() as i64);
    }

    #[test]
    fn impulse_comes_back_after_center_time() {
        let mut d = DefaultGSDelay::new(1000.0, 1000);
        d.set_time_center(0x2d); // 10 ms -> 10 samples at 1 kHz
        let mut input = vec![0.0f32; 64];
        input[0] = 1.0;
        let mut l = vec![0.0f32; 64];
        let mut r = vec![0.0f32; 64];
        let mut rev = vec![0.0f32; 64];
        d.process(&input, &mut l, &mut r, &mut rev, 0, 64);
        let first = l.iter().position(|&x| x != 0.0).unwrap();
        assert_eq!(first, 10);
        assert_eq!(l, r);
    }

    #[test]
    fn snapshot_captures_parameters() {
        let mut d = DefaultGSDelay::new(44_100.0, 128);
        d.set_level(100);
        d.set_feedback(80);
        d.set_time_center(0x20);
        let snap = d.get_snapshot();
        assert_eq!(snap.level, 100);
        assert_eq!(snap.feedback, 80);
        assert_eq!(snap.time_center, 0x20);
    }
}
