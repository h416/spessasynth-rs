//! The ring modulator of Keyboard Multi: one parabolic sine carrier.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/ring_modulator.ts

use super::lfo::{Lfo, parabolic_sine};

/// The ring modulator of Keyboard Multi: one sine carrier (the hardware's parabolic one) that the effect multiplies
/// the signal with. Call `process` once per sample, then read `carrier`.
pub struct RingModulator {
    /// The carrier of this sample, -1 to 1.
    pub carrier: f64,
    sample_rate: f64,
    lfo: Lfo,
    frequency: f64,
    start_delay: f64,
    pending: bool,
}

impl RingModulator {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            carrier: 0.0,
            sample_rate,
            lfo: Lfo::new(sample_rate),
            frequency: 0.0,
            start_delay: 0.0,
            pending: true,
        }
    }

    pub fn set_frequency(&mut self, hz: f64) {
        self.frequency = hz;
        self.lfo.set_rate(hz);
    }

    /// Starts the carrier later after a clear, so it lines up with a delayed signal (in host samples, fractions too).
    pub fn set_start_delay(&mut self, samples: f64) {
        self.start_delay = samples;
    }

    #[inline]
    pub fn process(&mut self) {
        if self.pending {
            // The first step after a clear: the phase starts back by the delay at the frequency set by now
            self.pending = false;
            self.lfo.phase -= (self.start_delay * 2.0 * self.frequency) / self.sample_rate;
            self.lfo.phase -= 2.0 * ((self.lfo.phase + 1.0) / 2.0).floor();
        }
        self.carrier = parabolic_sine(self.lfo.process());
    }

    pub fn clear(&mut self) {
        self.lfo.clear();
        self.pending = true;
        self.carrier = 0.0;
    }
}
