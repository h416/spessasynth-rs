//! The S-YXG50 tremolo program, shared by TREMOLO and AUTO PAN.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/tremolo/xg_tremolo.ts
//!
//! The TS subclasses are the [`XGTremoloKind`] of one struct; their `convert` lives in the type modules.

use super::super::xg_dsp_program::{
    chop32, to_u16, triangle, XGDSPProgram, XGProgram, XGProgramLayout, PHASE_MASK,
    TRIANGLE_SCALE,
};
use super::super::xg_stereo_variation_program::{XGStereoState, CATEGORY};
use super::super::xg_variation_program::{XGVariationPreset, XGVariationState, MEMORY_SIZE};
use super::{auto_pan, tremolo};

// LFO shaping push lists (input smoothing, feedback smoothing, offset, center)
pub const LIST_SHAPE_LEFT: &[usize] = &[0x72, 0x71, 0x6f, 0x73];
pub const LIST_SHAPE_RIGHT: &[usize] = &[0x7c, 0x7b, 0x7a, 0x7d];

/// Clamps a value to -1..1.
#[inline]
fn clip(v: f64) -> f64 {
    if v > 1.0 { 1.0 } else { v.max(-1.0) }
}

/// The tremolo types (the TS subclasses of XGTremoloBase).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XGTremoloKind {
    Tremolo,
    AutoPan,
}

/// The S-YXG50 tremolo program.
///
/// A triangle LFO, offset, clipped and smoothed (a rate-dependent one-pole) shapes the waveform,
/// then drives an amplitude modulation and a modulated delay tap (pitch modulation) on each side.
///
/// Equivalent to: abstract class XGTremoloBase
pub struct XGTremoloBase {
    pub(crate) program: XGDSPProgram,
    pub(crate) var: XGVariationState,
    pub(crate) stereo: XGStereoState,
    kind: XGTremoloKind,
    /// The variation output level of the type (/64).
    variation_gain: i32,
    // LFO shaping states and the modulation values (updated every 4 samples)
    smooth_left: f64,
    smooth_right: f64,
    lfo_counter: i32,
    int_left: i32,
    frac_left: f64,
    int_right: i32,
    frac_right: f64,
    amp_left: f64,
    amp_right: f64,
}

impl XGTremoloBase {
    pub fn new(
        effect_type: u16,
        kind: XGTremoloKind,
        variation_gain: i32,
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
            variation_gain,
            smooth_left: 0.0,
            smooth_right: 0.0,
            lfo_counter: 0,
            int_left: 0,
            frac_left: 0.0,
            int_right: 0,
            frac_right: 0.0,
            amp_left: 1.0,
            amp_right: 1.0,
        }
    }

    fn convert_impl(&mut self, param: usize) {
        match self.kind {
            XGTremoloKind::Tremolo => tremolo::convert(self, param),
            XGTremoloKind::AutoPan => auto_pan::convert(self, param),
        }
    }

    fn reset_state_impl(&mut self) {
        self.smooth_left = 0.0;
        self.smooth_right = 0.0;
        self.lfo_counter = 0;
        self.int_left = 0;
        self.int_right = 0;
        self.frac_left = 0.0;
        self.frac_right = 0.0;
        self.amp_left = 1.0;
        self.amp_right = 1.0;
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
        let (t_read_l, t_read_r, t_write_l, t_write_r) = (t[0x28], t[0x2d], t[0x3a], t[0x3b]);
        let (right_to_l, right_to_r) = (c(0xaa), c(0xad));
        let out_l = c(0xb4) * output_gain;
        let out_r = c(0xb9) * output_gain;
        let (lfo_step, lfo_offset1, lfo_offset2) =
            (self.stereo.lfo_step, self.stereo.lfo_offset1, self.stereo.lfo_offset2);
        let in_l = &self.var.input_left;
        let in_r = &self.var.input_right;
        let mut pointer = self.program.pointer;
        let mut lfo_phase = self.var.lfo_phase;
        let mut lfo_counter = self.lfo_counter;
        let (mut smooth_left, mut smooth_right) = (self.smooth_left, self.smooth_right);
        let (mut int_left, mut frac_left) = (self.int_left, self.frac_left);
        let (mut int_right, mut frac_right) = (self.int_right, self.frac_right);
        let (mut amp_left, mut amp_right) = (self.amp_left, self.amp_right);
        for i in 0..sample_count {
            let counter = lfo_counter;
            lfo_counter += 1;
            if (counter & 3) == 0 {
                // Triangle, offset and clip, smooth, then rescale and clip again.
                // Truncating float32 math like the original, the smoothing pole is close to 1
                lfo_phase = (lfo_phase + lfo_step) & PHASE_MASK;
                let shape_l = clip(chop32(
                    triangle((lfo_phase - lfo_offset1) & PHASE_MASK) as f64 * TRIANGLE_SCALE - c(0x6f),
                ));
                let shape_r = clip(chop32(
                    triangle((lfo_phase - lfo_offset2) & PHASE_MASK) as f64 * TRIANGLE_SCALE - c(0x7a),
                ));
                smooth_left = chop32(chop32(shape_l * c(0x72)) + chop32(c(0x71) * smooth_left));
                smooth_right = chop32(chop32(shape_r * c(0x7c)) + chop32(c(0x7b) * smooth_right));
                let x = clip(chop32(chop32(smooth_left - c(0x73)) * 4.0));
                let y = clip(chop32(chop32(smooth_right - c(0x7d)) * 4.0));
                // Delay modulation (truncated, so the fraction can be negative) and amplitude
                let v_l = chop32(chop32(x * c(0x75)) - chop32(c(0x76) * c(0x74)));
                let v_r = chop32(chop32(y * c(0x85)) - chop32(c(0x86) * c(0x84)));
                int_left = v_l.trunc() as i32;
                int_right = v_r.trunc() as i32;
                frac_left = chop32(v_l - int_left as f64);
                frac_right = chop32(v_r - int_right as f64);
                amp_left = chop32(chop32(x * c(0x83)) + 1.0);
                amp_right = chop32(chop32(y * c(0x94)) + 1.0);
            }
            let mut k = (t_read_l + pointer + int_left) & mask;
            let m0 = memory[k as usize] as f64;
            let tap_l = (memory[((k + 1) & mask) as usize] as f64 - m0) * frac_left + m0;
            k = (t_read_r + pointer + int_right) & mask;
            let m0 = memory[k as usize] as f64;
            let tap_r = (memory[((k + 1) & mask) as usize] as f64 - m0) * frac_right + m0;
            let r = in_r[i] as f64;
            // The right line is written first, so the left line wins if both inputs share an address.
            // Below 44.1 kHz the input addresses 1 and 0 both scale to 0, which would silence the left line.
            // At 44.1 kHz the left line overwrites the same sample one step later, so this matches it.
            memory[((t_write_r + pointer) & mask) as usize] = (right_to_r * r) as f32;
            memory[((t_write_l + pointer) & mask) as usize] = (right_to_l * r + in_l[i] as f64) as f32;
            output_left[i] = (tap_l * amp_left * out_l) as f32;
            output_right[i] = (tap_r * amp_right * out_r) as f32;
            pointer = (pointer - 1) & mask;
        }
        self.program.pointer = pointer;
        self.var.lfo_phase = lfo_phase;
        self.lfo_counter = lfo_counter & 3;
        self.smooth_left = smooth_left;
        self.smooth_right = smooth_right;
        self.int_left = int_left;
        self.frac_left = frac_left;
        self.int_right = int_right;
        self.frac_right = frac_right;
        self.amp_left = amp_left;
        self.amp_right = amp_right;
    }

    /// Shapes the LFO waveform from its rate: the smoothing follows the rate.
    /// `list` is the shaping coefficient list of one side.
    pub(crate) fn shape_lfo(&mut self, list: &[usize]) {
        let w = &mut self.program.coefficient_words;
        let rate = w[0x77] as i32;
        w[list[0]] = rate as u16;
        w[list[1]] = rate as u16;
        w[list[2]] = to_u16(10.min((rate << 16) >> 20) + 0xf5);
        w[list[3]] = 0x3_c0;
        self.push_coefficients_all(list);
    }

    /// The output level word for the given level (0x200 is unity).
    pub(crate) fn output_word(&self, level: i32) -> u16 {
        to_u16((self.variation_gain * level) >> 6)
    }
}

crate::impl_xg_stereo_variation_program!(XGTremoloBase);
