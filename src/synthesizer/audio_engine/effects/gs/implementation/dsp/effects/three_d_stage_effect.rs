//! 3D Chorus and 3D Delay: a mono effect whose left and right parts go through the 3D stage.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/three_d_stage_effect.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::crosstalk_3d::Crosstalk3D;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::stereo_output::StereoOutput;

const LIMITS: &[(u8, u8)] = &[(14, 1)];

/// 3D Chorus and 3D Delay: a mono effect whose left and right parts go through the 3D stage (at left and right
/// 90 degrees), then the balance and the EQ. Parameter 15 is the output (speaker, phones), EFX control source 2
/// modulates the balance.
///
/// The shared stage reads the left input one 32 kHz sample late, so the left output hears the effect one sample
/// earlier than the right one: the left output comes from a second 3D stage fed by reads one sample earlier.
///
/// TS has this as an abstract class; the Rust types hold it and implement `GSEffect` themselves.
pub struct GSThreeDStageEffect {
    pub base: GSEffectBase,
    pub mix: MonoMix,
    pub stage_l: Crosstalk3D,
    pub stage_r: Crosstalk3D,
    pub output: StereoOutput,
}

impl GSThreeDStageEffect {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory, defaults: [u8; 20]) -> Self {
        Self {
            base: GSEffectBase::new(sample_rate, Some(memory), defaults, LIMITS, &[]),
            mix: MonoMix::new(sample_rate),
            stage_l: Crosstalk3D::new(sample_rate),
            stage_r: Crosstalk3D::new(sample_rate),
            output: StereoOutput::new(sample_rate),
        }
    }

    pub fn clear(&mut self) {
        self.mix.clear();
        self.stage_l.clear();
        self.stage_r.clear();
        self.output.clear();
    }

    /// Sets the output mode, the balance, the EQ and the level.
    pub fn update_stage(&mut self) {
        let p = self.base.parameters;
        self.stage_l.set_phones(p[14] == 1);
        self.stage_r.set_phones(p[14] == 1);
        self.output.set(
            self.base.controlled(1, 15),
            p[16],
            p[17],
            level_gain(p[19]) * STAGE_GAIN,
        );
    }
}
