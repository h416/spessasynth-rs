//! Chorus / Delay (`11 00`) and Flanger / Delay (`11 01`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/modulation_parallel_delay.ts

use super::modulation_parallel::GSModulationParallel;
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::enums::combination_stages;

#[rustfmt::skip]
const CHORUS_DEFAULTS: [u8; 20] = [
    0x0a, 0x08, 0x78, 0x00, 0x40, 0x6f, 0x50, 0x7f, 0x00, 0x28, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x7f, 0x7f, 0x7f, 0x60,
];

/// Chorus / Delay (`11 00`): the chorus on the left input, a delay with the HF damp in the loop on the right input.
/// Parameters: 1 pre delay, 2 rate (Rate1 column), 3 depth, 5 chorus balance, 6 delay (Dly3 column), 7 delay
/// feedback, 8 HF damp, 10 delay balance, 16 chorus pan, 17 chorus level, 18 delay pan, 19 delay level, 20 level.
/// EFX control source 1 modulates the chorus balance, source 2 the delay balance.
pub struct GSChorusParallelDelay;

impl GSChorusParallelDelay {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSModulationParallel {
        let mut effect = GSModulationParallel::new(
            sample_rate,
            memory,
            0x11_00,
            CHORUS_DEFAULTS,
            false,
            combination_stages::DELAY,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const FLANGER_DEFAULTS: [u8; 20] = [
    0x10, 0x0b, 0x18, 0x68, 0x40, 0x6f, 0x50, 0x7f, 0x00, 0x30, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x7f, 0x7f, 0x7f, 0x60,
];

/// Flanger / Delay (`11 01`): the flanger on the left input, a delay with the HF damp in the loop on the right
/// input.
/// Parameters: 1 pre delay, 2 rate (Rate1 column), 3 depth, 4 feedback, 5 flanger balance, 6 delay (Dly3 column),
/// 7 delay feedback, 8 HF damp, 10 delay balance, 16 flanger pan, 17 flanger level, 18 delay pan, 19 delay level,
/// 20 level. EFX control source 1 modulates the flanger balance, source 2 the delay balance.
pub struct GSFlangerParallelDelay;

impl GSFlangerParallelDelay {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSModulationParallel {
        let mut effect = GSModulationParallel::new(
            sample_rate,
            memory,
            0x11_01,
            FLANGER_DEFAULTS,
            true,
            combination_stages::DELAY,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}
