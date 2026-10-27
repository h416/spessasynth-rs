//! OVER DRIVE (`4A 00`): the distortion with softer defaults and a narrower input band.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/distortion/overdrive.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::distortion::new_with;
use super::xg_distortion::XGDistortionBase;

pub const TYPE: u16 = 0x4a_00;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        512, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 256, 0, 0,
    ],
    defaults: [29, 24, 68, 45, 55, 0, 41, 72, 10, 127, 104, 0, 0, 0, 0, 0],
};

/// Equivalent to: class XGOverdrive
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGDistortionBase {
    // The input high pass and low pass frequency indices
    new_with(TYPE, &PRESET, 8, 0x2c, sample_rate, max_buffer_size)
}
