//! The default GS chorus implementation, modeled on the chorus of the Sound Canvas.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/chorus.ts

use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::delay_line::DelayLine;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::lfo::Lfo;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::send_input::{
    GAIN_SCALE, LEVEL_UNITY, SendInput,
};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::hardware::HARDWARE_RATE;

/// Equivalent to: GSChorusParameter
#[derive(Clone, Debug, PartialEq)]
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

/// CHORUS RATE to Hz. The LFO adds 64 x rate to a 24 bit phase every 32 kHz sample (15.5 Hz at 127).
const RATE_HZ: f64 = (64.0 * HARDWARE_RATE) / 16_777_216.0;

/// CHORUS DELAY in 32 kHz samples per step (23.8 ms at 127).
const DELAY_STEP: f64 = 6.0;

/// CHORUS DEPTH in 32 kHz samples per step: what the LFO adds on top of the delay, with one step at 0
/// (20 ms at 127).
const DEPTH_STEP: f64 = 5.0;

/// The default GS Chorus implementation.
///
/// It is modeled on the chorus of the Sound Canvas (as Sound Canvas VA runs it): a mono input through a DC blocker
/// and the PRE-LPF into one delay line, read at two places that a triangle LFO moves in opposite directions.
/// The first read is the left output and the feedback, the second one the right output.
/// The reads interpolate linearly between samples, like the Sound Canvas does at 32 kHz.
pub struct DefaultGSChorus {
    /// One 32 kHz sample in host samples.
    step: f64,
    input: SendInput,
    lfo: Lfo,
    delay_line: DelayLine,
    gain: f64,
    reverb_gain: f64,
    delay_gain: f64,
    feedback_gain: f64,
    // The delay at the bottom of the LFO's swing and the swing on top of it, in host samples
    delay_samples: f64,
    depth_samples: f64,
    /// The first read of the previous sample, which is what feeds back.
    feedback_sample: f64,

    level: u8,
    send_level_to_reverb: u8,
    send_level_to_delay: u8,
    pre_lowpass: u8,
    depth: u8,
    delay: u8,
    feedback: u8,
    rate: u8,
}

impl DefaultGSChorus {
    /// Constructs a new default GS chorus processor.
    /// `max_buffer_size` is unused (TS: for consistency across effects).
    pub fn new(sample_rate: f64, _max_buffer_size: usize) -> Self {
        let step = sample_rate / HARDWARE_RATE;
        let mut chorus = Self {
            step,
            input: SendInput::new(sample_rate),
            lfo: Lfo::new(sample_rate),
            // The longest read, plus the older point of the linear read
            delay_line: DelayLine::new(((127.0 * DELAY_STEP + 128.0 * DEPTH_STEP) * step).ceil() + 1.0),
            gain: 1.0,
            reverb_gain: 0.0,
            delay_gain: 0.0,
            feedback_gain: 0.0,
            delay_samples: 0.0,
            depth_samples: 0.0,
            feedback_sample: 0.0,
            level: LEVEL_UNITY as u8,
            send_level_to_reverb: 0,
            send_level_to_delay: 0,
            pre_lowpass: 0,
            depth: 0,
            delay: 0,
            feedback: 0,
            rate: 0,
        };
        chorus.reset();
        chorus
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

    /// Read back the raw GS send level (0-127). Used by `update_active_gs_effects`.
    pub fn send_level_to_delay(&self) -> u8 {
        self.send_level_to_delay
    }

    pub fn set_send_level_to_delay(&mut self, value: u8) {
        self.send_level_to_delay = value;
        self.delay_gain = value as f64 / GAIN_SCALE;
    }

    pub fn pre_lowpass(&self) -> u8 {
        self.pre_lowpass
    }

    pub fn set_pre_lowpass(&mut self, value: u8) {
        self.pre_lowpass = value;
        self.input.set_pre_low_pass(value as f64);
    }

    pub fn depth(&self) -> u8 {
        self.depth
    }

    pub fn set_depth(&mut self, value: u8) {
        self.depth = value;
        self.depth_samples = ((value as f64).clamp(0.0, 127.0) + 1.0) * DEPTH_STEP * self.step;
    }

    pub fn delay(&self) -> u8 {
        self.delay
    }

    pub fn set_delay(&mut self, value: u8) {
        self.delay = value;
        self.delay_samples = (value as f64).clamp(0.0, 127.0) * DELAY_STEP * self.step;
    }

    pub fn feedback(&self) -> u8 {
        self.feedback
    }

    pub fn set_feedback(&mut self, value: u8) {
        self.feedback = value;
        self.feedback_gain = value as f64 / GAIN_SCALE;
    }

    pub fn rate(&self) -> u8 {
        self.rate
    }

    pub fn set_rate(&mut self, value: u8) {
        self.rate = value;
        self.lfo.set_rate(value as f64 * RATE_HZ);
    }

    /// - input: 0-based mono input
    /// - output_left/right: start_index-based stereo output (ADDS)
    /// - output_reverb/delay: 0-based mono send outputs (ADDS)
    #[allow(clippy::too_many_arguments)]
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
        let gain = self.gain;
        let reverb_gain = self.reverb_gain;
        let delay_gain = self.delay_gain;
        let feedback_gain = self.feedback_gain;
        let delay_samples = self.delay_samples;
        let depth_samples = self.depth_samples;
        let mut feedback_sample = self.feedback_sample;
        for i in 0..sample_count {
            // The feedback does not pass through the input stage
            let x = self.input.process(input[i] as f64);
            self.delay_line.write(x + feedback_sample * feedback_gain);

            // Triangle LFO, the second read is half a cycle away
            let triangle = self.lfo.process().abs();
            let left = self.delay_line.read_linear(delay_samples + depth_samples * triangle);
            let right = self.delay_line.read_linear(delay_samples + depth_samples * (1.0 - triangle));
            feedback_sample = left;

            let o = i + start_index;
            output_left[o] = (output_left[o] as f64 + left * gain) as f32;
            output_right[o] = (output_right[o] as f64 + right * gain) as f32;
            // The sends take both reads, before the level
            let mono = left + right;
            output_reverb[i] = (output_reverb[i] as f64 + mono * reverb_gain) as f32;
            output_delay[i] = (output_delay[i] as f64 + mono * delay_gain) as f32;
        }
        self.feedback_sample = feedback_sample;
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.lfo.clear();
        self.delay_line.clear();
        self.feedback_sample = 0.0;
        self.set_level(LEVEL_UNITY as u8);
        self.set_pre_lowpass(0);
        self.set_feedback(0);
        self.set_delay(0);
        self.set_rate(0);
        self.set_depth(0);
        self.set_send_level_to_reverb(0);
        self.set_send_level_to_delay(0);
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

    /// Restores this effect processor instance from a snapshot.
    /// Equivalent to: applySnapshot(snapshot)
    pub fn apply_snapshot(&mut self, snapshot: &ChorusSnapshot) {
        self.set_level(snapshot.level);
        self.set_pre_lowpass(snapshot.pre_lowpass);
        self.set_feedback(snapshot.feedback);
        self.set_delay(snapshot.delay);
        self.set_rate(snapshot.rate);
        self.set_depth(snapshot.depth);
        self.set_send_level_to_reverb(snapshot.send_level_to_reverb);
        self.set_send_level_to_delay(snapshot.send_level_to_delay);
    }
}
