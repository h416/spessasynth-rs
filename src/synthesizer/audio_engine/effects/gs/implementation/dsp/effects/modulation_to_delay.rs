//! Chorus -> Delay (`02 09`) and Flanger -> Delay (`02 0a`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/modulation_to_delay.ts

use super::modulation_series::GSModulationSeries;
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::enums::combination_stages;

#[rustfmt::skip]
const CHORUS_DELAY_DEFAULTS: [u8; 20] = [
    0x0a, 0x09, 0x78, 0x00, 0x40, 0x6f, 0x50, 0x7f, 0x00, 0x30, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x7f,
];

/// Chorus -> Delay (`02 09`): the chorus, then both sides mixed into a delay with the HF damp in the loop. The right
/// chorus read is 180 degrees from the left one.
/// Parameters: 1 pre delay, 2 rate (Rate1 column), 3 depth, 5 chorus balance, 6 delay (Dly3 column), 7 delay
/// feedback, 8 HF damp, 10 delay balance, 17 low gain, 18 high gain, 20 level. EFX control source 1 modulates the
/// chorus balance, source 2 the delay balance.
pub struct GSChorusDelay;

impl GSChorusDelay {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSModulationSeries {
        let mut effect = GSModulationSeries::new(
            sample_rate,
            memory,
            0x02_09,
            CHORUS_DELAY_DEFAULTS,
            false,
            combination_stages::DELAY,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const FLANGER_DELAY_DEFAULTS: [u8; 20] = [
    0x10, 0x0b, 0x18, 0x68, 0x40, 0x6f, 0x50, 0x7f, 0x00, 0x30, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x7f,
];

/// Flanger -> Delay (`02 0a`): the flanger, then both sides mixed into a delay with the HF damp in the loop. Both
/// flanger reads have the same phase, on the hardware too.
/// Parameters: 1 pre delay, 2 rate (Rate1 column), 3 depth, 4 feedback, 5 flanger balance, 6 delay (Dly3 column),
/// 7 delay feedback, 8 HF damp, 10 delay balance, 17 low gain, 18 high gain, 20 level. EFX control source 1
/// modulates the feedback here, on the SC-88Pro and in the SC-8850 manual (SC-VA modulates the balance), source 2
/// the delay balance.
pub struct GSFlangerDelay;

impl GSFlangerDelay {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSModulationSeries {
        let mut effect = GSModulationSeries::new(
            sample_rate,
            memory,
            0x02_0a,
            FLANGER_DELAY_DEFAULTS,
            true,
            combination_stages::DELAY,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}
