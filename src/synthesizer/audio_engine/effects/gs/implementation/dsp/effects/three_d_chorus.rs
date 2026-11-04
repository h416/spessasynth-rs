//! 3D Chorus (`01 44`): `L+R -> chorus (two voices, 180 degrees apart) -> 3D stage`.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/three_d_chorus.ts

use super::prelude::*;
use super::three_d_stage_effect::GSThreeDStageEffect;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::lfo::Lfo;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::modulated_delay::ModulatedDelay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{SWING_MS, modulation_depth, pre_delay, rate1};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x0a, 0x08, 0x48, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x40, 0x40, 0x40, 0x00, 0x50,
];

/// 3D Chorus (`01 44`): `L+R -> chorus (two voices, 180 degrees apart) -> 3D stage`, then the balance and the EQ.
/// The voices sit at left and right 90 degrees.
/// Parameters: 1 pre delay, 2 rate (Rate1 column), 3 depth, 15 output (speaker, phones), 16 balance, 17 low gain,
/// 18 high gain, 20 level. EFX control source 1 modulates the rate, source 2 the balance.
pub struct GSThreeDChorus {
    stage: GSThreeDStageEffect,
    lfo: Lfo,
    voices: ModulatedDelay,
}

impl GSThreeDChorus {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let stage = GSThreeDStageEffect::new(sample_rate, memory, DEFAULTS);
        // Reads: the two voices for the right output, then the same one sample earlier for the left output
        // The line holds the 100 ms pre delay and the swing, with a ms to spare
        let voices = ModulatedDelay::new(sample_rate, memory, 4, 101.0 + SWING_MS);
        let mut effect = Self {
            stage,
            lfo: Lfo::new(sample_rate),
            voices,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSThreeDChorus {
    gs_effect_common!(0x01_44; stage.base);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let s = &mut self.stage;
        for i in 0..sample_count {
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            let x = s.mix.process(l, r);
            let phase = self.lfo.process();
            self.voices.process(x, phase);
            let v = &self.voices.output;
            s.stage_l.process(v[2], v[3]);
            s.stage_r.process(v[0], v[1]);
            s.output.process(l, s.stage_l.left, r, s.stage_r.right);
            output_left[i] = s.output.left as f32;
            output_right[i] = s.output.right as f32;
        }
    }

    fn clear(&mut self) {
        self.stage.clear();
        self.lfo.clear();
        self.voices.clear();
    }

    fn update(&mut self) {
        let p = self.stage.base.parameters;
        self.lfo.set_rate(rate1(self.stage.base.controlled(0, 1)));
        let pre = pre_delay(p[0]);
        let depth = modulation_depth(p[2]);
        for read in 0..4usize {
            self.voices
                .set_pre_delay(read, 0f64.max(pre - (read >> 1) as f64 / 32.0));
            self.voices.set_depth(read, depth);
            self.voices.set_phase(read, (read & 1) as f64 * 180.0);
        }
        self.stage.update_stage();
    }
}
