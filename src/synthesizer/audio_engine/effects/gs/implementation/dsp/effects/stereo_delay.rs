//! Stereo Delay (`01 50`): a delay per channel with the HF damp in the loop.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/stereo_delay.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::delay::Delay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::stereo_output::StereoOutput;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{delay3, feedback_gain, hf_damp};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x65, 0x74, 0x58, 0x01, 0x00, 0x00, 0x00, 0x7f, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x30, 0x40, 0x40, 0x00, 0x7f,
];
const LIMITS: &[(u8, u8)] = &[(3, 1), (4, 1), (5, 1)];

/// Stereo Delay (`01 50`): a delay per channel with the HF damp in the loop. The feedback goes back into its own line
/// (normal) or the other one (cross). An inverted output is also 6 dB quieter, on the hardware too.
/// Parameters: 1 delay left, 2 delay right (Dly3 column), 3 feedback, 4 feedback mode (normal, cross), 5 phase left,
/// 6 phase right (normal, invert), 8 HF damp, 16 balance, 17 low gain, 18 high gain, 20 level. EFX control source 1
/// modulates the feedback, source 2 the balance.
pub struct GSStereoDelay {
    base: GSEffectBase,
    delay_l: Delay,
    delay_r: Delay,
    output: StereoOutput,
    cross: bool,
    feedback: f64,
    phase_l: f64,
    phase_r: f64,
}

impl GSStereoDelay {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), DEFAULTS, LIMITS, &[]);
        let mut delay_l = Delay::new(sample_rate, memory, 1, 500.0);
        let mut delay_r = Delay::new(sample_rate, memory, 1, 500.0);
        delay_l.set_damp_taps(true);
        delay_r.set_damp_taps(true);
        let mut effect = Self {
            base,
            delay_l,
            delay_r,
            output: StereoOutput::new(sample_rate),
            cross: false,
            feedback: 0.0,
            phase_l: 1.0,
            phase_r: 1.0,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSStereoDelay {
    gs_effect_common!(0x01_50);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let feedback = self.feedback;
        let phase_l = self.phase_l;
        let phase_r = self.phase_r;
        for i in 0..sample_count {
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            if self.cross {
                // The shared stage runs the left chain one 32 kHz sample behind the right one
                // So the left echo reaches the right line two samples later
                // The right echo reaches the left line in the same sample
                let fb = self.delay_l.feedback(2.0);
                self.delay_r.process(r * LINE_INPUT + fb * feedback);
                self.delay_l
                    .process(l * LINE_INPUT + self.delay_r.output[0] * feedback);
            } else {
                let fb = self.delay_l.feedback(1.0);
                self.delay_l.process(l * LINE_INPUT + fb * feedback);
                let fb = self.delay_r.feedback(1.0);
                self.delay_r.process(r * LINE_INPUT + fb * feedback);
            }
            self.output.process(
                l,
                phase_l * self.delay_l.output[0],
                r,
                phase_r * self.delay_r.output[0],
            );
            output_left[i] = self.output.left as f32;
            output_right[i] = self.output.right as f32;
        }
    }

    fn clear(&mut self) {
        self.delay_l.clear();
        self.delay_r.clear();
        self.output.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.delay_l.set_tap(0, delay3(p[0]));
        self.delay_r.set_tap(0, delay3(p[1]));
        self.feedback = feedback_gain(self.base.controlled(0, 2));
        self.cross = p[3] == 1;
        self.phase_l = if p[4] == 1 { -0.5 } else { 1.0 };
        self.phase_r = if p[5] == 1 { -0.5 } else { 1.0 };
        self.delay_l.set_hf_damp(hf_damp(p[7]));
        self.delay_r.set_hf_damp(hf_damp(p[7]));
        self.output.set(
            self.base.controlled(1, 15),
            p[16],
            p[17],
            level_gain(p[19]) * STAGE_GAIN,
        );
    }
}
