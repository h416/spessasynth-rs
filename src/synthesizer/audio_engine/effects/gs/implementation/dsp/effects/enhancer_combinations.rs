//! Enhancer -> Chorus, Flanger, Delay (`02 06`-`02 08`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/enhancer_combinations.ts

use super::enhancer_combination::GSEnhancerCombination;
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::enums::combination_stages;

#[rustfmt::skip]
const ENHANCER_CHORUS_DEFAULTS: [u8; 20] = [
    0x40, 0x7f, 0x00, 0x00, 0x00, 0x40, 0x08, 0x65, 0x00, 0x40, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x50,
];

/// Enhancer -> Chorus (`02 06`). The right side is not enhanced, it only mixes the right enhancer's dry signal (the
/// SC-88Pro does the same).
pub struct GSEnhancerChorus;

impl GSEnhancerChorus {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSEnhancerCombination {
        let mut effect = GSEnhancerCombination::new(
            sample_rate,
            memory,
            0x02_06,
            ENHANCER_CHORUS_DEFAULTS,
            combination_stages::CHORUS,
            false,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const ENHANCER_FLANGER_DEFAULTS: [u8; 20] = [
    0x40, 0x7f, 0x00, 0x00, 0x00, 0x10, 0x0b, 0x18, 0x68, 0x30, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x60,
];

/// Enhancer -> Flanger (`02 07`). The right side is not enhanced, as in EH -> Chorus.
pub struct GSEnhancerFlanger;

impl GSEnhancerFlanger {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSEnhancerCombination {
        let mut effect = GSEnhancerCombination::new(
            sample_rate,
            memory,
            0x02_07,
            ENHANCER_FLANGER_DEFAULTS,
            combination_stages::FLANGER,
            false,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const ENHANCER_DELAY_DEFAULTS: [u8; 20] = [
    0x40, 0x7f, 0x00, 0x00, 0x00, 0x6f, 0x50, 0x7f, 0x00, 0x30, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x58,
];

/// Enhancer -> Delay (`02 08`).
pub struct GSEnhancerDelay;

impl GSEnhancerDelay {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSEnhancerCombination {
        let mut effect = GSEnhancerCombination::new(
            sample_rate,
            memory,
            0x02_08,
            ENHANCER_DELAY_DEFAULTS,
            combination_stages::DELAY,
            true,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}
