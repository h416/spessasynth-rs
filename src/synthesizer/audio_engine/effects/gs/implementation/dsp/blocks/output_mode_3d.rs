//! The 3D types' output: phones pass the two ear signals, speakers cancel the crosstalk between the speakers.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/output_mode_3d.ts

use super::delay_line::DelayLine;

/// The speaker gain of the network, and how long the speaker/phones switch ramps (in seconds).
const SPEAKER: f64 = 0.921_875;
const RAMP: f64 = 0.04;

/// The 3D types' output: phones pass the two ear signals, speakers cancel the crosstalk between the speakers.
///
/// The side signal goes through `1 + 8 g z^-7 T(z)` with `T = (0.1015625 - 0.015625 z^-d) / (1 - 0.53125 z^-1 -
/// 0.3828125 z^-7)`, the mid signal is untouched. The delays are the hardware's 32 kHz samples in seconds, read with
/// a Lagrange interpolator.
pub struct OutputMode3D {
    pub left: f64,
    pub right: f64,
    ear1: DelayLine,
    ear2: DelayLine,
    shuffled1: DelayLine,
    shuffled2: DelayLine,
    /// One 32 kHz sample in host samples.
    step: f64,
    /// The numerator's delay in 32 kHz samples (12 for 3D Auto and Manual, 18 for 3D Chorus and Delay).
    numerator: f64,
    ramp_step: f64,
    ramped: bool,
    gain: f64,
    target: f64,
}

impl OutputMode3D {
    /// `numerator` is the numerator's delay in 32 kHz samples, `ramped` whether a switch ramps over 40 ms
    /// (else it is hard).
    pub fn new(sample_rate: f64, numerator: f64, ramped: bool) -> Self {
        let step = sample_rate / 32_000.0;
        Self {
            left: 0.0,
            right: 0.0,
            ear1: DelayLine::new((numerator * step).ceil() + 4.0),
            ear2: DelayLine::new((numerator * step).ceil() + 4.0),
            shuffled1: DelayLine::new((7.0 * step).ceil() + 4.0),
            shuffled2: DelayLine::new((7.0 * step).ceil() + 4.0),
            step,
            numerator,
            ramp_step: SPEAKER / (RAMP * sample_rate),
            ramped,
            gain: SPEAKER,
            target: SPEAKER,
        }
    }

    /// Phones (no crosstalk canceller) instead of speakers.
    pub fn set_phones(&mut self, phones: bool) {
        self.target = if phones { 0.0 } else { SPEAKER };
        if !self.ramped {
            self.gain = self.target;
        }
    }

    /// Puts the switch at its position at once (a reset).
    pub fn settle(&mut self) {
        self.gain = self.target;
    }

    /// `e1` is the left ear signal, `e2` the right one.
    #[inline]
    pub fn process(&mut self, e1: f64, e2: f64) {
        let step = self.step;
        if self.gain != self.target {
            self.gain = if self.gain < self.target {
                self.target.min(self.gain + self.ramp_step)
            } else {
                self.target.max(self.gain - self.ramp_step)
            };
        }
        self.ear1.write(e1);
        self.ear2.write(e2);
        let t1 = Self::shuffle(step, self.numerator, &self.ear1, &self.shuffled1);
        let t2 = Self::shuffle(step, self.numerator, &self.ear2, &self.shuffled2);
        self.shuffled1.write(t1);
        self.shuffled2.write(t2);
        let side = 4.0
            * self.gain
            * (self.shuffled1.read_lagrange(7.0 * step) - self.shuffled2.read_lagrange(7.0 * step));
        self.left = e1 + side;
        self.right = e2 - side;
    }

    pub fn clear(&mut self) {
        self.ear1.clear();
        self.ear2.clear();
        self.shuffled1.clear();
        self.shuffled2.clear();
        self.left = 0.0;
        self.right = 0.0;
    }

    /// One ear's `T`: `0.1015625 e - 0.015625 e[d] + 0.53125 t[1] + 0.3828125 t[7]`, read before the write.
    /// `ear` is the ear signal's line (written this sample), `shuffled` the shuffled signal's line (not yet written).
    #[inline]
    fn shuffle(step: f64, numerator: f64, ear: &DelayLine, shuffled: &DelayLine) -> f64 {
        0.101_562_5 * ear.read(0) - 0.015_625 * ear.read_lagrange(numerator * step)
            + 0.531_25 * shuffled.read_lagrange(step - 1.0)
            + 0.382_812_5 * shuffled.read_lagrange(7.0 * step - 1.0)
    }
}
