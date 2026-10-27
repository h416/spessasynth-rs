//! GM PLATE (`04 07`) in the reverb block.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/reverb/gm_plate.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_reverb::{XGReverb, XGReverbPreset};

pub const TYPE: u16 = 0x04_07;

pub static PRESET: XGReverbPreset = XGReverbPreset {
    base: XGProgramPreset {
        words: &[
            0, 512, 512, 512, 512, 0, 512, 512, 512, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 18427, 0, 1256, 0, 2233, 0, 2914, 0, 3744, 0, 8315, 12236, 15532, 0,
        ],
        defaults: [25, 10, 6, 8, 49, 0, 0, 0, 0, 40, 2, 3, 64, 5, 64, 0],
    },
    gain: 3,
    early_level: 61,
    hpf_gain: -2,
    room: None,
};

/// Equivalent to: class XGGMPlate
pub fn new(sample_rate: f64, max_buffer_size: usize, variation: bool) -> XGReverb {
    XGReverb::new(TYPE, &PRESET, sample_rate, max_buffer_size, variation)
}
