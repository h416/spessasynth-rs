//! A first order section designed with the bilinear transform: the tone filters, shelves and DC blockers.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/first_order_filter.ts

use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::js_round;

/// The corner of the DC blocking high pass that the delays, reverbs, drives and Lo-Fi types put on their input.
pub const DC_BLOCKER_HZ: f64 = 19.88;

/// Recursive states under this are flushed to zero (-600 dB, far under any signal). A one-pole decay otherwise
/// ends on a subnormal fixed point, and subnormal arithmetic is many times slower.
pub const FLUSH: f64 = 1e-30;

/// The prewarped analog frequency of a corner, normalized to `2 fs` (so the bilinear transform is
/// `s = (1 - z^-1) / (1 + z^-1)`). Corners above 0.49 fs are held there.
pub fn prewarp(frequency: f64, sample_rate: f64) -> f64 {
    ((std::f64::consts::PI * frequency.min(sample_rate * 0.49)) / sample_rate).tan()
}

/// The corner of the hardware's one-pole low pass `(1 - B) x + B y[n-1]` at 32 kHz (-3 dB, the inverse of
/// `FirstOrderFilter::set_damp`). `pole` is B.
/// Equivalent to: dampCorner(pole)
pub fn damp_corner(pole: f64) -> f64 {
    let k = (1.0 + pole * pole) / (2.0 * pole);
    ((2.0 - k).acos() * 32_000.0) / (2.0 * std::f64::consts::PI)
}

/// A first order section, `y = b0 x + b1 x[n-1] + a1 y[n-1]`, designed with the bilinear transform.
/// The tone filters, the shelves and the DC blockers of the Sound Canvas are all this section.
#[derive(Clone)]
pub struct FirstOrderFilter {
    sample_rate: f64,
    b0: f64,
    b1: f64,
    a1: f64,
    x1: f64,
    y1: f64,
}

impl FirstOrderFilter {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            b0: 1.0,
            b1: 0.0,
            a1: 0.0,
            x1: 0.0,
            y1: 0.0,
        }
    }

    /// Passes the signal through unchanged.
    pub fn set_off(&mut self) {
        self.set(1.0, 0.0, 0.0);
    }

    /// A low pass. `frequency` is the -3 dB corner in Hz, `gain` the gain at DC (1 by default in TS).
    pub fn set_low_pass(&mut self, frequency: f64, gain: f64) {
        let t = prewarp(frequency, self.sample_rate);
        let b = (gain * t) / (1.0 + t);
        self.set(b, b, (1.0 - t) / (1.0 + t));
    }

    /// A high pass. `frequency` is the -3 dB corner in Hz, `gain` the gain at Nyquist (1 by default in TS).
    pub fn set_high_pass(&mut self, frequency: f64, gain: f64) {
        let t = prewarp(frequency, self.sample_rate);
        let b = gain / (1.0 + t);
        self.set(b, -b, (1.0 - t) / (1.0 + t));
    }

    /// The choruses' tone filters. On the hardware they are bilinear one-poles at 32 kHz with byte coefficients: the
    /// pole is C = round(128 c) and the gain A = round(64 (1 -+ c)). That rounding moves the corner slightly and the
    /// pass band by up to 0.6 dB, both kept. `gain` is a gain on top (1 by default in TS).
    pub fn set_tone(&mut self, high_pass: bool, frequency: f64, gain: f64) {
        let k = ((std::f64::consts::PI * frequency) / 32_000.0).tan();
        let c = (1.0 - k) / (1.0 + k);
        let pole = js_round(128.0 * c);
        let corner = (((128.0 - pole) / (128.0 + pole)).atan() * 32_000.0) / std::f64::consts::PI;
        if high_pass {
            self.set_high_pass(corner, (gain * 2.0 * js_round(64.0 * (1.0 + c))) / (128.0 + pole));
        } else {
            self.set_low_pass(corner, (gain * 2.0 * js_round(64.0 * (1.0 - c))) / (128.0 - pole));
        }
    }

    /// The delays' HF damp. On the hardware it is a one-pole without a zero at 32 kHz, `y = (1 - B) x + B y[n-1]`,
    /// unity at DC and -3 dB at the frequency. This is its inverse bilinear prototype at the host rate,
    /// exact at 32 kHz.
    pub fn set_damp(&mut self, frequency: f64) {
        let k = 2.0 - ((2.0 * std::f64::consts::PI * frequency) / 32_000.0).cos();
        let b = k - (k * k - 1.0).sqrt();
        // Normalized to 2 x 32 kHz the prototype is (1 - B)(1 + s) / ((1 - B) + (1 + B) s)
        let scale = 32_000.0 / self.sample_rate;
        let gain = (1.0 - b) / (1.0 + b);
        self.set_analog(gain, scale, gain * scale);
    }

    /// A plain gain.
    pub fn set_gain(&mut self, gain: f64) {
        self.set(gain, 0.0, 0.0);
    }

    /// A low shelf. One corner stays at the given frequency, the gain moves the other one: a boost puts the pole
    /// at `frequency / gain`, a cut puts the zero at `frequency * gain`, so a cut is the exact inverse of a boost.
    pub fn set_low_shelf(&mut self, frequency: f64, gain_db: f64) {
        let w = prewarp(frequency, self.sample_rate);
        let g = 10f64.powf(gain_db / 20.0);
        if g >= 1.0 {
            self.set_analog(1.0, w, w / g);
        } else {
            self.set_analog(1.0, w * g, w);
        }
    }

    /// A high shelf, the mirror image of the low shelf: a boost puts the pole at `frequency * gain`, a cut puts the
    /// zero at `frequency / gain`.
    pub fn set_high_shelf(&mut self, frequency: f64, gain_db: f64) {
        let w = prewarp(frequency, self.sample_rate);
        let g = 10f64.powf(gain_db / 20.0);
        if g >= 1.0 {
            self.set_analog(g, w, w * g);
        } else {
            self.set_analog(g, w / g, w);
        }
    }

    /// A general section `H(s) = k (s + zero) / (s + pole)`, for the presets that are neither shelves nor plain
    /// passes. Both corners (Hz) are prewarped, a negative zero is a zero in the right half plane.
    pub fn set_pole_zero(&mut self, zero: f64, pole: f64, k: f64) {
        self.set_analog(k, prewarp(zero, self.sample_rate), prewarp(pole, self.sample_rate));
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let mut y = self.b0 * x + self.b1 * self.x1 + self.a1 * self.y1;
        // A decaying state gets stuck among the subnormals, which are slow
        if y.abs() < FLUSH {
            y = 0.0;
        }
        self.x1 = x;
        self.y1 = y;
        y
    }

    pub fn clear(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }

    /// Sets `H(s) = k (s + zero) / (s + pole)` (normalized frequencies).
    fn set_analog(&mut self, k: f64, zero: f64, pole: f64) {
        self.set(
            (k * (1.0 + zero)) / (1.0 + pole),
            (k * (zero - 1.0)) / (1.0 + pole),
            (1.0 - pole) / (1.0 + pole),
        );
    }

    fn set(&mut self, b0: f64, b1: f64, a1: f64) {
        self.b0 = b0;
        self.b1 = b1;
        self.a1 = a1;
    }
}
