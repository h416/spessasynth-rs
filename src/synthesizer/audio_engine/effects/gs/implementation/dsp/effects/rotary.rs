//! Rotary (`01 22`): `L+R -> rotary -> EQ` per side.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/rotary.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::rotary::Rotary;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::shelf_eq::{ShelfEq, shelf_corners};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{rate1, slew_time};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x06, 0x71, 0x18, 0x7f, 0x11, 0x78, 0x58, 0x40, 0x60, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x7f,
];

/// Rotary (`01 22`): `L+R -> rotary -> EQ` per side.
/// Parameters: 1 low slow, 2 low fast, 3 low accel, 4 low level, 5 high slow, 6 high fast, 7 high accel, 8 high level
/// (rates on the Rate1 column), 9 separation, 11 speed (slow, fast from 64), 17 low gain, 18 high gain, 20 level.
/// EFX control source 1 moves the speed, source 2 the level.
pub struct GSRotary {
    base: GSEffectBase,
    mix: MonoMix,
    rotary: Rotary,
    shelves_l: ShelfEq,
    shelves_r: ShelfEq,
    gain: f64,
}

impl GSRotary {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), DEFAULTS, &[], &[]);
        let rotary = Rotary::new(sample_rate, memory);
        let mut effect = Self {
            base,
            mix: MonoMix::new(sample_rate),
            rotary,
            shelves_l: ShelfEq::new(sample_rate),
            shelves_r: ShelfEq::new(sample_rate),
            gain: 0.0,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSRotary {
    gs_effect_common!(0x01_22);

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
            let x = self.mix.process(input_left[i] as f64, input_right[i] as f64) * MONO_INPUT;
            self.rotary.process(x);
            output_left[i] = (self.shelves_l.process(self.rotary.left) * gain) as f32;
            output_right[i] = (self.shelves_r.process(self.rotary.right) * gain) as f32;
        }
    }

    fn reset(&mut self) {
        reset_base(self);
        self.rotary.settle();
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.rotary.clear();
        self.shelves_l.clear();
        self.shelves_r.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        let fast = self.base.controlled(0, 10) >= 64;
        self.rotary.set_rates(
            rate1(p[if fast { 5 } else { 4 }]),
            rate1(p[if fast { 1 } else { 0 }]),
        );
        self.rotary.set_acceleration(slew_time(p[6]), slew_time(p[2]));
        self.rotary.set_levels(level_gain(p[7]), level_gain(p[3]));
        self.rotary.set_separation(level_gain(p[8]));
        self.shelves_l.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves_l.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        self.shelves_r.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves_r.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        self.gain = level_gain(self.base.controlled(1, 19)) * STAGE_OUTPUT;
    }
}
