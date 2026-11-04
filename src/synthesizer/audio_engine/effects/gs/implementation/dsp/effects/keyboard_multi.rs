//! Keyboard Multi (`05 00`): per channel `ring modulator -> 3-band EQ`, then the pitch shifter, the phaser and the
//! delay on the mono sum.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/keyboard_multi.ts

use super::guitar_multi::{GSMulti, MultiDelay, MultiEq, MultiPhaser};
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::peaking_band::peaking_gain;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::pitch_shifter::PitchShifter;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::ring_modulator::RingModulator;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{pitch_ratio, pitch_window, ring_frequency};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::peaking_table::{PEAKING_TABLE, peaking_entry};
use std::sync::LazyLock;

/// The input gain of each channel.
const INPUT_GAIN: f64 = 127.0 / 128.0;

/// The pitch shifter's input, a full 7 bit gain byte.
const SHIFTER_INPUT: f64 = 127.0 / 128.0;

/// The shifted voice goes into both channels at the center pan's gain.
static PITCH_GAIN: LazyLock<f64> = LazyLock::new(|| pan_left(64));

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x32, 0x14, 0x43, 0x00, 0x02, 0x45, 0x3d, 0x47, 0x40, 0x00, 0x27, 0x24,
    0x08, 0x5a, 0x50, 0x4b, 0x14, 0x40, 0x28, 0x60,
];
// The coarse pitch is 40-76 (-24 to +12 semitones)
const LIMITS: &[(u8, u8)] = &[(4, 4), (7, 76), (9, 4)];
const MINIMUMS: &[(u8, u8)] = &[(7, 40)];

/// Keyboard Multi (`05 00`): per channel `ring modulator -> 3-band EQ`, then the pitch shifter, the phaser and the
/// delay, each on the mono sum of both channels with its wet signal added to each.
///
/// The shared stage reads the left input one 32 kHz sample late and writes the right output one sample late, on both
/// hardware units. The whole chain runs on the skewed inputs of a `MonoMix` and the right output lags (Rhodes Multi).
///
/// After a type select the right channel's mid band boosts four times as much as the left (its first coefficient's
/// scale bit is set): +12.3 dB instead of the default +5 dB at 200 Hz. The SC-88Pro has it too, and at 200 Hz it even
/// keeps it after mid band changes (not modeled). SC-VA rewrites the band with any mid band change, so does this.
/// Parameters: 1 ring modulator frequency, 2 its balance, 3 low gain, 4 mid frequency (EQ column), 5 mid Q, 6 mid gain,
/// 7 high gain, 8 pitch coarse, 9 fine, 10 mode, 11 pitch balance, 12 phaser manual, 13 rate (Rate2 column), 14 depth,
/// 15 resonance, 16 mix, 17 delay time (Dly4 column), 18 feedback, 19 delay level, 20 level. EFX control source 1
/// moves the ring modulator frequency, source 2 its balance.
pub struct GSKeyboardMulti {
    base: GSEffectBase,
    multi: GSMulti,
    ring: RingModulator,
    eq_l: MultiEq,
    eq_r: MultiEq,
    shifter: PitchShifter,
    phaser: MultiPhaser,
    delay: MultiDelay,
    /// The right mid band as after a type select (until a mid band parameter changes).
    preset_band: bool,
    ring_dry: f64,
    ring_wet: f64,
    pitch_dry: f64,
    pitch_wet: f64,
}

impl GSKeyboardMulti {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), DEFAULTS, LIMITS, MINIMUMS);
        let mut ring = RingModulator::new(sample_rate);
        // The hardware steps the carrier's phase once before its first sample (one 32 kHz step)
        ring.set_start_delay(1.0 - sample_rate / 32_000.0);
        let eq_l = MultiEq::new(sample_rate);
        let eq_r = MultiEq::new(sample_rate);
        let shifter = PitchShifter::new(sample_rate, memory, 1);
        let phaser = MultiPhaser::new(sample_rate);
        let delay = MultiDelay::new(sample_rate, memory);
        let mut effect = Self {
            base,
            multi: GSMulti::new(sample_rate),
            ring,
            eq_l,
            eq_r,
            shifter,
            phaser,
            delay,
            preset_band: true,
            ring_dry: 0.0,
            ring_wet: 0.0,
            pitch_dry: 0.0,
            pitch_wet: 0.0,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSKeyboardMulti {
    gs_effect_common!(0x05_00);

    fn reset(&mut self) {
        self.preset_band = true;
        reset_base(self);
    }

    fn set_parameter(&mut self, parameter: u8, value: u8) {
        // Like SC-VA, any change of the mid band writes both bands
        if (3..=5).contains(&parameter)
            && value != self.base.parameters[parameter as usize]
            && (parameter != 4 || value <= 4)
        {
            self.preset_band = false;
        }
        set_parameter_base(self, parameter, value);
    }

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        for i in 0..sample_count {
            let mix = &mut self.multi.mix;
            mix.process(input_left[i] as f64, input_right[i] as f64);
            let mut l = mix.skewed_left * INPUT_GAIN;
            let mut r = mix.skewed_right * INPUT_GAIN;

            self.ring.process();
            let ring_gain = self.ring_dry + self.ring_wet * self.ring.carrier;
            l = self.eq_l.process(l * ring_gain);
            r = self.eq_r.process(r * ring_gain);

            self.shifter.process(0.5 * (l + r) * SHIFTER_INPUT);
            let shifted = self.pitch_wet * self.shifter.output[0];
            l = self.pitch_dry * l + shifted;
            r = self.pitch_dry * r + shifted;

            self.phaser.process(l, r);
            self.delay.process(self.phaser.left, self.phaser.right);
            self.multi
                .output(self.delay.left, self.delay.right, output_left, output_right, i);
        }
    }

    fn clear(&mut self) {
        self.multi.clear();
        self.ring.clear();
        self.eq_l.clear();
        self.eq_r.clear();
        self.shifter.clear();
        self.phaser.clear();
        self.delay.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.ring.set_frequency(ring_frequency(self.base.controlled(0, 0)));
        let balance = self.base.controlled(1, 1);
        self.ring_dry = balance_dry(balance);
        self.ring_wet = balance_wet(balance);

        self.eq_l.set_shelves(p[2], p[6]);
        self.eq_r.set_shelves(p[2], p[6]);
        self.eq_l.set_mid(p[3], p[4], p[5]);
        if self.preset_band {
            // Four times the boost: the peaking section is 1 + (G - 1) x band pass
            let e = peaking_entry((p[3] >> 3) as usize, p[4] as usize, peaking_gain(p[5]));
            let boost = 1.0 + 4.0 * (10f64.powf(PEAKING_TABLE[e + 2] as f64 / 20.0) - 1.0);
            self.eq_r.mid.set(
                PEAKING_TABLE[e] as f64,
                PEAKING_TABLE[e + 1] as f64,
                20.0 * boost.log10(),
            );
        } else {
            self.eq_r.set_mid(p[3], p[4], p[5]);
        }

        self.shifter
            .set_voice(0, pitch_ratio(p[7] as f64, p[8] as f64), pitch_window(p[9]));
        self.pitch_dry = balance_dry(p[10]);
        self.pitch_wet = balance_wet(p[10]) * *PITCH_GAIN;

        self.phaser.update(p[11], p[12], p[13], p[14], p[15]);
        self.delay.update(p[16], p[17], p[18], 127);

        self.multi.gain = level_gain(p[19]) * STAGE_GAIN;
    }
}
