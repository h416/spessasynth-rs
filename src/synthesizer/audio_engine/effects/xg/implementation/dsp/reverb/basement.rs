//! BASEMENT (`13 00`) in the reverb block.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/reverb/basement.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_reverb::{XGReverb, XGReverbPreset, XGRoomReverbPreset, XGRoomWall};

pub const TYPE: u16 = 0x13_00;

pub static PRESET: XGReverbPreset = XGReverbPreset {
    base: XGProgramPreset {
        words: &[
            0, 512, 512, 512, 512, 0, 512, 512, 512, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 18427, 0, 1256, 0, 2233, 0, 2914, 0, 3744, 0, 5472, 8480, 12784, 0,
        ],
        defaults: [3, 6, 3, 0, 34, 26, 29, 59, 15, 40, 32, 3, 64, 8, 64, 0],
    },
    gain: 7,
    early_level: 24,
    hpf_gain: 0,
    room: Some(XGRoomReverbPreset {
        room_select: 3,
        room_step: 12,
        wall: XGRoomWall::Height,
    }),
};

/// Equivalent to: class XGBasement
pub fn new(sample_rate: f64, max_buffer_size: usize, variation: bool) -> XGReverb {
    XGReverb::new(TYPE, &PRESET, sample_rate, max_buffer_size, variation)
}
