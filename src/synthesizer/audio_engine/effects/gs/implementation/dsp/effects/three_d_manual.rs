//! 3D Manual (`01 71`): a source at a fixed azimuth.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/three_d_manual.ts

use super::prelude::*;
use super::three_d::{GSThreeD, LIMITS, ThreeDKind};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::azimuth;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7f,
];

/// 3D Manual (`01 71`): a source at a fixed azimuth.
/// Parameters: 1 azimuth (Azim column), 15 output (speaker, phones), 20 level. EFX control source 1 moves the azimuth,
/// source 2 the level.
pub struct GSThreeDManual;

impl GSThreeDManual {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> GSThreeD {
        let mut effect = GSThreeD::new(sample_rate, ThreeDKind::Manual, DEFAULTS, LIMITS);
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

/// Equivalent to: level
pub(super) fn level(effect: &GSThreeD) -> u8 {
    effect.base.controlled(1, 19)
}

/// Equivalent to: updatePosition
pub(super) fn update_position(effect: &mut GSThreeD) {
    let resetting = effect.base.resetting;
    let value = effect.base.controlled(0, 0);
    effect.positioner.set_azimuth(azimuth(value), resetting);
}
