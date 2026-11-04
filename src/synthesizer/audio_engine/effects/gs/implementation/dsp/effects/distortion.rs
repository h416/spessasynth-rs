//! Distortion (`01 11`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/distortion.ts

use super::drive_effect::GSDriveEffect;
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::drive::GUITAR_DISTORTION;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x4c, 0x03, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x38, 0x40, 0x54,
];

/// Distortion (`01 11`).
pub struct GSDistortion;

impl GSDistortion {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> GSDriveEffect {
        let mut effect = GSDriveEffect::new(sample_rate, 0x01_11, DEFAULTS, &GUITAR_DISTORTION);
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}
