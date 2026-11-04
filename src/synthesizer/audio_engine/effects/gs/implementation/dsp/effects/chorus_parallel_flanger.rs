//! Chorus / Flanger (`11 02`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/chorus_parallel_flanger.ts

use super::modulation_parallel::GSModulationParallel;
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::enums::combination_stages;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x10, 0x08, 0x78, 0x00, 0x40, 0x10, 0x0b, 0x18, 0x68, 0x40, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x7f, 0x7f, 0x7f, 0x58,
];

/// Chorus / Flanger (`11 02`): the chorus on the left input, a flanger on its own line on the right input (as on
/// the SC-88Pro, SC-VA's flanger reads the chorus line and its feedback does nothing). The flanger also mixes two
/// reads 180 degrees apart.
/// Parameters: 1 chorus pre delay, 2 chorus rate (Rate1 column), 3 chorus depth, 5 chorus balance, 6 flanger pre
/// delay, 7 flanger rate, 8 flanger depth, 9 flanger feedback, 10 flanger balance, 16 chorus pan, 17 chorus level,
/// 18 flanger pan, 19 flanger level, 20 level. EFX control source 1 modulates the chorus balance, source 2 the
/// flanger balance.
pub struct GSChorusParallelFlanger;

impl GSChorusParallelFlanger {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSModulationParallel {
        let mut effect = GSModulationParallel::new(
            sample_rate,
            memory,
            0x11_02,
            DEFAULTS,
            false,
            combination_stages::FLANGER,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}
