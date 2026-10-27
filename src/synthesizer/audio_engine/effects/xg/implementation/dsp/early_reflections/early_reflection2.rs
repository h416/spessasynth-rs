//! EARLY REF 2 (`09 01`): early reflections with other defaults.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/early_reflections/early_reflection2.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_early_reflections::{XGEarlyReflectionsBase, ER_RANGES};

pub const TYPE: u16 = 0x09_01;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        256, 256, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0,
        865, 3223, 1801, 0, 0, 0, 0, 0, 2428, 0, 0, 0, 0, 0, 0, 0, 0, 3059, 3141,
    ],
    defaults: [2, 7, 10, 16, 64, 3, 46, 0, 0, 32, 5, 2, 10, 0, 0, 0],
};

/// Equivalent to: class XGEarlyReflection2
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGEarlyReflectionsBase {
    // The first reflection pattern and the density row block of the type
    XGEarlyReflectionsBase::new(TYPE, &PRESET, 0, 1, ER_RANGES, sample_rate, max_buffer_size)
}
