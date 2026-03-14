use super::dattorro::DattorroReverb;
use super::delay_line::DelayLine;

#[derive(Clone, Debug)]
pub struct ReverbSnapshot {
    pub level: u8,
    pub pre_lowpass: u8,
    pub character: u8,
    pub time: u8,
    pub delay_feedback: u8,
    pub pre_delay_time: u8,
}

pub struct SpessaSynthReverb {
    dattorro: DattorroReverb,
    delay_left: DelayLine,
    delay_right: DelayLine,
    delay_left_output: Vec<f32>,
    delay_right_output: Vec<f32>,
    delay_left_input: Vec<f32>,
    delay_pre_lpf: Vec<f32>,
    sample_rate: f64,

    // Pre-LPF state
    pre_lpf_a: f64,
    pre_lpf_z: f64,

    // Character coefficients
    character_time_coefficient: f64,
    character_gain_coefficient: f64,
    character_lpf_coefficient: f64,

    // Delay mode state
    delay_gain: f64,
    pan_delay_feedback: f64,

    // Stored parameter values (0-127)
    character: u8,
    time: u8,
    level: u8,
    pre_lowpass: u8,
    pre_delay_time: u8,
    delay_feedback: u8,
}

impl SpessaSynthReverb {
    pub fn new(sample_rate: f64) -> Self {
        let buf_size = 128;
        let mut reverb = Self {
            dattorro: DattorroReverb::new(sample_rate),
            delay_left: DelayLine::new(sample_rate as usize),
            delay_right: DelayLine::new(sample_rate as usize),
            delay_left_output: vec![0.0; buf_size],
            delay_right_output: vec![0.0; buf_size],
            delay_left_input: vec![0.0; buf_size],
            delay_pre_lpf: vec![0.0; buf_size],
            sample_rate,
            pre_lpf_a: 0.0,
            pre_lpf_z: 0.0,
            character_time_coefficient: 1.0,
            character_gain_coefficient: 1.0,
            character_lpf_coefficient: 0.0,
            delay_gain: 1.0,
            pan_delay_feedback: 0.0,
            character: 0,
            time: 0,
            level: 0,
            pre_lowpass: 0,
            pre_delay_time: 0,
            delay_feedback: 0,
        };
        reverb.set_character(0);
        reverb
    }

    pub fn set_character(&mut self, value: u8) {
        self.character = value;
        self.dattorro.damping = 0.005;
        self.character_time_coefficient = 1.0;
        self.character_gain_coefficient = 1.0;
        self.character_lpf_coefficient = 0.0;
        self.dattorro.input_diffusion1 = 0.75;
        self.dattorro.input_diffusion2 = 0.625;
        self.dattorro.decay_diffusion1 = 0.7;
        self.dattorro.decay_diffusion2 = 0.5;
        self.dattorro.excursion_rate = 0.5;
        self.dattorro.excursion_depth = 0.7;

        match value {
            0 => {
                // Room1
                self.dattorro.damping = 0.85;
                self.character_time_coefficient = 0.9;
                self.character_gain_coefficient = 0.7;
                self.character_lpf_coefficient = 0.2;
            }
            1 => {
                // Room2
                self.dattorro.damping = 0.2;
                self.character_gain_coefficient = 0.5;
                self.character_time_coefficient = 1.0;
                self.dattorro.decay_diffusion2 = 0.64;
                self.dattorro.decay_diffusion1 = 0.6;
                self.character_lpf_coefficient = 0.2;
            }
            2 => {
                // Room3
                self.dattorro.damping = 0.56;
                self.character_gain_coefficient = 0.55;
                self.character_time_coefficient = 1.0;
                self.dattorro.decay_diffusion2 = 0.64;
                self.dattorro.decay_diffusion1 = 0.6;
                self.character_lpf_coefficient = 0.1;
            }
            3 => {
                // Hall1
                self.dattorro.damping = 0.6;
                self.character_gain_coefficient = 1.0;
                self.character_lpf_coefficient = 0.0;
                self.dattorro.decay_diffusion2 = 0.7;
                self.dattorro.decay_diffusion1 = 0.66;
            }
            4 => {
                // Hall2
                self.character_gain_coefficient = 0.75;
                self.dattorro.damping = 0.2;
                self.character_lpf_coefficient = 0.2;
            }
            5 => {
                // Plate
                self.character_gain_coefficient = 0.55;
                self.dattorro.damping = 0.65;
                self.character_time_coefficient = 0.5;
            }
            _ => {}
        }

        self.update_time();
        self.update_gain();
        self.update_lowpass();
        self.update_feedback();
        self.delay_left.clear();
        self.delay_right.clear();
    }

    pub fn set_time(&mut self, value: u8) {
        self.time = value;
        self.update_time();
    }

    pub fn set_pre_delay_time(&mut self, value: u8) {
        self.pre_delay_time = value;
        self.dattorro.pre_delay = (value as f64 / 1000.0) * self.sample_rate;
    }

    pub fn set_level(&mut self, value: u8) {
        self.level = value;
        self.update_gain();
    }

    pub fn set_pre_lowpass(&mut self, value: u8) {
        self.pre_lowpass = value;
        let fc = 8000.0 * 0.63_f64.powi(self.pre_lowpass as i32);
        let decay = (-2.0 * std::f64::consts::PI * fc / self.sample_rate).exp();
        self.pre_lpf_a = 1.0 - decay;
        self.update_lowpass();
    }

    pub fn set_delay_feedback(&mut self, value: u8) {
        self.delay_feedback = value;
        self.update_feedback();
    }

    pub fn process(
        &mut self,
        input: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        match self.character {
            6 => self.process_mono_delay(input, output_left, output_right, start_index, sample_count),
            7 => self.process_panning_delay(input, output_left, output_right, start_index, sample_count),
            _ => {
                self.dattorro.process(input, output_left, output_right, start_index, sample_count);
            }
        }
    }

    pub fn get_snapshot(&self) -> ReverbSnapshot {
        ReverbSnapshot {
            level: self.level,
            pre_lowpass: self.pre_lowpass,
            character: self.character,
            time: self.time,
            delay_feedback: self.delay_feedback,
            pre_delay_time: self.pre_delay_time,
        }
    }

    fn ensure_buffers(&mut self, sample_count: usize) {
        if self.delay_left_output.len() < sample_count {
            self.delay_left_output.resize(sample_count, 0.0);
            self.delay_right_output.resize(sample_count, 0.0);
            self.delay_left_input.resize(sample_count, 0.0);
            self.delay_pre_lpf.resize(sample_count, 0.0);
        }
    }

    fn apply_pre_lpf<'a>(&mut self, input: &'a [f32], sample_count: usize) -> bool {
        if self.pre_lowpass > 0 {
            let a = self.pre_lpf_a;
            let mut z = self.pre_lpf_z;
            for i in 0..sample_count {
                z += a * (input[i] as f64 - z);
                self.delay_pre_lpf[i] = z as f32;
            }
            self.pre_lpf_z = z;
            true
        } else {
            false
        }
    }

    fn process_mono_delay(
        &mut self,
        input: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        self.ensure_buffers(sample_count);
        let used_lpf = self.apply_pre_lpf(input, sample_count);
        let delay_in: &[f32] = if used_lpf {
            &self.delay_pre_lpf[..sample_count]
        } else {
            &input[..sample_count]
        };

        // Process delay - need to copy input since we borrow self mutably
        let mut temp_input = vec![0.0f32; sample_count];
        temp_input[..sample_count].copy_from_slice(&delay_in[..sample_count]);

        self.delay_left.process(&temp_input, &mut self.delay_left_output, sample_count);

        let g = self.delay_gain;
        for i in 0..sample_count {
            let sample = (self.delay_left_output[i] as f64 * g) as f32;
            let o = i + start_index;
            output_left[o] += sample;
            output_right[o] += sample;
        }
    }

    fn process_panning_delay(
        &mut self,
        input: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        self.ensure_buffers(sample_count);
        let used_lpf = self.apply_pre_lpf(input, sample_count);

        // Mix right output into left input
        let fb = self.pan_delay_feedback;
        for i in 0..sample_count {
            let dry = if used_lpf { self.delay_pre_lpf[i] as f64 } else { input[i] as f64 };
            self.delay_left_input[i] = (dry + self.delay_right_output[i] as f64 * fb) as f32;
        }

        // Process left
        let mut temp = vec![0.0f32; sample_count];
        temp[..sample_count].copy_from_slice(&self.delay_left_input[..sample_count]);
        self.delay_left.process(&temp, &mut self.delay_left_output, sample_count);

        // Process right (from left output)
        temp[..sample_count].copy_from_slice(&self.delay_left_output[..sample_count]);
        self.delay_right.process(&temp, &mut self.delay_right_output, sample_count);

        // Mix
        let g = self.delay_gain;
        for i in 0..sample_count {
            let o = i + start_index;
            output_left[o] += (self.delay_left_output[i] as f64 * g) as f32;
            output_right[o] += (self.delay_right_output[i] as f64 * g) as f32;
        }
    }

    fn update_feedback(&mut self) {
        let x = self.delay_feedback as f64 / 127.0;
        let exp = 1.0 - (1.0 - x).powf(1.9);
        if self.character == 6 {
            self.delay_left.feedback = exp * 0.73;
        } else {
            self.delay_left.feedback = 0.0;
            self.delay_right.feedback = 0.0;
            self.pan_delay_feedback = exp * 0.73;
        }
    }

    fn update_lowpass(&mut self) {
        self.dattorro.pre_lpf = (0.1 + (7.0 - self.pre_lowpass as f64) / 14.0
            + self.character_lpf_coefficient)
            .min(1.0);
    }

    fn update_gain(&mut self) {
        self.dattorro.gain = (self.level as f64 / 348.0) * self.character_gain_coefficient;
        self.delay_gain = self.level as f64 / 127.0;
    }

    fn update_time(&mut self) {
        let t = self.time as f64 / 127.0;
        self.dattorro.decay = self.character_time_coefficient * (0.05 + 0.65 * t);
        let time_samples = (t * self.sample_rate * 0.4468) as usize;
        let time_samples = time_samples.max(21);
        if self.character == 7 {
            let half = time_samples / 2;
            self.delay_left.set_time(half);
            self.delay_right.set_time(half);
        } else {
            self.delay_left.set_time(time_samples);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 44100.0;
    const EPS: f64 = 1e-6;

    // ---- Character tests ----

    #[test]
    fn character_0_room1_coefficients() {
        let mut r = SpessaSynthReverb::new(SR);
        r.set_character(0);
        assert!((r.dattorro.damping - 0.85).abs() < EPS);
        assert!((r.character_time_coefficient - 0.9).abs() < EPS);
        assert!((r.character_gain_coefficient - 0.7).abs() < EPS);
        assert!((r.character_lpf_coefficient - 0.2).abs() < EPS);
    }

    #[test]
    fn character_3_hall1_coefficients() {
        let mut r = SpessaSynthReverb::new(SR);
        r.set_character(3);
        assert!((r.dattorro.damping - 0.6).abs() < EPS);
        assert!((r.character_gain_coefficient - 1.0).abs() < EPS);
        assert!((r.character_lpf_coefficient - 0.0).abs() < EPS);
    }

    #[test]
    fn character_5_plate_coefficients() {
        let mut r = SpessaSynthReverb::new(SR);
        r.set_character(5);
        assert!((r.character_gain_coefficient - 0.55).abs() < EPS);
        assert!((r.dattorro.damping - 0.65).abs() < EPS);
        assert!((r.character_time_coefficient - 0.5).abs() < EPS);
    }

    #[test]
    fn character_resets_defaults_before_applying() {
        let mut r = SpessaSynthReverb::new(SR);
        r.set_character(0); // Room1 sets damping=0.85
        r.set_character(3); // Hall1 sets damping=0.6
        assert!((r.dattorro.damping - 0.6).abs() < EPS);
    }

    // ---- Gain/level tests ----

    #[test]
    fn set_level_updates_dattorro_gain() {
        let mut r = SpessaSynthReverb::new(SR);
        r.set_character(0); // gain_coefficient = 0.7
        r.set_level(127);
        let expected = (127.0_f64 / 348.0) * 0.7;
        assert!((r.dattorro.gain - expected).abs() < EPS);
    }

    #[test]
    fn set_level_updates_delay_gain() {
        let mut r = SpessaSynthReverb::new(SR);
        r.set_level(127);
        assert!((r.delay_gain - 1.0).abs() < EPS);
    }

    // ---- Time tests ----

    #[test]
    fn set_time_updates_decay() {
        let mut r = SpessaSynthReverb::new(SR);
        r.set_character(0); // time_coefficient = 0.9
        r.set_time(127);
        let expected = 0.9 * (0.05 + 0.65 * 1.0); // = 0.63
        assert!((r.dattorro.decay - expected).abs() < 0.001);
    }

    #[test]
    fn set_time_zero_gives_minimum_delay() {
        let mut r = SpessaSynthReverb::new(SR);
        r.set_time(0);
        assert!(r.delay_left.time() >= 21);
    }

    // ---- Pre-delay tests ----

    #[test]
    fn set_pre_delay_time_converts_to_samples() {
        let mut r = SpessaSynthReverb::new(SR);
        r.set_pre_delay_time(100);
        let expected = (100.0_f64 / 1000.0) * SR;
        assert!((r.dattorro.pre_delay - expected).abs() < 1.0);
    }

    // ---- Feedback tests ----

    #[test]
    fn set_delay_feedback_mono_character6() {
        let mut r = SpessaSynthReverb::new(SR);
        r.set_character(6);
        r.set_delay_feedback(127);
        let x = 127.0_f64 / 127.0;
        let exp = 1.0 - (1.0_f64 - x).powf(1.9);
        assert!((r.delay_left.feedback - exp * 0.73).abs() < EPS);
    }

    #[test]
    fn set_delay_feedback_panning_character7() {
        let mut r = SpessaSynthReverb::new(SR);
        r.set_character(7);
        r.set_delay_feedback(127);
        // For character 7, left/right feedback = 0, pan_delay_feedback gets the value
        assert!((r.delay_left.feedback).abs() < EPS);
        assert!((r.delay_right.feedback).abs() < EPS);
        let x = 1.0_f64;
        let exp = 1.0 - (1.0 - x).powf(1.9);
        assert!((r.pan_delay_feedback - exp * 0.73).abs() < EPS);
    }

    // ---- Lowpass tests ----

    #[test]
    fn set_pre_lowpass_updates_dattorro_lpf() {
        let mut r = SpessaSynthReverb::new(SR);
        r.set_character(0); // lpf_coefficient = 0.2
        r.set_pre_lowpass(7);
        let expected = (0.1 + (7.0 - 7.0_f64) / 14.0 + 0.2).min(1.0);
        assert!((r.dattorro.pre_lpf - expected).abs() < EPS);
    }

    // ---- Snapshot tests ----

    #[test]
    fn snapshot_captures_all_parameters() {
        let mut r = SpessaSynthReverb::new(SR);
        r.set_character(3);
        r.set_time(64);
        r.set_level(100);
        r.set_pre_lowpass(5);
        r.set_pre_delay_time(50);
        r.set_delay_feedback(80);
        let snap = r.get_snapshot();
        assert_eq!(snap.character, 3);
        assert_eq!(snap.time, 64);
        assert_eq!(snap.level, 100);
        assert_eq!(snap.pre_lowpass, 5);
        assert_eq!(snap.pre_delay_time, 50);
        assert_eq!(snap.delay_feedback, 80);
    }
}
