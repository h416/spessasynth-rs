//! DISTORTION (`49 00`): hard clipping with a low shelf, low pass and mid-EQ.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/distortion/distortion.ts

use super::super::xg_dsp_program::{
    XGBiquadShape, XGFilterShape, XGParameterRanges, XGProgram, XGProgramPreset,
};
use super::super::xg_mono_variation_program::{LIST_LOW_A, LIST_LOW_B};
use super::super::xg_variation_program::{variation_layout, XGVariationPreset};
use super::xg_distortion::{
    XGDistortionBase, XGDistortionKind, CONNECTION_PARAMETERS, LIST_LOW_PASS, LIST_MID, LIST_SHELF,
};

const PARAMETER_RANGES: XGParameterRanges =
    &[(1, 8, 40), (2, 52, 76), (3, 34, 60), (6, 28, 54), (7, 52, 76), (8, 10, 120)];

/// The converter index of the input filters, designed once per type load.
const INPUT_FILTERS: usize = 16;

pub const TYPE: u16 = 0x49_00;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        512, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 256, 0, 0,
    ],
    defaults: [40, 20, 72, 53, 48, 0, 43, 74, 10, 127, 120, 0, 0, 0, 0, 0],
};

pub(super) const INIT_ORDER: &[usize] = &[INPUT_FILTERS, 0, 1, 3, 6, 9, 10];

/// Builds a DISTORTION-family program (DISTORTION and OVER DRIVE).
pub(super) fn new_with(
    effect_type: u16,
    preset: &'static XGVariationPreset,
    high_pass_frequency: i32,
    low_pass_frequency: i32,
    sample_rate: f64,
    max_buffer_size: usize,
) -> XGDistortionBase {
    XGDistortionBase::new(
        effect_type,
        XGDistortionKind::Distortion { high_pass_frequency, low_pass_frequency },
        preset,
        variation_layout(INIT_ORDER, CONNECTION_PARAMETERS, PARAMETER_RANGES),
        sample_rate,
        max_buffer_size,
    )
}

/// Equivalent to: class XGDistortion
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGDistortionBase {
    // The input high pass and low pass frequency indices
    new_with(TYPE, &PRESET, 0, 0x36, sample_rate, max_buffer_size)
}

/// Equivalent to: XGDistortion.convert
pub(super) fn convert(d: &mut XGDistortionBase, param: usize) {
    let p = |d: &XGDistortionBase, i: usize| d.program.param(i);
    match param {
        INPUT_FILTERS => {
            let XGDistortionKind::Distortion { high_pass_frequency, low_pass_frequency } = d.kind
            else {
                return;
            };
            d.design_first_order(XGFilterShape::HighPass, high_pass_frequency, 0x46, LIST_LOW_A, 0);
            d.design_first_order(XGFilterShape::LowPass, low_pass_frequency, 0x46, LIST_LOW_B, 0);
        }
        0 | 4 => d.drive(p(d, 0), p(d, 4)),
        1 | 2 => d.design_first_order(XGFilterShape::LowShelf, p(d, 1), p(d, 2), LIST_SHELF, 0),
        3 => d.design_first_order(XGFilterShape::LowPass, p(d, 3), 0x43, LIST_LOW_PASS, 0),
        6..=8 => {
            let (f, g, q) = (p(d, 6), p(d, 7), p(d, 8));
            d.design_second_order(XGBiquadShape::Peaking, f, g, q, LIST_MID);
        }
        9 => d.dry_wet(),
        10 => d.edge(p(d, 10)),
        _ => {}
    }
}
