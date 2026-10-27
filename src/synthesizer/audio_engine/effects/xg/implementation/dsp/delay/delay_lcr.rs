//! DELAY L,C,R (`05 00`): left, right and center taps with a separate feedback tap.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/delay/delay_lcr.ts

use super::super::xg_dsp_program::{XGParameterRanges, XGProgram, XGProgramPreset};
use super::super::xg_dsp_tables::{MIX_DRY, MIX_WET};
use super::super::xg_stereo_variation_program::XGStereoVariationProgram;
use super::super::xg_variation_program::variation_layout;
use super::delay_tables::DELAY_LEVEL;
use super::xg_delay::{XGDelayBase, XGDelayKind, CONNECTION_PARAMETERS, LEFT_BASE, LIST_DAMP_A};

const PARAMETER_RANGES: XGParameterRanges = &[
    (0, 1, 7150),
    (1, 1, 7150),
    (2, 1, 7150),
    (3, 1, 7150),
    (4, 1, 127),
    (6, 1, 10),
    (9, 1, 127),
    (12, 8, 40),
    (13, 52, 76),
    (14, 28, 58),
    (15, 52, 76),
];

pub const TYPE: u16 = 0x05_00;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        128, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 256,
    ],
    defaults: [3333, 1667, 5000, 5000, 74, 100, 10, 0, 0, 32, 0, 60, 28, 64, 46, 64],
};

const INIT_ORDER: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 12, 14];

/// Equivalent to: class XGDelayLCR
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGDelayBase {
    XGDelayBase::new(
        TYPE,
        XGDelayKind::DelayLCR,
        &PRESET,
        variation_layout(INIT_ORDER, CONNECTION_PARAMETERS, PARAMETER_RANGES),
        sample_rate,
        max_buffer_size,
    )
}

/// Equivalent to: XGDelayLCR.convert
pub(super) fn convert(d: &mut XGDelayBase, param: usize) {
    let p = |d: &XGDelayBase, i: usize| d.program.param(i);
    match param {
        0 => d.place_tap(0x33, p(d, 0), LEFT_BASE),
        1 => d.place_tap(0x2d, p(d, 1), LEFT_BASE),
        2 => d.place_tap(0x38, p(d, 2), LEFT_BASE),
        3 => d.place_tap(0x28, p(d, 3), LEFT_BASE),
        4 => {
            d.program.coefficient_words[0xab] = d.program.feedback_word(p(d, 4));
            d.push_coefficients_all(&[0xab]);
        }
        5 | 9 => {
            let mix = d.program.mix_index();
            let w = &mut d.program.coefficient_words;
            w[0xb2] = (MIX_DRY[mix] & 0x3_ff) as u16;
            w[0xb8] = w[0xb2];
            let mut wet = MIX_WET[mix] as i32;
            if d.program.insertion {
                wet >>= 1;
            }
            let w = &mut d.program.coefficient_words;
            w[0xb5] = (wet & 0x3_ff) as u16;
            w[0xb9] = w[0xb5];
            let level = DELAY_LEVEL[d.program.param(5) as usize] as f64;
            let w = &mut d.program.coefficient_words;
            w[0xb6] = (((wet as f64 * level).trunc() as i32) & 0x3_ff) as u16;
            w[0xbb] = w[0xb6];
            d.push_coefficients_all(&[0xb2, 0xb5, 0xb8, 0xb9, 0xbb, 0xb6]);
        }
        6 => d.design_damping(p(d, 6), LIST_DAMP_A),
        12 | 13 => d.design_low_equalizer(p(d, 12), p(d, 13), false),
        14 | 15 => d.design_high_equalizer(p(d, 14), p(d, 15)),
        _ => {}
    }
}
