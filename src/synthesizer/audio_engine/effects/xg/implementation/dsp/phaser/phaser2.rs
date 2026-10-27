//! PHASER 2 (`48 08`): a stereo phaser with 3 to 5 stages and an LFO phase difference between the sides.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/phaser/phaser2.ts

use super::super::xg_dsp_program::{to_u16, XGParameterRanges, XGProgram, XGProgramPreset};
use super::super::xg_dsp_tables::{PAN_LEFT, PAN_RIGHT};
use super::super::xg_variation_program::variation_layout;
use super::xg_phaser::{XGPhaserBase, XGPhaserKind};

const PARAMETER_RANGES: XGParameterRanges =
    &[(3, 1, 127), (5, 8, 40), (6, 52, 76), (7, 28, 58), (8, 52, 76), (10, 3, 5), (12, 4, 124)];

pub const TYPE: u16 = 0x48_08;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0,
        512, 512, 0, 0, 512, 512, 0, 0, 512, 512, 0, 0, 512, 512, 0, 0, 512, 512, 0, 0, 0, 0, 512,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 512, 0, 0, 512, 512, 0, 0, 512, 512, 0, 0, 512, 512, 0, 0,
        512, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        256,
    ],
    defaults: [8, 111, 74, 108, 0, 28, 64, 46, 64, 64, 5, 1, 4, 0, 0, 0],
};

const INIT_ORDER: &[usize] = &[0, 1, 3, 5, 7, 9, 10, 12];
const CONNECTION_PARAMETERS: &[usize] = &[9];

/// Equivalent to: class XGPhaser2
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGPhaserBase {
    XGPhaserBase::new(
        TYPE,
        XGPhaserKind::Phaser2,
        &PRESET,
        variation_layout(INIT_ORDER, CONNECTION_PARAMETERS, PARAMETER_RANGES),
        sample_rate,
        max_buffer_size,
    )
}

/// Equivalent to: XGPhaser2.convert
pub(super) fn convert(d: &mut XGPhaserBase, param: usize) {
    let p = |d: &XGPhaserBase, i: usize| d.program.param(i);
    match param {
        0 => d.sweep_rate(),
        // The right side sweeps a slightly narrower range
        1 | 2 => d.sweep_depth(0x1_e0),
        3 => {
            let word = d.program.feedback_word(p(d, 3));
            d.program.coefficient_words[0x86] = word;
            d.program.coefficient_words[0xa1] = word;
            d.push_coefficients_all(&[0x86, 0xa1]);
        }
        5 | 6 => d.low_equalizer(),
        7 | 8 => d.high_equalizer(),
        9 => {
            let mix = d.program.mix_index();
            let w = &mut d.program.coefficient_words;
            w[0xa2] = to_u16(PAN_LEFT[mix] as i32);
            w[0xa6] = w[0xa2];
            w[0xa3] = to_u16(PAN_RIGHT[mix] as i32);
            w[0xa7] = w[0xa3];
            d.push_coefficients_all(&[0xa2, 0xa3]);
            d.push_coefficients_all(&[0xa6, 0xa7]);
        }
        10 => {
            // Both chains take the third, fourth or fifth stage
            let p10 = p(d, 10);
            let stage = if p10 < 4 { 0 } else if p10 == 4 { 1 } else { 2 };
            let on = |s: i32| if stage == s { 0x2_00 } else { 0 };
            let w = &mut d.program.coefficient_words;
            w[0x7b] = on(0);
            w[0x7e] = on(0);
            w[0x7c] = on(1);
            w[0x7f] = on(1);
            w[0x7d] = on(2);
            w[0x80] = on(2);
            d.push_coefficients_all(&[0x7b, 0x7c, 0x7d, 0x7e, 0x7f, 0x80]);
        }
        12 => {
            // LFO phase difference (64 is none)
            d.program.coefficient_words[0x9c] = d.program.phase_word(p(d, 12));
            d.push_coefficients_all(&[0x9c]);
        }
        _ => {}
    }
}
