//! GTR Multi 1, 2 and 3 (`04 00`-`04 02`) and the drive/EQ multi base of GTR Multi 2 and Bass Multi.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/guitar_multis.ts

use super::guitar_multi::{
    GSMulti, MULTI_WAH_GAIN, MultiCompressor, MultiDelay, MultiDrive, MultiDriveOptions, MultiEq,
    MultiModulation,
};
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::amp_simulator::AmpSimulator;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::drive::{GUITAR_DISTORTION_HIGH_PASS, GUITAR_OVERDRIVE};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::wah::Wah;
use std::sync::LazyLock;

/// The drive section bypass of GTR Multi 1 and 3: the drive's DC blocker, without a 127/128 that GTR Multi 2's has.
const BYPASS_GAIN: f64 = 128.0 / 127.0;

/// The chain goes into the chorus/flanger at the center pan's gain.
static CHAIN_GAIN: LazyLock<f64> = LazyLock::new(|| pan_left(64));

/// The section in front of the drive (parameters 1-4) of GTR Multi 1 and 3.
enum DriveDelayFront {
    /// GTR Multi 1's compressor.
    Compressor(MultiCompressor),
    /// GTR Multi 3's wah and its switch.
    Wah { wah: Wah, wah_on: bool },
}

/// GTR Multi 1 and 3: `L+R -> compressor or wah -> overdrive/distortion -> 2-band EQ -> amp simulator ->
/// chorus/flanger -> delay`. Parameters 5-20 are the same in both.
pub struct GSDriveDelayMulti {
    base: GSEffectBase,
    effect_type: u16,
    multi: GSMulti,
    drive: MultiDrive,
    modulation: MultiModulation,
    delay: MultiDelay,
    front: DriveDelayFront,
}

impl GSDriveDelayMulti {
    fn new(
        sample_rate: f64,
        memory: &mut DelayMemory,
        effect_type: u16,
        defaults: [u8; 20],
        limits: &'static [(u8, u8)],
        front: DriveDelayFront,
    ) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), defaults, limits, &[]);
        let drive = MultiDrive::new(
            sample_rate,
            MultiDriveOptions {
                overdrive: &GUITAR_OVERDRIVE,
                distortion: &GUITAR_DISTORTION_HIGH_PASS,
                amp: Box::new(AmpSimulator::new(sample_rate)),
                bypass_gain: BYPASS_GAIN,
                shelves: true,
            },
        );
        let modulation = MultiModulation::new(sample_rate, memory);
        let delay = MultiDelay::new(sample_rate, memory);
        Self {
            base,
            effect_type,
            multi: GSMulti::new(sample_rate),
            drive,
            modulation,
            delay,
            front,
        }
    }

    /// The section in front of the drive (parameters 1-4).
    fn front(&mut self, x: f64) -> f64 {
        match &mut self.front {
            DriveDelayFront::Compressor(compressor) => compressor.process(x),
            DriveDelayFront::Wah { wah, wah_on } => {
                let y = wah.process(x) * MULTI_WAH_GAIN;
                if *wah_on { y } else { x }
            }
        }
    }

    /// Applies parameters 5-20: the drive and the delay level, each with its control offset.
    fn update_chain(&mut self, drive: u8, delay_level: u8) {
        let p = self.base.parameters;
        self.drive.update(p[4] == 1, drive, p[6], p[7] == 1, p[10] == 1);
        self.drive.set_shelves(p[8], p[9]);
        self.modulation.update(p[11] == 1, p[12], p[13], p[14], p[15]);
        self.delay.update(p[16], p[17], delay_level, 127);
        self.multi.gain = level_gain(p[19]) * STAGE_OUTPUT;
    }
}

impl GSEffect for GSDriveDelayMulti {
    gs_effect_common!(dynamic);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let chain_gain = *CHAIN_GAIN;
        for i in 0..sample_count {
            let mixed = self.multi.mix.process(input_left[i] as f64, input_right[i] as f64) * MONO_INPUT;
            let x = self.front(mixed);
            // The drive switch bypasses the EQ and the amp too
            let driven = self.drive.process(x);
            self.modulation.process(driven * chain_gain);
            self.delay.process(self.modulation.left, self.modulation.right);
            self.multi
                .output(self.delay.left, self.delay.right, output_left, output_right, i);
        }
    }

    fn clear(&mut self) {
        self.multi.clear();
        self.drive.clear();
        self.modulation.clear();
        self.delay.clear();
        match &mut self.front {
            DriveDelayFront::Compressor(compressor) => compressor.clear(),
            DriveDelayFront::Wah { wah, .. } => wah.clear(),
        }
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        match &mut self.front {
            DriveDelayFront::Compressor(compressor) => {
                compressor.update(&p);
                let (drive, delay_level) = (self.base.controlled(0, 5), self.base.controlled(1, 18));
                self.update_chain(drive, delay_level);
            }
            DriveDelayFront::Wah { wah, wah_on } => {
                wah.set_filter_type(p[0] == 1);
                wah.set_manual(level_gain(self.base.controlled(0, 1)));
                wah.set_peak(p[2]);
                *wah_on = p[3] == 1;
                let drive = self.base.controlled(1, 5);
                self.update_chain(drive, p[18]);
            }
        }
    }
}

#[rustfmt::skip]
const GUITAR_MULTI_1_DEFAULTS: [u8; 20] = [
    0x64, 0x50, 0x64, 0x01, 0x00, 0x50, 0x00, 0x01, 0x45, 0x4a, 0x01, 0x00,
    0x08, 0x1e, 0x66, 0x28, 0x3c, 0x22, 0x0f, 0x6e,
];
const GUITAR_MULTI_1_LIMITS: &[(u8, u8)] = &[(3, 1), (4, 1), (6, 3), (7, 1), (10, 1), (11, 1)];

/// GTR Multi 1 (`04 00`): `L+R -> compressor -> overdrive/distortion -> 2-band EQ -> amp simulator -> chorus/flanger
/// -> delay`.
/// Parameters: 1-4 compressor sustain, attack, level and switch, 5 drive type (OD, DS), 6 drive, 7 amp type, 8 amp
/// switch, 9 low gain, 10 high gain, 11 drive switch, 12 chorus or flanger, 13 rate (Rate2 column), 14 depth,
/// 15 flanger feedback, 16 chorus level, 17 delay time (Dly4 column), 18 feedback, 19 delay level, 20 level.
/// EFX control source 1 moves the drive, source 2 the delay level.
pub struct GSGuitarMulti1;

impl GSGuitarMulti1 {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSDriveDelayMulti {
        let front = DriveDelayFront::Compressor(MultiCompressor::new(sample_rate, 0.001));
        let mut effect = GSDriveDelayMulti::new(
            sample_rate,
            memory,
            0x04_00,
            GUITAR_MULTI_1_DEFAULTS,
            GUITAR_MULTI_1_LIMITS,
            front,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

#[rustfmt::skip]
const GUITAR_MULTI_3_DEFAULTS: [u8; 20] = [
    0x01, 0x3c, 0x0a, 0x01, 0x01, 0x50, 0x02, 0x01, 0x40, 0x40, 0x01, 0x00,
    0x08, 0x00, 0x59, 0x32, 0x20, 0x40, 0x1e, 0x58,
];
const GUITAR_MULTI_3_LIMITS: &[(u8, u8)] =
    &[(0, 1), (3, 1), (4, 1), (6, 3), (7, 1), (10, 1), (11, 1)];

/// GTR Multi 3 (`04 02`): `L+R -> wah -> overdrive/distortion -> 2-band EQ -> amp simulator -> chorus/flanger ->
/// delay`. The wah is the Auto Wah's filter at its Manual, without the LFO and the envelope follower.
/// Parameters: 1 wah filter type (LPF, BPF), 2 manual, 3 peak, 4 wah switch, 5 drive type (OD, DS), 6 drive, 7 amp
/// type, 8 amp switch, 9 low gain, 10 high gain, 11 drive switch, 12 chorus or flanger, 13 rate (Rate2 column),
/// 14 depth, 15 flanger feedback, 16 chorus level, 17 delay time (Dly4 column), 18 feedback, 19 delay level,
/// 20 level. EFX control source 1 moves the wah manual, source 2 the drive.
pub struct GSGuitarMulti3;

impl GSGuitarMulti3 {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSDriveDelayMulti {
        let mut wah = Wah::new(sample_rate);
        wah.set_envelope(false);
        wah.set_polarity(true);
        wah.set_depth(0.0);
        let front = DriveDelayFront::Wah { wah, wah_on: false };
        let mut effect = GSDriveDelayMulti::new(
            sample_rate,
            memory,
            0x04_02,
            GUITAR_MULTI_3_DEFAULTS,
            GUITAR_MULTI_3_LIMITS,
            front,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

/// GTR Multi 2 and Bass Multi: `L+R -> compressor -> overdrive/distortion -> amp simulator -> 3-band EQ ->
/// chorus/flanger`, with the same parameters. They differ in the drives, the amp and some fixed gains.
pub struct GSDriveEqMulti {
    base: GSEffectBase,
    effect_type: u16,
    multi: GSMulti,
    pub(super) modulation: MultiModulation,
    compressor: MultiCompressor,
    drive: MultiDrive,
    eq: MultiEq,
    chain_gain: f64,
}

impl GSDriveEqMulti {
    /// `drive` is the drive section, `chain_gain` the gain into the chorus/flanger and `lookahead` the
    /// compressor's lookahead in seconds (the Compressor's is 1 ms). The subclass constructor resets.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        sample_rate: f64,
        memory: &mut DelayMemory,
        effect_type: u16,
        defaults: [u8; 20],
        limits: &'static [(u8, u8)],
        drive: MultiDriveOptions,
        chain_gain: f64,
        lookahead: f64,
    ) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), defaults, limits, &[]);
        let compressor = MultiCompressor::new(sample_rate, lookahead);
        let drive = MultiDrive::new(sample_rate, drive);
        let eq = MultiEq::new(sample_rate);
        let modulation = MultiModulation::new(sample_rate, memory);
        Self {
            base,
            effect_type,
            multi: GSMulti::new(sample_rate),
            modulation,
            compressor,
            drive,
            eq,
            chain_gain,
        }
    }
}

impl GSEffect for GSDriveEqMulti {
    gs_effect_common!(dynamic);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let chain_gain = self.chain_gain;
        for i in 0..sample_count {
            let mixed = self.multi.mix.process(input_left[i] as f64, input_right[i] as f64) * MONO_INPUT;
            let x = self.compressor.process(mixed);
            // The drive switch bypasses the amp too
            let driven = self.drive.process(x);
            self.modulation.process(self.eq.process(driven) * chain_gain);
            self.multi.output(
                self.modulation.left,
                self.modulation.right,
                output_left,
                output_right,
                i,
            );
        }
    }

    fn clear(&mut self) {
        self.multi.clear();
        self.compressor.clear();
        self.drive.clear();
        self.eq.clear();
        self.modulation.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.compressor.update(&p);
        self.drive
            .update(p[4] == 1, self.base.controlled(0, 5), p[6], p[7] == 1, p[8] == 1);
        self.eq.set_shelves(p[9], p[13]);
        self.eq.set_mid(p[10], p[11], p[12]);
        self.modulation
            .update(p[14] == 1, p[15], p[16], p[17], self.base.controlled(1, 18));
        self.multi.gain = level_gain(p[19]) * STAGE_OUTPUT;
    }
}

#[rustfmt::skip]
const GUITAR_MULTI_2_DEFAULTS: [u8; 20] = [
    0x46, 0x7f, 0x5a, 0x01, 0x00, 0x50, 0x02, 0x01, 0x01, 0x4c, 0x38, 0x02,
    0x45, 0x36, 0x00, 0x08, 0x60, 0x66, 0x00, 0x50,
];
const GUITAR_MULTI_2_LIMITS: &[(u8, u8)] = &[(3, 1), (4, 1), (6, 3), (7, 1), (8, 1), (11, 4), (14, 1)];

/// GTR Multi 2 (`04 01`): `L+R -> compressor -> overdrive/distortion -> amp simulator -> 3-band EQ -> chorus/flanger`.
/// Parameters: 1-4 compressor sustain, attack, level and switch, 5 drive type (OD, DS), 6 drive, 7 amp type, 8 amp
/// switch, 9 drive switch, 10 low gain, 11-13 mid frequency, Q and gain, 14 high gain, 15 chorus or flanger, 16 rate
/// (Rate2 column), 17 depth, 18 flanger feedback, 19 chorus level, 20 level. EFX control source 1 moves the drive,
/// source 2 the chorus level.
pub struct GSGuitarMulti2;

impl GSGuitarMulti2 {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSDriveEqMulti {
        let mut effect = GSDriveEqMulti::new(
            sample_rate,
            memory,
            0x04_01,
            GUITAR_MULTI_2_DEFAULTS,
            GUITAR_MULTI_2_LIMITS,
            MultiDriveOptions {
                overdrive: &GUITAR_OVERDRIVE,
                distortion: &GUITAR_DISTORTION_HIGH_PASS,
                amp: Box::new(AmpSimulator::new(sample_rate)),
                bypass_gain: 1.0,
                shelves: false,
            },
            *CHAIN_GAIN,
            0.001,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}
