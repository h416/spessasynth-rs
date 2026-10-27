//! The S-YXG50 distortion program, shared by DISTORTION, OVER DRIVE and AMP SIMULATOR.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/distortion/xg_distortion.ts
//!
//! The TS subclasses are the [`XGDistortionKind`] of one struct; their `convert` lives in the type modules.

use super::super::xg_dsp_program::{fround, to_u16, XGDSPProgram, XGProgram, XGProgramLayout};
use super::super::xg_dsp_tables::{PAN_LEFT, PAN_RIGHT};
use super::super::xg_mono_variation_program::{mix_output, XGMonoState, CATEGORY, UNWRITTEN_CATEGORY_2};
use super::super::xg_variation_program::{XGVariationPreset, XGVariationState};
use super::distortion_tables::OUTPUT_LEVEL;
use super::{amp_simulator, distortion};

// Output filter push lists: low pass (b0, b1, a1), low shelf (b0, b1, a1), mid (b0, b1, b2, a1, a2)
pub const LIST_LOW_PASS: &[usize] = &[0xa3, 0xa2, 0xa4];
pub const LIST_SHELF: &[usize] = &[0xab, 0xaa, 0xac];
pub const LIST_MID: &[usize] = &[0xa9, 0xa8, 0xa7, 0xa6, 0xa5];

/// The clipping range (the original works on a 16-bit scale, which is 4 times the full scale).
const CLIP_HIGH: f64 = 131_071.0 / 32_768.0;
const CLIP_LOW: f64 = -4.0;

/// The variation output level of the distortion types (/64).
const VARIATION_GAIN: i32 = 63;

/// Equivalent to: static memorySize = 0
const MEMORY_SIZE: usize = 0;

/// The distortion connection parameters.
pub const CONNECTION_PARAMETERS: &[usize] = &[9];

/// Clamps a value to the clipping range.
#[inline]
fn clip(v: f64) -> f64 {
    if v > CLIP_HIGH { CLIP_HIGH } else { v.max(CLIP_LOW) }
}

/// The distortion types (the TS subclasses of XGDistortionBase).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XGDistortionKind {
    /// DISTORTION and OVER DRIVE, with the input high pass and low pass frequency indices.
    Distortion { high_pass_frequency: i32, low_pass_frequency: i32 },
    AmpSimulator,
}

/// The S-YXG50 distortion program.
///
/// The mono input (band limited by the input filters) drives a chain of six gain-and-clip stages,
/// the weighted sum of all stages is the waveshaped signal.
/// A low pass, a mid peaking EQ and a low shelf (the amp simulator's cabinet) follow.
///
/// Equivalent to: abstract class XGDistortionBase
pub struct XGDistortionBase {
    pub(crate) program: XGDSPProgram,
    pub(crate) var: XGVariationState,
    pub(crate) mono: XGMonoState,
    pub(crate) kind: XGDistortionKind,
}

impl XGDistortionBase {
    pub fn new(
        effect_type: u16,
        kind: XGDistortionKind,
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
            var: XGVariationState::new(max_buffer_size, UNWRITTEN_CATEGORY_2),
            mono: XGMonoState::new(max_buffer_size),
            kind,
        }
    }

    fn convert_impl(&mut self, param: usize) {
        match self.kind {
            XGDistortionKind::Distortion { .. } => distortion::convert(self, param),
            XGDistortionKind::AmpSimulator => amp_simulator::convert(self, param),
        }
    }

    fn reset_state_impl(&mut self) {
        self.mono.reset_state();
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
        let c = |i: usize| self.program.coefficients[i] as f64;
        let (drive, gain2, gain3, gain4, gain5, gain6) =
            (c(0x76), c(0x83), c(0x87), c(0x8a), c(0x8d), c(0x90));
        let (mix1, mix2, mix3, mix4, mix5, mix6, level) =
            (c(0x91), c(0x92), c(0x93), c(0x94), c(0x95), c(0x96), c(0xa1));
        let m = &mut self.mono;
        for v in m.mono.iter_mut().take(sample_count) {
            let s1 = clip(drive * *v as f64);
            let s2 = clip(s1 * gain2);
            let s3 = clip(s2 * gain3);
            let s4 = clip(s3 * gain4);
            let s5 = clip(s4 * gain5);
            let s6 = clip(s5 * gain6);
            *v = ((s6 * mix6 + s5 * mix5 + s4 * mix4 + s3 * mix3 + s2 * mix2 + s1 * mix1) * level) as f32;
        }
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

    /// Writes the drive and output level words.
    /// The output level compensates the drive up to a drive of 10.
    pub(crate) fn drive(&mut self, drive: i32, level: i32) {
        let gain = (drive * 499) / 0x7f + 13;
        let w = &mut self.program.coefficient_words;
        w[0x76] = to_u16(gain);
        w[0x7a] = to_u16(gain);
        let compensation = if drive < 11 { 169.0 / (gain * gain) as f64 } else { 169.0 / 2704.0 };
        let mut output = fround(compensation * OUTPUT_LEVEL[level as usize] as f64 * 512.0).trunc() as i32;
        if output == 0 && level != 0 {
            output = 1;
        }
        w[0xa1] = to_u16(output);
        self.push_coefficients_all(&[0x76, 0x7a]);
        self.push_coefficients_all(&[0xa1]);
    }

    /// Writes the clipping stage words from the edge (clip curve) parameter.
    pub(crate) fn edge(&mut self, edge: i32) {
        let stage = 0xff - edge;
        let w = &mut self.program.coefficient_words;
        w[0x83] = 0x2_00;
        for k in [0x87, 0x8a, 0x8d, 0x90] {
            w[k] = to_u16(stage);
        }
        w[0x91] = 0;
        for k in [0x92, 0x93, 0x94, 0x95, 0x96] {
            w[k] = 0x66;
        }
        self.push_coefficients_all(&[0x83, 0x87, 0x8a, 0x8d, 0x90]);
        // The input level is normalized by the stage response
        let x = stage as f64 / 128.0;
        let response = fround(fround((((1.0 + x) * x + 1.0) * x + 1.0) * x + 1.0) / 5.0);
        let w = &mut self.program.coefficient_words;
        w[0x7d] = to_u16(fround(512.0 / response).trunc() as i32);
        w[0x80] = 0x2_00;
        self.push_coefficients_all(&[0x7d, 0x80]);
    }

    /// Writes the dry and wet words (the system mode is always fully wet, the insertion wet is halved).
    pub(crate) fn dry_wet(&mut self) {
        let mix = self.program.mix_index();
        let mut wet = PAN_RIGHT[mix] as i32;
        if self.program.insertion {
            wet >>= 1;
        }
        let w = &mut self.program.coefficient_words;
        w[0xad] = to_u16(PAN_LEFT[mix] as i32);
        w[0xb1] = w[0xad];
        w[0xb0] = to_u16((VARIATION_GAIN * wet) >> 6);
        w[0xb3] = w[0xb0];
        self.push_coefficients_all(&[0xad, 0xb0, 0xb1, 0xb3]);
    }
}

crate::impl_xg_mono_variation_program!(XGDistortionBase);
