//! Limiter (`01 31`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/limiter.ts

use super::dynamics_effect::{DynamicsBlock, GSDynamicsEffect};
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::dynamics::{Limiter, detector_gain};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x55, 0x03, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x40, 0x7f,
];
const LIMITS: &[(u8, u8)] = &[(1, 3), (3, 3)];

/// Limiter (`01 31`).
/// Parameters: 1 threshold, 2 ratio, 3 release, 4 post gain, then the EQ, pan and level.
pub struct GSLimiter;

impl GSLimiter {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> GSDynamicsEffect {
        let dynamics = DynamicsBlock::Limiter(Limiter::new(sample_rate));
        let mut effect =
            GSDynamicsEffect::new(sample_rate, 0x01_31, DEFAULTS, LIMITS, dynamics, 127.0 / 128.0);
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

/// Applies the type's parameters to the dynamics block. Returns the post gain index (0-3).
/// Equivalent to: updateDynamics
pub(super) fn update_dynamics(dynamics: &mut Limiter, p: &[u8; 20]) -> usize {
    // The threshold runs the other way: 0 is the highest detector gain (-59.8 dBFS)
    dynamics.set_threshold(detector_gain(127 - p[0]));
    dynamics.set_ratio(p[1]);
    // The gain grows by e in 63 ms (0) to 4.1 s (127): 4.98e-4 x the release word per 32 kHz sample
    // That is 2% more than the word says (SC-VA's float seed, kept like the floor since it sets the fade-in)
    let release = 2f64.powf(14.975 - p[2] as f64 / 21.16) / 32_768.0;
    dynamics.set_release(1.0 / (32_000.0 * 4.9828e-4 * release));
    p[3] as usize
}
