//! Rotary Multi (`03 00`): `L+R -> drive (switchable) -> 3-band EQ -> rotary`.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/rotary_multi.ts

use super::guitar_multi::MultiEq;
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::drive::{Drive, ROTARY_MULTI_DRIVE};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::rotary::Rotary;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{rate1, slew_time};

/// The gain into the rotary: the EQ's output byte (127/128), and twice the level the Rotary block expects (it halves
/// its input, Rotary Multi's program does not).
const ROTARY_GAIN: f64 = (2.0 * 127.0) / 128.0;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x0d, 0x01, 0x40, 0x48, 0x00, 0x40, 0x40, 0x06, 0x71, 0x18, 0x7f, 0x11,
    0x78, 0x58, 0x40, 0x60, 0x00, 0x00, 0x00, 0x60,
];
const LIMITS: &[(u8, u8)] = &[(1, 1), (4, 4)];

/// Rotary Multi (`03 00`): `L+R -> drive (switchable) -> 3-band EQ -> rotary`. The drive is the Humanizer's at half
/// the level, and the rotary takes the chain at full level (the Rotary halves its input).
/// Parameters: 1 drive, 2 drive switch, 3 low gain, 4 mid frequency (EQ column), 5 mid Q, 6 mid gain, 7 high gain,
/// 8 low slow, 9 low fast, 10 low accel, 11 low level, 12 high slow, 13 high fast, 14 high accel, 15 high level
/// (rates on the Rate1 column), 16 separation, 17 speed (slow, fast from 64), 20 level. EFX control source 1 moves
/// the drive, source 2 the speed.
pub struct GSRotaryMulti {
    base: GSEffectBase,
    mix: MonoMix,
    drive: Drive,
    eq: MultiEq,
    rotary: Rotary,
    drive_on: bool,
    gain: f64,
}

impl GSRotaryMulti {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), DEFAULTS, LIMITS, &[]);
        let mut drive = Drive::new(sample_rate);
        drive.set_preset(&ROTARY_MULTI_DRIVE);
        let eq = MultiEq::new(sample_rate);
        let rotary = Rotary::new(sample_rate, memory);
        let mut effect = Self {
            base,
            mix: MonoMix::new(sample_rate),
            drive,
            eq,
            rotary,
            drive_on: false,
            gain: 0.0,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSRotaryMulti {
    gs_effect_common!(0x03_00);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let gain = self.gain;
        for i in 0..sample_count {
            let x = self.mix.process(input_left[i] as f64, input_right[i] as f64) * MONO_INPUT;
            // Both paths run, so the switch does not click
            let driven = self.drive.process(x);
            let y = self.eq.process(if self.drive_on { driven } else { self.drive.input });
            self.rotary.process(ROTARY_GAIN * y);
            output_left[i] = (self.rotary.left * gain) as f32;
            output_right[i] = (self.rotary.right * gain) as f32;
        }
    }

    fn reset(&mut self) {
        reset_base(self);
        self.rotary.settle();
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.drive.clear();
        self.eq.clear();
        self.rotary.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.drive.set_drive(self.base.controlled(0, 0));
        self.drive_on = p[1] == 1;
        self.eq.set_shelves(p[2], p[6]);
        self.eq.set_mid(p[3], p[4], p[5]);
        let fast = self.base.controlled(1, 16) >= 64;
        self.rotary.set_rates(
            rate1(p[if fast { 12 } else { 11 }]),
            rate1(p[if fast { 8 } else { 7 }]),
        );
        self.rotary.set_acceleration(slew_time(p[13]), slew_time(p[9]));
        self.rotary.set_levels(level_gain(p[14]), level_gain(p[10]));
        self.rotary.set_separation(level_gain(p[15]));
        self.gain = level_gain(p[19]) * STAGE_OUTPUT;
    }
}
