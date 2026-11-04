//! 3D Auto (`01 70`): a source turning around the listener.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/three_d_auto.ts

use super::prelude::*;
use super::three_d::{GSThreeD, ThreeDKind};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{azimuth, rate1};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x40, 0x19, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7f,
];
const LIMITS: &[(u8, u8)] = &[(2, 1), (3, 1), (14, 1)];

/// 3D Auto (`01 70`): a source turning around the listener.
/// Parameters: 1 azimuth (where it stops), 2 speed (Rate1 column, turns per second), 3 clockwise, 4 turn, 15 output
/// (speaker, phones), 20 level. EFX control source 1 modulates the speed, source 2 switches the turning (in steps of
/// 64). Turned off, it runs on until it reaches the azimuth. The SC-88Pro honors the direction (SC-VA does not).
pub struct GSThreeDAuto;

impl GSThreeDAuto {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> GSThreeD {
        let mut effect = GSThreeD::new(sample_rate, ThreeDKind::Auto, DEFAULTS, LIMITS);
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

/// Equivalent to: level
pub(super) fn level(effect: &GSThreeD) -> u8 {
    effect.base.parameters[19]
}

/// Equivalent to: updatePosition
pub(super) fn update_position(effect: &mut GSThreeD) {
    let p = effect.base.parameters;
    let resetting = effect.base.resetting;
    effect.positioner.set_azimuth(azimuth(p[0]), resetting);
    let speed = rate1(effect.base.controlled(0, 1));
    effect.positioner.set_speed(speed, p[2] == 1);
    // Math.trunc(offset / 64): integer division truncates toward zero too
    let turning = 0i32.max(1i32.min(p[3] as i32 + effect.base.control_offset(1) as i32 / 64));
    effect.positioner.set_turning(turning == 1);
}
