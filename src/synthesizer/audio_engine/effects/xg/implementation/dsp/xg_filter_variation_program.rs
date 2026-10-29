//! The base for the S-YXG50 category 3 variation programs (auto wah, karaoke),
//! which run their own input filters.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/xg_filter_variation_program.ts
//!
//! 0x86 is scaled by 2^-19 and 0x87 by 8, 0x7e sets the LFO step and the frequency scale (0xbf).

use super::xg_dsp_program::{fround, XGDSPProgram};
use super::xg_variation_program::XGVariationState;

/// The category 3 coefficients that the original never writes.
pub const UNWRITTEN_CATEGORY_3: &[usize] = &[0x7e, 0x7f, 0x8c, 0x8e, 0x99, 0x9b];

/// The category 3 index.
pub const CATEGORY: usize = 3;

/// Access to the category 3 fields, for the `filter_push` helper used by the macro.
pub trait FilterFields {
    /// The program, the variation state and the auto wah LFO step (coefficient 0x7e).
    fn filter_fields(&mut self) -> (&mut XGDSPProgram, &mut XGVariationState, &mut i32);
}

/// Equivalent to: XGFilterVariationProgram.pushCoefficients
pub fn filter_push<P: FilterFields + ?Sized>(p: &mut P, list: &[usize], count: usize, offset: usize) {
    let (program, var, wah_step) = p.filter_fields();
    var.push_coefficients(program, list, count, offset);
    if !program.push_enabled {
        return;
    }
    for &k in &list[offset..offset + count] {
        match k {
            0x86 => {
                let c = &mut program.coefficients;
                c[k] = fround(c[k] as f64 * 2f64.powi(-19)) as f32;
            }
            0x87 => {
                let c = &mut program.coefficients;
                c[k] = fround(c[k] as f64 * 8.0) as f32;
            }
            0x7e => {
                // The LFO step and the rate-dependent frequency scale
                *wah_step = (program.raw_coefficient(0x7e) * program.rate_inverse * 16_777_216.0).trunc() as i32;
                program.coefficients[0xbf] = fround(program.rate_inverse / 256.0) as f32;
            }
            _ => {}
        }
    }
}

/// Implements `XGVariationProgram` + `XGProgram` for a category 3 program struct with the fields
/// `program`, `var` and `wah_step: i32`, and the inherent methods `convert_impl`,
/// `reset_state_impl` and `render_impl`.
#[macro_export]
macro_rules! impl_xg_filter_variation_program {
    ($ty:ty $(, clamp = $clamp:path)?) => {
        impl $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_variation_program::XGVariationProgram
            for $ty
        {
            fn variation(
                &self,
            ) -> &$crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_variation_program::XGVariationState
            {
                &self.var
            }

            fn variation_mut(
                &mut self,
            ) -> &mut $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_variation_program::XGVariationState
            {
                &mut self.var
            }

            fn render(
                &mut self,
                memory: &mut [f32],
                output_left: &mut [f32],
                output_right: &mut [f32],
                sample_count: usize,
                output_gain: f64,
            ) {
                self.render_impl(memory, output_left, output_right, sample_count, output_gain);
            }
        }

        impl $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_filter_variation_program::FilterFields
            for $ty
        {
            fn filter_fields(
                &mut self,
            ) -> (
                &mut $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_dsp_program::XGDSPProgram,
                &mut $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_variation_program::XGVariationState,
                &mut i32,
            ) {
                (&mut self.program, &mut self.var, &mut self.wah_step)
            }
        }

        $crate::impl_xg_variation_program!(
            $ty,
            push = $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_filter_variation_program::filter_push,
            reset = $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_variation_program::variation_reset
            $(, clamp = $clamp)?
        );
    };
}
