//! Rhodes Multi (`04 06`): per channel `enhancer -> + phaser -> + chorus/flanger -> tremolo/auto pan`.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/rhodes_multi.ts

use super::guitar_multi::{GSMulti, MultiPhaser};
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::enhancer::Enhancer;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::lfo::Lfo;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::modulated_delay::ModulatedDelay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::tremolo::Tremolo;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{SWING_MS, cutoff, feedback_gain, modulation_depth, pre_delay, rate2};

/// The input gain of each channel (the enhancer's).
const INPUT_GAIN: f64 = 127.0 / 128.0;

/// The chorus stage's dry gain.
const CHORUS_DRY: f64 = 127.0 / 128.0;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x40, 0x40, 0x24, 0x10, 0x20, 0x10, 0x40, 0x00, 0x7f, 0x0a, 0x08, 0x40,
    0x68, 0x7f, 0x01, 0x02, 0x3c, 0x40, 0x01, 0x7f,
];
const LIMITS: &[(u8, u8)] = &[(7, 1), (14, 1), (15, 4), (18, 1)];

/// Rhodes Multi (`04 06`): per channel `enhancer -> + phaser -> + chorus/flanger -> tremolo/auto pan`. The phaser and
/// the chorus/flanger run on the mono sum of both channels and add their wet signal to each.
///
/// The shared stage reads the left input one 32 kHz sample late and writes the right output one sample late, on both
/// hardware units. The mono stages mix both inputs into each output, so that shows (the phaser notches of the two
/// outputs split). The whole chain runs on the skewed inputs of a `MonoMix` and the right output lags.
/// Parameters: 1 enhancer sensitivity, 2 enhancer mix, 3 phaser manual, 4 rate (Rate2 column), 5 depth,
/// 6 resonance, 7 mix, 8 chorus or flanger, 9 its low pass (Cut column, 120-127 off), 10 pre delay, 11 rate (Rate2),
/// 12 depth, 13 flanger feedback, 14 level, 15 tremolo or auto pan, 16 wave, 17 rate (Rate2), 18 depth, 19 switch,
/// 20 level. EFX control source 1 moves the tremolo rate, source 2 its depth.
pub struct GSRhodesMulti {
    base: GSEffectBase,
    multi: GSMulti,
    enhancer_l: Enhancer,
    enhancer_r: Enhancer,
    phaser: MultiPhaser,
    lfo: Lfo,
    chorus: ModulatedDelay,
    dc_blocker_l: FirstOrderFilter,
    dc_blocker_r: FirstOrderFilter,
    tremolo: Tremolo,
    chorus_wet: f64,
    tremolo_on: bool,
}

impl GSRhodesMulti {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), DEFAULTS, LIMITS, &[]);
        let chorus = ModulatedDelay::new(sample_rate, memory, 2, 101.0 + SWING_MS);
        let mut dc_blocker_l = FirstOrderFilter::new(sample_rate);
        dc_blocker_l.set_high_pass(DC_BLOCKER_HZ, 1.0);
        let mut dc_blocker_r = FirstOrderFilter::new(sample_rate);
        dc_blocker_r.set_high_pass(DC_BLOCKER_HZ, 1.0);
        let mut effect = Self {
            base,
            multi: GSMulti::new(sample_rate),
            enhancer_l: Enhancer::new(sample_rate),
            enhancer_r: Enhancer::new(sample_rate),
            phaser: MultiPhaser::new(sample_rate),
            lfo: Lfo::new(sample_rate),
            chorus,
            dc_blocker_l,
            dc_blocker_r,
            tremolo: Tremolo::new(sample_rate),
            chorus_wet: 0.0,
            tremolo_on: false,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSRhodesMulti {
    gs_effect_common!(0x04_06);

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
            let mut l = self.enhancer_l.process(mix.skewed_left * INPUT_GAIN);
            let mut r = self.enhancer_r.process(mix.skewed_right * INPUT_GAIN);

            self.phaser.process(l, r);
            l = self.phaser.left;
            r = self.phaser.right;

            let phase = self.lfo.process();
            self.chorus.process(0.5 * (l + r) * LINE_INPUT, phase);
            l = CHORUS_DRY * l + self.chorus_wet * self.chorus.output[0];
            r = CHORUS_DRY * r + self.chorus_wet * self.chorus.output[1];

            l = self.dc_blocker_l.process(l);
            r = self.dc_blocker_r.process(r);
            // The LFO runs with the switch off too
            self.tremolo.process();
            if self.tremolo_on {
                l *= self.tremolo.left;
                r *= self.tremolo.right;
            }
            self.multi.output(l, r, output_left, output_right, i);
        }
    }

    fn clear(&mut self) {
        self.multi.clear();
        self.enhancer_l.clear();
        self.enhancer_r.clear();
        self.phaser.clear();
        self.lfo.clear();
        self.chorus.clear();
        self.dc_blocker_l.clear();
        self.dc_blocker_r.clear();
        self.tremolo.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        // The sensitivity is 0 dB at 16 and grows linearly, the mix follows the level curve
        let sensitivity = p[0].max(1) as f64 / 16.0;
        self.enhancer_l.set(sensitivity, level_gain(p[1]));
        self.enhancer_r.set(sensitivity, level_gain(p[1]));

        self.phaser.update(p[2], p[3], p[4], p[5], p[6]);

        let flanger = p[7] == 1;
        let chorus = &mut self.chorus;
        chorus.set_curve(flanger);
        for read in 0..2 {
            chorus.set_pre_delay(read, pre_delay(p[9]));
            chorus.set_depth(read, modulation_depth(p[11]));
        }
        // The chorus reads are half a cycle apart, the flanger's together
        chorus.set_phase(1, if flanger { 0.0 } else { 180.0 });
        chorus.set_feedback(if flanger { feedback_gain(p[12]) } else { 0.0 });
        // The tone filter's low pass, 120-127 is off (a plain 127/128, the filter's full gain byte)
        if p[8] >= 120 {
            chorus.tone.set_gain(127.0 / 128.0);
        } else {
            chorus.tone.set_tone(false, cutoff(p[8]), 1.0);
        }
        self.lfo.set_rate(rate2(p[10]));
        self.chorus_wet = level_gain(p[13]);

        self.tremolo_on = p[18] == 1;
        self.tremolo.set_pan(p[14] == 1);
        self.tremolo.set_wave(p[15]);
        self.tremolo.set_rate(rate2(self.base.controlled(0, 16)));
        self.tremolo.set_depth(level_gain(self.base.controlled(1, 17)));

        self.multi.gain = level_gain(p[19]) * STAGE_GAIN;
    }
}
