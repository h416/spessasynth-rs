//! PHASER 1 (`48 00`): a mono-input phaser with 6 to 10 stages.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/phaser/phaser1.ts

use super::super::xg_dsp_program::{to_u16, XGParameterRanges, XGProgram, XGProgramPreset};
use super::super::xg_dsp_tables::{PAN_LEFT, PAN_RIGHT};
use super::super::xg_variation_program::variation_layout;
use super::xg_phaser::{XGPhaserBase, XGPhaserKind};

const PARAMETER_RANGES: XGParameterRanges =
    &[(3, 1, 127), (5, 8, 40), (6, 52, 76), (7, 28, 58), (8, 52, 76), (10, 6, 10)];

/// The converter index of the input mix level, which depends on the connection.
const INPUT_MIX: usize = 16;

/// Stage words (0x7b-0x7e, 0x80) and whether the left output takes the fourth stage of the right chain,
/// per stage parameter (6 and lower, 7, 8, 9, 10).
const STAGES: [([u16; 5], bool); 5] = [
    ([0x2_00, 0, 0, 0x2_00, 0], false),
    ([0, 0x2_00, 0, 0x2_00, 0], false),
    ([0, 0, 0x2_00, 0x2_00, 0], false),
    ([0, 0x2_00, 0, 0, 0x2_00], true),
    ([0, 0, 0x2_00, 0, 0x2_00], true),
];

pub const TYPE: u16 = 0x48_00;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 256, 256, 0, 0, 0,
        512, 512, 0, 0, 512, 512, 0, 0, 512, 512, 0, 0, 512, 512, 0, 0, 512, 512, 0, 0, 0, 0, 0,
        512, 0, 0, 0, 0, 0, 0, 0, 0, 512, 512, 0, 0, 512, 512, 0, 0, 512, 512, 0, 0, 512, 512, 0,
        0, 512, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 256,
    ],
    defaults: [8, 111, 74, 104, 0, 28, 64, 46, 64, 64, 6, 1, 64, 0, 0, 0],
};

const INIT_ORDER: &[usize] = &[0, 1, 3, 5, 7, 9, 10, INPUT_MIX];
const CONNECTION_PARAMETERS: &[usize] = &[9, INPUT_MIX];

/// Equivalent to: class XGPhaser1
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGPhaserBase {
    XGPhaserBase::new(
        TYPE,
        XGPhaserKind::Phaser1,
        &PRESET,
        variation_layout(INIT_ORDER, CONNECTION_PARAMETERS, PARAMETER_RANGES),
        sample_rate,
        max_buffer_size,
    )
}

/// Equivalent to: XGPhaser1.convert
pub(super) fn convert(d: &mut XGPhaserBase, param: usize) {
    let p = |d: &XGPhaserBase, i: usize| d.program.param(i);
    let mix = d.program.mix_index();
    match param {
        0 => d.sweep_rate(),
        1 | 2 => d.sweep_depth(0x2_00),
        3 => {
            d.program.coefficient_words[0x87] = d.program.feedback_word(p(d, 3));
            d.push_coefficients_all(&[0x87]);
        }
        5 | 6 => d.low_equalizer(),
        7 | 8 => d.high_equalizer(),
        9 => {
            let w = &mut d.program.coefficient_words;
            w[0xa2] = to_u16(PAN_LEFT[mix] as i32);
            w[0xa6] = w[0xa2];
            w[0xa7] = to_u16(PAN_RIGHT[mix] as i32);
            d.push_coefficients_all(&[0xa2]);
            d.push_coefficients_all(&[0xa6, 0xa7]);
            if p(d, 10) < 9 {
                d.program.coefficient_words[0xa4] = to_u16(PAN_RIGHT[mix] as i32);
                d.push_coefficients_all(&[0xa4]);
            } else {
                d.program.coefficient_words[0xa5] = to_u16(PAN_RIGHT[mix] as i32);
                d.push_coefficients_all(&[0xa5]);
            }
        }
        10 => {
            let (words, fourth) = STAGES[(0.max(10.min(p(d, 10)) - 6)) as usize];
            let w = &mut d.program.coefficient_words;
            w[0x7b] = words[0];
            w[0x7c] = words[1];
            w[0x7d] = words[2];
            w[0x7e] = words[3];
            w[0x80] = words[4];
            // The left output takes the second or the fourth stage of the right chain
            let right = to_u16(PAN_RIGHT[mix] as i32);
            w[0xa4] = if fourth { 0 } else { right };
            w[0xa5] = if fourth { right } else { 0 };
            d.push_coefficients_all(&[0x7b, 0x7c, 0x7d, 0x7e]);
            d.push_coefficients_all(&[0x80]);
            d.push_coefficients_all(&[0xa4, 0xa5]);
        }
        INPUT_MIX => {
            // L+R in the system mode, their average in the insertion mode
            let word = if d.program.insertion { 0x1_00 } else { 0x2_00 };
            d.program.coefficient_words[0x84] = word;
            d.program.coefficient_words[0x85] = word;
            d.push_coefficients_all(&[0x84, 0x85]);
        }
        _ => {}
    }
}
