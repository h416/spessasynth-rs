//! The parallel combinations of a chorus or flanger: the chorus or flanger on the left input, the second effect on
//! the right input.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/modulation_parallel.ts

use super::combination_stage::{CombinationStage, ModulationStage, create_combination_stage};
use super::parallel_effect::GSParallelEffect;
use super::prelude::*;

/// The parallel combinations of a chorus or flanger: the left input runs through the chorus or flanger, the right
/// input through the second effect, each with its balance, level and pan. No EQ.
/// The chorus/flanger mixes two reads 180 degrees apart. Parameters 1-5 are the chorus/flanger (pre delay, rate,
/// depth, feedback, balance), 6-10 the second effect, 16-20 the pans and levels. EFX control source 1 modulates the
/// chorus/flanger balance, source 2 the second effect's.
/// The second effect's reads for the right output are one 32 kHz sample later (`GSParallelEffect`).
pub struct GSModulationParallel {
    pe: GSParallelEffect,
    effect_type: u16,
    first: ModulationStage,
    second: Box<dyn CombinationStage>,
    first_dry: f64,
    first_wet: f64,
    dry: f64,
    wet: f64,
}

impl GSModulationParallel {
    /// `flanger` is the flanger instead of the chorus on the left input, `stage` the second effect. The subclass
    /// constructor resets.
    pub fn new(
        sample_rate: f64,
        memory: &mut DelayMemory,
        effect_type: u16,
        defaults: [u8; 20],
        flanger: bool,
        stage: u8,
    ) -> Self {
        let pe = GSParallelEffect::new(sample_rate, Some(memory), defaults, &[]);
        let first = ModulationStage::new(sample_rate, memory, flanger, 0, true);
        let second = create_combination_stage(stage, sample_rate, memory, 1);
        Self {
            pe,
            effect_type,
            first,
            second,
            first_dry: 0.0,
            first_wet: 0.0,
            dry: 0.0,
            wet: 0.0,
        }
    }
}

impl GSEffect for GSModulationParallel {
    fn effect_type(&self) -> u16 {
        self.effect_type
    }

    gs_effect_common!(@base pe.base);

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
        let dry = self.dry;
        let wet = self.wet;
        let gain_l = self.pe.gain_l;
        let gain_r = self.pe.gain_r;
        for i in 0..sample_count {
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            self.second.process(r * LINE_INPUT);
            self.first.process(l * LINE_INPUT);
            self.pe.route_first(first_dry * l + first_wet * self.first.left());
            output_left[i] = (self.pe.left + gain_l * (dry * r + wet * self.second.left())) as f32;
            let lagged = self.pe.lag_second(dry * r);
            output_right[i] = (self.pe.right + gain_r * (lagged + wet * self.second.right())) as f32;
        }
    }

    fn clear(&mut self) {
        self.pe.clear();
        self.first.clear();
        self.second.clear();
    }

    fn update(&mut self) {
        let p = self.pe.base.parameters;
        self.first.update(&p, 0, None);
        let first = self.pe.base.controlled(0, 4);
        self.first_dry = balance_dry(first);
        self.first_wet = balance_wet(first);
        self.second.update(&p, 5, None);
        let second = self.pe.base.controlled(1, 9);
        self.dry = balance_dry(second);
        self.wet = balance_wet(second);
        let gain = level_gain(p[19]) * STAGE_GAIN;
        self.pe.update_pans(gain, gain);
    }
}
