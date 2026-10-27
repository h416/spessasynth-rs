//! TUNNEL (`11 00`) in the reverb block.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/reverb/tunnel.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_reverb::{XGReverb, XGReverbPreset, XGRoomReverbPreset, XGRoomWall};

pub const TYPE: u16 = 0x11_00;

pub static PRESET: XGReverbPreset = XGReverbPreset {
    base: XGProgramPreset {
        words: &[
            0, 512, 512, 512, 512, 0, 512, 512, 512, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 18427, 0, 1256, 0, 2233, 0, 2914, 0, 3744, 0, 5472, 8480, 12784, 0,
        ],
        defaults: [48, 6, 19, 0, 44, 33, 52, 70, 16, 40, 20, 3, 64, 7, 64, 0],
    },
    gain: 7,
    early_level: 48,
    hpf_gain: 0,
    room: Some(XGRoomReverbPreset {
        room_select: 1,
        room_step: 4,
        wall: XGRoomWall::Depth,
    }),
};

/// Equivalent to: class XGTunnel
pub fn new(sample_rate: f64, max_buffer_size: usize, variation: bool) -> XGReverb {
    XGReverb::new(TYPE, &PRESET, sample_rate, max_buffer_size, variation)
}
