//! Overdrive (`01 10`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/overdrive.ts

use super::drive_effect::GSDriveEffect;
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::drive::GUITAR_OVERDRIVE;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x30, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x40, 0x60,
];

/// Overdrive (`01 10`).
pub struct GSOverdrive;

impl GSOverdrive {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> GSDriveEffect {
        let mut effect = GSDriveEffect::new(sample_rate, 0x01_10, DEFAULTS, &GUITAR_OVERDRIVE);
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}
