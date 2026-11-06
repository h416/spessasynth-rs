//! Time Ctrl Delay (`01 54`): a mono delay whose time slews toward the delay parameter.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/time_control_delay.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::delay::Delay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::stereo_output::StereoOutput;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{delay2, feedback_gain, hf_damp, slew_time};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x3c, 0x50, 0x50, 0x7f, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x30, 0x40, 0x40, 0x00, 0x7f,
];

/// Time Ctrl Delay (`01 54`): a mono delay whose time slews toward the delay parameter, so changing it bends the
/// pitch. The HF damp is in the loop, the output is panned.
/// The time is the Dly2 column x 127/128 (on the hardware too), the full 200 to 1000 ms (SC-VA wraps above 256 ms).
/// The left output reads one 32 kHz sample before the right one (the shared stage's timing).
/// Parameters: 1 delay (Dly2 column), 2 acceleration, 3 feedback, 4 HF damp, 5 pan, 16 balance, 17 low gain, 18 high
/// gain, 20 level. EFX control source 1 modulates the delay, source 2 the feedback.
pub struct GSTimeControlDelay {
    base: GSEffectBase,
    mix: MonoMix,
    /// Tap 0 is the left output's read, tap 1 the right one's (and the feedback).
    delay: Delay,
    output: StereoOutput,
    /// The delay and its target in 32 kHz samples.
    position: f64,
    target: f64,
    slew: f64,
    feedback: f64,
    pan_l: f64,
    pan_r: f64,
}

impl GSTimeControlDelay {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), DEFAULTS, &[], &[]);
        let mut delay = Delay::new(sample_rate, memory, 2, 1000.0);
        delay.set_feedback_tap(1);
        delay.set_damp_taps(true);
        // The time slews by itself
        delay.set_crossfade(false);
        let mut effect = Self {
            base,
            mix: MonoMix::new(sample_rate),
            delay,
            output: StereoOutput::new(sample_rate),
            position: 0.0,
            target: 0.0,
            slew: 0.0,
            feedback: 0.0,
            pan_l: 0.0,
            pan_r: 0.0,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSTimeControlDelay {
    gs_effect_common!(0x01_54);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let pan_l = self.pan_l;
        let pan_r = self.pan_r;
        for i in 0..sample_count {
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            self.position += self.slew * (self.target - self.position);
            self.delay.set_tap(0, (self.position - 1.0) / 32.0);
            self.delay.set_tap(1, self.position / 32.0);
            let x = self.mix.process(l, r) * LINE_INPUT + self.delay.feedback(1.0) * self.feedback;
            self.delay.process(x);
            self.output
                .process(l, pan_l * self.delay.output[0], r, pan_r * self.delay.output[1]);
            output_left[i] = self.output.left as f32;
            output_right[i] = self.output.right as f32;
        }
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.delay.clear();
        self.output.clear();
        // The time slews up from zero after a type select
        self.position = 0.0;
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.target = ((delay2(self.base.controlled(0, 0)) * 127.0) / 128.0) * 32.0;
        // The slew per 32 kHz sample is 1 / (32000 x the time constant)
        let step = 1.0 / (32_000.0 * slew_time(p[1]));
        self.slew = 1.0 - (1.0 - step).powf(32_000.0 / self.base.sample_rate);
        self.feedback = feedback_gain(self.base.controlled(1, 2));
        self.delay.set_hf_damp(hf_damp(p[3]));
        self.pan_l = pan_left(p[4]);
        self.pan_r = pan_right(p[4]);
        self.output.set(p[15], p[16], p[17], level_gain(p[19]) * STAGE_GAIN);
    }
}
