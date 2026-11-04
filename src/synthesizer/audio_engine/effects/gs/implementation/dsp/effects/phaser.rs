//! Phaser (`01 20`): per channel `phaser + dry -> EQ`, one LFO for both channels.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/phaser.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::phaser::Phaser;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::shelf_eq::{ShelfEq, shelf_corners};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{modulation_depth, rate1};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x24, 0x10, 0x40, 0x10, 0x7f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x68,
];

/// Phaser (`01 20`): per channel `phaser + dry -> EQ`, one LFO for both channels.
/// Parameters: 1 manual, 2 rate (Rate1 column), 3 depth, 4 resonance, 5 mix, 17 low gain, 18 high gain, 20 level.
/// EFX control source 1 moves the manual, source 2 the rate.
pub struct GSPhaser {
    base: GSEffectBase,
    phaser: Phaser,
    shelves_l: ShelfEq,
    shelves_r: ShelfEq,
    wet: f64,
    gain: f64,
}

impl GSPhaser {
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> Self {
        let mut effect = Self {
            base: GSEffectBase::new(sample_rate, None, DEFAULTS, &[], &[]),
            phaser: Phaser::new(sample_rate, 2),
            shelves_l: ShelfEq::new(sample_rate),
            shelves_r: ShelfEq::new(sample_rate),
            wet: 0.0,
            gain: 0.0,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSPhaser {
    gs_effect_common!(0x01_20);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let wet = self.wet;
        let gain = self.gain;
        for i in 0..sample_count {
            self.phaser.step();
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            let pl = self.phaser.process(0, l);
            output_left[i] = (self.shelves_l.process(l + wet * pl) * gain) as f32;
            let pr = self.phaser.process(1, r);
            output_right[i] = (self.shelves_r.process(r + wet * pr) * gain) as f32;
        }
    }

    fn clear(&mut self) {
        self.phaser.clear();
        self.shelves_l.clear();
        self.shelves_r.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.phaser.set_manual(self.base.controlled(0, 0));
        self.phaser.set_rate(rate1(self.base.controlled(1, 1)));
        // The depth word is the chorus swing in ms over 16
        self.phaser.set_depth(modulation_depth(p[2]) / 16.0);
        self.phaser.set_resonance(p[3]);
        // The chain runs at 1/8 of the 127/128 input and comes out x 8 the mix level
        self.wet = level_gain(p[4]) * (127.0 / 128.0);
        self.shelves_l.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves_l.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        self.shelves_r.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves_r.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        self.gain = level_gain(p[19]) * STAGE_GAIN;
    }
}
