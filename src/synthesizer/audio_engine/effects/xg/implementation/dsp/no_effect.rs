//! NO EFFECT.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/no_effect.ts

use super::effect_processor::DefaultXGEffectProcessor;

/// MU128 manual description:
/// Turn off the effect.
pub struct XGNoEffect;

impl XGNoEffect {
    pub const MEMORY_SIZE: usize = 0;
}

impl DefaultXGEffectProcessor for XGNoEffect {
    fn effect_type(&self) -> u16 {
        0x00_00
    }

    fn memory_size(&self) -> usize {
        Self::MEMORY_SIZE
    }

    fn reset(&mut self, _memory: &mut [f32]) {
        // Noop
    }

    fn set_parameter(&mut self, _param: i32, _value: i32) {}

    fn process(
        &mut self,
        _memory: &mut [f32],
        _input_left: &[f32],
        _input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
        _is_insertion: bool,
    ) {
        // Silence, honoring the OVERWRITE behavior.
        output_left[..sample_count].fill(0.0);
        output_right[..sample_count].fill(0.0);
    }

    fn get_snapshot(&self) -> [i16; 16] {
        [0; 16]
    }
}
