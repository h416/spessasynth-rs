//! St. Flanger (`01 23`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/stereo_flanger.ts

use super::prelude::*;
use super::stereo_chorus_flanger::GSStereoChorusFlanger;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x00, 0x00, 0x10, 0x0b, 0x18, 0x68, 0x5a, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x40, 0x40, 0x40, 0x00, 0x68,
];

/// St. Flanger (`01 23`).
pub struct GSStereoFlanger;

impl GSStereoFlanger {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSStereoChorusFlanger {
        let mut effect = GSStereoChorusFlanger::new(sample_rate, memory, 0x01_23, DEFAULTS, true);
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}
