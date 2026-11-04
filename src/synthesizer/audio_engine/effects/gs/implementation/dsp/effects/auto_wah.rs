//! Auto Wah (`01 21`): `L+R -> wah -> EQ -> pan`.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/auto_wah.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_pan::MonoPan;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::shelf_eq::{ShelfEq, shelf_corners};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::wah::Wah;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{modulation_depth, rate1};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x01, 0x00, 0x44, 0x3e, 0x28, 0x48, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x40, 0x60,
];
const LIMITS: &[(u8, u8)] = &[(0, 1), (6, 1)];

/// Auto Wah (`01 21`): `L+R -> wah -> EQ -> pan`.
/// Parameters: 1 filter type (LPF, BPF), 2 sens, 3 manual, 4 peak, 5 rate (Rate1 column), 6 depth, 7 polarity (down,
/// up), 17 low gain, 18 high gain, 19 pan, 20 level. EFX control source 1 moves the manual, source 2 the rate.
pub struct GSAutoWah {
    base: GSEffectBase,
    mix: MonoMix,
    wah: Wah,
    shelves: ShelfEq,
    pan: MonoPan,
}

impl GSAutoWah {
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> Self {
        let mut effect = Self {
            base: GSEffectBase::new(sample_rate, None, DEFAULTS, LIMITS, &[]),
            mix: MonoMix::new(sample_rate),
            wah: Wah::new(sample_rate),
            shelves: ShelfEq::new(sample_rate),
            pan: MonoPan::new(sample_rate),
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSAutoWah {
    gs_effect_common!(0x01_21);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        for i in 0..sample_count {
            let x = self.mix.process(input_left[i] as f64, input_right[i] as f64) * MONO_INPUT;
            let y = self.wah.process(x);
            self.pan.process(self.shelves.process(y));
            output_left[i] = self.pan.left as f32;
            output_right[i] = self.pan.right as f32;
        }
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.wah.clear();
        self.shelves.clear();
        self.pan.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.wah.set_filter_type(p[0] == 1);
        self.wah.set_peak(p[3]);
        self.wah.set_sensitivity(level_gain(p[1]));
        self.wah.set_manual(level_gain(self.base.controlled(0, 2)));
        self.wah.set_rate(rate1(self.base.controlled(1, 4)));
        // The depth word is the chorus swing in ms over 16
        self.wah.set_depth(modulation_depth(p[5]) / 16.0);
        self.wah.set_polarity(p[6] == 1);
        self.shelves.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        // The pan stage has a gain of 4
        self.pan.set(p[18], level_gain(p[19]) * 4.0 * STAGE_OUTPUT);
    }
}
