//! HALL 2 (`01 01`) in the reverb block.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/reverb/hall2.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_reverb::{XGReverb, XGReverbPreset};

pub const TYPE: u16 = 0x01_01;

pub static PRESET: XGReverbPreset = XGReverbPreset {
    base: XGProgramPreset {
        words: &[
            0, 512, 512, 512, 512, 0, 512, 512, 512, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 18427, 0, 1256, 0, 2233, 0, 2914, 0, 3744, 0, 8315, 12236, 15532, 0,
        ],
        defaults: [25, 10, 28, 6, 46, 0, 0, 0, 0, 40, 13, 3, 74, 7, 64, 0],
    },
    gain: 0,
    early_level: 76,
    hpf_gain: -1,
    room: None,
};

/// Equivalent to: class XGHall2
pub fn new(sample_rate: f64, max_buffer_size: usize, variation: bool) -> XGReverb {
    XGReverb::new(TYPE, &PRESET, sample_rate, max_buffer_size, variation)
}
