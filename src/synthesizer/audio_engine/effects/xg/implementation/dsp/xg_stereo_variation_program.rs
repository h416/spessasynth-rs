//! The base for the S-YXG50 category 0 variation programs
//! (delays, tremolo, auto pan, symphonic, rotary speaker, 2-band EQ).
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/xg_stereo_variation_program.ts
//!
//! A stereo two-band input EQ runs before every routine,
//! and the LFO integers and rate-dependent values are derived from the coefficients.

use super::xg_dsp_program::{fround, XGDSPProgram, XGFilterShape};
use super::xg_dsp_tables::LFO_RATE;
use super::xg_variation_program::{variation_reset, XGVariationProgram, XGVariationState};

// Input EQ push lists (b0, b1, a1)
const LIST_LOW_LEFT: &[usize] = &[0x7f, 0x7e, 0x80];
const LIST_LOW_RIGHT: &[usize] = &[0x99, 0x98, 0x9a];
const LIST_HIGH_LEFT: &[usize] = &[0xa8, 0xa7, 0xa9];
const LIST_HIGH_RIGHT: &[usize] = &[0x9c, 0x9b, 0x9d];
// Input EQ coefficients (b1, b0, a1): left low, left high, right low, right high
const LIST_EQUALIZER: &[usize] =
    &[0x7e, 0x7f, 0x80, 0xa7, 0xa8, 0xa9, 0x98, 0x99, 0x9a, 0x9b, 0x9c, 0x9d];

/// The category 0 index.
pub const CATEGORY: usize = 0;

/// The fields of XGStereoVariationProgram.
#[derive(Default)]
pub struct XGStereoState {
    // LFO steps (rate) and phase offsets, derived from the coefficients.
    // The offsets come from the coefficients 0x6e, 0x79, 0x74, 0x84, 0x88 and 0x8d
    pub lfo_step: i32,
    pub lfo_step2: i32,
    pub lfo_offset1: i32,
    pub lfo_offset2: i32,
    pub lfo_offset3: i32,
    pub lfo_offset4: i32,
    pub lfo_offset5: i32,
    pub lfo_offset6: i32,
    equalizer_state: [f64; 6],
}

impl XGStereoState {
    /// Equivalent to: XGStereoVariationProgram.pushCoefficients
    /// (note: the LFO rules run even when the push is disabled, like the TS code)
    pub fn push_coefficients(
        &mut self,
        program: &mut XGDSPProgram,
        var: &mut XGVariationState,
        list: &[usize],
        count: usize,
        offset: usize,
    ) {
        var.push_coefficients(program, list, count, offset);
        // LFO integers and rate-dependent values
        let inverse = program.rate_inverse;
        let depth_scale = fround(program.rate_ratio * 2048.0);
        let c = &mut program.coefficients;
        for &k in &list[offset..offset + count] {
            let raw = c[k] as f64;
            match k {
                // LFO smoothing feedback and input
                0x71 | 0x7b | 0x8a | 0x8f => {
                    c[k] = 0f64.max(fround(1.0 - raw * inverse * 4.0)) as f32;
                }
                0x72 | 0x7c | 0x8b | 0x90 => {
                    c[k] = 1f64.min(fround(raw * inverse * 4.0)) as f32;
                }
                // Modulation depths in samples
                0x75 | 0x76 | 0x85 | 0x86 | 0x95 | 0x96 | 0xa5 => {
                    c[k] = fround(raw * depth_scale) as f32;
                }
                0x77 => {
                    self.lfo_step = (raw * inverse * 16_777_216.0).trunc() as i32;
                    self.lfo_step2 = (c[0x70] as f64 * c[0x6d] as f64 * raw * inverse * 16_777_216.0)
                        .trunc() as i32;
                }
                0x6d | 0x70 => {
                    self.lfo_step2 = (c[0x77] as f64 * c[0x70] as f64 * c[0x6d] as f64 * inverse
                        * 16_777_216.0)
                        .trunc() as i32;
                }
                0x6e => self.lfo_offset1 = (raw * 4_194_304.0).trunc() as i32,
                0x79 => self.lfo_offset2 = (raw * 4_194_304.0).trunc() as i32,
                0x74 => self.lfo_offset3 = (raw * 4_194_304.0).trunc() as i32,
                0x84 => self.lfo_offset4 = (raw * 4_194_304.0).trunc() as i32,
                0x88 => self.lfo_offset5 = (raw * 4_194_304.0).trunc() as i32,
                0x8d => self.lfo_offset6 = (raw * 4_194_304.0).trunc() as i32,
                _ => {}
            }
        }
    }

    /// Runs the input EQ (two first-order sections per side).
    /// Equivalent to: XGStereoVariationProgram.prepareInput
    pub fn prepare_input(
        &mut self,
        program: &XGDSPProgram,
        var: &mut XGVariationState,
        input_left: &[f32],
        input_right: &[f32],
        sample_count: usize,
        gain: f64,
    ) {
        program.equalize_stereo(
            input_left,
            input_right,
            &mut var.input_left,
            &mut var.input_right,
            sample_count,
            gain,
            LIST_EQUALIZER,
            &mut self.equalizer_state,
        );
    }
}

/// The overridable part of XGStereoVariationProgram and its helpers.
pub trait XGStereoVariationProgram: XGVariationProgram {
    fn stereo_mut(&mut self) -> &mut XGStereoState;

    /// Writes and pushes the LFO rate coefficient word.
    fn lfo_rate(&mut self, rate: i32) {
        self.program_mut().coefficient_words[0x77] = LFO_RATE[rate as usize];
        self.push_coefficients_all(&[0x77]);
    }

    /// Shapes one LFO smoothing stage (input, feedback, offset, center) from the rate.
    /// The input takes half of the rate mantissa, or the rate itself when the feedback is doubled.
    fn shape_smooth_lfo(&mut self, list: &[usize], doubled: bool) {
        let w = &mut self.program_mut().coefficient_words;
        let rate = w[0x77];
        if doubled {
            // One more exponent step, saturating at the largest mantissa
            let exponent = (rate & 0x3_80) + 0x80;
            w[list[0]] = rate;
            w[list[1]] = if exponent < 0x3_80 { exponent | (rate & 0x7f) } else { 0x3_7f };
        } else {
            w[list[0]] = ((rate >> 1) & 0x3f) | (rate & 0x3_80);
            w[list[1]] = rate;
        }
        w[list[2]] = 0x1_00;
        w[list[3]] = 0x3_c0;
        self.push_coefficients_all(list);
    }

    /// The amplitude modulation depth word (-value / 127).
    fn amplitude_depth(&self, value: i32) -> u16 {
        // The full depth words avoid the ambiguous 0x200
        if value == 127 {
            return 0x2_01;
        }
        if value == -127 {
            return 0x1_ff;
        }
        ((-((value * 0x2_00) / 0x7f)) & 0x3_ff) as u16
    }

    /// The pitch modulation depth word (value * 128 / 127).
    fn pitch_depth(&self, value: i32) -> u16 {
        ((value * 0x80) / 0x7f) as u16
    }

    /// Designs the low band of the input EQ.
    fn design_low_equalizer(&mut self, low_frequency: i32, low_gain: i32, boosted: bool) {
        let shape = if boosted { XGFilterShape::LowShelfBoosted } else { XGFilterShape::LowShelf };
        self.design_first_order(shape, low_frequency, low_gain, LIST_LOW_LEFT, 0);
        self.design_first_order(shape, low_frequency, low_gain, LIST_LOW_RIGHT, 0);
    }

    /// Designs the high band of the input EQ.
    fn design_high_equalizer(&mut self, high_frequency: i32, high_gain: i32) {
        self.design_first_order(XGFilterShape::HighShelf, high_frequency, high_gain, LIST_HIGH_LEFT, 0);
        self.design_first_order(XGFilterShape::HighShelf, high_frequency, high_gain, LIST_HIGH_RIGHT, 0);
    }
}

/// Equivalent to: XGStereoVariationProgram.reset
pub fn stereo_reset<P: XGStereoVariationProgram + ?Sized>(program: &mut P, memory: &mut [f32]) {
    program.stereo_mut().equalizer_state = [0.0; 6];
    variation_reset(program, memory);
}

/// Implements `XGVariationProgram` + `XGStereoVariationProgram` + `XGProgram` for a category 0
/// program struct with the fields `program`, `var` and `stereo`, and the inherent methods
/// `convert_impl`, `reset_state_impl` and `render_impl`.
#[macro_export]
macro_rules! impl_xg_stereo_variation_program {
    ($ty:ty) => {
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

            fn prepare_input(
                &mut self,
                input_left: &[f32],
                input_right: &[f32],
                sample_count: usize,
                gain: f64,
            ) {
                self.stereo.prepare_input(
                    &self.program,
                    &mut self.var,
                    input_left,
                    input_right,
                    sample_count,
                    gain,
                );
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

        impl $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_stereo_variation_program::XGStereoVariationProgram
            for $ty
        {
            fn stereo_mut(
                &mut self,
            ) -> &mut $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_stereo_variation_program::XGStereoState
            {
                &mut self.stereo
            }
        }

        impl $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_stereo_variation_program::StereoFields
            for $ty
        {
            fn fields(
                &mut self,
            ) -> (
                &mut $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_dsp_program::XGDSPProgram,
                &mut $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_variation_program::XGVariationState,
                &mut $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_stereo_variation_program::XGStereoState,
            ) {
                (&mut self.program, &mut self.var, &mut self.stereo)
            }
        }

        $crate::impl_xg_variation_program!(
            $ty,
            push = $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_stereo_variation_program::stereo_push,
            reset = $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_stereo_variation_program::stereo_reset
        );
    };
}

/// Access to the category 0 fields, for the `stereo_push` helper used by the macro.
pub trait StereoFields {
    fn fields(&mut self) -> (&mut XGDSPProgram, &mut XGVariationState, &mut XGStereoState);
}

/// The pushCoefficients override of a category 0 program.
pub fn stereo_push<P: StereoFields + ?Sized>(p: &mut P, list: &[usize], count: usize, offset: usize) {
    let (program, var, stereo) = p.fields();
    stereo.push_coefficients(program, var, list, count, offset);
}
