//! REVERSE GATE (`0B 00`): the gate reverb with rising reflections.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/early_reflections/reverse_gate.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_early_reflections::{XGEarlyReflectionsBase, GATE_RANGES};

pub const TYPE: u16 = 0x0b_00;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        256, 256, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0,
        865, 3223, 1801, 0, 0, 0, 0, 0, 2428, 0, 0, 0, 0, 0, 0, 0, 0, 3059, 3141,
    ],
    defaults: [1, 19, 8, 3, 64, 0, 47, 0, 0, 32, 6, 3, 10, 0, 0, 0],
};

/// Equivalent to: class XGReverseGate
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGEarlyReflectionsBase {
    // The first reflection pattern and the density row block of the type
    XGEarlyReflectionsBase::new(TYPE, &PRESET, 8, 3, GATE_RANGES, sample_rate, max_buffer_size)
}
