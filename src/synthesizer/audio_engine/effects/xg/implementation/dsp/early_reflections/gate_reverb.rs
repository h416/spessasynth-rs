//! GATE REVERB (`0A 00`): a dense burst of reflections that stops abruptly.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/early_reflections/gate_reverb.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_early_reflections::{XGEarlyReflectionsBase, GATE_RANGES};

pub const TYPE: u16 = 0x0a_00;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        256, 256, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0,
        865, 3223, 1801, 0, 0, 0, 0, 0, 2428, 0, 0, 0, 0, 0, 0, 0, 0, 3059, 3141,
    ],
    defaults: [0, 15, 6, 2, 64, 0, 44, 0, 0, 32, 4, 3, 10, 0, 0, 0],
};

/// Equivalent to: class XGGateReverb
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGEarlyReflectionsBase {
    // The first reflection pattern and the density row block of the type
    XGEarlyReflectionsBase::new(TYPE, &PRESET, 6, 2, GATE_RANGES, sample_rate, max_buffer_size)
}
