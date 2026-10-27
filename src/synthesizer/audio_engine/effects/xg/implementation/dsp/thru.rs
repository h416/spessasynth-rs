//! THRU.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/thru.ts

use super::effect_processor::DefaultXGEffectProcessor;

/// MU128 manual description:
/// Bypass without applying an effect.
pub struct XGThru;

impl XGThru {
    pub const MEMORY_SIZE: usize = 0;
}

impl DefaultXGEffectProcessor for XGThru {
    fn effect_type(&self) -> u16 {
        0x40_00
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
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
        is_insertion: bool,
    ) {
        if is_insertion {
            // Thru is bypass, not disabled
            output_left[..sample_count].copy_from_slice(&input_left[..sample_count]);
            output_right[..sample_count].copy_from_slice(&input_right[..sample_count]);
        } else {
            output_left[..sample_count].fill(0.0);
            output_right[..sample_count].fill(0.0);
        }
    }

    fn get_snapshot(&self) -> [i16; 16] {
        [0; 16]
    }
}
