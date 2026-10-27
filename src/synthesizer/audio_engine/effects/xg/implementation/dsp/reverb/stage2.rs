//! STAGE 2 (`03 01`) in the reverb block.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/reverb/stage2.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_reverb::{XGReverb, XGReverbPreset};

pub const TYPE: u16 = 0x03_01;

pub static PRESET: XGReverbPreset = XGReverbPreset {
    base: XGProgramPreset {
        words: &[
            0, 512, 512, 512, 512, 0, 512, 512, 512, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 18427, 0, 1467, 0, 2725, 0, 3983, 0, 5303, 0, 8912, 12305, 15527, 0,
        ],
        defaults: [11, 10, 16, 7, 51, 0, 0, 0, 0, 40, 2, 2, 64, 6, 64, 0],
    },
    gain: 2,
    early_level: 49,
    hpf_gain: -2,
    room: None,
};

/// Equivalent to: class XGStage2
pub fn new(sample_rate: f64, max_buffer_size: usize, variation: bool) -> XGReverb {
    XGReverb::new(TYPE, &PRESET, sample_rate, max_buffer_size, variation)
}
