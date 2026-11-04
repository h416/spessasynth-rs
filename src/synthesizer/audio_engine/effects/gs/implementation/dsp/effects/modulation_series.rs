//! The series combinations of a chorus or flanger: `L+R -> chorus/flanger -> balance`, then the second stage.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/modulation_series.ts

use super::combination_stage::{CombinationStage, STAGE_MIX_GAIN, create_combination_stage};
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::stereo_output::StereoOutput;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::enums::combination_stages;

/// The series combinations of a chorus or flanger: `L+R -> chorus/flanger -> balance`, the two sides mixed again
/// into the second stage, its balance, then the 2-band EQ.
/// Parameters 1-5 are the first stage (pre delay, rate, depth, feedback, balance), 6-10 the second, 17 low gain,
/// 18 high gain, 20 level.
///
/// The shared stage reads the left input one 32 kHz sample late, so every mono line is read one sample earlier for
/// the left output than for the right one (as in the tap delays).
pub struct GSModulationSeries {
    base: GSEffectBase,
    effect_type: u16,
    /// The flanger instead of the chorus in the first stage.
    flanger: bool,
    mix: MonoMix,
    first: Box<dyn CombinationStage>,
    second_mix: MonoMix,
    second: Box<dyn CombinationStage>,
    output: StereoOutput,
    first_dry: f64,
    first_wet: f64,
}

impl GSModulationSeries {
    /// `flanger` is the flanger instead of the chorus in the first stage, `stage` the second stage. The subclass
    /// constructor resets.
    pub fn new(
        sample_rate: f64,
        memory: &mut DelayMemory,
        effect_type: u16,
        defaults: [u8; 20],
        flanger: bool,
        stage: u8,
    ) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), defaults, &[], &[]);
        let first = create_combination_stage(
            if flanger { combination_stages::FLANGER } else { combination_stages::CHORUS },
            sample_rate,
            memory,
            -1,
        );
        let second = create_combination_stage(stage, sample_rate, memory, -1);
        Self {
            base,
            effect_type,
            flanger,
            mix: MonoMix::new(sample_rate),
            first,
            second_mix: MonoMix::new(sample_rate),
            second,
            output: StereoOutput::new(sample_rate),
            first_dry: 0.0,
            first_wet: 0.0,
        }
    }
}

impl GSEffect for GSModulationSeries {
    gs_effect_common!(dynamic);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let first_dry = self.first_dry;
        let first_wet = self.first_wet;
        for i in 0..sample_count {
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            self.first.process(self.mix.process(l, r) * STAGE_MIX_GAIN);
            let left = first_dry * l + first_wet * self.first.left();
            let right = first_dry * r + first_wet * self.first.right();
            self.second
                .process(self.second_mix.process(left, right) * STAGE_MIX_GAIN);
            self.output
                .process(left, self.second.left(), right, self.second.right());
            output_left[i] = self.output.left as f32;
            output_right[i] = self.output.right as f32;
        }
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.first.clear();
        self.second_mix.clear();
        self.second.clear();
        self.output.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        // On the SC-88Pro source 1 moves the flanger's feedback, as the SC-8850 manual says (SC-VA moves its balance)
        self.first.update(&p, 0, Some(self.base.controlled(0, 3)));
        let balance = if self.flanger { p[4] } else { self.base.controlled(0, 4) };
        self.first_dry = balance_dry(balance);
        self.first_wet = balance_wet(balance);
        self.second.update(&p, 5, None);
        self.output.set(
            self.base.controlled(1, 9),
            p[16],
            p[17],
            level_gain(p[19]) * STAGE_GAIN,
        );
    }
}
