//! 3D Auto and 3D Manual: `L+R -> positioner -> output mode`.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/three_d.ts

use super::prelude::*;
use super::{three_d_auto, three_d_manual};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::output_lag::OutputLag;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::output_mode_3d::OutputMode3D;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::positioner_3d::Positioner3D;

/// Which 3D type (TS subclasses).
#[derive(Clone, Copy, PartialEq)]
pub enum ThreeDKind {
    Auto,
    Manual,
}

/// The limits of 3D Manual (3D Auto has its own).
pub(super) const LIMITS: &[(u8, u8)] = &[(14, 1)];

/// 3D Auto and 3D Manual: `L+R -> positioner -> output mode`.
/// The right output comes one 32 kHz sample after the left one, as from the shared stage (it adds to the time
/// between the ears).
pub struct GSThreeD {
    pub(super) base: GSEffectBase,
    kind: ThreeDKind,
    pub(super) positioner: Positioner3D,
    mix: MonoMix,
    output: OutputMode3D,
    lag: OutputLag,
    gain: f64,
}

impl GSThreeD {
    /// The subclass constructor resets.
    pub fn new(
        sample_rate: f64,
        kind: ThreeDKind,
        defaults: [u8; 20],
        limits: &'static [(u8, u8)],
    ) -> Self {
        Self {
            base: GSEffectBase::new(sample_rate, None, defaults, limits, &[]),
            kind,
            positioner: Positioner3D::new(sample_rate),
            mix: MonoMix::new(sample_rate),
            output: OutputMode3D::new(sample_rate, 12.0, true),
            lag: OutputLag::new(sample_rate, 1.0),
            gain: 0.0,
        }
    }

    /// The level parameter with its EFX control.
    fn level(&self) -> u8 {
        match self.kind {
            ThreeDKind::Auto => three_d_auto::level(self),
            ThreeDKind::Manual => three_d_manual::level(self),
        }
    }

    /// Applies the azimuth and the turning.
    fn update_position(&mut self) {
        match self.kind {
            ThreeDKind::Auto => three_d_auto::update_position(self),
            ThreeDKind::Manual => three_d_manual::update_position(self),
        }
    }
}

impl GSEffect for GSThreeD {
    fn effect_type(&self) -> u16 {
        match self.kind {
            ThreeDKind::Auto => 0x01_70,
            ThreeDKind::Manual => 0x01_71,
        }
    }

    gs_effect_common!(@base base);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let gain = self.gain;
        for i in 0..sample_count {
            let x = self.mix.process(input_left[i] as f64, input_right[i] as f64);
            self.positioner.process(x);
            self.output.process(self.positioner.left, self.positioner.right);
            output_left[i] = (self.output.left * gain) as f32;
            output_right[i] = self.lag.process(self.output.right * gain) as f32;
        }
    }

    fn reset(&mut self) {
        reset_base(self);
        self.output.settle();
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.positioner.clear();
        self.output.clear();
        self.lag.clear();
    }

    fn update(&mut self) {
        self.output.set_phones(self.base.parameters[14] == 1);
        self.gain = level_gain(self.level()) * STAGE_GAIN;
        self.update_position();
    }
}
