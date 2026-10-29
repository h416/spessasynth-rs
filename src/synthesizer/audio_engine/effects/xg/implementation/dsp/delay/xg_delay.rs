//! The S-YXG50 variation delay program, shared by DELAY L,C,R, DELAY L,R, ECHO and CROSS DELAY.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/delay/xg_delay.ts
//!
//! The TS subclasses (one per type, each with its own converters) are the [`XGDelayKind`]
//! of one struct; their `convert` lives in the type modules.

use super::super::xg_dsp_program::{biased_round, fround, to_u16, XGDSPProgram, XGProgram};
use super::super::xg_dsp_tables::FILTER_GAIN;
use super::super::xg_stereo_variation_program::{XGStereoState, CATEGORY};
use super::super::xg_variation_program::{XGVariationPreset, XGVariationState, MEMORY_SIZE};
use super::super::xg_dsp_program::XGProgramLayout;
use super::{cross_delay, delay_lcr, delay_lr, echo};

/// The input position of the left line.
pub const LEFT_BASE: usize = 0x3a;
/// The input position of the right line.
pub const RIGHT_BASE: usize = 0x3b;
/// The right line input position used by the two-line delays (in the upper half of the memory).
pub const RIGHT_LINE_START: u16 = 16_384;

// Damping filter push lists (input, feedback)
pub const LIST_DAMP_A: &[usize] = &[0x9f, 0x9e];
pub const LIST_DAMP_B: &[usize] = &[0xa1, 0xa0];

/// The delay types (the TS subclasses of XGDelayBase).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XGDelayKind {
    DelayLCR,
    DelayLR,
    Echo,
    CrossDelay,
}

/// The delay connection parameters.
pub const CONNECTION_PARAMETERS: &[usize] = &[9];

/// The S-YXG50 variation delay program.
///
/// Two delay lines with two damped feedback taps (A, B) and two output taps (C, D).
/// A coefficient matrix routes the inputs and feedback taps into the lines and the taps into the outputs,
/// each type sets its own routing.
///
/// Equivalent to: abstract class XGDelayBase
pub struct XGDelayBase {
    pub(crate) program: XGDSPProgram,
    pub(crate) var: XGVariationState,
    pub(crate) stereo: XGStereoState,
    kind: XGDelayKind,
    // Damping filter states
    damp_a: f64,
    damp_b: f64,
}

impl XGDelayBase {
    pub fn new(
        effect_type: u16,
        kind: XGDelayKind,
        preset: &'static XGVariationPreset,
        layout: XGProgramLayout,
        sample_rate: f64,
        max_buffer_size: usize,
    ) -> Self {
        Self {
            program: XGDSPProgram::new(
                effect_type,
                MEMORY_SIZE,
                sample_rate,
                max_buffer_size,
                CATEGORY,
                preset,
                layout,
            ),
            var: XGVariationState::new(max_buffer_size, &[]),
            stereo: XGStereoState::default(),
            kind,
            damp_a: 0.0,
            damp_b: 0.0,
        }
    }

    fn convert_impl(&mut self, param: usize) {
        match self.kind {
            XGDelayKind::DelayLCR => delay_lcr::convert(self, param),
            XGDelayKind::DelayLR => delay_lr::convert(self, param),
            XGDelayKind::Echo => echo::convert(self, param),
            XGDelayKind::CrossDelay => cross_delay::convert(self, param),
        }
    }

    fn reset_state_impl(&mut self) {
        self.damp_a = 0.0;
        self.damp_b = 0.0;
    }

    fn render_impl(
        &mut self,
        memory: &mut [f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
        output_gain: f64,
    ) {
        let mask = self.program.mask;
        let c = |i: usize| self.program.coefficients[i] as f64;
        let t = &self.program.taps;
        let (t_a, t_b, t_c, t_d, t_write_a, t_write_b) =
            (t[0x28], t[0x2d], t[0x33], t[0x38], t[0x3a], t[0x3b]);
        let (damp_in_a, damp_fb_a, damp_in_b, damp_fb_b) = (c(0x9f), c(0x9e), c(0xa1), c(0xa0));
        // Line inputs: right input, damped A, damped B (the left input enters line A directly)
        let (right_to_a, a_to_a, b_to_a) = (c(0xaa), c(0xab), c(0xac));
        let (right_to_b, a_to_b, b_to_b) = (c(0xad), c(0xae), c(0xaf));
        // Outputs: left input, right input, A or B, C, D
        let left_to_l = c(0xb2) * output_gain;
        let right_to_l = c(0xb3) * output_gain;
        let a_to_l = c(0xb4) * output_gain;
        let c_to_l = c(0xb5) * output_gain;
        let d_to_l = c(0xb6) * output_gain;
        let left_to_r = c(0xb7) * output_gain;
        let right_to_r = c(0xb8) * output_gain;
        let b_to_r = c(0xb9) * output_gain;
        let c_to_r = c(0xba) * output_gain;
        let d_to_r = c(0xbb) * output_gain;
        let in_l = &self.var.input_left;
        let in_r = &self.var.input_right;
        let mut pointer = self.program.pointer;
        let (mut damp_a, mut damp_b) = (self.damp_a, self.damp_b);
        macro_rules! at {
            ($tap:expr) => {
                ((($tap) + pointer) & mask) as usize
            };
        }
        for i in 0..sample_count {
            let a = memory[at!(t_a)] as f64;
            let b = memory[at!(t_b)] as f64;
            let tap_c = memory[at!(t_c)] as f64;
            let tap_d = memory[at!(t_d)] as f64;
            let damped_a = a * damp_in_a + damp_fb_a * damp_a;
            damp_a = a;
            let damped_b = b * damp_in_b + damp_fb_b * damp_b;
            damp_b = b;
            let l = in_l[i] as f64;
            let r = in_r[i] as f64;
            // The right line is written first, so the left line wins if both inputs share an address.
            // Below 44.1 kHz the input addresses 1 and 0 both scale to 0, which would silence the left line.
            // At 44.1 kHz the left line overwrites the same sample one step later, so this matches it.
            memory[at!(t_write_b)] = (right_to_b * r + damped_a * a_to_b + damped_b * b_to_b) as f32;
            memory[at!(t_write_a)] = (right_to_a * r + damped_a * a_to_a + damped_b * b_to_a + l) as f32;
            output_left[i] =
                (right_to_l * r + a * a_to_l + tap_c * c_to_l + tap_d * d_to_l + left_to_l * l) as f32;
            output_right[i] =
                (left_to_r * l + b * b_to_r + tap_c * c_to_r + tap_d * d_to_r + right_to_r * r) as f32;
            pointer = (pointer - 1) & mask;
        }
        self.program.pointer = pointer;
        self.damp_a = damp_a;
        self.damp_b = damp_b;
    }

    /// Places a delay tap relative to a line input.
    /// `time` is in 0.1 ms units, `base` is the line input address index.
    pub(crate) fn place_tap(&mut self, tap: usize, time: i32, base: usize) {
        let p = &mut self.program;
        p.address_words[tap] = to_u16(p.to_address100us(time) + p.signed_address(base));
        p.push_addresses_all(&[tap]);
    }

    /// Designs a feedback damping filter (a one-pole low-pass) from the high damp parameter.
    /// `damp` is the high damp value (1-10), `list` the input and feedback coefficient indices.
    pub(crate) fn design_damping(&mut self, damp: i32, list: &[usize]) {
        let gain = FILTER_GAIN[0x40] as f64;
        let k = fround((damp as f64 + 8.0) / 18.0);
        let input = fround(k * gain);
        let feedback = fround(fround(1.0 - k) * gain);
        let round = |v: f64| ((biased_round(v * 512.0).trunc() as i32) & 0x3_ff) as u16;
        self.program.coefficient_words[list[0]] = round(input);
        self.program.coefficient_words[list[1]] = round(feedback);
        self.push_coefficients_all(list);
    }
}

crate::impl_xg_stereo_variation_program!(XGDelayBase);
