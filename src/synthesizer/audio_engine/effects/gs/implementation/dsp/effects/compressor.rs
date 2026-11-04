//! Compressor (`01 30`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/compressor.ts

use super::dynamics_effect::{DynamicsBlock, GSDynamicsEffect};
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::dynamics::{Compressor, detector_gain, recovery_rate};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x48, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x40, 0x68,
];
const LIMITS: &[(u8, u8)] = &[(2, 3)];

/// Compressor (`01 30`).
/// Parameters: 1 sustain (how fast the gain comes back), 2 attack (the makeup gain: the output ceiling stays, the
/// knee moves), 3 post gain, then the EQ, pan and level.
pub struct GSCompressor;

impl GSCompressor {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> GSDynamicsEffect {
        let dynamics = DynamicsBlock::Compressor(Compressor::new(sample_rate, 0.001));
        let mut effect = GSDynamicsEffect::new(sample_rate, 0x01_30, DEFAULTS, LIMITS, dynamics, 1.0);
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

/// Applies the type's parameters to the dynamics block. Returns the post gain index (0-3).
/// Equivalent to: updateDynamics
pub(super) fn update_dynamics(dynamics: &mut Compressor, p: &[u8; 20]) -> usize {
    // 0.09 to 2.25 per second, 0 to the full gain in 11 s to 0.44 s
    dynamics.set_recovery_rate(recovery_rate(p[0]));
    dynamics.set_makeup(detector_gain(p[1]));
    p[2] as usize
}
