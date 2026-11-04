//! EH -> Chorus, Flanger, Delay: a stereo enhancer is the dry signal, its mono sum feeds the second stage.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/enhancer_combination.ts

use super::combination_stage::{CombinationStage, STAGE_MIX_GAIN, create_combination_stage};
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::enhancer::Enhancer;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::stereo_output::StereoOutput;

/// The enhancer input's gain.
const INPUT_GAIN: f64 = 127.0 / 128.0;

/// EH -> Chorus, Flanger, Delay: a stereo enhancer is the dry signal, the mono sum of its sides feeds the chorus,
/// flanger or delay. Then the balance and the 2-band EQ per side.
/// Parameters: 1 sensitivity, 2 mix level, 6-9 the second stage, 10 balance, 17 low gain, 18 high gain, 20 level.
/// EFX control source 1 modulates the sensitivity, source 2 the balance.
pub struct GSEnhancerCombination {
    base: GSEffectBase,
    effect_type: u16,
    enhancer_l: Enhancer,
    enhancer_r: Enhancer,
    mix: MonoMix,
    stage: Box<dyn CombinationStage>,
    output: StereoOutput,
    enhance_right: bool,
}

impl GSEnhancerCombination {
    /// `stage` is the second stage, `enhance_right` enhance the right side too, else it only mixes the right
    /// enhancer's dry signal. The subclass constructor resets.
    pub fn new(
        sample_rate: f64,
        memory: &mut DelayMemory,
        effect_type: u16,
        defaults: [u8; 20],
        stage: u8,
        enhance_right: bool,
    ) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), defaults, &[], &[]);
        let stage = create_combination_stage(stage, sample_rate, memory, -1);
        Self {
            base,
            effect_type,
            enhancer_l: Enhancer::new(sample_rate),
            enhancer_r: Enhancer::new(sample_rate),
            mix: MonoMix::new(sample_rate),
            stage,
            output: StereoOutput::new(sample_rate),
            enhance_right,
        }
    }
}

impl GSEffect for GSEnhancerCombination {
    gs_effect_common!(dynamic);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        for i in 0..sample_count {
            let l = self.enhancer_l.process(input_left[i] as f64 * INPUT_GAIN);
            let r = self.enhancer_r.process(input_right[i] as f64 * INPUT_GAIN);
            self.stage.process(self.mix.process(l, r) * STAGE_MIX_GAIN);
            self.output.process(l, self.stage.left(), r, self.stage.right());
            output_left[i] = self.output.left as f32;
            output_right[i] = self.output.right as f32;
        }
    }

    fn clear(&mut self) {
        self.enhancer_l.clear();
        self.enhancer_r.clear();
        self.mix.clear();
        self.stage.clear();
        self.output.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        // The sensitivity is 0 dB at 16 and grows linearly, the mix follows the level curve
        let sensitivity = self.base.controlled(0, 0).max(1) as f64 / 16.0;
        let mix = level_gain(p[1]);
        self.enhancer_l.set(sensitivity, mix);
        self.enhancer_r
            .set(if self.enhance_right { sensitivity } else { 0.0 }, mix);
        self.stage.update(&p, 5, None);
        self.output.set(
            self.base.controlled(1, 9),
            p[16],
            p[17],
            level_gain(p[19]) * STAGE_GAIN,
        );
    }
}
