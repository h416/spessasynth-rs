//! 3-BAND EQ (`4C 00`): a mono low shelf, mid peaking EQ and high shelf.
//! Each shelf is split into two cascaded sections of half the gain.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/equalizer/three_band_eq.ts

use super::super::xg_dsp_program::{
    XGBiquadShape, XGDSPProgram, XGFilterShape, XGParameterRanges, XGProgram, XGProgramPreset,
};
use super::super::xg_mono_variation_program::{
    mix_output, mono_input_level, XGMonoState, CATEGORY, LIST_LOW_A, LIST_LOW_B,
    UNWRITTEN_CATEGORY_2,
};
use super::super::xg_variation_program::{variation_layout, XGVariationState};

const PARAMETER_RANGES: XGParameterRanges =
    &[(0, 52, 76), (1, 28, 54), (2, 52, 76), (3, 10, 120), (4, 52, 76), (5, 8, 40), (6, 28, 58)];

// High shelf push lists (b0, b1, a1) and the mid peaking push list (b0, b1, b2, a1, a2)
const LIST_HIGH_A: &[usize] = &[0xa3, 0xa2, 0xa4];
const LIST_HIGH_B: &[usize] = &[0xab, 0xaa, 0xac];
const LIST_MID: &[usize] = &[0xa9, 0xa8, 0xa7, 0xa6, 0xa5];

/// The converter index of the mono input level, which depends on the connection.
const MONO_INPUT: usize = 16;

pub const TYPE: u16 = 0x4c_00;
/// Equivalent to: static memorySize = 0
const MEMORY_SIZE: usize = 0;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        256, 256, 0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 0, 128, 0, 0, 128, 0, 0, 128, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 256, 0, 0,
    ],
    defaults: [70, 34, 60, 10, 70, 28, 46, 0, 0, 127, 0, 0, 0, 0, 0, 0],
};

const INIT_ORDER: &[usize] = &[0, 1, 4, MONO_INPUT];
const CONNECTION_PARAMETERS: &[usize] = &[MONO_INPUT];

/// Equivalent to: class XGThreeBandEQ
pub struct XGThreeBandEQ {
    pub(crate) program: XGDSPProgram,
    pub(crate) var: XGVariationState,
    pub(crate) mono: XGMonoState,
}

impl XGThreeBandEQ {
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
            var: XGVariationState::new(max_buffer_size, UNWRITTEN_CATEGORY_2),
            mono: XGMonoState::new(max_buffer_size),
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
        self.mono.mix_mono(&self.program, &self.var, sample_count);
        let m = &mut self.mono;
        m.filters.filter_output(&self.program, &mut m.mono, sample_count);
        mix_output(
            &self.program,
            &m.mono,
            &m.dry_left,
            &m.dry_right,
            output_left,
            output_right,
            sample_count,
            output_gain,
        );
    }

    fn reset_state_impl(&mut self) {
        self.mono.reset_state();
    }

    fn convert_impl(&mut self, param: usize) {
        let p = |d: &Self, i: usize| d.program.param(i);
        match param {
            0 | 5 => {
                // The gain is split between the two sections
                let half = (p(self, 0) - 0x40) / 2;
                self.design_first_order(XGFilterShape::LowShelf, p(self, 5), half + 0x40, LIST_LOW_A, 0);
                self.design_first_order(XGFilterShape::LowShelf, p(self, 5), p(self, 0) - half, LIST_LOW_B, 0);
            }
            1..=3 => {
                let (f, g, q) = (p(self, 1), p(self, 2), p(self, 3));
                self.design_second_order(XGBiquadShape::Peaking, f, g, q, LIST_MID);
            }
            4 | 6 => {
                let half = (p(self, 4) - 0x40) / 2;
                self.design_first_order(XGFilterShape::HighShelf, p(self, 6), half + 0x40, LIST_HIGH_A, 0);
                self.design_first_order(XGFilterShape::HighShelf, p(self, 6), p(self, 4) - half, LIST_HIGH_B, 0);
            }
            MONO_INPUT => mono_input_level(self),
            _ => {}
        }
    }
}

crate::impl_xg_mono_variation_program!(XGThreeBandEQ);
