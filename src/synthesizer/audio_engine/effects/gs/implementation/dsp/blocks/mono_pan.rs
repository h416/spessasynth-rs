//! The panned output of the mono wet types, the right output one 32 kHz sample after the left one.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/mono_pan.ts

use super::output_lag::OutputLag;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::glue::{pan_left, pan_right};

/// The panned output of the mono wet types. Like on the hardware, the right output comes one 32 kHz sample after
/// the left one.
pub struct MonoPan {
    pub left: f64,
    pub right: f64,
    lag: OutputLag,
    gain_l: f64,
    gain_r: f64,
}

impl MonoPan {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            lag: OutputLag::new(sample_rate, 1.0),
            gain_l: 0.0,
            gain_r: 0.0,
        }
    }

    /// `pan` is the pan parameter (0-127, 64 is the center), `gain` the gain in front of the pan.
    pub fn set(&mut self, pan: u8, gain: f64) {
        self.gain_l = pan_left(pan) * gain;
        self.gain_r = pan_right(pan) * gain;
    }

    /// Pans one sample into `left` and `right`.
    #[inline]
    pub fn process(&mut self, x: f64) {
        self.left = x * self.gain_l;
        self.right = self.lag.process(x * self.gain_r);
    }

    pub fn clear(&mut self) {
        self.lag.clear();
    }
}
