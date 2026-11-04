//! Triple Tap and Quad Tap: the mono input into one line with several taps.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/tap_delay.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::delay::Delay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::stereo_output::StereoOutput;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{STEP_MS, delay1, feedback_gain, hf_damp};

/// The highest delay value of the taps, a higher one is taken as this (the Dly1 column's 1000 ms).
/// The column is 1000 ms from 115 up anyway, so only the stored byte changes. It is kept because SC-VA stores
/// the clamped byte too.
const MAX_DELAY_VALUE: u8 = 115;

/// Which of the two tap delays (TS subclasses).
#[derive(Clone, Copy, PartialEq)]
enum TapKind {
    Triple,
    Quad,
}

/// Triple Tap and Quad Tap: the mono input into one line with several taps. Tap 1 feeds back through the HF damp,
/// the outputs take the raw taps, each with its gain per side.
/// The left output reads every tap one 32 kHz sample before the right output (the shared stage's timing).
pub struct GSTapDelay {
    base: GSEffectBase,
    kind: TapKind,
    /// Each tap's gain into the left and right outputs.
    gains_l: Vec<f64>,
    gains_r: Vec<f64>,
    /// The delay time parameters, tap by tap.
    time_parameters: &'static [u8],
    mix: MonoMix,
    /// Reads 0 to n - 1 are the taps for the left output, n to 2n - 1 the same taps for the right.
    delay: Delay,
    output: StereoOutput,
    tap_count: usize,
    feedback: f64,
}

impl GSTapDelay {
    /// `taps` is the number of taps.
    fn new(
        sample_rate: f64,
        memory: &mut DelayMemory,
        taps: usize,
        kind: TapKind,
        defaults: [u8; 20],
        time_parameters: &'static [u8],
    ) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), defaults, &[], &[]);
        let mut delay = Delay::new(sample_rate, memory, 2 * taps, 1000.0);
        // The right output's tap 1 is the line's own timing
        delay.set_feedback_tap(taps);
        Self {
            base,
            kind,
            gains_l: vec![0.0; taps],
            gains_r: vec![0.0; taps],
            time_parameters,
            mix: MonoMix::new(sample_rate),
            delay,
            output: StereoOutput::new(sample_rate),
            tap_count: taps,
            feedback: 0.0,
        }
    }

    /// Sets the taps, the loop and the output stage.
    /// `feedback` is the feedback parameter, `hf` the HF damp parameter.
    fn update_delay(&mut self, feedback: u8, hf: u8) {
        let p = self.base.parameters;
        for tap in 0..self.tap_count {
            let ms = delay1(p[self.time_parameters[tap] as usize]);
            self.delay.set_tap(tap, ms - STEP_MS);
            self.delay.set_tap(tap + self.tap_count, ms);
        }
        self.feedback = feedback_gain(feedback);
        self.delay.set_hf_damp(hf_damp(hf));
        self.output.set(
            self.base.controlled(1, 15),
            p[16],
            p[17],
            level_gain(p[19]) * STAGE_GAIN,
        );
    }
}

impl GSEffect for GSTapDelay {
    fn effect_type(&self) -> u16 {
        match self.kind {
            TapKind::Triple => 0x01_52,
            TapKind::Quad => 0x01_53,
        }
    }

    gs_effect_common!(@base base);

    fn set_parameter(&mut self, parameter: u8, mut value: u8) {
        if self.time_parameters.contains(&parameter) {
            value = value.min(MAX_DELAY_VALUE);
        }
        set_parameter_base(self, parameter, value);
    }

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let tap_count = self.tap_count;
        for i in 0..sample_count {
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            let x = self.mix.process(l, r) * LINE_INPUT + self.delay.feedback(1.0) * self.feedback;
            self.delay.process(x);
            let taps = &self.delay.output;
            let mut left = 0.0;
            let mut right = 0.0;
            for tap in 0..tap_count {
                left += self.gains_l[tap] * taps[tap];
                right += self.gains_r[tap] * taps[tap + tap_count];
            }
            self.output.process(l, left, r, right);
            output_left[i] = self.output.left as f32;
            output_right[i] = self.output.right as f32;
        }
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.delay.clear();
        self.output.clear();
    }

    fn update(&mut self) {
        match self.kind {
            TapKind::Triple => GSTripleTapDelay::update(self),
            TapKind::Quad => GSQuadTapDelay::update(self),
        }
    }
}

#[rustfmt::skip]
const TRIPLE_DEFAULTS: [u8; 20] = [
    0x14, 0x00, 0x07, 0x50, 0x7f, 0x7f, 0x7f, 0x7f, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x30, 0x40, 0x40, 0x00, 0x7f,
];

/// Triple Tap (`01 52`): center, left and right taps. The center goes to both sides and feeds back.
/// Parameters: 1-3 delay center, left, right (Dly1 column, up to 115), 4 feedback, 5-7 level center, left, right,
/// 8 HF damp, 16 balance, 17 low gain, 18 high gain, 20 level. EFX control source 1 modulates the feedback, source 2
/// the balance.
pub struct GSTripleTapDelay;

impl GSTripleTapDelay {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSTapDelay {
        let mut effect = GSTapDelay::new(sample_rate, memory, 3, TapKind::Triple, TRIPLE_DEFAULTS, &[0, 1, 2]);
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }

    fn update(effect: &mut GSTapDelay) {
        let p = effect.base.parameters;
        effect.gains_r[0] = level_gain(p[4]);
        effect.gains_l[0] = effect.gains_r[0];
        effect.gains_l[1] = level_gain(p[5]);
        effect.gains_r[2] = level_gain(p[6]);
        let feedback = effect.base.controlled(0, 3);
        effect.update_delay(feedback, p[7]);
    }
}

/// Quad Tap's fixed pans per tap, the left side (the right is the mirror image). The outer taps are inverted on
/// their main side, on the hardware too.
const QUAD_PANS: [f64; 4] = [-103.0 / 128.0, 77.0 / 128.0, 51.0 / 128.0, 25.0 / 128.0];

#[rustfmt::skip]
const QUAD_DEFAULTS: [u8; 20] = [
    0x3c, 0x14, 0x28, 0x00, 0x7f, 0x7f, 0x7f, 0x7f, 0x50, 0x7f, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x30, 0x40, 0x40, 0x00, 0x7f,
];

/// Quad Tap (`01 53`): four taps at fixed pans, tap 1 feeds back.
/// Parameters: 1-4 delays (Dly1 column, up to 115), 5-8 levels, 9 feedback, 10 HF damp, 16 balance, 17 low gain,
/// 18 high gain, 20 level. EFX control source 1 modulates the feedback, source 2 the balance.
pub struct GSQuadTapDelay;

impl GSQuadTapDelay {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSTapDelay {
        let mut effect = GSTapDelay::new(sample_rate, memory, 4, TapKind::Quad, QUAD_DEFAULTS, &[0, 1, 2, 3]);
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }

    fn update(effect: &mut GSTapDelay) {
        let p = effect.base.parameters;
        for tap in 0..4 {
            let level = level_gain(p[4 + tap]);
            effect.gains_l[tap] = level * QUAD_PANS[tap];
            effect.gains_r[tap] = level * QUAD_PANS[3 - tap];
        }
        let feedback = effect.base.controlled(0, 8);
        effect.update_delay(feedback, p[9]);
    }
}
