//! The delay line of every chorus and flanger: one line read at several places that an LFO moves.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/modulated_delay.ts

use super::crossfade::Crossfade;
use super::delay_line::DelayLine;
use super::delay_memory::DelayMemory;
use super::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};

/// The step flanger's smoother after the held LFO: a one-pole of 3.2 ms (its pole per 32 kHz sample).
const STEP_POLE: f64 = 8112.0 / 8192.0;

/// The delay line of every chorus and flanger: the input plus the feedback through a DC blocker and a tone filter
/// into one line, read at several places that an LFO moves.
///
/// Each read's delay is its pre delay plus its swing times the LFO curve. The curve is the triangle `t = |phase|`
/// (chorus) or `t^2` (flanger, which lingers near the pre delay). Each read has its own phase offset.
/// The feedback is read 0 one 32 kHz sample later, as on the hardware, where it is the previous sample's read.
///
/// The hardware interpolates linearly between its 32 kHz samples, which cuts the highs of a moving read but not of
/// a still one. So the reads do the same: a mix of two points one 32 kHz sample apart, each read band-limited from
/// the line at the host rate.
pub struct ModulatedDelay {
    /// The reads of this sample.
    pub output: Vec<f64>,
    /// The tone filter in the loop (and the gain into the line), off by default.
    pub tone: FirstOrderFilter,
    line: DelayLine,
    dc_blocker: FirstOrderFilter,
    /// One 32 kHz sample in host samples.
    step: f64,
    step_smoothing: f64,
    /// Each read's pre delay and swing in 32 kHz samples: the ones read, the ones set, and the ones read before them
    /// while a change crossfades.
    pre_delays: Vec<f64>,
    swings: Vec<f64>,
    target_pre_delays: Vec<f64>,
    target_swings: Vec<f64>,
    previous_pre_delays: Vec<f64>,
    previous_swings: Vec<f64>,
    fade: Crossfade,
    offsets: Vec<f64>,
    /// This sample's delay of each read in 32 kHz samples, and with the previous settings.
    delays: Vec<f64>,
    previous_delays: Vec<f64>,
    /// The smoothed triangle of each read.
    smoothed: Vec<f64>,
    smoothing: f64,
    flanger: bool,
    feedback: f64,
}

impl ModulatedDelay {
    /// `reads` is the number of reads, `max_delay` the longest delay of any read (pre delay plus swing), in ms.
    pub fn new(sample_rate: f64, memory: &mut DelayMemory, reads: usize, max_delay: f64) -> Self {
        let step = sample_rate / 32_000.0;
        let step_smoothing = 1.0 - STEP_POLE.powf(1.0 / step);
        // Two more 32 kHz samples for the feedback's points
        let line = memory.line(((max_delay * sample_rate) / 1000.0 + 2.0 * step).ceil());
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        Self {
            output: vec![0.0; reads],
            tone: FirstOrderFilter::new(sample_rate),
            line,
            dc_blocker,
            step,
            step_smoothing,
            pre_delays: vec![0.0; reads],
            swings: vec![0.0; reads],
            target_pre_delays: vec![0.0; reads],
            target_swings: vec![0.0; reads],
            previous_pre_delays: vec![0.0; reads],
            previous_swings: vec![0.0; reads],
            fade: Crossfade::new(sample_rate),
            offsets: vec![0.0; reads],
            delays: vec![0.0; reads],
            previous_delays: vec![0.0; reads],
            smoothed: vec![0.0; reads],
            smoothing: 1.0,
            flanger: false,
            feedback: 0.0,
        }
    }

    /// The squared triangle instead of the triangle.
    pub fn set_curve(&mut self, flanger: bool) {
        self.flanger = flanger;
    }

    /// The DC blocker on the input, on by default (a cascaded line fed by another line's read has none).
    pub fn set_dc_blocker(&mut self, on: bool) {
        if on {
            self.dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        } else {
            self.dc_blocker.set_off();
        }
    }

    /// Smooth the triangle of each read (the step flanger, whose LFO phase is held in steps).
    pub fn set_step_smoothing(&mut self, smooth: bool) {
        self.smoothing = if smooth { self.step_smoothing } else { 1.0 };
    }

    /// The delay at the bottom of the swing of a read, in ms.
    pub fn set_pre_delay(&mut self, read: usize, ms: f64) {
        if ms * 32.0 == self.target_pre_delays[read] {
            return;
        }
        self.target_pre_delays[read] = ms * 32.0;
        self.fade.request();
    }

    /// The swing on top of the pre delay of a read, in ms.
    pub fn set_depth(&mut self, read: usize, ms: f64) {
        if ms * 32.0 == self.target_swings[read] {
            return;
        }
        self.target_swings[read] = ms * 32.0;
        self.fade.request();
    }

    /// The LFO phase offset of a read, in degrees.
    pub fn set_phase(&mut self, read: usize, degrees: f64) {
        self.offsets[read] = degrees / 180.0;
    }

    /// The feedback gain from read 0 (negative inverts).
    pub fn set_feedback(&mut self, gain: f64) {
        self.feedback = gain;
    }

    /// `x` is the input sample, `phase` the LFO phase of this sample (-1 to 1).
    #[inline]
    pub fn process(&mut self, mut x: f64, phase: f64) {
        if self.fade.tick() {
            self.previous_pre_delays.copy_from_slice(&self.pre_delays);
            self.previous_swings.copy_from_slice(&self.swings);
            self.pre_delays.copy_from_slice(&self.target_pre_delays);
            self.swings.copy_from_slice(&self.target_swings);
        }
        let smoothing = self.smoothing;
        for i in 0..self.delays.len() {
            let mut p = phase + self.offsets[i];
            if p >= 1.0 {
                p -= 2.0;
            }
            self.smoothed[i] += smoothing * (p.abs() - self.smoothed[i]);
            let mut t = self.smoothed[i];
            if self.flanger {
                t *= t;
            }
            self.delays[i] = self.pre_delays[i] + self.swings[i] * t;
            self.previous_delays[i] = self.previous_pre_delays[i] + self.previous_swings[i] * t;
        }
        if self.feedback != 0.0 {
            // The feedback is read before the write, so one host sample less
            let mut y = self.line.read_grid(self.delays[0] + 1.0, self.step, 1.0);
            if self.fade.active {
                let old = self.line.read_grid(self.previous_delays[0] + 1.0, self.step, 1.0);
                y = old + self.fade.gain * (y - old);
            }
            x += self.feedback * y;
        }
        let blocked = self.dc_blocker.process(x);
        let toned = self.tone.process(blocked);
        self.line.write(toned);
        for i in 0..self.output.len() {
            let mut y = self.line.read_grid(self.delays[i], self.step, 0.0);
            if self.fade.active {
                let old = self.line.read_grid(self.previous_delays[i], self.step, 0.0);
                y = old + self.fade.gain * (y - old);
            }
            self.output[i] = y;
        }
    }

    pub fn clear(&mut self) {
        self.line.clear();
        self.dc_blocker.clear();
        self.tone.clear();
        self.output.fill(0.0);
        self.smoothed.fill(0.0);
        self.fade.clear();
    }
}
