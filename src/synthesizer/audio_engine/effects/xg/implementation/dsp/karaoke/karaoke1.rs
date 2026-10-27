//! KARAOKE 1 (`14 00`): a filtered echo for vocals.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/karaoke/karaoke1.ts

use super::super::xg_dsp_program::XGProgramPreset;
use super::xg_karaoke::XGKaraokeBase;

pub const TYPE: u16 = 0x14_00;

pub static PRESET: XGProgramPreset = XGProgramPreset {
    words: &[
        181, 181, 0, 128, 0, 0, 128, 0, 0, 0, 0, 0, 128, 0, 0, 0, 128, 0, 0, 0, 128, 0, 0, 128, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 0, 0, 0, 0, 0, 0,
        0, 128, 0, 0, 512, 256, 256, 0, 0, 512, 0, 512, 0, 0, 0, 128, 256, 256, 0, 256, 256, 0,
        512, 0, 512, 4616, 0, 0, 4615, 0, 1467, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4624, 4618, 3551, 0,
        4617, 4620, 4619, 4622, 2725, 4621, 4623,
    ],
    defaults: [63, 97, 0, 48, 0, 0, 0, 0, 0, 64, 2, 0, 0, 0, 0, 0],
};

/// Equivalent to: class XGKaraoke1
pub fn new(sample_rate: f64, max_buffer_size: usize) -> XGKaraokeBase {
    XGKaraokeBase::new(TYPE, &PRESET, sample_rate, max_buffer_size)
}
