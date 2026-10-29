//! ECHO (`07 00`): two independent delay lines with feedback and a second tap each.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/delay/echo.ts

use super::super::xg_dsp_program::{XGParameterRanges, XGProgram, XGProgramPreset};
use super::super::xg_dsp_tables::{MIX_DRY, MIX_WET};
use super::super::xg_stereo_variation_program::XGStereoVariationProgram;
use super::super::xg_variation_program::variation_layout;
use super::delay_tables::DELAY_LEVEL;
use super::xg_delay::{
    XGDelayBase, XGDelayKind, CONNECTION_PARAMETERS, LEFT_BASE, LIST_DAMP_A, LIST_DAMP_B,
    RIGHT_BASE,
};

const PARAMETER_RANGES: XGParameterRanges = &[
    (0, 1, 3550),
    (1, 1, 127),
    (2, 1, 3550),
    (3, 1, 127),
    (4, 1, 10),
    (5, 1, 3550),
    (6, 1, 3550),
    (9, 1, 127),
    (12, 8, 40),
    (13, 52, 76),
    (14, 28, 58),
    (15, 52, 76),
];

pub const TYPE: u16 = 0x07_00;

// The right line input sits in the upper half of the memory
pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        128, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 16_384, 0, 0, 256,
    ],
    defaults: [1700, 80, 1780, 80, 10, 1700, 1780, 0, 0, 40, 0, 60, 28, 64, 46, 64],
};

const INIT_ORDER: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 12, 14];

/// Equivalent to: class XGEcho
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGDelayBase {
    XGDelayBase::new(
        TYPE,
        XGDelayKind::Echo,
        &PRESET,
        variation_layout(INIT_ORDER, CONNECTION_PARAMETERS, PARAMETER_RANGES),
        sample_rate,
        max_buffer_size,
    )
}

/// Equivalent to: XGEcho.convert
pub(super) fn convert(d: &mut XGDelayBase, param: usize) {
    let p = |d: &XGDelayBase, i: usize| d.program.param(i);
    match param {
        0 => d.place_tap(0x28, p(d, 0), LEFT_BASE),
        1 => {
            d.program.coefficient_words[0xab] = d.program.feedback_word(p(d, 1));
            d.push_coefficients_all(&[0xab]);
        }
        2 => d.place_tap(0x2d, p(d, 2), RIGHT_BASE),
        3 => {
            d.program.coefficient_words[0xaf] = d.program.feedback_word(p(d, 3));
            d.push_coefficients_all(&[0xaf]);
        }
        4 => {
            d.design_damping(p(d, 4), LIST_DAMP_A);
            d.design_damping(p(d, 4), LIST_DAMP_B);
        }
        5 => d.place_tap(0x33, p(d, 5), LEFT_BASE),
        6 => d.place_tap(0x38, p(d, 6), RIGHT_BASE),
        7 | 9 => {
            let mix = d.program.mix_index();
            let wet = MIX_WET[mix] as i32;
            let level = DELAY_LEVEL[d.program.param(7) as usize] as f64;
            let w = &mut d.program.coefficient_words;
            w[0xb2] = (MIX_DRY[mix] & 0x3_ff) as u16;
            w[0xb8] = w[0xb2];
            w[0xb4] = (wet & 0x3_ff) as u16;
            w[0xb9] = w[0xb4];
            w[0xb5] = (((wet as f64 * level).trunc() as i32) & 0x3_ff) as u16;
            w[0xbb] = w[0xb5];
            d.push_coefficients_all(&[0xb2, 0xb4, 0xb5, 0xb8, 0xb9, 0xbb]);
        }
        12 | 13 => d.design_low_equalizer(p(d, 12), p(d, 13), false),
        14 | 15 => d.design_high_equalizer(p(d, 14), p(d, 15)),
        _ => {}
    }
}
