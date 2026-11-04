//! The Compressor and Limiter of the Sound Canvas.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/dynamics.ts

use super::delay_line::DelayLine;
use super::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};
use super::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::glue::saturate;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::js_round;

/// The rate the hardware's per sample laws were measured at.
const HARDWARE_RATE: f64 = 32_000.0;

/// The detector level above which the gain comes down, the same for both modes.
const THRESHOLD: f64 = 519.0 / 65_536.0;

/// The gain the program starts from on a type select (about -100 dB), so a fresh compressor or limiter fades in.
const INITIAL_GAIN: f64 = 1e-5;

/// The limiter's gain floor recovery per 32 kHz sample. It is what opens the gain from closed, so it sets how long
/// the fade-in after a type select takes.
const LIMITER_FLOOR: f64 = 1.34e-7;

/// The limiter's gain curves, three lines `g = slope t + offset` split at t = 0.01 and 0.1. The upper two are the
/// chords of `t ^ e` (e = 1 - 1 / ratio), the first one keeps a floor, so the lower ratios never close fully.
const LIMITER_FIRST_SEGMENT: [[f64; 2]; 4] = [[15.26, 0.1624], [7.5402, 0.024], [2.8669, 0.002_747], [1.0, 0.0]];
const LIMITER_EXPONENTS: [f64; 4] = [0.25, 0.5, 0.75, 1.0];

/// The detector gain of a threshold parameter: 1/256 to 4, 0.474 dB per step (127 is the highest gain).
pub fn detector_gain(value: u8) -> f64 {
    4.0 * 2f64.powf((-10.0 * (127.0 - value as f64)) / 127.0)
}

/// The compressor's recovery per second of a sustain parameter (0.09 to 2.25).
pub fn recovery_rate(value: u8) -> f64 {
    2f64.powf(7.5583 + value as f64 / 27.33) / 2097.15
}

/// The Compressor and Limiter of the Sound Canvas: one detector on the input under the current gain, a lookahead
/// line, and a gain that falls by the overshoot per sample and recovers below the threshold. Both end with the post
/// gain, saturating at the internal full scale.
///
/// The laws were measured per 32 kHz sample and are mapped to the host rate by time.
/// TS `abstract class Dynamics`; Rust keeps the shared part in this struct, which `Compressor` and `Limiter` hold.
pub struct Dynamics {
    sample_rate: f64,
    /// The 32 kHz samples per host sample.
    step: f64,
    detector_gain: f64,
    post_gain: f64,
    gain: f64,
    dc_blocker: FirstOrderFilter,
    lookahead: DelayLine,
    lookahead_samples: isize,
    smoothing: f64,
    envelope: f64,
}

impl Dynamics {
    /// `lookahead` is in seconds.
    fn new(sample_rate: f64, lookahead: f64) -> Self {
        let step = HARDWARE_RATE / sample_rate;
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        // Whole samples: interpolating the line would low pass the signal
        let lookahead_samples = js_round(lookahead * sample_rate);
        Self {
            sample_rate,
            step,
            detector_gain: 0.0,
            post_gain: 1.0,
            gain: INITIAL_GAIN,
            dc_blocker,
            lookahead: DelayLine::new(lookahead_samples),
            lookahead_samples: lookahead_samples as isize,
            // A one-pole smoother of the gain, 0.672 per 32 kHz sample (78 us)
            smoothing: 1.0 - 0.671_875f64.powf(step),
            envelope: INITIAL_GAIN,
        }
    }

    /// The gain after the dynamics, before they saturate.
    pub fn set_post_gain(&mut self, gain: f64) {
        self.post_gain = gain;
    }

    fn clear(&mut self) {
        self.dc_blocker.clear();
        self.lookahead.clear();
        self.gain = INITIAL_GAIN;
        self.envelope = INITIAL_GAIN;
    }

    /// Writes the input into the lookahead and runs the detector on it.
    /// Returns whether the input was over the threshold (the gain came down), else the gain recovers.
    #[inline]
    fn detect(&mut self, x: f64) -> bool {
        let h = self.dc_blocker.process(x);
        self.lookahead.write(h);
        // The detector sees the current input under the gain. It saturates at full scale before the last x4
        // That way a hot input can close the gain in one step
        let over = 3.968_75 * (self.detector_gain * (self.gain * h).abs()).min(1.0) - THRESHOLD;
        if over < 0.0 {
            return false;
        }
        self.gain *= (1.0 - over.min(1.0)).powf(self.step);
        true
    }

    /// Returns the lookahead's output under the smoothed gain `g` of this sample.
    #[inline]
    fn apply(&mut self, g: f64) -> f64 {
        self.envelope += self.smoothing * (g - self.envelope);
        self.lookahead.read(self.lookahead_samples) * self.envelope
    }
}

/// The compressor: 1 ms lookahead, the gain recovers linearly, the output has a makeup gain (the "Attack" parameter,
/// which sets how far below the fixed output ceiling the knee sits) and a two-sample average.
pub struct Compressor {
    pub dynamics: Dynamics,
    /// The output average, the same skewed two-sample mean as the mono input.
    average: MonoMix,
    makeup: f64,
    recovery: f64,
}

impl Compressor {
    /// `lookahead` is in seconds (0.001 by default in TS, Bass Multi's is longer).
    pub fn new(sample_rate: f64, lookahead: f64) -> Self {
        Self {
            dynamics: Dynamics::new(sample_rate, lookahead),
            average: MonoMix::new(sample_rate),
            makeup: 1.0,
            recovery: 0.0,
        }
    }

    pub fn set_post_gain(&mut self, gain: f64) {
        self.dynamics.set_post_gain(gain);
    }

    /// The makeup gain, below the knee, 1/256 to 4 (the output has 16x of it). The detector sees the signal at the
    /// output's level, so the output ceiling stays put and the knee moves down as the gain goes up.
    pub fn set_makeup(&mut self, gain: f64) {
        self.dynamics.detector_gain = gain;
        self.makeup = gain;
    }

    /// How fast the gain comes back, linear in amplitude: the rise of the gain (1 is the full gain) per second.
    pub fn set_recovery_rate(&mut self, per_second: f64) {
        self.recovery = per_second / self.dynamics.sample_rate;
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let d = &mut self.dynamics;
        if !d.detect(x) {
            d.gain = 1.0f64.min(d.gain + self.recovery);
        }
        let g = d.gain;
        let compressed = saturate(saturate(4.0 * d.apply(g)) * self.makeup);
        // Two samples averaged and x4: 2 (y[n] + y[n-1]) at 32 kHz
        let averaged = saturate(4.0 * self.average.process(compressed, compressed));
        saturate(averaged * self.dynamics.post_gain)
    }

    pub fn clear(&mut self) {
        self.dynamics.clear();
        self.average.clear();
    }
}

/// The limiter: 3.75 ms lookahead, the gain recovers exponentially and goes through the ratio curve.
pub struct Limiter {
    pub dynamics: Dynamics,
    floor: f64,
    slopes: [f64; 3],
    offsets: [f64; 3],
    release: f64,
}

impl Limiter {
    pub fn new(sample_rate: f64) -> Self {
        let dynamics = Dynamics::new(sample_rate, 0.003_75);
        let floor = LIMITER_FLOOR * dynamics.step;
        Self {
            dynamics,
            floor,
            slopes: [0.0; 3],
            offsets: [0.0; 3],
            release: 1.0,
        }
    }

    pub fn set_post_gain(&mut self, gain: f64) {
        self.dynamics.set_post_gain(gain);
    }

    /// The threshold as the detector gain (1/256 to 4, higher is a lower threshold).
    pub fn set_threshold(&mut self, gain: f64) {
        self.dynamics.detector_gain = gain;
    }

    /// How fast the gain comes back, exponentially: the time the gain takes to grow by e, in seconds.
    pub fn set_release(&mut self, time_constant: f64) {
        self.release = (1.0 / (time_constant * self.dynamics.sample_rate)).exp();
    }

    /// The ratio index (0-3: 1.5:1 (really 4:3), 2:1, 4:1, 100:1).
    pub fn set_ratio(&mut self, ratio: u8) {
        let ratio = ratio as usize;
        let e = LIMITER_EXPONENTS[ratio];
        self.slopes[0] = LIMITER_FIRST_SEGMENT[ratio][0];
        self.offsets[0] = LIMITER_FIRST_SEGMENT[ratio][1];
        self.slopes[1] = (0.1f64.powf(e) - 0.01f64.powf(e)) / 0.09;
        self.offsets[1] = 0.01f64.powf(e) - self.slopes[1] * 0.01;
        self.slopes[2] = (1.0 - 0.1f64.powf(e)) / 0.9;
        self.offsets[2] = 0.1f64.powf(e) - self.slopes[2] * 0.1;
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let d = &mut self.dynamics;
        if !d.detect(x) {
            d.gain = 1.0f64.min(d.gain * self.release + self.floor);
        }
        let g = d.gain;
        let s = if g < 0.01 {
            0
        } else if g < 0.1 {
            1
        } else {
            2
        };
        let applied = d.apply(self.slopes[s] * g + self.offsets[s]);
        saturate(applied * d.post_gain)
    }

    pub fn clear(&mut self) {
        self.dynamics.clear();
    }
}
