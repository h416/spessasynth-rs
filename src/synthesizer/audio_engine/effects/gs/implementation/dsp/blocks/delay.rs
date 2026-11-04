//! The line of the delays: a DC blocked input read at several taps, one tap feeding back through the HF damp.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/delay.ts

use super::crossfade::Crossfade;
use super::delay_line::DelayLine;
use super::delay_memory::DelayMemory;
use super::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};

/// The line of the delays: the input (with the feedback the effect adds to it) through a DC blocker into one line,
/// read at several taps. One tap feeds back through the HF damp.
///
/// On the hardware the feedback is the damped tap of the previous sample. Here it is the tap read one 32 kHz sample
/// further back through its own copy of the damp, which is the same signal at every rate.
/// The taps the columns give are whole 32 kHz samples. A tap between them (Time Ctrl Delay's slewing one) mixes the
/// two neighbors linearly like the hardware, each read band-limited at the host rate. A tap change crossfades
/// (`Crossfade`) unless `set_crossfade` turned it off.
pub struct Delay {
    /// The reads of this sample, through the damp when `set_damp_taps` is on.
    pub output: Vec<f64>,
    line: DelayLine,
    dc_blocker: FirstOrderFilter,
    damp_taps: Vec<FirstOrderFilter>,
    damp_feedback: FirstOrderFilter,
    /// One 32 kHz sample in host samples.
    step: f64,
    /// The taps in 32 kHz samples: the ones read, the ones set, and the ones read before them while fading.
    taps: Vec<f64>,
    targets: Vec<f64>,
    previous: Vec<f64>,
    fade: Crossfade,
    fading: bool,
    feedback_tap: usize,
    damped: bool,
}

impl Delay {
    /// `taps` is the number of taps, `max_delay` the longest tap in ms.
    pub fn new(sample_rate: f64, memory: &mut DelayMemory, taps: usize, max_delay: f64) -> Self {
        let step = sample_rate / 32_000.0;
        let line = memory.line(((max_delay * sample_rate) / 1000.0 + 3.0 * step).ceil());
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        let damp_taps = (0..taps).map(|_| FirstOrderFilter::new(sample_rate)).collect();
        Self {
            output: vec![0.0; taps],
            line,
            dc_blocker,
            damp_taps,
            damp_feedback: FirstOrderFilter::new(sample_rate),
            step,
            taps: vec![0.0; taps],
            targets: vec![0.0; taps],
            previous: vec![0.0; taps],
            fade: Crossfade::new(sample_rate),
            fading: true,
            feedback_tap: 0,
            damped: false,
        }
    }

    /// Sets a tap's delay in ms.
    pub fn set_tap(&mut self, tap: usize, ms: f64) {
        let samples = ms * 32.0;
        if !self.fading {
            self.targets[tap] = samples;
            self.taps[tap] = samples;
        } else if samples != self.targets[tap] {
            self.targets[tap] = samples;
            self.fade.request();
        }
    }

    /// Crossfade tap changes (on by default). Off for a tap that slews by itself.
    pub fn set_crossfade(&mut self, on: bool) {
        self.fading = on;
    }

    /// The tap that feeds back through the damp (tap 0 if never set).
    pub fn set_feedback_tap(&mut self, tap: usize) {
        self.feedback_tap = tap;
    }

    /// Damp every output tap (a single tap delay's echo is the damped one), not only the feedback.
    pub fn set_damp_taps(&mut self, on: bool) {
        self.damped = on;
    }

    /// The HF damp corner in Hz, infinite for Bypass.
    pub fn set_hf_damp(&mut self, frequency: f64) {
        for damp in &mut self.damp_taps {
            if frequency == f64::INFINITY {
                damp.set_off();
            } else {
                damp.set_damp(frequency);
            }
        }
        if frequency == f64::INFINITY {
            self.damp_feedback.set_off();
        } else {
            self.damp_feedback.set_damp(frequency);
        }
    }

    /// The damped feedback tap some 32 kHz samples ago (the hardware's loop takes one, 1 by default in TS).
    /// Call it once per sample, before `process`.
    #[inline]
    pub fn feedback(&mut self, samples: f64) -> f64 {
        // Before the write, so one host sample less
        let mut y = self.line.read_grid(self.taps[self.feedback_tap] + samples, self.step, 1.0);
        if self.fade.active {
            let old = self.line.read_grid(self.previous[self.feedback_tap] + samples, self.step, 1.0);
            y = old + self.fade.gain * (y - old);
        }
        self.damp_feedback.process(y)
    }

    /// `x` is the input sample, the feedback included.
    #[inline]
    pub fn process(&mut self, x: f64) {
        if self.fade.tick() {
            self.previous.copy_from_slice(&self.taps);
            self.taps.copy_from_slice(&self.targets);
        }
        let input = self.dc_blocker.process(x);
        self.line.write(input);
        for i in 0..self.output.len() {
            let mut y = self.line.read_grid(self.taps[i], self.step, 0.0);
            if self.fade.active {
                let old = self.line.read_grid(self.previous[i], self.step, 0.0);
                y = old + self.fade.gain * (y - old);
            }
            self.output[i] = if self.damped { self.damp_taps[i].process(y) } else { y };
        }
    }

    pub fn clear(&mut self) {
        self.line.clear();
        self.dc_blocker.clear();
        for damp in &mut self.damp_taps {
            damp.clear();
        }
        self.damp_feedback.clear();
        self.fade.clear();
        self.output.fill(0.0);
    }
}
