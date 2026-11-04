//! Compressor and Limiter: `L+R -> dynamics -> EQ -> pan`.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/dynamics_effect.ts

use super::prelude::*;
use super::{compressor, limiter};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::dynamics::{Compressor, Limiter};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_pan::MonoPan;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::shelf_eq::{ShelfEq, shelf_corners};

/// The post gains (0, +6, +12 and +18 dB, the last one is 2 x 127/32).
const POST_GAINS: [f64; 4] = [1.0, 2.0, 4.0, (2.0 * 127.0) / 32.0];

/// The dynamics block of the type (TS `abstract readonly dynamics` with the subclass's `updateDynamics`).
pub enum DynamicsBlock {
    Compressor(Compressor),
    Limiter(Limiter),
}

/// Compressor and Limiter: `L+R -> dynamics -> EQ -> pan`.
/// Parameters 17 low gain, 18 high gain, 19 pan, 20 level. EFX control source 1 modulates the pan, source 2 the
/// level.
pub struct GSDynamicsEffect {
    base: GSEffectBase,
    effect_type: u16,
    dynamics: DynamicsBlock,
    /// The fixed gain in front of the post gain.
    pre_gain: f64,
    mix: MonoMix,
    shelves: ShelfEq,
    pan: MonoPan,
}

impl GSDynamicsEffect {
    /// The subclass constructor resets.
    pub fn new(
        sample_rate: f64,
        effect_type: u16,
        defaults: [u8; 20],
        limits: &'static [(u8, u8)],
        dynamics: DynamicsBlock,
        pre_gain: f64,
    ) -> Self {
        Self {
            base: GSEffectBase::new(sample_rate, None, defaults, limits, &[]),
            effect_type,
            dynamics,
            pre_gain,
            mix: MonoMix::new(sample_rate),
            shelves: ShelfEq::new(sample_rate),
            pan: MonoPan::new(sample_rate),
        }
    }
}

impl GSEffect for GSDynamicsEffect {
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
            let x = self.mix.process(
                saturate_at(input_left[i] as f64, SATURATED_INPUT),
                saturate_at(input_right[i] as f64, SATURATED_INPUT),
            );
            let y = match &mut self.dynamics {
                DynamicsBlock::Compressor(d) => d.process(x * MONO_INPUT),
                DynamicsBlock::Limiter(d) => d.process(x * MONO_INPUT),
            };
            self.pan.process(self.shelves.process(y));
            let l = self.pan.left;
            let r = self.pan.right;
            output_left[i] = saturate_at(l, SATURATED_OUTPUT) as f32;
            output_right[i] = saturate_at(r, SATURATED_OUTPUT) as f32;
        }
    }

    fn clear(&mut self) {
        self.mix.clear();
        match &mut self.dynamics {
            DynamicsBlock::Compressor(d) => d.clear(),
            DynamicsBlock::Limiter(d) => d.clear(),
        }
        self.shelves.clear();
        self.pan.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.shelves.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        // this.updateDynamics()
        match &mut self.dynamics {
            DynamicsBlock::Compressor(d) => {
                let index = compressor::update_dynamics(d, &p);
                d.set_post_gain(POST_GAINS[index] * self.pre_gain);
            }
            DynamicsBlock::Limiter(d) => {
                let index = limiter::update_dynamics(d, &p);
                d.set_post_gain(POST_GAINS[index] * self.pre_gain);
            }
        }
        // The EQ input has one more 127/128 than the output stage
        self.pan.set(
            self.base.controlled(0, 18),
            (level_gain(self.base.controlled(1, 19)) * STAGE_OUTPUT * 127.0) / 128.0,
        );
    }
}
