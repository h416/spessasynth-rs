//! Tremolo (`01 25`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/tremolo.ts

use super::prelude::*;
use super::tremolo_effect::GSTremoloEffect;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x01, 0x3c, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x7f,
];

/// Tremolo (`01 25`).
pub struct GSTremolo;

impl GSTremolo {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> GSTremoloEffect {
        let mut effect = GSTremoloEffect::new(sample_rate, 0x01_25, DEFAULTS, false);
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}
