//! EARLY REF 1 (`09 00`): early reflections without the reverb tail.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/early_reflections/early_reflection1.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_early_reflections::{XGEarlyReflectionsBase, ER_RANGES};

pub const TYPE: u16 = 0x09_00;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        256, 256, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0,
        865, 3223, 1801, 0, 0, 0, 0, 0, 2428, 0, 0, 0, 0, 0, 0, 0, 0, 3059, 3141,
    ],
    defaults: [0, 19, 5, 16, 64, 0, 46, 0, 0, 32, 5, 0, 10, 0, 0, 0],
};

/// Equivalent to: class XGEarlyReflection1
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGEarlyReflectionsBase {
    // The first reflection pattern and the density row block of the type
    XGEarlyReflectionsBase::new(TYPE, &PRESET, 0, 0, ER_RANGES, sample_rate, max_buffer_size)
}
