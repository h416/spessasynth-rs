//! Auto Pan (`01 26`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/auto_pan.ts

use super::prelude::*;
use super::tremolo_effect::GSTremoloEffect;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x01, 0x3c, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x7f,
];

/// Auto Pan (`01 26`).
pub struct GSAutoPan;

impl GSAutoPan {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> GSTremoloEffect {
        let mut effect = GSTremoloEffect::new(sample_rate, 0x01_26, DEFAULTS, true);
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}
