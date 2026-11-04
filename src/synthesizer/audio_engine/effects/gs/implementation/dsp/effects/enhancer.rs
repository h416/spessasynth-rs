//! Enhancer (`01 02`): a stereo high frequency enhancer, then the 2-band EQ.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/enhancer.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::enhancer::Enhancer;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::shelf_eq::{ShelfEq, shelf_corners};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x40, 0x7f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x43, 0x40, 0x00, 0x7f,
];

/// Enhancer (`01 02`): a stereo high frequency enhancer, then the 2-band EQ.
/// Parameters: 1 sensitivity, 2 mix level, 17 low gain, 18 high gain, 20 level. EFX control source 1 modulates the
/// sensitivity, source 2 the mix level.
pub struct GSEnhancer {
    base: GSEffectBase,
    enhancer_l: Enhancer,
    enhancer_r: Enhancer,
    shelves_l: ShelfEq,
    shelves_r: ShelfEq,
    gain: f64,
}

impl GSEnhancer {
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> Self {
        let mut effect = Self {
            base: GSEffectBase::new(sample_rate, None, DEFAULTS, &[], &[]),
            enhancer_l: Enhancer::new(sample_rate),
            enhancer_r: Enhancer::new(sample_rate),
            shelves_l: ShelfEq::new(sample_rate),
            shelves_r: ShelfEq::new(sample_rate),
            gain: 0.0,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSEnhancer {
    gs_effect_common!(0x01_02);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let gain = self.gain;
        for i in 0..sample_count {
            let l = self.shelves_l.process(self.enhancer_l.process(input_left[i] as f64));
            let r = self.shelves_r.process(self.enhancer_r.process(input_right[i] as f64));
            output_left[i] = (l * gain) as f32;
            output_right[i] = (r * gain) as f32;
        }
    }

    fn clear(&mut self) {
        self.enhancer_l.clear();
        self.enhancer_r.clear();
        self.shelves_l.clear();
        self.shelves_r.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        // The sensitivity is 0 dB at 16 and grows linearly, the mix follows the level curve
        let sensitivity = (self.base.controlled(0, 0).max(1)) as f64 / 16.0;
        let mix = level_gain(self.base.controlled(1, 1));
        self.enhancer_l.set(sensitivity, mix);
        self.enhancer_r.set(sensitivity, mix);
        self.shelves_l.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves_l.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        self.shelves_r.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves_r.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        // The enhancer input has one more 127/128 than the output stage
        self.gain = (level_gain(p[19]) * STAGE_GAIN * 127.0) / 128.0;
    }
}
