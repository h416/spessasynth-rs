//! The default GS Delay implementation, modeled on the delay of the Sound Canvas.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/delay.ts

use std::sync::LazyLock;

use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::delay_line::DelayLine;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::send_input::{
    GAIN_SCALE, LEVEL_UNITY, SendInput,
};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::js_round;

/// SC-8850 delay time segments (manual p.236).
/// How nice of Roland to provide the conversion values to ms!
struct DelayTimeSegment {
    start: usize,
    end: usize,
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

/// The longest DELAY TIME CENTER, 1000 ms. Higher values are held there.
const MAX_TIME_CENTER: usize = 0x73;

/// DELAY TIME CENTER to ms, from the segments above. Index 0 is the same 0.1 ms as 1.
static TIME_CENTER_MS: LazyLock<[f64; MAX_TIME_CENTER + 1]> = LazyLock::new(|| {
    let mut table = [0.1; MAX_TIME_CENTER + 1];
    for segment in DELAY_TIME_SEGMENTS {
        for value in segment.start..segment.end {
            table[value] = segment.time_start + (value - segment.start) as f64 * segment.resolution;
        }
    }
    table
});

/// The longest delay of any tap, in ms. The left and right taps stop here.
const MAX_DELAY_MS: f64 = 1000.0;

/// DELAY FEEDBACK to the loop gain per step from the center (64), -0.97 to +0.95.
const FEEDBACK_STEP: f64 = 124.0 / 8192.0;

/// DELAY TIME RATIO LEFT and RIGHT set their tap in steps of 100/24 % of DELAY TIME CENTER.
const RATIO_STEP: f64 = 1.0 / 24.0;

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

/// The default GS Delay implementation.
///
/// It is modeled on the delay of the Sound Canvas (as Sound Canvas VA runs it): a mono input through a DC blocker
/// and the PRE-LPF into one delay line with three taps.
/// ```text
/// input --> (+) --> delay line --+-- center --> L and R
///            ^                   +-- left ----> L
///            |                   +-- right ---> R
///            +---- x feedback <-- center (previous sample)
/// ```
/// The center always feeds back, at the center time, even with LEVEL CENTER at 0 (hardware and SC-VA agree).
/// The left and right taps follow the center time through their ratios.
/// The send to reverb takes the three taps (the center once) before LEVEL, so it works even at LEVEL 0.
pub struct DefaultGSDelay {
    sample_rate: f64,
    input: SendInput,
    delay_line: DelayLine,
    gain: f64,
    reverb_gain: f64,
    feedback_gain: f64,
    delay_center_ms: f64,
    // The tap delays, in samples
    delay_center: isize,
    delay_left: isize,
    delay_right: isize,
    gain_center: f64,
    gain_left: f64,
    gain_right: f64,
    /// The center tap of the previous sample, which is what feeds back.
    feedback_sample: f64,

    level: u8,
    send_level_to_reverb: u8,
    level_center: u8,
    pre_lowpass: u8,
    feedback: u8,
    level_right: u8,
    time_center: u8,
    level_left: u8,
    time_ratio_right: u8,
    time_ratio_left: u8,
}

impl DefaultGSDelay {
    /// Constructs a new default GS delay processor.
    /// `max_buffer_size` is unused (TS: for consistency across effects).
    pub fn new(sample_rate: f64, _max_buffer_size: usize) -> Self {
        let mut delay = Self {
            sample_rate,
            input: SendInput::new(sample_rate),
            delay_line: DelayLine::new(((MAX_DELAY_MS * sample_rate) / 1000.0).ceil()),
            gain: 1.0,
            reverb_gain: 0.0,
            feedback_gain: 0.0,
            delay_center_ms: 0.0,
            delay_center: 1,
            delay_left: 1,
            delay_right: 1,
            gain_center: 0.0,
            gain_left: 0.0,
            gain_right: 0.0,
            feedback_sample: 0.0,
            level: LEVEL_UNITY as u8,
            send_level_to_reverb: 0,
            level_center: 0,
            pre_lowpass: 0,
            feedback: 64,
            level_right: 0,
            time_center: 0,
            level_left: 0,
            time_ratio_right: 0,
            time_ratio_left: 0,
        };
        delay.reset();
        delay
    }

    pub fn level(&self) -> u8 {
        self.level
    }

    pub fn set_level(&mut self, value: u8) {
        self.level = value;
        self.gain = value as f64 / LEVEL_UNITY;
    }

    pub fn send_level_to_reverb(&self) -> u8 {
        self.send_level_to_reverb
    }

    pub fn set_send_level_to_reverb(&mut self, value: u8) {
        self.send_level_to_reverb = value;
        self.reverb_gain = value as f64 / GAIN_SCALE;
    }

    pub fn level_center(&self) -> u8 {
        self.level_center
    }

    pub fn set_level_center(&mut self, value: u8) {
        self.level_center = value;
        self.gain_center = value as f64 / GAIN_SCALE;
    }

    pub fn pre_lowpass(&self) -> u8 {
        self.pre_lowpass
    }

    pub fn set_pre_lowpass(&mut self, value: u8) {
        self.pre_lowpass = value;
        self.input.set_pre_low_pass(value as f64);
    }

    pub fn feedback(&self) -> u8 {
        self.feedback
    }

    pub fn set_feedback(&mut self, value: u8) {
        // 64 is no feedback, below it the phase is inverted
        self.feedback_gain = (value as f64 - 64.0) * FEEDBACK_STEP;
        self.feedback = value;
    }

    pub fn level_right(&self) -> u8 {
        self.level_right
    }

    pub fn set_level_right(&mut self, value: u8) {
        self.level_right = value;
        self.gain_right = value as f64 / GAIN_SCALE;
    }

    pub fn time_center(&self) -> u8 {
        self.time_center
    }

    pub fn set_time_center(&mut self, value: u8) {
        // The line holds repeats at the old time
        if value != self.time_center {
            self.clear();
        }
        self.time_center = value;
        self.delay_center_ms = TIME_CENTER_MS[(value as usize).min(MAX_TIME_CENTER)];
        self.update_delay_times();
    }

    pub fn level_left(&self) -> u8 {
        self.level_left
    }

    pub fn set_level_left(&mut self, value: u8) {
        self.level_left = value;
        self.gain_left = value as f64 / GAIN_SCALE;
    }

    pub fn time_ratio_right(&self) -> u8 {
        self.time_ratio_right
    }

    pub fn set_time_ratio_right(&mut self, value: u8) {
        self.time_ratio_right = value;
        self.update_delay_times();
    }

    pub fn time_ratio_left(&self) -> u8 {
        self.time_ratio_left
    }

    pub fn set_time_ratio_left(&mut self, value: u8) {
        self.time_ratio_left = value;
        self.update_delay_times();
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
        let gain = self.gain;
        let reverb_gain = self.reverb_gain;
        let delay_center = self.delay_center;
        let delay_left = self.delay_left;
        let delay_right = self.delay_right;
        let feedback_gain = self.feedback_gain;
        let gain_center = self.gain_center;
        let gain_left = self.gain_left;
        let gain_right = self.gain_right;
        let mut feedback_sample = self.feedback_sample;

        for i in 0..sample_count {
            let x = self.input.process(input[i] as f64);
            self.delay_line.write(x + feedback_sample * feedback_gain);
            feedback_sample = self.delay_line.read(delay_center);
            let center = feedback_sample * gain_center;
            let left = self.delay_line.read(delay_left) * gain_left;
            let right = self.delay_line.read(delay_right) * gain_right;

            let o = start_index + i;
            output_left[o] = (output_left[o] as f64 + (left + center) * gain) as f32;
            output_right[o] = (output_right[o] as f64 + (right + center) * gain) as f32;
            output_reverb[i] = (output_reverb[i] as f64 + (left + right + center) * reverb_gain) as f32;
        }
        self.feedback_sample = feedback_sample;
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.clear();
        // Delay 1
        self.set_level(LEVEL_UNITY as u8);
        self.set_pre_lowpass(0);
        self.set_time_center(97);
        // TS: this.timeRatioLeft = this.timeRatioRight = 1 (right is set first)
        self.set_time_ratio_right(1);
        self.set_time_ratio_left(1);
        self.set_level_center(127);
        // TS: this.levelLeft = this.levelRight = 0
        self.set_level_right(0);
        self.set_level_left(0);
        self.set_feedback(80);
        self.set_send_level_to_reverb(0);
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

    fn clear(&mut self) {
        self.delay_line.clear();
        self.feedback_sample = 0.0;
    }

    fn update_delay_times(&mut self) {
        // The left and right taps follow the center time and stop at the end of the line
        let to_samples = self.sample_rate / 1000.0;
        let center = self.delay_center_ms;
        let left = MAX_DELAY_MS.min(center * (self.time_ratio_left as f64 * RATIO_STEP));
        let right = MAX_DELAY_MS.min(center * (self.time_ratio_right as f64 * RATIO_STEP));
        self.delay_center = js_round(center * to_samples).max(1.0) as isize;
        self.delay_left = js_round(left * to_samples).max(1.0) as isize;
        self.delay_right = js_round(right * to_samples).max(1.0) as isize;
    }
}
