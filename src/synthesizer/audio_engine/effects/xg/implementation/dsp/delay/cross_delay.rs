//! CROSS DELAY (`08 00`): two delay lines feeding back into each other.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/delay/cross_delay.ts

use super::super::xg_dsp_program::{XGParameterRanges, XGProgram, XGProgramPreset};
use super::super::xg_dsp_tables::{MIX_DRY, MIX_WET};
use super::super::xg_stereo_variation_program::XGStereoVariationProgram;
use super::super::xg_variation_program::variation_layout;
use super::xg_delay::{
    XGDelayBase, XGDelayKind, CONNECTION_PARAMETERS, LEFT_BASE, LIST_DAMP_A, LIST_DAMP_B,
    RIGHT_BASE, RIGHT_LINE_START,
};

const PARAMETER_RANGES: XGParameterRanges = &[
    (0, 1, 3550),
    (1, 1, 3550),
    (2, 1, 127),
    (3, 0, 2),
    (4, 1, 10),
    (9, 1, 127),
    (12, 8, 40),
    (13, 52, 76),
    (14, 28, 58),
    (15, 52, 76),
];

pub const TYPE: u16 = 0x08_00;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        128, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 256,
    ],
    defaults: [1700, 1750, 111, 1, 10, 0, 0, 0, 0, 32, 0, 60, 28, 64, 46, 64],
};

const INIT_ORDER: &[usize] = &[3, 0, 1, 4, 12, 14];

/// Equivalent to: class XGCrossDelay
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGDelayBase {
    XGDelayBase::new(
        TYPE,
        XGDelayKind::CrossDelay,
        &PRESET,
        variation_layout(INIT_ORDER, CONNECTION_PARAMETERS, PARAMETER_RANGES),
        sample_rate,
        max_buffer_size,
    )
}

/// Equivalent to: XGCrossDelay.convert
pub(super) fn convert(d: &mut XGDelayBase, param: usize) {
    let p = |d: &XGDelayBase, i: usize| d.program.param(i);
    // Input select: 0 L, 1 R (the lines swap), 2 L&R
    let swapped = p(d, 3) == 1;
    match param {
        0 => d.place_tap(0x2d, p(d, 0), if swapped { RIGHT_BASE } else { LEFT_BASE }),
        1 => d.place_tap(0x28, p(d, 1), if swapped { LEFT_BASE } else { RIGHT_BASE }),
        2 => {
            let level = d.program.feedback_word(p(d, 2));
            let w = &mut d.program.coefficient_words;
            w[0xab] = if swapped { 0 } else { level };
            w[0xaf] = w[0xab];
            w[0xac] = if swapped { level } else { 0 };
            w[0xae] = w[0xac];
            d.push_coefficients_all(&[0xab, 0xaf, 0xac, 0xae]);
        }
        3 => {
            let separate = p(d, 3) == 2;
            let w = &mut d.program.coefficient_words;
            w[0xaa] = if separate { 0 } else { 0x2_00 };
            w[0xad] = if separate { 0x2_00 } else { 0 };
            let a = &mut d.program.address_words;
            a[LEFT_BASE] = if swapped { RIGHT_LINE_START } else { 1 };
            a[RIGHT_BASE] = if swapped { 1 } else { RIGHT_LINE_START };
            d.push_coefficients_all(&[0xaa, 0xad]);
            d.program.push_addresses_all(&[LEFT_BASE, RIGHT_BASE]);
            convert(d, 0);
            convert(d, 1);
            convert(d, 2);
            convert(d, 9);
        }
        4 => {
            d.design_damping(p(d, 4), LIST_DAMP_A);
            d.design_damping(p(d, 4), LIST_DAMP_B);
        }
        9 => {
            let mix = d.program.mix_index();
            let mut wet = MIX_WET[mix] as i32;
            if p(d, 3) < 2 {
                wet >>= 1;
            }
            let w = &mut d.program.coefficient_words;
            w[0xb2] = (MIX_DRY[mix] & 0x3_ff) as u16;
            w[0xb8] = w[0xb2];
            w[0xb4] = (wet & 0x3_ff) as u16;
            w[0xb9] = w[0xb4];
            d.push_coefficients_all(&[0xb2, 0xb4, 0xb8, 0xb9]);
        }
        12 | 13 => d.design_low_equalizer(p(d, 12), p(d, 13), false),
        14 | 15 => d.design_high_equalizer(p(d, 14), p(d, 15)),
        _ => {}
    }
}
