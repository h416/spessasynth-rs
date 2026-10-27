//! The base for the variation-only XG programs.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/xg_variation_program.ts
//!
//! The TS `abstract class XGVariationProgram` is split into [`XGVariationState`] (its fields),
//! the [`XGVariationProgram`] trait (`prepareInput` / `render`) and the free functions
//! [`variation_process`], [`variation_reset`] and `XGVariationState::push_coefficients`.

use super::xg_dsp_program::{
    XGDSPProgram, XGParameterRanges, XGProgram, XGProgramLayout, XGProgramPreset, RETURN_GAIN,
    STEREO_SEND_LEVEL, VARIATION_INPUT_LEVEL,
};

/// The type-specific data of a variation-only XG effect.
/// The words are the 79 coefficient words followed by the 27 address words of the program.
pub type XGVariationPreset = XGProgramPreset;

/// The variation delay memory size at 44.1 kHz.
pub const MEMORY_SIZE: usize = 131_072;

// Program layout: which coefficient and address indices the preset words fill
const COEFFICIENT_LIST: [usize; 79] = {
    let mut a = [0usize; 79];
    let mut i = 0;
    while i < 79 {
        a[i] = 0x6d + i;
        i += 1;
    }
    a
};
const ADDRESS_LIST: [usize; 27] = {
    let mut a = [0usize; 27];
    let mut i = 0;
    while i < 27 {
        a[i] = 0x24 + i;
        i += 1;
    }
    a
};

/// The largest number of coefficients a category never writes.
const MAX_UNWRITTEN: usize = 32;

/// The original feeds an inserted part to the variation at 24/11 of its dry level
/// and mixes the result back at 235/512, which matters for the nonlinear programs.
const INSERTION_INPUT_LEVEL: f64 = 24.0 / 11.0;
const INSERTION_OUTPUT_GAIN: f64 = 235.0 / 512.0;

/// Builds the layout of a variation program from its own converter lists.
pub const fn variation_layout(
    init_order: &'static [usize],
    connection_parameters: &'static [usize],
    ranges: XGParameterRanges,
) -> XGProgramLayout {
    XGProgramLayout {
        coefficient_list: &COEFFICIENT_LIST,
        address_list: &ADDRESS_LIST,
        init_order,
        connection_parameters,
        ranges,
    }
}

/// The fields of XGVariationProgram.
pub struct XGVariationState {
    /// The LFO phase.
    pub lfo_phase: i32,
    /// The input of the current block, after the category's input stage.
    pub input_left: Vec<f32>,
    pub input_right: Vec<f32>,
    /// The coefficients the category's rules never write, the DSP keeps their previous values.
    unwritten_coefficients: &'static [usize],
    kept_coefficients: [f32; MAX_UNWRITTEN],
}

impl XGVariationState {
    pub fn new(max_buffer_size: usize, unwritten_coefficients: &'static [usize]) -> Self {
        Self {
            lfo_phase: 0,
            input_left: vec![0.0; max_buffer_size],
            input_right: vec![0.0; max_buffer_size],
            unwritten_coefficients,
            kept_coefficients: [0.0; MAX_UNWRITTEN],
        }
    }

    /// Equivalent to: XGVariationProgram.pushCoefficients
    pub fn push_coefficients(
        &mut self,
        program: &mut XGDSPProgram,
        list: &[usize],
        count: usize,
        offset: usize,
    ) {
        let c = &mut program.coefficients;
        for (i, &k) in self.unwritten_coefficients.iter().enumerate() {
            self.kept_coefficients[i] = c[k];
        }
        program.push_coefficients_base(list, count, offset);
        let c = &mut program.coefficients;
        for (i, &k) in self.unwritten_coefficients.iter().enumerate() {
            c[k] = self.kept_coefficients[i];
        }
    }

    /// Fills `input_left` and `input_right` from the block input.
    /// Without a shared input stage, the input is only scaled.
    /// Equivalent to: XGVariationProgram.prepareInput
    pub fn prepare_input_plain(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        sample_count: usize,
        gain: f64,
    ) {
        for i in 0..sample_count {
            self.input_left[i] = (input_left[i] as f64 * gain) as f32;
            self.input_right[i] = (input_right[i] as f64 * gain) as f32;
        }
    }
}

/// The overridable part of XGVariationProgram.
pub trait XGVariationProgram: XGProgram {
    fn variation(&self) -> &XGVariationState;
    fn variation_mut(&mut self) -> &mut XGVariationState;

    /// Fills the variation input buffers from the block input (the category's input stage).
    fn prepare_input(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        sample_count: usize,
        gain: f64,
    ) {
        self.variation_mut().prepare_input_plain(input_left, input_right, sample_count, gain);
    }

    /// Runs the routine on the prepared input (`input_left`, `input_right`).
    /// `output_gain` is the output scale for the current connection.
    fn render(
        &mut self,
        memory: &mut [f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
        output_gain: f64,
    );
}

/// Equivalent to: XGVariationProgram.reset
pub fn variation_reset<P: XGVariationProgram + ?Sized>(program: &mut P, memory: &mut [f32]) {
    program.variation_mut().lfo_phase = 0;
    program.reset_program(memory);
}

/// Equivalent to: XGVariationProgram.process
#[allow(clippy::too_many_arguments)]
pub fn variation_process<P: XGVariationProgram + ?Sized>(
    program: &mut P,
    memory: &mut [f32],
    input_left: &[f32],
    input_right: &[f32],
    output_left: &mut [f32],
    output_right: &mut [f32],
    sample_count: usize,
    is_insertion: bool,
) {
    program.update_connection(is_insertion);
    // System sends are halved like the original
    let gain = program.program().input_level
        * (if is_insertion { INSERTION_INPUT_LEVEL } else { STEREO_SEND_LEVEL });
    program.prepare_input(input_left, input_right, sample_count, gain);
    let length = program.program().memory_length;
    program.render(
        &mut memory[..length],
        output_left,
        output_right,
        sample_count,
        if is_insertion { INSERTION_OUTPUT_GAIN } else { RETURN_GAIN },
    );
    program
        .program_mut()
        .advance_fade(sample_count, if is_insertion { 1.0 } else { VARIATION_INPUT_LEVEL });
}

/// Implements `XGProgram` for a variation program struct with the fields
/// `program: XGDSPProgram` and `var: XGVariationState`.
/// The category supplies `push_coefficients` and `reset_xg`; the struct supplies the
/// inherent methods `convert_impl` and `reset_state_impl`.
#[macro_export]
macro_rules! impl_xg_variation_program {
    ($ty:ty, push = $push:path, reset = $reset:path) => {
        impl $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_dsp_program::XGProgram
            for $ty
        {
            fn program(
                &self,
            ) -> &$crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_dsp_program::XGDSPProgram
            {
                &self.program
            }

            fn program_mut(
                &mut self,
            ) -> &mut $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_dsp_program::XGDSPProgram
            {
                &mut self.program
            }

            fn convert(&mut self, param: usize) {
                self.convert_impl(param);
            }

            fn reset_state(&mut self) {
                self.reset_state_impl();
            }

            fn push_coefficients(&mut self, list: &[usize], count: usize, offset: usize) {
                $push(self, list, count, offset);
            }

            fn reset_xg(&mut self, memory: &mut [f32]) {
                $reset(self, memory);
            }

            fn process(
                &mut self,
                memory: &mut [f32],
                input_left: &[f32],
                input_right: &[f32],
                output_left: &mut [f32],
                output_right: &mut [f32],
                sample_count: usize,
                is_insertion: bool,
            ) {
                $crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_variation_program::variation_process(
                    self,
                    memory,
                    input_left,
                    input_right,
                    output_left,
                    output_right,
                    sample_count,
                    is_insertion,
                );
            }
        }
    };
}
