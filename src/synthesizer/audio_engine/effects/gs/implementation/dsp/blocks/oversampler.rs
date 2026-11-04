//! Twice the rate around a nonlinear stage, with a windowed sinc half band filter.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/oversampler.ts

use super::kaiser::kaiser;

/// The taps of the half band filter (odd, so one phase is a plain delay).
const TAPS: usize = 31;
const CENTER: f64 = (TAPS as f64 - 1.0) / 2.0;
const KAISER_BETA: f64 = 8.0;

/// Twice the rate around a nonlinear stage: `up` makes two samples at the double rate, `down` takes two back.
/// Both use the same windowed sinc half band filter.
pub struct Oversampler {
    /// The two samples at the double rate after `up`.
    pub first: f64,
    pub second: f64,
    taps: [f64; TAPS],
    /// The last inputs at the normal rate (`up`) and at the double rate (`down`).
    input: [f64; TAPS],
    doubled: [f64; TAPS],
}

impl Default for Oversampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Oversampler {
    pub fn new() -> Self {
        let mut taps = [0.0; TAPS];
        for (i, tap) in taps.iter_mut().enumerate() {
            let t = (i as f64 - CENTER) / 2.0;
            let sinc = if t == 0.0 {
                1.0
            } else {
                (std::f64::consts::PI * t).sin() / (std::f64::consts::PI * t)
            };
            *tap = 0.5 * sinc * kaiser((i as f64 - CENTER) / CENTER, KAISER_BETA);
        }
        Self {
            first: 0.0,
            second: 0.0,
            taps,
            input: [0.0; TAPS],
            doubled: [0.0; TAPS],
        }
    }

    #[inline]
    pub fn up(&mut self, x: f64) {
        self.input.copy_within(0..TAPS - 1, 1);
        self.input[0] = x;
        // Zero stuffing: the even taps make the first sample, the odd ones the second (times 2 for the zeros)
        let mut first = 0.0;
        let mut second = 0.0;
        let mut i = 0;
        while i < TAPS {
            first += self.taps[i] * self.input[i >> 1];
            i += 2;
        }
        let mut i = 1;
        while i < TAPS {
            second += self.taps[i] * self.input[i >> 1];
            i += 2;
        }
        self.first = 2.0 * first;
        self.second = 2.0 * second;
    }

    #[inline]
    pub fn down(&mut self, first: f64, second: f64) -> f64 {
        self.doubled.copy_within(0..TAPS - 2, 2);
        self.doubled[1] = first;
        self.doubled[0] = second;
        let mut y = 0.0;
        for i in 0..TAPS {
            y += self.taps[i] * self.doubled[i];
        }
        y
    }

    pub fn clear(&mut self) {
        self.input.fill(0.0);
        self.doubled.fill(0.0);
    }
}
