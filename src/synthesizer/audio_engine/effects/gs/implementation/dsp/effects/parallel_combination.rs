//! The parallel combinations with a drive, phaser, auto wah or rotary.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/parallel_combination.ts

use super::parallel_effect::GSParallelEffect;
use super::parallel_stage::{ParallelStage, create_parallel_stage};
use super::prelude::*;

/// An effect of a parallel combination and the gains around it. The programs differ in a 127/128 here and there:
/// the phaser of PH / Rotary and PH / AutoWah takes its input (and its dry signal) at 127/128, OD / Phaser's at
/// unity. PH / AutoWah's wah takes its input at 127/128 like the Auto Wah, OD / AutoWah's at unity and puts the
/// 127/128 after the filter.
pub struct ParallelStageSpec {
    pub r#type: u8,
    /// The gain in front of the effect, 1 if missing.
    pub input: Option<f64>,
    /// The gain after the effect, 1 if missing.
    pub output: Option<f64>,
}

/// The parallel combinations with a drive, phaser, auto wah or rotary (OD1 / OD2, OD / Rotary, OD / Phaser,
/// OD / AutoWah, PH / Rotary, PH / AutoWah). No EQ. Effect 1 takes parameters 1-5 and EFX control source 1,
/// effect 2 parameters 6-15 and source 2.
/// Unlike the chorus/flanger and delay combinations, there is no mix with the input around the effects (the phaser
/// has its own mix).
pub struct GSParallelCombination {
    pe: GSParallelEffect,
    effect_type: u16,
    first: Box<dyn ParallelStage>,
    second: Box<dyn ParallelStage>,
    first_input: f64,
    second_input: f64,
    first_output: f64,
    second_output: f64,
}

impl GSParallelCombination {
    /// The subclass constructor resets.
    pub fn new(
        sample_rate: f64,
        memory: &mut DelayMemory,
        effect_type: u16,
        defaults: [u8; 20],
        limits: &'static [(u8, u8)],
        first: ParallelStageSpec,
        second: ParallelStageSpec,
    ) -> Self {
        let pe = GSParallelEffect::new(sample_rate, Some(memory), defaults, limits);
        let first_stage = create_parallel_stage(first.r#type, sample_rate, memory);
        let second_stage = create_parallel_stage(second.r#type, sample_rate, memory);
        Self {
            pe,
            effect_type,
            first: first_stage,
            second: second_stage,
            first_input: first.input.unwrap_or(1.0) * STAGE_INPUT,
            second_input: second.input.unwrap_or(1.0) * STAGE_INPUT,
            first_output: first.output.unwrap_or(1.0) * STAGE_OUTPUT,
            second_output: second.output.unwrap_or(1.0) * STAGE_OUTPUT,
        }
    }
}

impl GSEffect for GSParallelCombination {
    fn effect_type(&self) -> u16 {
        self.effect_type
    }

    gs_effect_common!(@base pe.base);

    fn reset(&mut self) {
        reset_base(self);
        self.first.settle();
        self.second.settle();
    }

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let first_input = self.first_input;
        let second_input = self.second_input;
        let gain_l = self.pe.gain_l;
        let gain_r = self.pe.gain_r;
        let lagged = self.second.lagged();
        for i in 0..sample_count {
            self.first.process(input_left[i] as f64 * first_input);
            self.pe.route_first(self.first.left());
            self.second.process(input_right[i] as f64 * second_input);
            output_left[i] = (self.pe.left + gain_l * self.second.left()) as f32;
            let right = if lagged {
                self.second.right()
            } else {
                self.pe.lag_second(self.second.right())
            };
            output_right[i] = (self.pe.right + gain_r * right) as f32;
        }
    }

    fn clear(&mut self) {
        self.pe.clear();
        self.first.clear();
        self.second.clear();
    }

    fn update(&mut self) {
        let p = self.pe.base.parameters;
        let first_control = self.pe.base.controlled(0, self.first.target());
        self.first.update(&p, 0, Some(first_control));
        let second_control = self.pe.base.controlled(1, 5 + self.second.target());
        self.second.update(&p, 5, Some(second_control));
        let level = level_gain(p[19]);
        self.pe
            .update_pans(level * self.first_output, level * self.second_output);
    }
}
