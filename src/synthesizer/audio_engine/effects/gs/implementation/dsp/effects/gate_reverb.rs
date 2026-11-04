//! Gate Reverb (`01 56`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/gate_reverb.ts

use super::prelude::*;
use super::reverb_effect::GSReverbEffect;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::gate_reverb::GateReverb;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::pre_delay;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x00, 0x05, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x2a, 0x40, 0x3d, 0x00, 0x70,
];
const LIMITS: &[(u8, u8)] = &[(0, 3)];

/// Gate Reverb (`01 56`).
/// Parameters: 1 type (Normal, Reverse, Sweep 1, Sweep 2), 2 pre delay (PreDly column), 3 gate time (5 to 500 ms in
/// 5 ms steps), 16 balance, 17 low gain, 18 high gain, 20 level. EFX control source 1 modulates the balance, source
/// 2 the level.
pub struct GSGateReverb {
    re: GSReverbEffect,
    reverb: GateReverb,
}

impl GSGateReverb {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let re = GSReverbEffect::new(sample_rate, memory, DEFAULTS, LIMITS);
        let reverb = GateReverb::new(sample_rate, memory);
        let mut effect = Self { re, reverb };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSGateReverb {
    gs_effect_common!(0x01_56; re.base);

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
        self.reverb.set_pattern(p[0]);
        self.reverb.set_pre_delay(pre_delay(p[1]));
        self.reverb.set_gate_time(5.0 * (p[2].min(99) as f64 + 1.0));
        // The pre delay read's 127/128
        let balance = self.re.base.controlled(0, 15);
        let gain = level_gain(self.re.base.controlled(1, 19)) * STAGE_GAIN;
        self.re.set_output(balance, gain, 127.0 / 128.0);
    }
}
