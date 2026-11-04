//! Bass Multi (`04 05`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/bass_multi.ts

use super::guitar_multi::MultiDriveOptions;
use super::guitar_multis::GSDriveEqMulti;
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::bass_amp_simulator::BassAmpSimulator;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::drive::{BASS_DISTORTION, BASS_OVERDRIVE};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x48, 0x64, 0x4b, 0x01, 0x00, 0x30, 0x00, 0x00, 0x01, 0x42, 0x48, 0x01,
    0x44, 0x40, 0x00, 0x05, 0x14, 0x66, 0x40, 0x4c,
];
const LIMITS: &[(u8, u8)] = &[(3, 1), (4, 1), (6, 2), (7, 1), (8, 1), (11, 4), (14, 1)];

/// Bass Multi (`04 05`): `L+R -> compressor -> bass overdrive/distortion -> bass amp simulator -> 3-band EQ ->
/// chorus/flanger`.
/// Parameters: 1-4 compressor sustain, attack, level and switch, 5 drive type (OD, DS), 6 drive, 7 amp type (0-2),
/// 8 amp switch, 9 drive switch, 10 low gain, 11-13 mid frequency, Q and gain, 14 high gain, 15 chorus or flanger,
/// 16 rate (Rate2 column), 17 depth, 18 flanger feedback, 19 chorus level, 20 level. EFX control source 1 moves the
/// drive, source 2 the chorus level.
pub struct GSBassMulti;

impl GSBassMulti {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSDriveEqMulti {
        // The chain goes into the chorus/flanger at full level, and its compressor looks 50 samples ahead, not 32
        let mut effect = GSDriveEqMulti::new(
            sample_rate,
            memory,
            0x04_05,
            DEFAULTS,
            LIMITS,
            MultiDriveOptions {
                overdrive: &BASS_OVERDRIVE,
                distortion: &BASS_DISTORTION,
                amp: Box::new(BassAmpSimulator::new(sample_rate)),
                bypass_gain: 1.0,
                shelves: false,
            },
            1.0,
            50.0 / 32_000.0,
        );
        // The chorus/flanger takes nothing below 200 Hz (the hardware's one-pole, pole 123/128)
        effect.modulation.set_high_pass(202.88, 252.0 / 251.0);
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}
