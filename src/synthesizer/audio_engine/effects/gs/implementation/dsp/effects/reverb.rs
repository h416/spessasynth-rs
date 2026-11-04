//! Reverb (`01 55`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/reverb.ts

use super::prelude::*;
use super::reverb_effect::GSReverbEffect;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::reverb::Reverb;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{hf_damp, pre_delay, reverb_time};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x03, 0x70, 0x78, 0x68, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x40, 0x40, 0x40, 0x00, 0x7f,
];
const LIMITS: &[(u8, u8)] = &[(0, 5)];

/// Reverb (`01 55`).
/// Parameters: 1 type (Room 1, Room 2, Stage 1, Stage 2, Hall 1, Hall 2), 2 pre delay (PreDly column), 3 time, 4 HF
/// damp (HF column), 16 balance, 17 low gain, 18 high gain, 20 level. EFX control source 1 modulates the time,
/// source 2 the balance.
pub struct GSReverb {
    re: GSReverbEffect,
    reverb: Reverb,
}

impl GSReverb {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let re = GSReverbEffect::new(sample_rate, memory, DEFAULTS, LIMITS);
        let reverb = Reverb::new(sample_rate, memory);
        let mut effect = Self { re, reverb };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSReverb {
    gs_effect_common!(0x01_55; re.base);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let re = &mut self.re;
        let dry = re.dry;
        let wet = re.wet;
        for i in 0..sample_count {
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            self.reverb.process(re.mix.process(l, r));
            output_left[i] = re.shelves_l.process(dry * l + wet * self.reverb.left) as f32;
            output_right[i] = re.shelves_r.process(dry * r + wet * self.reverb.right) as f32;
        }
    }

    fn clear(&mut self) {
        self.re.clear();
        self.reverb.clear();
    }

    fn update(&mut self) {
        let p = self.re.base.parameters;
        self.reverb.set_type(p[0]);
        self.reverb.set_pre_delay(pre_delay(p[1]));
        self.reverb.set_time(reverb_time(self.re.base.controlled(0, 2)));
        self.reverb.set_damping(hf_damp(p[3]));
        let balance = self.re.base.controlled(1, 15);
        self.re.set_output(balance, level_gain(p[19]) * STAGE_GAIN, 1.0);
    }
}
