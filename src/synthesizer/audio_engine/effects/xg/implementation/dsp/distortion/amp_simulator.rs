//! AMP SIMULATOR (`4B 00`): the distortion with four amp types (input filters and cabinet EQ).
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/distortion/amp_simulator.ts

use super::super::xg_dsp_program::{
    XGBiquadShape, XGFilterShape, XGParameterRanges, XGProgram, XGProgramPreset,
};
use super::super::xg_mono_variation_program::{LIST_LOW_A, LIST_LOW_B};
use super::super::xg_variation_program::variation_layout;
use super::xg_distortion::{
    XGDistortionBase, XGDistortionKind, CONNECTION_PARAMETERS, LIST_LOW_PASS, LIST_MID, LIST_SHELF,
};

const PARAMETER_RANGES: XGParameterRanges = &[(1, 0, 3), (2, 34, 60)];

/// A filter of an amp type: shape, frequency index and gain index (and the width for the mid-filter).
type AmpFilter<S> = (S, i32, i32, i32);

use XGBiquadShape::{First, Peaking};
use XGFilterShape::{Gain, HighPass, HighShelf, LowPass, LowShelf};

/// The filters of each amp type: two input filters, the cabinet shelf and the cabinet mid-filter.
#[allow(clippy::type_complexity)]
const AMP_TYPES: [(AmpFilter<XGFilterShape>, AmpFilter<XGFilterShape>, AmpFilter<XGFilterShape>, AmpFilter<XGBiquadShape>); 4] = [
    ((Gain, 0, 70, 0), (Gain, 0, 70, 0), (Gain, 0, 64, 0), (First(Gain), 0, 64, 0)),
    ((HighPass, 25, 70, 0), (LowPass, 48, 70, 0), (LowShelf, 25, 76, 0), (First(HighShelf), 41, 76, 0)),
    ((HighPass, 23, 70, 0), (LowPass, 51, 70, 0), (LowShelf, 26, 69, 0), (Peaking, 34, 58, 19)),
    ((HighPass, 26, 70, 0), (LowPass, 41, 70, 0), (LowShelf, 15, 76, 0), (Peaking, 43, 76, 20)),
];

pub const TYPE: u16 = 0x4b_00;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        512, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 256, 0, 0,
    ],
    defaults: [39, 1, 48, 55, 0, 0, 0, 0, 0, 127, 112, 0, 0, 0, 0, 0],
};

const INIT_ORDER: &[usize] = &[0, 1, 9, 10];

/// Equivalent to: class XGAmpSimulator
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGDistortionBase {
    XGDistortionBase::new(
        TYPE,
        XGDistortionKind::AmpSimulator,
        &PRESET,
        variation_layout(INIT_ORDER, CONNECTION_PARAMETERS, PARAMETER_RANGES),
        sample_rate,
        max_buffer_size,
    )
}

/// Equivalent to: XGAmpSimulator.convert
pub(super) fn convert(d: &mut XGDistortionBase, param: usize) {
    let p = |d: &XGDistortionBase, i: usize| d.program.param(i);
    match param {
        0 | 3 => d.drive(p(d, 0), p(d, 3)),
        1 => {
            let (input_a, input_b, shelf, mid) = AMP_TYPES[p(d, 1) as usize];
            d.design_first_order(input_a.0, input_a.1, input_a.2, LIST_LOW_A, 0);
            d.design_first_order(input_b.0, input_b.1, input_b.2, LIST_LOW_B, 0);
            d.design_first_order(shelf.0, shelf.1, shelf.2, LIST_SHELF, 0);
            d.design_second_order(mid.0, mid.1, mid.2, mid.3, LIST_MID);
            d.convert(2);
        }
        2 => {
            // The low pass gain depends on the amp type
            let gain = match p(d, 1) {
                1 => 0x3e,
                3 => 0x45,
                _ => 0x46,
            };
            d.design_first_order(XGFilterShape::LowPass, p(d, 2), gain, LIST_LOW_PASS, 0);
        }
        9 => d.dry_wet(),
        10 => d.edge(p(d, 10)),
        _ => {}
    }
}
