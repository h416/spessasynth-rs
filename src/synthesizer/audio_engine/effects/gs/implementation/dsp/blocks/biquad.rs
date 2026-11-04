//! A second order section designed with the bilinear transform.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/biquad.ts

use super::first_order_filter::{FLUSH, prewarp};

/// A second order section, designed with the bilinear transform. Each pole and zero pair is prewarped at its own
/// frequency, so a design taken from the 32 kHz hardware comes back exactly at 32 kHz.
#[derive(Clone)]
pub struct Biquad {
    pub(crate) sample_rate: f64,
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    s1: f64,
    s2: f64,
}

impl Biquad {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// Passes the signal through unchanged.
    pub fn set_off(&mut self) {
        self.b0 = 1.0;
        self.b1 = 0.0;
        self.b2 = 0.0;
        self.a1 = 0.0;
        self.a2 = 0.0;
    }

    /// A peaking band. A cut is the exact inverse of the boost of the same size, so the Q belongs to the boost side.
    /// `frequency` is the center in Hz, `q` the Q of the boost (on the prewarped axis), `gain_db` the gain at the
    /// center.
    pub fn set_peaking(&mut self, frequency: f64, q: f64, gain_db: f64) {
        let g = 10f64.powf(gain_db / 20.0);
        if g >= 1.0 {
            self.set_pole_zero(frequency, q, frequency, q / g);
        } else {
            self.set_pole_zero(frequency, g * q, frequency, q);
        }
    }

    /// `H(s) = (s^2 + s wz / qz + wz^2) / (s^2 + s wp / qp + wp^2)`, unity at Nyquist.
    pub fn set_pole_zero(&mut self, pole: f64, pole_q: f64, zero: f64, zero_q: f64) {
        let wp = self.prewarp(pole);
        let wz = self.prewarp(zero);
        self.set_analog(1.0, wz / zero_q, wz * wz, 1.0, wp / pole_q, wp * wp);
    }

    /// Sets `H(s) = (n2 s^2 + n1 s + n0) / (d2 s^2 + d1 s + d0)` with `s` normalized to `2 fs` (prewarp the
    /// frequencies with `prewarp`).
    pub fn set_analog(&mut self, n2: f64, n1: f64, n0: f64, d2: f64, d1: f64, d0: f64) {
        let a0 = d2 + d1 + d0;
        self.b0 = (n2 + n1 + n0) / a0;
        self.b1 = (2.0 * (n0 - n2)) / a0;
        self.b2 = (n2 - n1 + n0) / a0;
        self.a1 = (2.0 * (d0 - d2)) / a0;
        self.a2 = (d2 - d1 + d0) / a0;
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        // Transposed direct form II
        let y = self.b0 * x + self.s1;
        let s1 = self.b1 * x - self.a1 * y + self.s2;
        let s2 = self.b2 * x - self.a2 * y;
        // A decaying state gets stuck among the subnormals, which are slow
        self.s1 = if s1.abs() < FLUSH { 0.0 } else { s1 };
        self.s2 = if s2.abs() < FLUSH { 0.0 } else { s2 };
        y
    }

    pub fn clear(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    /// The prewarped analog frequency, normalized to `2 fs`.
    pub fn prewarp(&self, frequency: f64) -> f64 {
        prewarp(frequency, self.sample_rate)
    }
}
