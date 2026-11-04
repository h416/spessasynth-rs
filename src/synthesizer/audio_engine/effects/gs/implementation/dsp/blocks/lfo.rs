//! The LFO phase ramp of the choruses, flangers, delays, tremolos and the ring modulator.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/lfo.ts

/// The hardware's parabolic sine of an LFO phase (-1 to 1).
#[inline]
pub fn parabolic_sine(phase: f64) -> f64 {
    4.0 * phase * (1.0 - phase.abs())
}

/// The LFO phase of the choruses, flangers, delays, tremolos and the ring modulator: a ramp over -1 to 1 per cycle.
/// The blocks take what they need from it (the triangle is its absolute value).
#[derive(Clone)]
pub struct Lfo {
    /// The phase of this sample, -1 to 1.
    pub phase: f64,
    sample_rate: f64,
    increment: f64,
}

impl Lfo {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            phase: 0.0,
            sample_rate,
            increment: 0.0,
        }
    }

    pub fn set_rate(&mut self, hz: f64) {
        self.increment = (2.0 * hz) / self.sample_rate;
    }

    #[inline]
    pub fn process(&mut self) -> f64 {
        let mut phase = self.phase + self.increment;
        if phase >= 1.0 {
            phase -= 2.0;
        }
        self.phase = phase;
        phase
    }

    pub fn clear(&mut self) {
        self.phase = 0.0;
    }
}
