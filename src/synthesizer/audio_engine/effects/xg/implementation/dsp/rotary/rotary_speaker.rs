//! ROTARY SPEAKER (`45 00`): four delay taps of a mono line with pitch modulation.
//! Three taps (amplitude modulated too) follow the first LFO phase, the fourth follows the second phase.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/rotary/rotary_speaker.ts

use super::super::xg_dsp_program::{
    chop32, to_u16, triangle, XGDSPProgram, XGParameterRanges, XGProgram, XGProgramPreset,
    PHASE_MASK, TRIANGLE_SCALE,
};
use super::super::xg_dsp_tables::{PAN_LEFT, PAN_RIGHT};
use super::super::xg_stereo_variation_program::{XGStereoState, XGStereoVariationProgram, CATEGORY};
use super::super::xg_variation_program::{variation_layout, XGVariationState, MEMORY_SIZE};

const PARAMETER_RANGES: XGParameterRanges = &[(5, 8, 40), (6, 52, 76), (7, 28, 58), (8, 52, 76)];

// LFO shaping push lists (input smoothing, feedback smoothing, offset, center)
const LIST_SHAPE_A: &[usize] = &[0x72, 0x71, 0x6f, 0x73];
const LIST_SHAPE_B: &[usize] = &[0x7c, 0x7b, 0x7a, 0x7d];
const LIST_SHAPE_C: &[usize] = &[0x8b, 0x8a, 0x89, 0x8c];
const LIST_SHAPE_D: &[usize] = &[0x90, 0x8f, 0x8e, 0x91];
const LIST_OUTPUT: &[usize] = &[0xb2, 0xb4, 0xb5, 0xb8, 0xb9, 0xbb, 0xb6, 0xba];

pub const TYPE: u16 = 0x45_00;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        256, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 512, 683, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        341, 0, 0, 0, 0, 768, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 3, 0,
        0, 0, 2, 3, 0, 0, 0, 0, 2, 3, 0, 0, 0, 2, 3, 1, 0, 0, 0, 256,
    ],
    defaults: [81, 35, 0, 0, 0, 24, 60, 45, 54, 127, 33, 52, 30, 0, 0, 0],
};

const INIT_ORDER: &[usize] = &[0, 1, 5, 7, 9];
const CONNECTION_PARAMETERS: &[usize] = &[5, 9];

/// Equivalent to: class XGRotarySpeaker
pub struct XGRotarySpeaker {
    pub(crate) program: XGDSPProgram,
    pub(crate) var: XGVariationState,
    pub(crate) stereo: XGStereoState,
    // The second LFO phase, the shaping states, the tap modulations and gains (updated every 4 samples)
    lfo_phase2: i32,
    smooth: [f64; 4],
    lfo_counter: i32,
    int: [i32; 4],
    frac: [f64; 4],
    amp: [f64; 3],
}

impl XGRotarySpeaker {
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
            smooth: [0.0; 4],
            lfo_counter: 0,
            int: [0; 4],
            frac: [0.0; 4],
            amp: [1.0; 3],
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
        let (tap_a, tap_b, tap_c, tap_d) = (t[0x28], t[0x2d], t[0x33], t[0x38]);
        let (write_l, write_r) = (t[0x3a], t[0x3b]);
        let (right_to_l, right_to_r) = (c(0xaa), c(0xad));
        let dry_l = c(0xb2) * output_gain;
        let wet_a_l = c(0xb4) * output_gain;
        let wet_c_l = c(0xb5) * output_gain;
        let wet_d_l = c(0xb6) * output_gain;
        let dry_r = c(0xb8) * output_gain;
        let wet_b_r = c(0xb9) * output_gain;
        let wet_c_r = c(0xba) * output_gain;
        let wet_d_r = c(0xbb) * output_gain;
        let s = &self.stereo;
        let (lfo_step, lfo_step2) = (s.lfo_step, s.lfo_step2);
        let (lfo_offset1, lfo_offset2, lfo_offset5, lfo_offset6) =
            (s.lfo_offset1, s.lfo_offset2, s.lfo_offset5, s.lfo_offset6);
        let in_l = &self.var.input_left;
        let in_r = &self.var.input_right;
        let mut pointer = self.program.pointer;
        let mut lfo_phase = self.var.lfo_phase;
        let mut lfo_phase2 = self.lfo_phase2;
        let mut lfo_counter = self.lfo_counter;
        let [mut smooth_a, mut smooth_b, mut smooth_c, mut smooth_d] = self.smooth;
        let [mut int_a, mut int_b, mut int_c, mut int_d] = self.int;
        let [mut frac_a, mut frac_b, mut frac_c, mut frac_d] = self.frac;
        let [mut amp_a, mut amp_b, mut amp_c] = self.amp;
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
                let shape_d = chop32(tri(lfo_phase2, lfo_offset6) * TRIANGLE_SCALE - c(0x8e));
                smooth_a = chop32(chop32(shape_a * c(0x72)) + chop32(c(0x71) * smooth_a));
                smooth_b = chop32(chop32(shape_b * c(0x7c)) + chop32(c(0x7b) * smooth_b));
                smooth_c = chop32(chop32(shape_c * c(0x8b)) + chop32(c(0x8a) * smooth_c));
                smooth_d = chop32(chop32(shape_d * c(0x90)) + chop32(c(0x8f) * smooth_d));
                let x_a = chop32(smooth_a - c(0x73)) * 4.0;
                let x_b = chop32(smooth_b - c(0x7d)) * 4.0;
                let x_c = chop32(smooth_c - c(0x8c)) * 4.0;
                let x_d = chop32(smooth_d - c(0x91)) * 4.0;
                let v_a = chop32(x_a * c(0x75));
                let v_b = chop32(x_b * c(0x85));
                let v_c = chop32(x_c * c(0x95));
                let v_d = chop32(x_d * c(0xa5));
                amp_a = chop32(chop32(x_a * c(0x83)) + 1.0);
                amp_b = chop32(chop32(x_b * c(0x94)) + 1.0);
                amp_c = chop32(chop32(x_c * c(0xa4)) + 1.0);
                int_a = v_a.trunc() as i32;
                int_b = v_b.trunc() as i32;
                int_c = v_c.trunc() as i32;
                int_d = v_d.trunc() as i32;
                frac_a = chop32(v_a - int_a as f64);
                frac_b = chop32(v_b - int_b as f64);
                frac_c = chop32(v_c - int_c as f64);
                frac_d = chop32(v_d - int_d as f64);
            }
            let read = |memory: &[f32], k: i32, frac: f64| {
                let m0 = memory[k as usize] as f64;
                (memory[((k + 1) & mask) as usize] as f64 - m0) * frac + m0
            };
            let a = read(memory, (tap_a + pointer + int_a) & mask, frac_a) * amp_a;
            let b = read(memory, (tap_b + pointer + int_b) & mask, frac_b) * amp_b;
            let s = read(memory, (tap_c + pointer + int_c) & mask, frac_c) * amp_c;
            let d = read(memory, (tap_d + pointer + int_d) & mask, frac_d);
            let l = in_l[i] as f64;
            let r = in_r[i] as f64;
            memory[((write_l + pointer) & mask) as usize] = (right_to_l * r + l) as f32;
            memory[((write_r + pointer) & mask) as usize] = (right_to_r * r) as f32;
            output_left[i] = (dry_l * l + a * wet_a_l + s * wet_c_l + d * wet_d_l) as f32;
            output_right[i] = (dry_r * r + b * wet_b_r + s * wet_c_r + d * wet_d_r) as f32;
            pointer = (pointer - 1) & mask;
        }
        self.program.pointer = pointer;
        self.var.lfo_phase = lfo_phase;
        self.lfo_phase2 = lfo_phase2;
        self.lfo_counter = lfo_counter & 3;
        self.smooth = [smooth_a, smooth_b, smooth_c, smooth_d];
        self.int = [int_a, int_b, int_c, int_d];
        self.frac = [frac_a, frac_b, frac_c, frac_d];
        self.amp = [amp_a, amp_b, amp_c];
    }

    fn reset_state_impl(&mut self) {
        self.lfo_phase2 = 0;
        self.smooth = [0.0; 4];
        self.lfo_counter = 0;
        self.int = [0; 4];
        self.frac = [0.0; 4];
        self.amp = [1.0; 3];
    }

    fn convert_impl(&mut self, param: usize) {
        let p = |d: &Self, i: usize| d.program.param(i);
        match param {
            0 => {
                self.lfo_rate(p(self, 0));
                self.shape_smooth_lfo(LIST_SHAPE_A, false);
                self.shape_smooth_lfo(LIST_SHAPE_B, false);
                self.shape_smooth_lfo(LIST_SHAPE_C, false);
                self.shape_smooth_lfo(LIST_SHAPE_D, true);
            }
            1 => {
                // The amplitude depth saturates from 32 up, the pitch depth is half the value
                let p1 = p(self, 1);
                let amplitude = self.amplitude_depth(if p1 < 0x20 { p1 << 2 } else { 0x7f });
                let pitch = self.pitch_depth(p1 >> 1);
                let w = &mut self.program.coefficient_words;
                w[0x83] = amplitude;
                w[0x94] = amplitude;
                w[0xa4] = amplitude;
                w[0x75] = pitch;
                w[0x85] = pitch;
                w[0x95] = pitch;
                w[0xa5] = pitch;
                self.push_coefficients_all(&[0x83, 0x94, 0xa4, 0x75, 0x85, 0x95, 0xa5]);
            }
            5 | 6 => {
                // The +3 dB low shelf applies in the system mode
                let boosted = !self.program.insertion;
                self.design_low_equalizer(p(self, 5), p(self, 6), boosted);
            }
            7 | 8 => self.design_high_equalizer(p(self, 7), p(self, 8)),
            9 => {
                // The system mode is always fully wet, the insertion wet is at 70%
                let mix = self.program.mix_index();
                let mut wet = PAN_RIGHT[mix] as i32;
                if self.program.insertion {
                    wet = (wet * 7) / 10;
                }
                let w = &mut self.program.coefficient_words;
                w[0xb2] = to_u16(PAN_LEFT[mix] as i32);
                w[0xb8] = w[0xb2];
                let wet_word = to_u16((56 * wet) >> 6);
                for k in [0xb4, 0xb5, 0xb6, 0xb9, 0xba, 0xbb] {
                    w[k] = wet_word;
                }
                self.push_coefficients_all(LIST_OUTPUT);
            }
            _ => {}
        }
    }
}

crate::impl_xg_stereo_variation_program!(XGRotarySpeaker);
