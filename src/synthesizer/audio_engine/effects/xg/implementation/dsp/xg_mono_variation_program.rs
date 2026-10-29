//! The base for the S-YXG50 category 2 variation programs (3-band EQ, distortion, early reflections).
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/xg_mono_variation_program.ts
//!
//! They process a mono sum of the input through a two-section low shelf, keeping scaled copies
//! of the dry input that the routines mix back in.
//! Any of 0x76, 0x7a, 0x7d and 0x80 sets 0x76 to their product.

use super::xg_dsp_program::{fround, XGDSPProgram};
use super::xg_variation_program::XGVariationState;

// Input low shelf push lists (b0, b1, a1), the shelf is split into two sections
pub const LIST_LOW_A: &[usize] = &[0x70, 0x6f, 0x71];
pub const LIST_LOW_B: &[usize] = &[0x73, 0x72, 0x74];

/// The category 2 coefficients that the original never writes.
pub const UNWRITTEN_CATEGORY_2: &[usize] = &[0x75, 0x7a, 0x7d, 0x80, 0xae];

/// The category 2 index.
pub const CATEGORY: usize = 2;

/// The filter states of XGMonoVariationProgram.
#[derive(Default)]
pub struct XGMonoFilterState {
    // Low shelf states
    mono_x: f64,
    mono_y: f64,
    mono_shelf_y: f64,
    // Output filter states (shelf, peaking, shelf)
    post_x: f64,
    post_y: f64,
    peak_x2: f64,
    peak_y: f64,
    peak_y2: f64,
    post_shelf_y: f64,
}

/// The fields of XGMonoVariationProgram.
pub struct XGMonoState {
    /// The mono input after the low shelf.
    pub mono: Vec<f32>,
    /// The scaled dry input copies (coefficients 0xad and 0xb1).
    pub dry_left: Vec<f32>,
    pub dry_right: Vec<f32>,
    pub filters: XGMonoFilterState,
}

impl XGMonoState {
    pub fn new(max_buffer_size: usize) -> Self {
        Self {
            mono: vec![0.0; max_buffer_size],
            dry_left: vec![0.0; max_buffer_size],
            dry_right: vec![0.0; max_buffer_size],
            filters: XGMonoFilterState::default(),
        }
    }

    /// Equivalent to: XGMonoVariationProgram.resetState
    pub fn reset_state(&mut self) {
        self.filters = XGMonoFilterState::default();
    }

    /// Fills `mono`, `dry_left` and `dry_right` from the input.
    /// Equivalent to: XGMonoVariationProgram.mixMono
    pub fn mix_mono(&mut self, program: &XGDSPProgram, var: &XGVariationState, sample_count: usize) {
        let c = |i: usize| program.coefficients[i] as f64;
        let in_l = &var.input_left;
        let in_r = &var.input_right;
        let (dry_left_gain, dry_right_gain, left_gain, right_gain) = (c(0xad), c(0xb1), c(0x6d), c(0x6e));
        let (b1_a, b0_a, a1_a) = (c(0x6f), c(0x70), c(0x71));
        let (b1_b, b0_b, a1_b) = (c(0x72), c(0x73), c(0x74));
        let f = &mut self.filters;
        let (mut mono_x, mut mono_y, mut mono_shelf_y) = (f.mono_x, f.mono_y, f.mono_shelf_y);
        for i in 0..sample_count {
            let l = in_l[i] as f64;
            let r = in_r[i] as f64;
            self.dry_left[i] = (dry_left_gain * l) as f32;
            self.dry_right[i] = (dry_right_gain * r) as f32;
            let x = right_gain * r + left_gain * l;
            let y = b1_a * mono_x + b0_a * x + a1_a * mono_y;
            mono_shelf_y = y * b0_b + b1_b * mono_y + a1_b * mono_shelf_y;
            mono_x = x;
            mono_y = y;
            self.mono[i] = mono_shelf_y as f32;
        }
        f.mono_x = mono_x;
        f.mono_y = mono_y;
        f.mono_shelf_y = mono_shelf_y;
    }
}

impl XGMonoFilterState {
    /// Runs the output filters on the mono signal in place: a first-order section (0xa2-0xa4),
    /// a second-order section (0xa5-0xa9) and another first-order section (0xaa-0xac).
    /// Equivalent to: XGMonoVariationProgram.filterOutput
    pub fn filter_output(&mut self, program: &XGDSPProgram, buffer: &mut [f32], sample_count: usize) {
        let c = |i: usize| program.coefficients[i] as f64;
        let (hb1, hb0, ha1) = (c(0xa2), c(0xa3), c(0xa4));
        let (ma2, ma1, mb2, mb1, mb0) = (c(0xa5), c(0xa6), c(0xa7), c(0xa8), c(0xa9));
        let (sb1, sb0, sa1) = (c(0xaa), c(0xab), c(0xac));
        let (mut post_x, mut post_y, mut peak_x2) = (self.post_x, self.post_y, self.peak_x2);
        let (mut peak_y, mut peak_y2, mut post_shelf_y) = (self.peak_y, self.peak_y2, self.post_shelf_y);
        for v in buffer.iter_mut().take(sample_count) {
            let x = *v as f64;
            let first = ha1 * post_y + hb1 * post_x + hb0 * x;
            post_x = x;
            let peak = ma2 * peak_y2 + ma1 * peak_y + mb2 * peak_x2 + mb1 * post_y + mb0 * first;
            peak_x2 = post_y;
            post_y = first;
            post_shelf_y = sb1 * peak_y + peak * sb0 + sa1 * post_shelf_y;
            peak_y2 = peak_y;
            peak_y = peak;
            *v = post_shelf_y as f32;
        }
        self.post_x = post_x;
        self.post_y = post_y;
        self.peak_x2 = peak_x2;
        self.peak_y = peak_y;
        self.peak_y2 = peak_y2;
        self.post_shelf_y = post_shelf_y;
    }
}

/// Writes the outputs: the processed mono signal (0xb0, 0xb3) plus the dry copies.
/// Equivalent to: XGMonoVariationProgram.mixOutput
#[allow(clippy::too_many_arguments)]
pub fn mix_output(
    program: &XGDSPProgram,
    buffer: &[f32],
    dry_l: &[f32],
    dry_r: &[f32],
    output_left: &mut [f32],
    output_right: &mut [f32],
    sample_count: usize,
    output_gain: f64,
) {
    let out_l = program.coefficients[0xb0] as f64;
    let out_r = program.coefficients[0xb3] as f64;
    for i in 0..sample_count {
        let y = buffer[i] as f64;
        output_left[i] = ((out_l * y + dry_l[i] as f64) * output_gain) as f32;
        output_right[i] = ((out_r * y + dry_r[i] as f64) * output_gain) as f32;
    }
}

/// Writes the mono input level words: L+R in the system mode, their average in the insertion mode.
/// Equivalent to: XGMonoVariationProgram.monoInputLevel
pub fn mono_input_level<P: super::xg_dsp_program::XGProgram + ?Sized>(p: &mut P) {
    let word = if p.program().insertion { 0x1_00 } else { 0x2_00 };
    let w = &mut p.program_mut().coefficient_words;
    w[0x6d] = word;
    w[0x6e] = word;
    p.push_coefficients_all(&[0x6d, 0x6e]);
}

/// Equivalent to: XGMonoVariationProgram.pushCoefficients
pub fn mono_push_coefficients(
    program: &mut XGDSPProgram,
    var: &mut XGVariationState,
    list: &[usize],
    count: usize,
    offset: usize,
) {
    var.push_coefficients(program, list, count, offset);
    if !program.push_enabled {
        return;
    }
    for &k in &list[offset..offset + count] {
        if k == 0x76 || k == 0x7a || k == 0x7d || k == 0x80 {
            program.coefficients[0x76] = fround(
                fround(fround(program.raw_coefficient(0x80) * program.raw_coefficient(0x7d)) * program.raw_coefficient(0x7a))
                    * program.raw_coefficient(0x76),
            ) as f32;
            return;
        }
    }
}

/// Implements `XGVariationProgram` + `XGProgram` for a category 2 program struct with the fields
/// `program`, `var` and `mono`, and the inherent methods `convert_impl`, `reset_state_impl`
/// and `render_impl`.
#[macro_export]
macro_rules! impl_xg_mono_variation_program {
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

        impl $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_mono_variation_program::MonoFields
            for $ty
        {
            fn mono_fields(
                &mut self,
            ) -> (
                &mut $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_dsp_program::XGDSPProgram,
                &mut $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_variation_program::XGVariationState,
            ) {
                (&mut self.program, &mut self.var)
            }
        }

        $crate::impl_xg_variation_program!(
            $ty,
            push = $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_mono_variation_program::mono_push,
            reset = $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_variation_program::variation_reset
        );
    };
}

/// Access to the category 2 fields, for the `mono_push` helper used by the macro.
pub trait MonoFields {
    fn mono_fields(&mut self) -> (&mut XGDSPProgram, &mut XGVariationState);
}

/// The pushCoefficients override of a category 2 program.
pub fn mono_push<P: MonoFields + ?Sized>(p: &mut P, list: &[usize], count: usize, offset: usize) {
    let (program, var) = p.mono_fields();
    mono_push_coefficients(program, var, list, count, offset);
}
