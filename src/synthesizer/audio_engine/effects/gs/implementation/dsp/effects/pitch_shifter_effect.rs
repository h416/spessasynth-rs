//! The pitch shifters: the mono input into a `PitchShifter`, each voice panned, then the balance and the 2-band EQ.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/pitch_shifter_effect.ts

use super::prelude::*;
use super::{feedback_pitch_shifter, two_voice_pitch_shifter};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::pitch_shifter::PitchShifter;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::stereo_output::StereoOutput;

/// Which pitch shifter (TS subclasses).
#[derive(Clone, Copy, PartialEq)]
pub enum PitchShifterKind {
    TwoVoice,
    Feedback,
}

/// The pitch shifters: the mono input into a `PitchShifter`, each voice panned, then the balance and the 2-band EQ.
/// The left output takes the voices one 32 kHz sample ahead of the right (the shared stage's timing).
pub struct GSPitchShifterEffect {
    pub(super) base: GSEffectBase,
    kind: PitchShifterKind,
    /// Each voice's gain into the left and right outputs.
    pub(super) gains_l: Vec<f64>,
    pub(super) gains_r: Vec<f64>,
    pub(super) shifter: PitchShifter,
    /// The mono input's gain into the line.
    input_gain: f64,
    /// The fine pitch parameters, clamped to -100 to +100 cents (14-114).
    fine_parameters: &'static [u8],
    mix: MonoMix,
    output: StereoOutput,
}

impl GSPitchShifterEffect {
    /// `voices` is the number of voices. The subclass constructor resets.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sample_rate: f64,
        memory: &mut DelayMemory,
        voices: usize,
        kind: PitchShifterKind,
        defaults: [u8; 20],
        limits: &'static [(u8, u8)],
        minimums: &'static [(u8, u8)],
        fine_parameters: &'static [u8],
        input_gain: f64,
    ) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), defaults, limits, minimums);
        let shifter = PitchShifter::new(sample_rate, memory, voices);
        Self {
            base,
            kind,
            gains_l: vec![0.0; voices],
            gains_r: vec![0.0; voices],
            shifter,
            input_gain,
            fine_parameters,
            mix: MonoMix::new(sample_rate),
            output: StereoOutput::new(sample_rate),
        }
    }

    /// Sets the balance and the 2-band EQ.
    pub(super) fn update_output(&mut self) {
        let p = self.base.parameters;
        self.output.set(p[15], p[16], p[17], level_gain(p[19]) * STAGE_GAIN);
    }
}

impl GSEffect for GSPitchShifterEffect {
    fn effect_type(&self) -> u16 {
        match self.kind {
            PitchShifterKind::TwoVoice => 0x01_60,
            PitchShifterKind::Feedback => 0x01_61,
        }
    }

    gs_effect_common!(@base base);

    fn set_parameter(&mut self, parameter: u8, mut value: u8) {
        if self.fine_parameters.contains(&parameter) {
            value = 14.max(114.min(value));
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
        for i in 0..sample_count {
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            let x = self.mix.process(l, r) * self.input_gain;
            self.shifter.process(x);
            let mut left = 0.0;
            let mut right = 0.0;
            for v in 0..self.gains_l.len() {
                left += self.gains_l[v] * self.shifter.early[v];
                right += self.gains_r[v] * self.shifter.output[v];
            }
            self.output.process(l, left, r, right);
            output_left[i] = self.output.left as f32;
            output_right[i] = self.output.right as f32;
        }
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.shifter.clear();
        self.output.clear();
    }

    fn update(&mut self) {
        match self.kind {
            PitchShifterKind::TwoVoice => two_voice_pitch_shifter::update(self),
            PitchShifterKind::Feedback => feedback_pitch_shifter::update(self),
        }
    }
}
