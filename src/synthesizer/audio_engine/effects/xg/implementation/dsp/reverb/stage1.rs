//! STAGE 1 (`03 00`) in the reverb block.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/reverb/stage1.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_reverb::{XGReverb, XGReverbPreset};

pub const TYPE: u16 = 0x03_00;

pub static PRESET: XGReverbPreset = XGReverbPreset {
    base: XGProgramPreset {
        words: &[
            0, 512, 512, 512, 512, 0, 512, 512, 512, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 18427, 0, 1467, 0, 2725, 0, 3983, 0, 5303, 0, 8912, 12305, 15527, 0,
        ],
        defaults: [19, 10, 16, 7, 54, 0, 0, 0, 0, 40, 0, 3, 64, 6, 64, 0],
    },
    gain: 2,
    early_level: 49,
    hpf_gain: -1,
    room: None,
};

/// Equivalent to: class XGStage1
pub fn new(sample_rate: f64, max_buffer_size: usize, variation: bool) -> XGReverb {
    XGReverb::new(TYPE, &PRESET, sample_rate, max_buffer_size, variation)
}
