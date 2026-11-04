//! 3D Delay (`01 57`): a delay whose left and right taps go through the 3D stage.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/three_d_delay.ts

use super::prelude::*;
use super::three_d_stage_effect::GSThreeDStageEffect;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::delay::Delay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{STEP_MS, delay3, feedback_gain, hf_damp};

/// The taps: center, left and right for the right output, then the same one 32 kHz sample earlier for the left one.
/// The delay parameters 1-3 come in the same order.
const CENTER: usize = 0;
const LEFT: usize = 1;
const RIGHT: usize = 2;
const EARLY: usize = 3;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x74, 0x6a, 0x6e, 0x50, 0x28, 0x40, 0x40, 0x7f, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x30, 0x40, 0x40, 0x00, 0x7f,
];

/// 3D Delay (`01 57`): `L+R + feedback -> HF damp -> line`, the left and right taps through the 3D stage (at left and
/// right 90 degrees), the center tap to both sides, then the balance and the EQ.
/// Parameters: 1-3 center, left and right delay (Dly3 column), 4 feedback (from the center tap), 5-7 center, left and
/// right level, 8 HF damp, 15 output (speaker, phones), 16 balance, 17 low gain, 18 high gain, 20 level. EFX control
/// source 1 modulates the feedback, source 2 the balance.
pub struct GSThreeDDelay {
    stage: GSThreeDStageEffect,
    delay: Delay,
    feedback: f64,
    center: f64,
    left: f64,
    right: f64,
}

impl GSThreeDDelay {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let stage = GSThreeDStageEffect::new(sample_rate, memory, DEFAULTS);
        let mut delay = Delay::new(sample_rate, memory, 6, 501.0);
        delay.set_feedback_tap(CENTER);
        delay.set_damp_taps(true);
        let mut effect = Self {
            stage,
            delay,
            feedback: 0.0,
            center: 0.0,
            left: 0.0,
            right: 0.0,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSThreeDDelay {
    gs_effect_common!(0x01_57; stage.base);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let center = self.center;
        let left = self.left;
        let right = self.right;
        let s = &mut self.stage;
        for i in 0..sample_count {
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            let x = s.mix.process(l, r) + self.delay.feedback(1.0) * self.feedback;
            self.delay.process(x);
            let t = &self.delay.output;
            s.stage_l.process(left * t[EARLY + LEFT], right * t[EARLY + RIGHT]);
            s.stage_r.process(left * t[LEFT], right * t[RIGHT]);
            s.output.process(
                l,
                s.stage_l.left + center * t[EARLY + CENTER],
                r,
                s.stage_r.right + center * t[CENTER],
            );
            output_left[i] = s.output.left as f32;
            output_right[i] = s.output.right as f32;
        }
    }

    fn clear(&mut self) {
        self.stage.clear();
        self.delay.clear();
    }

    fn update(&mut self) {
        let p = self.stage.base.parameters;
        for tap in CENTER..=RIGHT {
            let ms = delay3(p[tap]);
            self.delay.set_tap(tap, ms);
            self.delay.set_tap(EARLY + tap, 0f64.max(ms - STEP_MS));
        }
        self.feedback = feedback_gain(self.stage.base.controlled(0, 3));
        self.center = level_gain(p[4]);
        self.left = level_gain(p[5]);
        self.right = level_gain(p[6]);
        self.delay.set_hf_damp(hf_damp(p[7]));
        self.stage.update_stage();
    }
}
