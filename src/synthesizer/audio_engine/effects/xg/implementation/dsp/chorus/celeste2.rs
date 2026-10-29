//! CELESTE 2 (`42 01`).
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/chorus/celeste2.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_chorus::{XGChorusBase, XGChorusPreset, XGChorusKind};

pub const TYPE: u16 = 0x42_01;

pub static PRESET: XGChorusPreset = XGChorusPreset {
    base: XGProgramPreset {
        words: &[
            0, 512, 0, 683, 0, 341, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 336, 336, 0,
            336, 336, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ],
        defaults: [28, 18, 90, 2, 0, 28, 62, 42, 60, 84, 40, 68, 10, 0, 0, 0],
    },
    variation_gain: 48,
};

/// Equivalent to: class XGCeleste2
pub fn new(sample_rate: f64, max_buffer_size: usize, variation: bool) -> XGChorusBase {
    XGChorusBase::new(TYPE, &PRESET, XGChorusKind::Celeste, sample_rate, max_buffer_size, variation)
}
