//! AUTO PAN (`47 00`): moves the sound between the sides (and front/back with pitch modulation).
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/tremolo/auto_pan.ts

use super::super::xg_dsp_program::{XGParameterRanges, XGProgram, XGProgramPreset};
use super::super::xg_stereo_variation_program::XGStereoVariationProgram;
use super::super::xg_variation_program::variation_layout;
use super::xg_tremolo::{XGTremoloBase, XGTremoloKind, LIST_SHAPE_LEFT, LIST_SHAPE_RIGHT};

const PARAMETER_RANGES: XGParameterRanges =
    &[(3, 0, 5), (5, 8, 40), (6, 52, 76), (7, 28, 58), (8, 52, 76)];

/// LFO phases (left, right) per pan direction: L<->R, L->R, L<-R, L turn, R turn.
const DIRECTION_PHASES: [[i32; 2]; 5] = [[0x40, 0x7c], [0x54, 0x40], [0x40, 0x54], [0x5e, 0x40], [0x40, 0x5e]];

pub const TYPE: u16 = 0x47_00;
const VARIATION_GAIN: i32 = 45;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        0, 0, 0, 0, 0, 0, 0, 1008, 0, 512, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1008, 0, 512,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 256, 0, 0, 0, 0, 256, 0, 0, 0, 0, 0, 0, 2,
        3, 0, 0, 0, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 256,
    ],
    defaults: [76, 80, 32, 5, 0, 28, 64, 46, 64, 127, 40, 64, 10, 0, 0, 0],
};

const INIT_ORDER: &[usize] = &[0, 3, 5, 7];
const CONNECTION_PARAMETERS: &[usize] = &[9];

/// Equivalent to: class XGAutoPan
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGTremoloBase {
    XGTremoloBase::new(
        TYPE,
        XGTremoloKind::AutoPan,
        VARIATION_GAIN,
        &PRESET,
        variation_layout(INIT_ORDER, CONNECTION_PARAMETERS, PARAMETER_RANGES),
        sample_rate,
        max_buffer_size,
    )
}

/// Equivalent to: XGAutoPan.convert
pub(super) fn convert(d: &mut XGTremoloBase, param: usize) {
    let p = |d: &XGTremoloBase, i: usize| d.program.param(i);
    match param {
        0 => {
            d.lfo_rate(p(d, 0));
            if p(d, 3) <= 4 {
                d.shape_lfo(LIST_SHAPE_LEFT);
                d.shape_lfo(LIST_SHAPE_RIGHT);
            }
        }
        1 => {
            // The L/R direction inverts the depth
            let word = d.amplitude_depth(if p(d, 3) < 5 { p(d, 1) } else { -p(d, 1) });
            d.program.coefficient_words[0x83] = word;
            d.program.coefficient_words[0x94] = word;
            d.push_coefficients_all(&[0x83, 0x94]);
        }
        2 => {
            // Front/back depth only for the turning directions
            let word = d.pitch_depth(if p(d, 3) == 3 || p(d, 3) == 4 { p(d, 2) >> 2 } else { 0 });
            d.program.coefficient_words[0x75] = word;
            d.program.coefficient_words[0x85] = word;
            d.push_coefficients_all(&[0x75, 0x85]);
        }
        3 => {
            if p(d, 3) > 4 {
                // L/R: a hard square-like switch
                let left = d.program.phase_word(0x40);
                let right = d.program.phase_word(0x7c);
                let w = &mut d.program.coefficient_words;
                w[0x6e] = left;
                w[0x79] = right;
                w[0x71] = 0x3_ff;
                w[0x72] = 0x3_ff;
                w[0x7b] = 0x3_ff;
                w[0x7c] = 0x3_ff;
                w[0x6f] = 0x3_40;
                w[0x7a] = 0x3_40;
                w[0x73] = 0x2_00;
                w[0x7d] = 0x2_00;
                d.push_coefficients_all(&[0x6e, 0x79]);
                d.push_coefficients_all(LIST_SHAPE_LEFT);
                d.push_coefficients_all(LIST_SHAPE_RIGHT);
            } else {
                let [left, right] = DIRECTION_PHASES[p(d, 3) as usize];
                let left = d.program.phase_word(left);
                let right = d.program.phase_word(right);
                d.program.coefficient_words[0x6e] = left;
                d.program.coefficient_words[0x79] = right;
                d.push_coefficients_all(&[0x6e, 0x79]);
                d.shape_lfo(LIST_SHAPE_LEFT);
                d.shape_lfo(LIST_SHAPE_RIGHT);
            }
            d.convert(1);
            d.convert(2);
        }
        5 | 6 => d.design_low_equalizer(p(d, 5), p(d, 6), false),
        7 | 8 => d.design_high_equalizer(p(d, 7), p(d, 8)),
        9 => {
            // Not a parameter in the original: the output level follows the connection
            let out = d.output_word(if d.program.insertion { 0x1_6a } else { 0x2_d4 });
            d.program.coefficient_words[0xb4] = out;
            d.program.coefficient_words[0xb9] = out;
            d.push_coefficients_all(&[0xb4, 0xb9]);
        }
        _ => {}
    }
}
