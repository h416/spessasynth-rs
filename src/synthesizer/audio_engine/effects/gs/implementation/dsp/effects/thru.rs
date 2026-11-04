//! Thru (`00 00`): the input through the output stage.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/thru.ts

use super::prelude::*;

/// The output stage with the level fixed at 127/128.
const GAIN: f64 = (STAGE_GAIN * 127.0) / 128.0;

// Only the level (20) has a default, and nothing reads it
#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7f,
];

/// Thru (`00 00`): the input through the output stage, -0.272 dB.
pub struct GSThru {
    base: GSEffectBase,
}

impl GSThru {
    pub fn new(sample_rate: f64) -> Self {
        let mut effect = Self {
            base: GSEffectBase::new(sample_rate, None, DEFAULTS, &[], &[]),
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSThru {
    gs_effect_common!(0x00_00);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        for i in 0..sample_count {
            output_left[i] = (input_left[i] as f64 * GAIN) as f32;
            output_right[i] = (input_right[i] as f64 * GAIN) as f32;
        }
    }

    fn clear(&mut self) {
        // Nothing to clear
    }

    fn update(&mut self) {
        // Thru has no parameters
    }
}
