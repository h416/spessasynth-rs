//! GM CHORUS 4 (`41 06`).
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/chorus/gm_chorus4.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_chorus::{XGChorusBase, XGChorusPreset, XGChorusKind};

pub const TYPE: u16 = 0x41_06;

pub static PRESET: XGChorusPreset = XGChorusPreset {
    base: XGProgramPreset {
        words: &[
            0, 512, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 504, 0, 0, 504,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ],
        defaults: [26, 29, 75, 102, 0, 28, 64, 46, 64, 64, 46, 64, 10, 0, 0, 0],
    },
    variation_gain: 63,
};

/// Equivalent to: class XGGMChorus4
pub fn new(sample_rate: f64, max_buffer_size: usize, variation: bool) -> XGChorusBase {
    XGChorusBase::new(TYPE, &PRESET, XGChorusKind::Chorus, sample_rate, max_buffer_size, variation)
}
