//! CELESTE 3 (`42 02`).
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/chorus/celeste3.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_chorus::{XGChorusBase, XGChorusPreset, XGChorusKind};

pub const TYPE: u16 = 0x42_02;

pub static PRESET: XGChorusPreset = XGChorusPreset {
    base: XGProgramPreset {
        words: &[
            0, 512, 0, 683, 0, 341, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 336, 336, 0,
            336, 336, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ],
        defaults: [4, 63, 44, 2, 0, 28, 64, 46, 68, 127, 40, 68, 10, 0, 0, 0],
    },
    variation_gain: 48,
};

/// Equivalent to: class XGCeleste3
pub fn new(sample_rate: f64, max_buffer_size: usize, variation: bool) -> XGChorusBase {
    XGChorusBase::new(TYPE, &PRESET, XGChorusKind::Celeste, sample_rate, max_buffer_size, variation)
}
