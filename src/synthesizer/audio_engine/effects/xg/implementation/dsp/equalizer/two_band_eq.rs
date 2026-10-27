//! 2-BAND EQ (`4D 00`): a stereo low and high shelf.
//! The original routine is only the variation input EQ, so the output is the EQ'd input.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/equalizer/two_band_eq.ts

use super::super::xg_dsp_program::{XGDSPProgram, XGParameterRanges, XGProgramPreset};
use super::super::xg_stereo_variation_program::{XGStereoState, XGStereoVariationProgram, CATEGORY};
use super::super::xg_variation_program::{variation_layout, XGVariationState};

const PARAMETER_RANGES: XGParameterRanges = &[(0, 8, 40), (1, 52, 76), (2, 28, 58), (3, 52, 76)];

pub const TYPE: u16 = 0x4d_00;
/// Equivalent to: static memorySize = 0
const MEMORY_SIZE: usize = 0;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        128, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 256,
    ],
    defaults: [28, 70, 46, 70, 0, 0, 0, 0, 0, 127, 34, 64, 10, 0, 0, 0],
};

const INIT_ORDER: &[usize] = &[0, 2];
const CONNECTION_PARAMETERS: &[usize] = &[];

/// Equivalent to: class XGTwoBandEQ
pub struct XGTwoBandEQ {
    pub(crate) program: XGDSPProgram,
    pub(crate) var: XGVariationState,
    pub(crate) stereo: XGStereoState,
}

impl XGTwoBandEQ {
    pub fn new(sample_rate: f64, max_buffer_size: usize) -> Self {
        Self {
            program: XGDSPProgram::new(
                TYPE,
                MEMORY_SIZE,
                sample_rate,
                max_buffer_size,
                CATEGORY,
                &PRESET,
                variation_layout(INIT_ORDER, CONNECTION_PARAMETERS, PARAMETER_RANGES),
            ),
            var: XGVariationState::new(max_buffer_size, &[]),
            stereo: XGStereoState::default(),
        }
    }

    fn render_impl(
        &mut self,
        _memory: &mut [f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
        output_gain: f64,
    ) {
        let in_l = &self.var.input_left;
        let in_r = &self.var.input_right;
        for i in 0..sample_count {
            output_left[i] = (in_l[i] as f64 * output_gain) as f32;
            output_right[i] = (in_r[i] as f64 * output_gain) as f32;
        }
    }

    fn reset_state_impl(&mut self) {}

    fn convert_impl(&mut self, param: usize) {
        let p = |d: &Self, i: usize| d.program.param(i);
        match param {
            0 | 1 => self.design_low_equalizer(p(self, 0), p(self, 1), false),
            2 | 3 => self.design_high_equalizer(p(self, 2), p(self, 3)),
            _ => {}
        }
    }
}

crate::impl_xg_stereo_variation_program!(XGTwoBandEQ);
