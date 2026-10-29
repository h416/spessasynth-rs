//! GM CHORUS 3 (`41 05`).
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/chorus/gm_chorus3.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_chorus::{XGChorusBase, XGChorusPreset, XGChorusKind};

pub const TYPE: u16 = 0x41_05;

pub static PRESET: XGChorusPreset = XGChorusPreset {
    base: XGProgramPreset {
        words: &[
            0, 512, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 504, 0, 0, 504,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ],
        defaults: [9, 34, 69, 105, 0, 28, 64, 46, 66, 64, 46, 64, 10, 0, 0, 0],
    },
    variation_gain: 63,
};

/// Equivalent to: class XGGMChorus3
pub fn new(sample_rate: f64, max_buffer_size: usize, variation: bool) -> XGChorusBase {
    XGChorusBase::new(TYPE, &PRESET, XGChorusKind::Chorus, sample_rate, max_buffer_size, variation)
}
