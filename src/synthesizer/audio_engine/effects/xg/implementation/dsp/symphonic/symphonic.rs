//! SYMPHONIC (`44 00`): three delay taps of a mono line, each modulated by a smoothed slow triangle
//! and a fast triangle of a second LFO phase.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/symphonic/symphonic.ts

use super::super::xg_dsp_program::{
    chop32, to_u16, triangle, XGDSPProgram, XGParameterRanges, XGProgram, XGProgramPreset,
    PHASE_MASK, TRIANGLE_SCALE,
};
use super::super::xg_dsp_tables::{DELAY_OFFSET, PAN_LEFT, PAN_RIGHT};
use super::super::xg_stereo_variation_program::{XGStereoState, XGStereoVariationProgram, CATEGORY};
use super::super::xg_variation_program::{variation_layout, XGVariationState, MEMORY_SIZE};

const PARAMETER_RANGES: XGParameterRanges = &[(5, 8, 40), (6, 52, 76), (7, 28, 58), (8, 52, 76)];

// LFO shaping push lists (input smoothing, feedback smoothing, offset, center)
const LIST_SHAPE_A: &[usize] = &[0x72, 0x71, 0x6f, 0x73];
const LIST_SHAPE_B: &[usize] = &[0x7c, 0x7b, 0x7a, 0x7d];
const LIST_SHAPE_C: &[usize] = &[0x8b, 0x8a, 0x89, 0x8c];
const LIST_TAPS: &[usize] = &[0x28, 0x29, 0x33, 0x34, 0x2d, 0x2e];
const LIST_OUTPUT: &[usize] = &[0xb2, 0xb4, 0xb5, 0xb8, 0xb9, 0xba];

pub const TYPE: u16 = 0x44_00;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        512, 0, 0, 256, 0, 0, 0, 0, 0, 0, 0, 512, 683, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 683, 0, 0, 0,
        341, 0, 0, 0, 0, 341, 256, 1023, 927, 960, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 256,
    ],
    defaults: [12, 25, 16, 0, 0, 28, 64, 46, 64, 127, 46, 64, 10, 0, 0, 0],
};

const INIT_ORDER: &[usize] = &[0, 1, 2, 5, 7, 9];
const CONNECTION_PARAMETERS: &[usize] = &[9];

/// Equivalent to: class XGSymphonic
pub struct XGSymphonic {
    pub(crate) program: XGDSPProgram,
    pub(crate) var: XGVariationState,
    pub(crate) stereo: XGStereoState,
    // The second LFO phase, the shaping states and the tap modulations (updated every 4 samples)
    lfo_phase2: i32,
    smooth_a: f64,
    smooth_b: f64,
    smooth_c: f64,
    lfo_counter: i32,
    int_a: i32,
    frac_a: f64,
    int_b: i32,
    frac_b: f64,
    int_c: i32,
    frac_c: f64,
}

impl XGSymphonic {
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
            lfo_phase2: 0,
            smooth_a: 0.0,
            smooth_b: 0.0,
            smooth_c: 0.0,
            lfo_counter: 0,
            int_a: 0,
            frac_a: 0.0,
            int_b: 0,
            frac_b: 0.0,
            int_c: 0,
            frac_c: 0.0,
        }
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
        let (tap_a, tap_b, tap_c, write_l, write_r) = (t[0x28], t[0x2d], t[0x33], t[0x3a], t[0x3b]);
        let (right_to_l, right_to_r) = (c(0xaa), c(0xad));
        let dry_l = c(0xb2) * output_gain;
        let wet_a_l = c(0xb4) * output_gain;
        let wet_c_l = c(0xb5) * output_gain;
        let dry_r = c(0xb8) * output_gain;
        let wet_b_r = c(0xb9) * output_gain;
        let wet_c_r = c(0xba) * output_gain;
        let s = &self.stereo;
        let (lfo_step, lfo_step2) = (s.lfo_step, s.lfo_step2);
        let (lfo_offset1, lfo_offset2, lfo_offset3) = (s.lfo_offset1, s.lfo_offset2, s.lfo_offset3);
        let (lfo_offset4, lfo_offset5, lfo_offset6) = (s.lfo_offset4, s.lfo_offset5, s.lfo_offset6);
        let in_l = &self.var.input_left;
        let in_r = &self.var.input_right;
        let mut pointer = self.program.pointer;
        let mut lfo_phase = self.var.lfo_phase;
        let mut lfo_phase2 = self.lfo_phase2;
        let mut lfo_counter = self.lfo_counter;
        let (mut smooth_a, mut smooth_b, mut smooth_c) = (self.smooth_a, self.smooth_b, self.smooth_c);
        let (mut int_a, mut frac_a) = (self.int_a, self.frac_a);
        let (mut int_b, mut frac_b) = (self.int_b, self.frac_b);
        let (mut int_c, mut frac_c) = (self.int_c, self.frac_c);
        let tri = |phase: i32, offset: i32| triangle((phase - offset) & PHASE_MASK) as f64;
        for i in 0..sample_count {
            let counter = lfo_counter;
            lfo_counter += 1;
            if (counter & 3) == 0 {
                // Truncating float32 math like the original, the smoothing poles are close to 1
                lfo_phase = (lfo_phase + lfo_step) & PHASE_MASK;
                lfo_phase2 = (lfo_phase2 + lfo_step2) & PHASE_MASK;
                let shape_a = chop32(tri(lfo_phase, lfo_offset1) * TRIANGLE_SCALE - c(0x6f));
                let shape_b = chop32(tri(lfo_phase, lfo_offset2) * TRIANGLE_SCALE - c(0x7a));
                let shape_c = chop32(tri(lfo_phase, lfo_offset5) * TRIANGLE_SCALE - c(0x89));
                smooth_a = chop32(chop32(shape_a * c(0x72)) + chop32(c(0x71) * smooth_a));
                smooth_b = chop32(chop32(shape_b * c(0x7c)) + chop32(c(0x7b) * smooth_b));
                smooth_c = chop32(chop32(shape_c * c(0x8b)) + chop32(c(0x8a) * smooth_c));
                // Slow smoothed sweep plus a fast triangle of the second phase
                let v_a = chop32(
                    chop32(chop32(smooth_a - c(0x73)) * c(0x75)) * 4.0
                        + chop32(tri(lfo_phase2, lfo_offset3) * c(0x76)) * TRIANGLE_SCALE,
                );
                let v_b = chop32(
                    chop32(chop32(smooth_b - c(0x7d)) * c(0x85)) * 4.0
                        + chop32(tri(lfo_phase2, lfo_offset4) * c(0x86)) * TRIANGLE_SCALE,
                );
                let v_c = chop32(
                    chop32(chop32(smooth_c - c(0x8c)) * c(0x95)) * 4.0
                        + chop32(tri(lfo_phase2, lfo_offset6) * c(0x96)) * TRIANGLE_SCALE,
                );
                int_a = v_a.trunc() as i32;
                int_b = v_b.trunc() as i32;
                int_c = v_c.trunc() as i32;
                frac_a = chop32(v_a - int_a as f64);
                frac_b = chop32(v_b - int_b as f64);
                frac_c = chop32(v_c - int_c as f64);
            }
            let read = |memory: &[f32], k: i32, frac: f64| {
                let m0 = memory[k as usize] as f64;
                (memory[((k + 1) & mask) as usize] as f64 - m0) * frac + m0
            };
            let a = read(memory, (tap_a + pointer + int_a) & mask, frac_a);
            let b = read(memory, (tap_b + pointer + int_b) & mask, frac_b);
            let s = read(memory, (tap_c + pointer + int_c) & mask, frac_c);
            let l = in_l[i] as f64;
            let r = in_r[i] as f64;
            memory[((write_l + pointer) & mask) as usize] = (right_to_l * r + l) as f32;
            memory[((write_r + pointer) & mask) as usize] = (right_to_r * r) as f32;
            output_left[i] = (dry_l * l + a * wet_a_l + s * wet_c_l) as f32;
            output_right[i] = (dry_r * r + b * wet_b_r + s * wet_c_r) as f32;
            pointer = (pointer - 1) & mask;
        }
        self.program.pointer = pointer;
        self.var.lfo_phase = lfo_phase;
        self.lfo_phase2 = lfo_phase2;
        self.lfo_counter = lfo_counter & 3;
        self.smooth_a = smooth_a;
        self.smooth_b = smooth_b;
        self.smooth_c = smooth_c;
        self.int_a = int_a;
        self.frac_a = frac_a;
        self.int_b = int_b;
        self.frac_b = frac_b;
        self.int_c = int_c;
        self.frac_c = frac_c;
    }

    fn reset_state_impl(&mut self) {
        self.lfo_phase2 = 0;
        self.smooth_a = 0.0;
        self.smooth_b = 0.0;
        self.smooth_c = 0.0;
        self.lfo_counter = 0;
        self.int_a = 0;
        self.int_b = 0;
        self.int_c = 0;
        self.frac_a = 0.0;
        self.frac_b = 0.0;
        self.frac_c = 0.0;
    }

    fn convert_impl(&mut self, param: usize) {
        let p = |d: &Self, i: usize| d.program.param(i);
        match param {
            0 => {
                self.lfo_rate(p(self, 0));
                self.shape_smooth_lfo(LIST_SHAPE_A, false);
                self.shape_smooth_lfo(LIST_SHAPE_B, false);
                self.shape_smooth_lfo(LIST_SHAPE_C, false);
            }
            1 => {
                // The slow sweep takes 5/4 of the depth, the fast one 3/8
                let slow = self.pitch_depth((p(self, 1) * 5) >> 2);
                let fast = self.pitch_depth((p(self, 1) * 3) >> 3);
                let w = &mut self.program.coefficient_words;
                w[0x75] = slow;
                w[0x85] = slow;
                w[0x95] = slow;
                w[0x76] = fast;
                w[0x86] = fast;
                w[0x96] = fast;
                self.push_coefficients_all(&[0x75, 0x85, 0x95, 0x76, 0x86, 0x96]);
            }
            2 => {
                let p2 = p(self, 2) as usize;
                let pr = &mut self.program;
                let delay = (pr.to_address100us(DELAY_OFFSET[p2] as i32) << 16) >> 16;
                let base = pr.signed_address(0x3a) + delay;
                let a = &mut pr.address_words;
                a[0x28] = to_u16(base + 1);
                a[0x29] = to_u16(base + 2);
                a[0x33] = to_u16(base + 0x41);
                a[0x34] = to_u16(base + 0x42);
                a[0x2d] = to_u16(base + 0x81);
                a[0x2e] = to_u16(base + 0x82);
                pr.push_addresses_all(LIST_TAPS);
            }
            5 | 6 => self.design_low_equalizer(p(self, 5), p(self, 6), false),
            7 | 8 => self.design_high_equalizer(p(self, 7), p(self, 8)),
            9 => {
                // The system mode is always fully wet, the insertion wet is halved
                let mix = self.program.mix_index();
                let mut wet = PAN_RIGHT[mix] as i32;
                if self.program.insertion {
                    wet >>= 1;
                }
                let w = &mut self.program.coefficient_words;
                w[0xb2] = to_u16(PAN_LEFT[mix] as i32);
                w[0xb8] = w[0xb2];
                let wet_word = to_u16((37 * ((wet * 10) / 7)) >> 6);
                w[0xb4] = wet_word;
                w[0xb5] = wet_word;
                w[0xb9] = wet_word;
                w[0xba] = wet_word;
                self.push_coefficients_all(LIST_OUTPUT);
            }
            _ => {}
        }
    }
}

crate::impl_xg_stereo_variation_program!(XGSymphonic);
