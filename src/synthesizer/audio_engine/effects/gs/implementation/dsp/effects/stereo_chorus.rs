//! St. Chorus (`01 42`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/stereo_chorus.ts

use super::prelude::*;
use super::stereo_chorus_flanger::GSStereoChorusFlanger;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x00, 0x00, 0x0a, 0x08, 0x6f, 0x00, 0x5a, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x40, 0x40, 0x40, 0x00, 0x68,
];

/// St. Chorus (`01 42`).
pub struct GSStereoChorus;

impl GSStereoChorus {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSStereoChorusFlanger {
        let mut effect = GSStereoChorusFlanger::new(sample_rate, memory, 0x01_42, DEFAULTS, false);
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}
