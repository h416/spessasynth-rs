//! The parallel combinations OD1 / OD2, OD / Rotary, OD / Phaser, OD / AutoWah, PH / Rotary and PH / AutoWah.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/parallel_combinations.ts

use super::parallel_combination::{GSParallelCombination, ParallelStageSpec};
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::enums::parallel_stages;

#[rustfmt::skip]
const OVERDRIVE_PARALLEL_OVERDRIVE_DEFAULTS: [u8; 20] = [
    0x00, 0x30, 0x01, 0x01, 0x00, 0x01, 0x4c, 0x03, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x60, 0x7f, 0x54, 0x7f,
];

/// OD1 / OD2 (`11 03`): a drive with the amp simulator on each input.
/// Parameters: 1 OD1 type, 2 OD1 drive, 3 OD1 amp type, 4 OD1 amp switch, 6-9 the same for OD2, 16 OD1 pan,
/// 17 OD1 level, 18 OD2 pan, 19 OD2 level, 20 level. EFX control source 1 modulates OD1's drive, source 2 OD2's.
pub struct GSOverdriveParallelOverdrive;

impl GSOverdriveParallelOverdrive {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSParallelCombination {
        let mut effect = GSParallelCombination::new(
            sample_rate,
            memory,
            0x11_03,
            OVERDRIVE_PARALLEL_OVERDRIVE_DEFAULTS,
            &[(0, 1), (2, 3), (3, 1), (5, 1), (7, 3), (8, 1)],
            ParallelStageSpec { r#type: parallel_stages::DRIVE, input: None, output: None },
            ParallelStageSpec { r#type: parallel_stages::DRIVE, input: None, output: None },
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const OVERDRIVE_PARALLEL_ROTARY_DEFAULTS: [u8; 20] = [
    0x00, 0x30, 0x01, 0x01, 0x00, 0x06, 0x71, 0x18, 0x7f, 0x11, 0x78, 0x58,
    0x40, 0x60, 0x00, 0x00, 0x60, 0x7f, 0x7f, 0x7f,
];

/// OD / Rotary (`11 04`): the drive on the left input, the rotary on the right input.
/// Parameters: 1 type, 2 drive, 3 amp type, 4 amp switch, 6 low slow, 7 low fast, 8 low accel, 9 low level, 10 high
/// slow, 11 high fast, 12 high accel, 13 high level, 14 separation, 15 speed, 16 drive pan, 17 drive level, 18
/// rotary pan, 19 rotary level, 20 level. EFX control source 1 modulates the drive, source 2 the speed.
pub struct GSOverdriveParallelRotary;

impl GSOverdriveParallelRotary {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSParallelCombination {
        let mut effect = GSParallelCombination::new(
            sample_rate,
            memory,
            0x11_04,
            OVERDRIVE_PARALLEL_ROTARY_DEFAULTS,
            &[(0, 1), (2, 3), (3, 1)],
            ParallelStageSpec { r#type: parallel_stages::DRIVE, input: None, output: None },
            ParallelStageSpec { r#type: parallel_stages::ROTARY, input: None, output: None },
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const OVERDRIVE_PARALLEL_PHASER_DEFAULTS: [u8; 20] = [
    0x00, 0x30, 0x01, 0x01, 0x00, 0x24, 0x10, 0x40, 0x10, 0x7f, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x60, 0x7f, 0x7f, 0x7f,
];

/// OD / Phaser (`11 05`): the drive on the left input, the phaser on the right input.
/// Parameters: 1 type, 2 drive, 3 amp type, 4 amp switch, 6 manual, 7 rate, 8 depth, 9 resonance, 10 mix, 16 drive
/// pan, 17 drive level, 18 phaser pan, 19 phaser level, 20 level. EFX control source 1 modulates the drive, source 2
/// the rate.
pub struct GSOverdriveParallelPhaser;

impl GSOverdriveParallelPhaser {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSParallelCombination {
        let mut effect = GSParallelCombination::new(
            sample_rate,
            memory,
            0x11_05,
            OVERDRIVE_PARALLEL_PHASER_DEFAULTS,
            &[(0, 1), (2, 3), (3, 1)],
            ParallelStageSpec { r#type: parallel_stages::DRIVE, input: None, output: None },
            ParallelStageSpec { r#type: parallel_stages::PHASER, input: None, output: None },
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const OVERDRIVE_PARALLEL_AUTO_WAH_DEFAULTS: [u8; 20] = [
    0x00, 0x30, 0x01, 0x01, 0x00, 0x01, 0x00, 0x44, 0x3e, 0x28, 0x48, 0x01,
    0x00, 0x00, 0x00, 0x00, 0x60, 0x7f, 0x7f, 0x7f,
];

/// OD / AutoWah (`11 06`): the drive on the left input, the auto wah on the right input.
/// Parameters: 1 type, 2 drive, 3 amp type, 4 amp switch, 6 filter type, 7 sens, 8 manual, 9 peak, 10 rate, 11
/// depth, 12 polarity, 16 drive pan, 17 drive level, 18 wah pan, 19 wah level, 20 level. EFX control source 1
/// modulates the drive, source 2 the manual.
pub struct GSOverdriveParallelAutoWah;

impl GSOverdriveParallelAutoWah {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSParallelCombination {
        let mut effect = GSParallelCombination::new(
            sample_rate,
            memory,
            0x11_06,
            OVERDRIVE_PARALLEL_AUTO_WAH_DEFAULTS,
            &[(0, 1), (2, 3), (3, 1), (5, 1), (11, 1)],
            ParallelStageSpec { r#type: parallel_stages::DRIVE, input: None, output: None },
            ParallelStageSpec { r#type: parallel_stages::WAH, input: None, output: Some(127.0 / 128.0) },
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const PHASER_PARALLEL_ROTARY_DEFAULTS: [u8; 20] = [
    0x24, 0x10, 0x40, 0x10, 0x7f, 0x06, 0x71, 0x18, 0x7f, 0x11, 0x78, 0x58,
    0x40, 0x60, 0x00, 0x00, 0x7f, 0x7f, 0x7f, 0x7f,
];

/// PH / Rotary (`11 07`): the phaser on the left input, the rotary on the right input.
/// Parameters: 1 manual, 2 rate, 3 depth, 4 resonance, 5 mix, 6-15 the rotary (as OD / Rotary), 16 phaser pan,
/// 17 phaser level, 18 rotary pan, 19 rotary level, 20 level. EFX control source 1 modulates the rate, source 2 the
/// speed.
pub struct GSPhaserParallelRotary;

impl GSPhaserParallelRotary {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSParallelCombination {
        let mut effect = GSParallelCombination::new(
            sample_rate,
            memory,
            0x11_07,
            PHASER_PARALLEL_ROTARY_DEFAULTS,
            &[],
            ParallelStageSpec { r#type: parallel_stages::PHASER, input: Some(127.0 / 128.0), output: None },
            ParallelStageSpec { r#type: parallel_stages::ROTARY, input: None, output: None },
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const PHASER_PARALLEL_AUTO_WAH_DEFAULTS: [u8; 20] = [
    0x24, 0x10, 0x40, 0x10, 0x7f, 0x01, 0x00, 0x44, 0x3e, 0x28, 0x48, 0x01,
    0x00, 0x00, 0x00, 0x00, 0x7f, 0x7f, 0x7f, 0x7f,
];

/// PH / AutoWah (`11 08`): the phaser on the left input, the auto wah on the right input.
/// Parameters: 1 manual, 2 rate, 3 depth, 4 resonance, 5 mix, 6-12 the wah (as OD / AutoWah), 16 phaser pan,
/// 17 phaser level, 18 wah pan, 19 wah level, 20 level. EFX control source 1 modulates the rate, source 2 the
/// manual.
pub struct GSPhaserParallelAutoWah;

impl GSPhaserParallelAutoWah {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSParallelCombination {
        let mut effect = GSParallelCombination::new(
            sample_rate,
            memory,
            0x11_08,
            PHASER_PARALLEL_AUTO_WAH_DEFAULTS,
            &[(5, 1), (11, 1)],
            ParallelStageSpec { r#type: parallel_stages::PHASER, input: Some(127.0 / 128.0), output: None },
            ParallelStageSpec { r#type: parallel_stages::WAH, input: Some(127.0 / 128.0), output: None },
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}
