//! Overdrive and Distortion -> Chorus, Flanger, Delay (`02 00`-`02 05`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/drive_combinations.ts

use super::drive_combination::GSDriveCombination;
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::drive::{GUITAR_DISTORTION_HIGH_PASS, GUITAR_OVERDRIVE};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::enums::combination_stages;

#[rustfmt::skip]
const OVERDRIVE_CHORUS_DEFAULTS: [u8; 20] = [
    0x30, 0x40, 0x01, 0x00, 0x00, 0x0a, 0x08, 0x48, 0x00, 0x40, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x50,
];

/// Overdrive -> Chorus (`02 00`).
pub struct GSOverdriveChorus;

impl GSOverdriveChorus {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSDriveCombination {
        let mut effect = GSDriveCombination::new(
            sample_rate,
            memory,
            0x02_00,
            OVERDRIVE_CHORUS_DEFAULTS,
            &GUITAR_OVERDRIVE,
            combination_stages::CHORUS,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const OVERDRIVE_FLANGER_DEFAULTS: [u8; 20] = [
    0x30, 0x40, 0x01, 0x00, 0x00, 0x10, 0x0b, 0x28, 0x68, 0x20, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x50,
];

/// Overdrive -> Flanger (`02 01`).
pub struct GSOverdriveFlanger;

impl GSOverdriveFlanger {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSDriveCombination {
        let mut effect = GSDriveCombination::new(
            sample_rate,
            memory,
            0x02_01,
            OVERDRIVE_FLANGER_DEFAULTS,
            &GUITAR_OVERDRIVE,
            combination_stages::FLANGER,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const OVERDRIVE_DELAY_DEFAULTS: [u8; 20] = [
    0x30, 0x40, 0x01, 0x00, 0x00, 0x6f, 0x50, 0x7f, 0x00, 0x30, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x50,
];

/// Overdrive -> Delay (`02 02`).
pub struct GSOverdriveDelay;

impl GSOverdriveDelay {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSDriveCombination {
        let mut effect = GSDriveCombination::new(
            sample_rate,
            memory,
            0x02_02,
            OVERDRIVE_DELAY_DEFAULTS,
            &GUITAR_OVERDRIVE,
            combination_stages::DELAY,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const DISTORTION_CHORUS_DEFAULTS: [u8; 20] = [
    0x30, 0x40, 0x01, 0x00, 0x00, 0x0a, 0x08, 0x48, 0x00, 0x40, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x48,
];

/// Distortion -> Chorus (`02 03`).
pub struct GSDistortionChorus;

impl GSDistortionChorus {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSDriveCombination {
        let mut effect = GSDriveCombination::new(
            sample_rate,
            memory,
            0x02_03,
            DISTORTION_CHORUS_DEFAULTS,
            &GUITAR_DISTORTION_HIGH_PASS,
            combination_stages::CHORUS,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const DISTORTION_FLANGER_DEFAULTS: [u8; 20] = [
    0x30, 0x40, 0x01, 0x00, 0x00, 0x0b, 0x0b, 0x18, 0x68, 0x20, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x48,
];

/// Distortion -> Flanger (`02 04`).
pub struct GSDistortionFlanger;

impl GSDistortionFlanger {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSDriveCombination {
        let mut effect = GSDriveCombination::new(
            sample_rate,
            memory,
            0x02_04,
            DISTORTION_FLANGER_DEFAULTS,
            &GUITAR_DISTORTION_HIGH_PASS,
            combination_stages::FLANGER,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const DISTORTION_DELAY_DEFAULTS: [u8; 20] = [
    0x30, 0x40, 0x01, 0x00, 0x00, 0x6f, 0x50, 0x7f, 0x00, 0x30, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x48,
];

/// Distortion -> Delay (`02 05`).
pub struct GSDistortionDelay;

impl GSDistortionDelay {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSDriveCombination {
        let mut effect = GSDriveCombination::new(
            sample_rate,
            memory,
            0x02_05,
            DISTORTION_DELAY_DEFAULTS,
            &GUITAR_DISTORTION_HIGH_PASS,
            combination_stages::DELAY,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}
