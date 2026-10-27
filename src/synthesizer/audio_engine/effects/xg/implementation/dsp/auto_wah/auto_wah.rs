//! AUTO WAH (`4E 00`): a resonant filter per side, swept by an exponential LFO.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/auto_wah/auto_wah.ts

use super::super::xg_dsp_program::{
    chop32, fround, to_u16, triangle, XGDSPProgram, XGFilterShape, XGParameterRanges, XGProgram,
    XGProgramPreset, PHASE_MASK,
};
use super::super::xg_dsp_tables::{LFO_RATE, PAN_LEFT, PAN_RIGHT};
use super::super::xg_filter_variation_program::{CATEGORY, UNWRITTEN_CATEGORY_3};
use super::super::xg_variation_program::{variation_layout, XGVariationState};
use super::auto_wah_tables::CUTOFF_OFFSET;

const PARAMETER_RANGES: XGParameterRanges =
    &[(3, 10, 120), (5, 8, 40), (6, 52, 76), (7, 28, 58), (8, 52, 76)];

// Input EQ push lists (b0, b1, a1): low and high shelves of the right and left sides
const LIST_LOW_RIGHT: &[usize] = &[0x70, 0x6f, 0x71];
const LIST_LOW_LEFT: &[usize] = &[0x81, 0x80, 0x82];
const LIST_HIGH_RIGHT: &[usize] = &[0x73, 0x72, 0x74];
const LIST_HIGH_LEFT: &[usize] = &[0x84, 0x83, 0x85];

pub const TYPE: u16 = 0x4e_00;
/// Equivalent to: static memorySize = 0
const MEMORY_SIZE: usize = 0;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 128,
        0, 513, 512, 0, 512, 0, 0, 0, 0, 0, 0, 0, 128, 0, 513, 512, 0, 512, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 256, 0, 0, 0,
    ],
    defaults: [70, 56, 39, 25, 0, 28, 66, 46, 64, 127, 0, 0, 0, 0, 0, 0],
};

const INIT_ORDER: &[usize] = &[0, 1, 3, 5, 7, 9];
const CONNECTION_PARAMETERS: &[usize] = &[9];

/// Equivalent to: class XGAutoWah
pub struct XGAutoWah {
    pub(crate) program: XGDSPProgram,
    pub(crate) var: XGVariationState,
    /// The auto wah LFO step (coefficient 0x7e).
    pub(crate) wah_step: i32,
    // Input EQ states (two first-order sections per side)
    eq_xl: f64,
    eq_yl: f64,
    eq_zl: f64,
    eq_xr: f64,
    eq_yr: f64,
    eq_zr: f64,
    // Wah filter states and the swept frequency coefficient (updated every 4 samples)
    band_l: f64,
    low_l: f64,
    band_r: f64,
    low_r: f64,
    frequency: f64,
    lfo_counter: i32,
}

impl XGAutoWah {
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
            var: XGVariationState::new(max_buffer_size, UNWRITTEN_CATEGORY_3),
            wah_step: 0,
            eq_xl: 0.0,
            eq_yl: 0.0,
            eq_zl: 0.0,
            eq_xr: 0.0,
            eq_yr: 0.0,
            eq_zr: 0.0,
            band_l: 0.0,
            low_l: 0.0,
            band_r: 0.0,
            low_r: 0.0,
            frequency: 0.0,
            lfo_counter: 0,
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
        let c = |i: usize| self.program.coefficients[i] as f64;
        let in_l = &self.var.input_left;
        let in_r = &self.var.input_right;
        let (lb0_l, lb1_l, la1_l) = (c(0x81), c(0x80), c(0x82));
        let (hb0_l, hb1_l, ha1_l) = (c(0x84), c(0x83), c(0x85));
        let (lb0_r, lb1_r, la1_r) = (c(0x70), c(0x6f), c(0x71));
        let (hb0_r, hb1_r, ha1_r) = (c(0x73), c(0x72), c(0x74));
        let (dry_left_gain, dry_right_gain) = (c(0xb9), c(0xab));
        let (left_gain, left_to_right, right_gain) = (c(0x88), c(0x94), c(0x95));
        let (depth, offset, scale) = (c(0x86), c(0x87), c(0xbf));
        let (resonance_l, low_in_l, keep_l, low_fb_l) = (c(0x89), c(0x8a), c(0x8b), c(0x8d));
        let (resonance_r, low_in_r, keep_r, low_fb_r) = (c(0x96), c(0x97), c(0x98), c(0x9a));
        let (out_l, out_r) = (c(0xba), c(0xad));
        let step = self.wah_step;
        let (mut eq_xl, mut eq_yl, mut eq_zl) = (self.eq_xl, self.eq_yl, self.eq_zl);
        let (mut eq_xr, mut eq_yr, mut eq_zr) = (self.eq_xr, self.eq_yr, self.eq_zr);
        let (mut band_l, mut low_l, mut band_r, mut low_r) = (self.band_l, self.low_l, self.band_r, self.low_r);
        let mut frequency = self.frequency;
        let mut lfo_counter = self.lfo_counter;
        let mut lfo_phase = self.var.lfo_phase;
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
            let wet_in_l = left_gain * left;
            let wet_in_r = left_to_right * left + right_gain * right;

            let counter = lfo_counter;
            lfo_counter += 1;
            if (counter & 3) == 0 {
                // Exponential sweep: 2^v with the integer part as a shift
                lfo_phase = (lfo_phase + step) & PHASE_MASK;
                let v = chop32(chop32(triangle(lfo_phase) as f64 * depth) - offset);
                let n = v.trunc() as i32;
                frequency = chop32(chop32(v - n as f64 + 1.0) * chop32((1i32 << (n & 0x1f)) as f64 * scale));
                frequency = (-1f64).max(1f64.min(frequency));
            }

            // Resonant filters (state variable form)
            band_l = fround(keep_l * band_l + (low_in_l * low_l + resonance_l * band_l + wet_in_l) * frequency);
            low_l = fround(frequency * band_l + low_fb_l * low_l);
            band_r = fround(keep_r * band_r + (resonance_r * band_r + low_in_r * low_r + wet_in_r) * frequency);
            low_r = fround(frequency * band_r + low_fb_r * low_r);

            output_left[i] = ((out_l * band_l + dry_left_gain * left) * output_gain) as f32;
            output_right[i] = ((out_r * band_r + dry_right_gain * right) * output_gain) as f32;
        }
        self.eq_xl = eq_xl;
        self.eq_yl = eq_yl;
        self.eq_zl = eq_zl;
        self.eq_xr = eq_xr;
        self.eq_yr = eq_yr;
        self.eq_zr = eq_zr;
        self.band_l = band_l;
        self.low_l = low_l;
        self.band_r = band_r;
        self.low_r = low_r;
        self.frequency = frequency;
        self.lfo_counter = lfo_counter & 3;
        self.var.lfo_phase = lfo_phase;
    }

    fn reset_state_impl(&mut self) {
        self.eq_xl = 0.0;
        self.eq_yl = 0.0;
        self.eq_zl = 0.0;
        self.eq_xr = 0.0;
        self.eq_yr = 0.0;
        self.eq_zr = 0.0;
        self.band_l = 0.0;
        self.low_l = 0.0;
        self.band_r = 0.0;
        self.low_r = 0.0;
        self.frequency = 0.0;
        self.lfo_counter = 0;
    }

    fn convert_impl(&mut self, param: usize) {
        let p = |d: &Self, i: usize| d.program.param(i);
        match param {
            0 => {
                self.program.coefficient_words[0x7e] = LFO_RATE[p(self, 0) as usize];
                self.push_coefficients_all(&[0x7e]);
            }
            1 | 2 => {
                // The cutoff offset, and the depth relative to it
                let cutoff = CUTOFF_OFFSET[p(self, 2) as usize] as i32;
                let depth = (((cutoff - 0x2_00) & 0x3_ff) * p(self, 1)) / 0x7f;
                let w = &mut self.program.coefficient_words;
                w[0x87] = to_u16(cutoff);
                w[0x86] = to_u16(depth);
                self.push_coefficients_all(&[0x87]);
                self.push_coefficients_all(&[0x86]);
            }
            3 => {
                // Resonance
                let p3 = p(self, 3);
                let word = if p3 < 11 { 0x2_01 } else { (fround(-5120.0 / p3 as f64).trunc() as i32) & 0x3_ff };
                let w = &mut self.program.coefficient_words;
                w[0x89] = to_u16(word);
                w[0x96] = to_u16(word);
                self.push_coefficients_all(&[0x89, 0x96]);
            }
            5 | 6 => {
                let (f, g) = (p(self, 5), p(self, 6));
                self.design_first_order(XGFilterShape::LowShelf, f, g, LIST_LOW_RIGHT, 0);
                self.design_first_order(XGFilterShape::LowShelf, f, g, LIST_LOW_LEFT, 0);
            }
            7 | 8 => {
                let (f, g) = (p(self, 7), p(self, 8));
                self.design_first_order(XGFilterShape::HighShelf, f, g, LIST_HIGH_RIGHT, 0);
                self.design_first_order(XGFilterShape::HighShelf, f, g, LIST_HIGH_LEFT, 0);
            }
            9 => {
                // The system mode is always fully wet
                let mix = self.program.mix_index();
                let w = &mut self.program.coefficient_words;
                w[0xab] = to_u16(PAN_LEFT[mix] as i32);
                w[0xb9] = w[0xab];
                w[0xad] = to_u16(PAN_RIGHT[mix] as i32);
                w[0xba] = w[0xad];
                self.push_coefficients_all(&[0xb9, 0xba]);
                self.push_coefficients_all(&[0xab, 0xad]);
            }
            _ => {}
        }
    }
}

crate::impl_xg_filter_variation_program!(XGAutoWah);
