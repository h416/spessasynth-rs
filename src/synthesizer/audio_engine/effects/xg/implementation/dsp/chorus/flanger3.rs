//! FLANGER 3 (`43 08`).
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/chorus/flanger3.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_chorus::{XGChorusBase, XGChorusPreset, XGChorusKind};

pub const TYPE: u16 = 0x43_08;

pub static PRESET: XGChorusPreset = XGChorusPreset {
    base: XGProgramPreset {
        words: &[
            0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 312, 0, 0, 312, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ],
        defaults: [4, 109, 109, 2, 0, 28, 64, 46, 64, 127, 40, 64, 10, 4, 1, 0],
    },
    variation_gain: 45,
};

/// Equivalent to: class XGFlanger3
pub fn new(sample_rate: f64, max_buffer_size: usize, variation: bool) -> XGChorusBase {
    XGChorusBase::new(TYPE, &PRESET, XGChorusKind::Flanger, sample_rate, max_buffer_size, variation)
}
