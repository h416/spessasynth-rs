//! The pitch shifters: one line, one or more voices of two crossfaded ramp reads.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/pitch_shifter.ts

use super::crossfade::Crossfade;
use super::delay_line::DelayLine;
use super::delay_memory::DelayMemory;
use super::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};

/// The longest window (mode 4) in ms.
const MAX_WINDOW_MS: f64 = 256.0;

/// The pitch shifters: one line, one or more voices. Each voice is a ramp through a window: two reads half a window
/// apart move at `1 - ratio` samples per sample (down for a higher pitch) and jump back by a window at its end.
/// Each read's gain is a triangle of its window position (0 where it jumps), so the two crossfade linearly.
/// Every voice has a second pair of reads one 32 kHz sample earlier, for an output that runs ahead of the line (the
/// shared stage's left output of the mono types). Voice 0 can feed back.
pub struct PitchShifter {
    /// Each voice's output this sample, at the line's timing.
    pub output: Vec<f64>,
    /// Each voice's output one 32 kHz sample ahead.
    pub early: Vec<f64>,
    line: DelayLine,
    dc_blocker: FirstOrderFilter,
    /// One 32 kHz sample in host samples.
    step: f64,
    /// Per voice: the window position (0-1) of its first read, its step per host sample, the window and the pre delay
    /// in 32 kHz samples.
    phases: Vec<f64>,
    increments: Vec<f64>,
    windows: Vec<f64>,
    /// The pre delays in 32 kHz samples: the ones read, the ones set, and the ones read before them while a change
    /// crossfades.
    pre_delays: Vec<f64>,
    target_pre_delays: Vec<f64>,
    previous_pre_delays: Vec<f64>,
    fade: Crossfade,
    feedback: f64,
}

impl PitchShifter {
    /// `voices` is the number of voices.
    pub fn new(sample_rate: f64, memory: &mut DelayMemory, voices: usize) -> Self {
        let step = sample_rate / 32_000.0;
        // 100 ms of pre delay plus the window, read past 256 ms too (SC-VA wraps there)
        let line = memory.line((((100.0 + MAX_WINDOW_MS) * sample_rate) / 1000.0 + 3.0 * step).ceil());
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        Self {
            output: vec![0.0; voices],
            early: vec![0.0; voices],
            line,
            dc_blocker,
            step,
            phases: vec![0.0; voices],
            increments: vec![0.0; voices],
            windows: vec![0.0; voices],
            pre_delays: vec![0.0; voices],
            target_pre_delays: vec![0.0; voices],
            previous_pre_delays: vec![0.0; voices],
            fade: Crossfade::new(sample_rate),
            feedback: 0.0,
        }
    }

    /// `ratio` is the pitch ratio (2 is an octave up), `window` the window in ms (64 to 256 by mode).
    pub fn set_voice(&mut self, voice: usize, ratio: f64, window: f64) {
        self.windows[voice] = window * 32.0;
        self.increments[voice] = (1.0 - ratio) / (window * 32.0 * self.step);
    }

    /// `ms` is the pre delay.
    pub fn set_pre_delay(&mut self, voice: usize, ms: f64) {
        if ms * 32.0 == self.target_pre_delays[voice] {
            return;
        }
        self.target_pre_delays[voice] = ms * 32.0;
        self.fade.request();
    }

    /// `gain` is the feedback gain of voice 0 (its output one 32 kHz sample ago back into the input).
    pub fn set_feedback(&mut self, gain: f64) {
        self.feedback = gain;
    }

    #[inline]
    pub fn process(&mut self, mut x: f64) {
        let step = self.step;
        if self.fade.tick() {
            self.previous_pre_delays.copy_from_slice(&self.pre_delays);
            self.pre_delays.copy_from_slice(&self.target_pre_delays);
        }
        for v in 0..self.phases.len() {
            let mut phase = self.phases[v] + self.increments[v];
            phase -= phase.floor();
            self.phases[v] = phase;
        }
        if self.feedback != 0.0 {
            // The output of one 32 kHz sample ago, at the window position then
            // It is read before the write, so one host sample less, and the loop runs in the line's own time
            let mut phase = self.phases[0] - self.increments[0] * step;
            phase -= phase.floor();
            x += self.feedback * self.read(0, phase, 1.0, 1.0);
        }
        let input = self.dc_blocker.process(x);
        self.line.write(input);
        for v in 0..self.phases.len() {
            self.output[v] = self.read(v, self.phases[v], 0.0, 0.0);
            // The output of one 32 kHz sample later, so the window position then too
            let mut ahead = self.phases[v] + self.increments[v] * step;
            ahead -= ahead.floor();
            self.early[v] = self.read(v, ahead, -1.0, 0.0);
        }
    }

    pub fn clear(&mut self) {
        self.line.clear();
        self.dc_blocker.clear();
        // Both reads start mid-way, the first at the top of its triangle
        self.phases.fill(0.5);
        self.output.fill(0.0);
        self.early.fill(0.0);
        self.fade.clear();
    }

    /// A voice's reads, crossfaded from the previous pre delay while it changes.
    /// `phase` is the first read's window position, `shift` 32 kHz samples added to the pre delay, `offset` host
    /// samples taken off the reads.
    fn read(&self, v: usize, phase: f64, shift: f64, offset: f64) -> f64 {
        let window = self.windows[v];
        let y = self.voice(phase, window, self.pre_delays[v] + shift, offset);
        if !self.fade.active {
            return y;
        }
        let old = self.voice(phase, window, self.previous_pre_delays[v] + shift, offset);
        old + self.fade.gain * (y - old)
    }

    /// The crossfaded reads of a voice. `window` is in 32 kHz samples, `pre_delay` in 32 kHz samples, `offset` host
    /// samples taken off the reads.
    fn voice(&self, phase: f64, window: f64, pre_delay: f64, offset: f64) -> f64 {
        let other = if phase < 0.5 { phase + 0.5 } else { phase - 0.5 };
        let gain = 1.0 - (2.0 * phase - 1.0).abs();
        let line = &self.line;
        gain * line.read_grid(pre_delay + window * phase, self.step, offset)
            + (1.0 - gain) * line.read_grid(pre_delay + window * other, self.step, offset)
    }
}
