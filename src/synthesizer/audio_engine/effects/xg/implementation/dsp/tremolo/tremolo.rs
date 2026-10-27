//! TREMOLO (`46 00`): amplitude and pitch modulation.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/tremolo/tremolo.ts

use super::super::xg_dsp_program::{XGParameterRanges, XGProgram, XGProgramPreset};
use super::super::xg_stereo_variation_program::XGStereoVariationProgram;
use super::super::xg_variation_program::variation_layout;
use super::xg_tremolo::{XGTremoloBase, XGTremoloKind, LIST_SHAPE_LEFT, LIST_SHAPE_RIGHT};

const PARAMETER_RANGES: XGParameterRanges =
    &[(5, 8, 40), (6, 52, 76), (7, 28, 58), (8, 52, 76), (13, 4, 124), (14, 0, 1)];

pub const TYPE: u16 = 0x46_00;
const VARIATION_GAIN: i32 = 56;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        0, 0, 0, 0, 0, 0, 0, 1008, 0, 512, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1008, 0, 512,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 2,
        3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 256,
    ],
    defaults: [83, 56, 0, 0, 0, 28, 64, 46, 64, 127, 40, 64, 10, 64, 0, 0],
};

const INIT_ORDER: &[usize] = &[0, 1, 2, 13, 14];
const CONNECTION_PARAMETERS: &[usize] = &[14];

/// Equivalent to: class XGTremolo
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGTremoloBase {
    XGTremoloBase::new(
        TYPE,
        XGTremoloKind::Tremolo,
        VARIATION_GAIN,
        &PRESET,
        variation_layout(INIT_ORDER, CONNECTION_PARAMETERS, PARAMETER_RANGES),
        sample_rate,
        max_buffer_size,
    )
}

/// Equivalent to: XGTremolo.convert
pub(super) fn convert(d: &mut XGTremoloBase, param: usize) {
    let p = |d: &XGTremoloBase, i: usize| d.program.param(i);
    match param {
        0 => {
            d.lfo_rate(p(d, 0));
            d.shape_lfo(LIST_SHAPE_LEFT);
            d.shape_lfo(LIST_SHAPE_RIGHT);
        }
        1 => {
            let word = d.amplitude_depth(p(d, 1));
            d.program.coefficient_words[0x83] = word;
            d.program.coefficient_words[0x94] = word;
            d.push_coefficients_all(&[0x83, 0x94]);
        }
        2 => {
            let word = d.pitch_depth(p(d, 2) >> 2);
            d.program.coefficient_words[0x75] = word;
            d.program.coefficient_words[0x85] = word;
            d.push_coefficients_all(&[0x75, 0x85]);
        }
        5 | 6 => {
            // The +3 dB low shelf applies in the system mode or with the stereo input
            let boosted = !d.program.insertion || p(d, 14) != 0;
            d.design_low_equalizer(p(d, 5), p(d, 6), boosted);
        }
        7 | 8 => d.design_high_equalizer(p(d, 7), p(d, 8)),
        13 => {
            d.program.coefficient_words[0x79] = d.program.phase_word(p(d, 13));
            d.push_coefficients_all(&[0x79]);
        }
        14 => {
            if p(d, 14) == 0 {
                let out = d.output_word(if d.program.insertion { 0x1_6a } else { 0x2_00 });
                let a0x3a = d.program.signed_address(0x3a);
                let w = &mut d.program.coefficient_words;
                w[0xaa] = 0x2_00;
                w[0xb4] = out;
                w[0xb9] = out;
                let a = &mut d.program.address_words;
                a[0x3b] = 0;
                a[0x2d] = (a0x3a + 1) as u16;
                a[0x2e] = (a0x3a + 2) as u16;
            } else {
                // Stereo: the right line starts at 1024
                let out = d.output_word(0x2_00);
                let w = &mut d.program.coefficient_words;
                w[0xaa] = 0;
                w[0xb4] = out;
                w[0xb9] = out;
                let a = &mut d.program.address_words;
                a[0x3b] = 0x4_00;
                a[0x2d] = 0x4_01;
                a[0x2e] = 0x4_02;
            }
            d.push_coefficients_all(&[0xaa, 0xb4, 0xb9]);
            d.program.push_addresses_all(&[0x3b, 0x2d, 0x2e]);
            d.convert(5);
            d.convert(7);
        }
        _ => {}
    }
}
