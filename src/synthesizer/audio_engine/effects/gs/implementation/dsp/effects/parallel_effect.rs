//! The base of the parallel combinations: effect 1 on the left input, effect 2 on the right input.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/parallel_effect.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::output_lag::OutputLag;

/// The base of the parallel combinations: effect 1 on the left input, effect 2 on the right input, each with its
/// pan (16, 18) and level (17, 19) into both outputs, then the output level (20).
///
/// The shared stage reads the left input one 32 kHz sample late and writes the right output one sample late. So
/// effect 1 reaches the left output one 32 kHz sample late and the right output two late (`OutputLag`s), effect 2
/// the left output at once and the right one a sample late (`lag_second`, the delay line effects read one sample
/// later for the right output instead).
///
/// TS has this as an abstract class; the Rust types hold it and implement `GSEffect` themselves.
pub struct GSParallelEffect {
    pub base: GSEffectBase,
    /// Effect 1's share of the outputs of this sample.
    pub left: f64,
    pub right: f64,
    /// Effect 2's pans with its level and the output level.
    pub gain_l: f64,
    pub gain_r: f64,
    /// Effect 1's lags to the left and the right output, effect 2's to the right one.
    first_lag_l: OutputLag,
    first_lag_r: OutputLag,
    second_lag: OutputLag,
    first_l: f64,
    first_r: f64,
}

impl GSParallelEffect {
    pub fn new(
        sample_rate: f64,
        memory: Option<&mut DelayMemory>,
        defaults: [u8; 20],
        limits: &'static [(u8, u8)],
    ) -> Self {
        Self {
            base: GSEffectBase::new(sample_rate, memory, defaults, limits, &[]),
            left: 0.0,
            right: 0.0,
            gain_l: 0.0,
            gain_r: 0.0,
            first_lag_l: OutputLag::new(sample_rate, 1.0),
            first_lag_r: OutputLag::new(sample_rate, 2.0),
            second_lag: OutputLag::new(sample_rate, 1.0),
            first_l: 0.0,
            first_r: 0.0,
        }
    }

    pub fn clear(&mut self) {
        self.first_lag_l.clear();
        self.first_lag_r.clear();
        self.second_lag.clear();
    }

    /// Pans effect 1's output `y` into `left` and `right`.
    #[inline]
    pub fn route_first(&mut self, y: f64) {
        self.left = self.first_l * self.first_lag_l.process(y);
        self.right = self.first_r * self.first_lag_r.process(y);
    }

    /// Effect 2's signal for the right output, one 32 kHz sample late.
    #[inline]
    pub fn lag_second(&mut self, x: f64) -> f64 {
        self.second_lag.process(x)
    }

    /// Applies the pans and levels (16-19).
    /// `first` is effect 1's gain after its level (the output level, the output stage), `second` effect 2's.
    pub fn update_pans(&mut self, first: f64, second: f64) {
        let p = self.base.parameters;
        self.first_l = pan_left(p[15]) * level_gain(p[16]) * first;
        self.first_r = pan_right(p[15]) * level_gain(p[16]) * first;
        self.gain_l = pan_left(p[17]) * level_gain(p[18]) * second;
        self.gain_r = pan_right(p[17]) * level_gain(p[18]) * second;
    }
}
