//! One channel of the Enhancer: the input plus a copy of itself high passed twice.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/enhancer.ts

use super::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};

/// The corners of the two high passes in the wet path.
const HIGH_PASS_A_HZ: f64 = 6577.0;
const HIGH_PASS_B_HZ: f64 = 7523.0;

/// One channel of the Enhancer. Despite the name it has no nonlinearity: the input gets a copy of itself added,
/// high passed twice (second order in total), so the top end rises with the sensitivity and the mix.
pub struct Enhancer {
    dc_blocker: FirstOrderFilter,
    high_pass_a: FirstOrderFilter,
    high_pass_b: FirstOrderFilter,
    wet: f64,
}

impl Enhancer {
    pub fn new(sample_rate: f64) -> Self {
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        let mut high_pass_a = FirstOrderFilter::new(sample_rate);
        high_pass_a.set_high_pass(HIGH_PASS_A_HZ, 1.0);
        let mut high_pass_b = FirstOrderFilter::new(sample_rate);
        high_pass_b.set_high_pass(HIGH_PASS_B_HZ, 1.0);
        Self {
            dc_blocker,
            high_pass_a,
            high_pass_b,
            wet: 0.0,
        }
    }

    /// `sensitivity` is the gain of the wet path (1 is 0 dB), `mix` the mix gain of the wet path.
    pub fn set(&mut self, sensitivity: f64, mix: f64) {
        self.wet = sensitivity * mix;
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let dry = self.dc_blocker.process(x);
        let a = self.high_pass_a.process(dry);
        dry + self.wet * self.high_pass_b.process(a)
    }

    pub fn clear(&mut self) {
        self.dc_blocker.clear();
        self.high_pass_a.clear();
        self.high_pass_b.clear();
    }
}
