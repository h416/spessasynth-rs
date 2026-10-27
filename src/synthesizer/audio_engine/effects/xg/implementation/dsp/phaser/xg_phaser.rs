//! The S-YXG50 phaser program, shared by PHASER 1 and PHASER 2.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/phaser/xg_phaser.ts
//!
//! The TS subclasses are the [`XGPhaserKind`] of one struct; their `convert` lives in the type modules.

use super::super::xg_dsp_program::{
    chop32, fround, to_u16, triangle, XGDSPProgram, XGFilterShape, XGProgram, XGProgramLayout,
    PHASE_MASK,
};
use super::super::xg_dsp_tables::LFO_RATE;
use super::super::xg_variation_program::{XGVariationPreset, XGVariationProgram, XGVariationState};
use super::{phaser1, phaser2};

// Input EQ push lists (b0, b1, a1): low and high shelves of the left and right sides
const LIST_LOW_LEFT: &[usize] = &[0x6e, 0x6d, 0x6f];
const LIST_HIGH_LEFT: &[usize] = &[0x71, 0x70, 0x72];
const LIST_LOW_RIGHT: &[usize] = &[0x74, 0x73, 0x75];
const LIST_HIGH_RIGHT: &[usize] = &[0x77, 0x76, 0x78];

/// The category 1 coefficients that the original never writes (0x79 and 0x9c become integers).
const UNWRITTEN_CATEGORY_1: &[usize] = &[
    0x79, 0x7a, 0x81, 0x88, 0x8b, 0x8c, 0x8f, 0x90, 0x93, 0x94, 0x97, 0x98, 0x9b, 0x9c, 0xa8, 0xab,
    0xac, 0xaf, 0xb0, 0xb3, 0xb4, 0xb7, 0xb8, 0xbb,
];

/// The category 1 index.
const CATEGORY: usize = 1;
/// Equivalent to: static memorySize = 0
const MEMORY_SIZE: usize = 0;

/// The phaser types (the TS subclasses of XGPhaserBase).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XGPhaserKind {
    Phaser1,
    Phaser2,
}

/// The S-YXG50 phaser program.
///
/// Two chains of five first-order allpasses, one per side, are swept by an exponential triangle LFO
/// (the second side with a phase offset). Selected stages of both chains feed back and form the outputs.
///
/// The phaser is the only S-YXG50 category 1 program: 0x79 and 0x9c become the LFO step and phase offset
/// integers, 0x82 and 0x9d are scaled by 8, 0x83 and 0x9e by 2^-19, the slots the phaser does not use are never written.
///
/// Equivalent to: abstract class XGPhaserBase
pub struct XGPhaserBase {
    pub(crate) program: XGDSPProgram,
    pub(crate) var: XGVariationState,
    kind: XGPhaserKind,
    /// The LFO step and phase offset (coefficients 0x79 and 0x9c).
    sweep_step: i32,
    sweep_offset: i32,
    // Input EQ states
    eq_xl: f64,
    eq_yl: f64,
    eq_zl: f64,
    eq_xr: f64,
    eq_yr: f64,
    eq_zr: f64,
    // Allpass chain states: the previous input and the five previous stage outputs of each chain
    chain_a: [f32; 6],
    chain_b: [f32; 6],
}

impl XGPhaserBase {
    pub fn new(
        effect_type: u16,
        kind: XGPhaserKind,
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
            var: XGVariationState::new(max_buffer_size, UNWRITTEN_CATEGORY_1),
            kind,
            sweep_step: 0,
            sweep_offset: 0,
            eq_xl: 0.0,
            eq_yl: 0.0,
            eq_zl: 0.0,
            eq_xr: 0.0,
            eq_yr: 0.0,
            eq_zr: 0.0,
            chain_a: [0.0; 6],
            chain_b: [0.0; 6],
        }
    }

    fn convert_impl(&mut self, param: usize) {
        match self.kind {
            XGPhaserKind::Phaser1 => phaser1::convert(self, param),
            XGPhaserKind::Phaser2 => phaser2::convert(self, param),
        }
    }

    fn reset_state_impl(&mut self) {
        self.eq_xl = 0.0;
        self.eq_yl = 0.0;
        self.eq_zl = 0.0;
        self.eq_xr = 0.0;
        self.eq_yr = 0.0;
        self.eq_zr = 0.0;
        self.chain_a.fill(0.0);
        self.chain_b.fill(0.0);
    }

    fn render_impl(
        &mut self,
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
        output_gain: f64,
    ) {
        let c = |i: usize| self.program.coefficients[i] as f64;
        let in_l = &self.var.input_left;
        let in_r = &self.var.input_right;
        let (lb0_l, lb1_l, la1_l) = (c(0x6e), c(0x6d), c(0x6f));
        let (hb0_l, hb1_l, ha1_l) = (c(0x71), c(0x70), c(0x72));
        let (lb0_r, lb1_r, la1_r) = (c(0x74), c(0x73), c(0x75));
        let (hb0_r, hb1_r, ha1_r) = (c(0x77), c(0x76), c(0x78));
        let dry_left_gain = c(0xa2) * output_gain;
        let dry_right_gain = c(0xa6) * output_gain;
        let (left_gain, right_to_left, right_gain) = (c(0x84), c(0x85), c(0x9f));
        let (sel_a3, sel_a4, sel_a5) = (c(0x7b), c(0x7c), c(0x7d));
        let (sel_b3, sel_b4, sel_b5) = (c(0x7e), c(0x7f), c(0x80));
        let (out_a, out_b2, out_b4, out_right) = (c(0xa3), c(0xa4), c(0xa5), c(0xa7));
        let (feed_aa, feed_ba, feed_ab, feed_bb) = (c(0x86), c(0x87), c(0xa0), c(0xa1));
        let (bias_a, depth_a, bias_b, depth_b) = (c(0x82), c(0x83), c(0x9d), c(0x9e));
        let step = self.sweep_step;
        let offset = self.sweep_offset;
        let a = &mut self.chain_a;
        let b = &mut self.chain_b;
        let (mut eq_xl, mut eq_yl, mut eq_zl) = (self.eq_xl, self.eq_yl, self.eq_zl);
        let (mut eq_xr, mut eq_yr, mut eq_zr) = (self.eq_xr, self.eq_yr, self.eq_zr);
        let mut lfo_phase = self.var.lfo_phase;
        // Exponential LFO: 1 - 2^v / 256 with the integer part as a shift
        let coefficient = |v: f64| {
            let n = v.trunc() as i32;
            chop32(1.0 - chop32(chop32((1i32 << (n & 0x1f)) as f64 * chop32(v - n as f64 + 1.0)) / 256.0))
        };
        for i in 0..sample_count {
            // Input EQ, a low shelf then a high shelf on each side
            let mut x = in_l[i] as f64;
            let mut y = lb0_l * x + la1_l * eq_yl + lb1_l * eq_xl;
            eq_zl = hb1_l * eq_yl + ha1_l * eq_zl + y * hb0_l;
            eq_xl = x;
            eq_yl = y;
            x = in_r[i] as f64;
            y = la1_r * eq_yr + lb0_r * x + lb1_r * eq_xr;
            eq_zr = hb1_r * eq_yr + ha1_r * eq_zr + y * hb0_r;
            eq_xr = x;
            eq_yr = y;
            let left = eq_zl;
            let right = eq_zr;
            let wet_in_l = left_gain * left + right_to_left * right;
            let wet_in_r = right_gain * right;

            // Stage mixes and the outputs, from the previous states
            let mix_a = sel_a4 * a[4] as f64 + sel_a3 * a[3] as f64 + sel_a5 * a[5] as f64;
            let mix_b = sel_b5 * b[5] as f64 + sel_b4 * b[4] as f64 + sel_b3 * b[3] as f64;
            let wet_l = mix_a * out_a + out_b4 * b[4] as f64 + out_b2 * b[2] as f64;
            let wet_r = mix_b * out_right;

            lfo_phase = (lfo_phase + step) & PHASE_MASK;
            let v_a = chop32(chop32(triangle(lfo_phase) as f64 * depth_a) + bias_a);
            let v_b = chop32(chop32(triangle((lfo_phase - offset) & PHASE_MASK) as f64 * depth_b) + bias_b);
            let coefficient_a = coefficient(v_a);
            let coefficient_b = coefficient(v_b);

            // Allpass chains
            let mut input = mix_b * feed_ba + mix_a * feed_aa + wet_in_l;
            for k in 1..6 {
                let out = coefficient_a * a[k] as f64 + a[k - 1] as f64 - input * coefficient_a;
                a[k - 1] = input as f32;
                input = out;
            }
            a[5] = input as f32;
            input = mix_a * feed_ab + mix_b * feed_bb + wet_in_r;
            for k in 1..6 {
                let out = coefficient_b * b[k] as f64 + b[k - 1] as f64 - input * coefficient_b;
                b[k - 1] = input as f32;
                input = out;
            }
            b[5] = input as f32;

            output_left[i] = (wet_l * output_gain + dry_left_gain * left) as f32;
            output_right[i] = (wet_r * output_gain + dry_right_gain * right) as f32;
        }
        self.eq_xl = eq_xl;
        self.eq_yl = eq_yl;
        self.eq_zl = eq_zl;
        self.eq_xr = eq_xr;
        self.eq_yr = eq_yr;
        self.eq_zr = eq_zr;
        self.var.lfo_phase = lfo_phase;
    }

    /// Writes the LFO rate word.
    pub(crate) fn sweep_rate(&mut self) {
        let p0 = self.program.param(0) as usize;
        self.program.coefficient_words[0x79] = LFO_RATE[p0];
        self.push_coefficients_all(&[0x79]);
    }

    /// Writes the LFO bias and depth words of both sides.
    /// `bias_scale_b` is the full bias of the second side (512 is the same as the first side).
    pub(crate) fn sweep_depth(&mut self, bias_scale_b: i32) {
        let (p1, p2) = (self.program.param(1), self.program.param(2));
        let bias_a = (p2 * 0x2_00) / 0x7f;
        let bias_b = (p2 * bias_scale_b) / 0x7f;
        let w = &mut self.program.coefficient_words;
        w[0x82] = to_u16(bias_a);
        w[0x83] = to_u16(((0x2_00 - bias_a) * p1) / 0x7f);
        w[0x9d] = to_u16(bias_b);
        w[0x9e] = to_u16(((0x2_00 - bias_b) * p1) / 0x7f);
        self.push_coefficients_all(&[0x83, 0x9e]);
        self.push_coefficients_all(&[0x82, 0x9d]);
    }

    /// Designs the input EQ low shelves.
    pub(crate) fn low_equalizer(&mut self) {
        let (f, g) = (self.program.param(5), self.program.param(6));
        self.design_first_order(XGFilterShape::LowShelf, f, g, LIST_LOW_LEFT, 0);
        self.design_first_order(XGFilterShape::LowShelf, f, g, LIST_LOW_RIGHT, 0);
    }

    /// Designs the input EQ high shelves.
    pub(crate) fn high_equalizer(&mut self) {
        let (f, g) = (self.program.param(7), self.program.param(8));
        self.design_first_order(XGFilterShape::HighShelf, f, g, LIST_HIGH_LEFT, 0);
        self.design_first_order(XGFilterShape::HighShelf, f, g, LIST_HIGH_RIGHT, 0);
    }
}

/// Equivalent to: XGPhaserBase.pushCoefficients
fn phaser_push(d: &mut XGPhaserBase, list: &[usize], count: usize, offset: usize) {
    d.var.push_coefficients(&mut d.program, list, count, offset);
    if !d.program.push_enabled {
        return;
    }
    for &k in &list[offset..offset + count] {
        match k {
            0x82 | 0x9d => {
                let c = &mut d.program.coefficients;
                c[k] = fround(c[k] as f64 * 8.0) as f32;
            }
            0x83 | 0x9e => {
                let c = &mut d.program.coefficients;
                c[k] = fround(c[k] as f64 * 2f64.powi(-19)) as f32;
            }
            0x79 => {
                d.sweep_step = (d.program.raw_coefficient(0x79) * d.program.rate_inverse * 4_194_304.0).trunc()
                    as i32;
            }
            0x9c => {
                d.sweep_offset = (d.program.raw_coefficient(0x9c) * 4_194_304.0).trunc() as i32;
            }
            _ => {}
        }
    }
}

impl XGVariationProgram for XGPhaserBase {
    fn variation(&self) -> &XGVariationState {
        &self.var
    }

    fn variation_mut(&mut self) -> &mut XGVariationState {
        &mut self.var
    }

    fn render(
        &mut self,
        _memory: &mut [f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
        output_gain: f64,
    ) {
        self.render_impl(output_left, output_right, sample_count, output_gain);
    }
}

crate::impl_xg_variation_program!(
    XGPhaserBase,
    push = phaser_push,
    reset = crate::synthesizer::audio_engine::effects::xg::implementation::dsp::xg_variation_program::variation_reset
);
