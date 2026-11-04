//! Clean GTR Multi 1 and 2 (`04 03`, `04 04`).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/clean_guitar_multi.ts

use super::guitar_multi::{GSMulti, MULTI_WAH_GAIN, MultiCompressor, MultiDelay, MultiEq, MultiModulation};
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::wah::Wah;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{modulation_depth, rate1};

/// Clean GTR Multi 2's chain input, only the mono mix's 127/128. Its chain is linear (no compressor), so the whole
/// output stage goes on the level (`STAGE_GAIN`) instead of being split around the chain like Clean GTR Multi 1's.
const LINEAR_INPUT: f64 = 127.0 / 128.0;

#[rustfmt::skip]
const CLEAN_GUITAR_MULTI_1_DEFAULTS: [u8; 20] = [
    0x32, 0x7f, 0x4b, 0x01, 0x4c, 0x00, 0x02, 0x45, 0x34, 0x00, 0x08, 0x28,
    0x4f, 0x64, 0x18, 0x28, 0x7f, 0x1e, 0x00, 0x5f,
];
const CLEAN_GUITAR_MULTI_1_LIMITS: &[(u8, u8)] = &[(3, 1), (6, 4), (9, 1)];

/// Clean GTR Multi 1 (`04 03`): `L+R -> compressor -> 3-band EQ -> chorus/flanger -> delay`.
/// Parameters: 1-4 compressor sustain, attack, level and switch, 5 low gain, 6-8 mid frequency, Q and gain, 9 high
/// gain, 10 chorus or flanger, 11 rate (Rate2 column), 12 depth, 13 flanger feedback, 14 chorus level, 15 delay time
/// (Dly4 column), 16 feedback, 17 HF damp, 18 delay level, 20 level. EFX control source 1 moves the chorus level,
/// source 2 the delay level.
pub struct GSCleanGuitarMulti1 {
    base: GSEffectBase,
    multi: GSMulti,
    compressor: MultiCompressor,
    eq: MultiEq,
    modulation: MultiModulation,
    delay: MultiDelay,
}

impl GSCleanGuitarMulti1 {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let base = GSEffectBase::new(
            sample_rate,
            Some(memory),
            CLEAN_GUITAR_MULTI_1_DEFAULTS,
            CLEAN_GUITAR_MULTI_1_LIMITS,
            &[],
        );
        let compressor = MultiCompressor::new(sample_rate, 0.001);
        let eq = MultiEq::new(sample_rate);
        let modulation = MultiModulation::new(sample_rate, memory);
        let delay = MultiDelay::new(sample_rate, memory);
        let mut effect = Self {
            base,
            multi: GSMulti::new(sample_rate),
            compressor,
            eq,
            modulation,
            delay,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSCleanGuitarMulti1 {
    gs_effect_common!(0x04_03);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        for i in 0..sample_count {
            let x = self.multi.mix.process(input_left[i] as f64, input_right[i] as f64) * MONO_INPUT;
            let compressed = self.compressor.process(x);
            self.modulation.process(self.eq.process(compressed));
            self.delay.process(self.modulation.left, self.modulation.right);
            self.multi
                .output(self.delay.left, self.delay.right, output_left, output_right, i);
        }
    }

    fn clear(&mut self) {
        self.multi.clear();
        self.compressor.clear();
        self.eq.clear();
        self.modulation.clear();
        self.delay.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.compressor.update(&p);
        self.eq.set_shelves(p[4], p[8]);
        self.eq.set_mid(p[5], p[6], p[7]);
        self.modulation
            .update(p[9] == 1, p[10], p[11], p[12], self.base.controlled(0, 13));
        self.delay
            .update(p[14], p[15], self.base.controlled(1, 17), p[16]);
        self.multi.gain = level_gain(p[19]) * STAGE_OUTPUT;
    }
}

#[rustfmt::skip]
const CLEAN_GUITAR_MULTI_2_DEFAULTS: [u8; 20] = [
    0x01, 0x37, 0x28, 0x1d, 0x50, 0x01, 0x4c, 0x48, 0x00, 0x40, 0x40, 0x00,
    0x08, 0x14, 0x66, 0x64, 0x06, 0x0f, 0x50, 0x4c,
];
const CLEAN_GUITAR_MULTI_2_LIMITS: &[(u8, u8)] = &[(0, 1), (5, 1), (8, 4), (11, 1)];

/// Clean GTR Multi 2 (`04 04`): `L+R -> wah -> 3-band EQ -> chorus/flanger -> delay`. The wah is the Auto Wah's without
/// the envelope follower, its LFO raises the cutoff (polarity up).
/// Parameters: 1 wah filter type (LPF, BPF), 2 manual, 3 peak, 4 rate (Rate1 column), 5 depth, 6 wah switch (SC-VA's
/// code calls it the EQ switch, but it bypasses the wah), 7 low gain, 8-10 mid frequency, Q and gain, 11 high gain,
/// 12 chorus or flanger, 13 rate (Rate2 column), 14 depth, 15 flanger feedback, 16 chorus level, 17 delay time (Dly4
/// column), 18 feedback, 19 delay level, 20 level. EFX control source 1 moves the wah manual, source 2 the delay
/// level.
pub struct GSCleanGuitarMulti2 {
    base: GSEffectBase,
    multi: GSMulti,
    dc_blocker: FirstOrderFilter,
    wah: Wah,
    eq: MultiEq,
    modulation: MultiModulation,
    delay: MultiDelay,
    wah_on: bool,
}

impl GSCleanGuitarMulti2 {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let base = GSEffectBase::new(
            sample_rate,
            Some(memory),
            CLEAN_GUITAR_MULTI_2_DEFAULTS,
            CLEAN_GUITAR_MULTI_2_LIMITS,
            &[],
        );
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        let mut wah = Wah::new(sample_rate);
        wah.set_envelope(false);
        wah.set_polarity(true);
        let eq = MultiEq::new(sample_rate);
        let modulation = MultiModulation::new(sample_rate, memory);
        let delay = MultiDelay::new(sample_rate, memory);
        let mut effect = Self {
            base,
            multi: GSMulti::new(sample_rate),
            dc_blocker,
            wah,
            eq,
            modulation,
            delay,
            wah_on: false,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSCleanGuitarMulti2 {
    gs_effect_common!(0x04_04);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        for i in 0..sample_count {
            let mut x = self.multi.mix.process(input_left[i] as f64, input_right[i] as f64) * LINEAR_INPUT;
            // The wah runs when it is switched off too. Unlike the Auto Wah's, a DC blocker comes first
            let blocked = self.dc_blocker.process(x);
            let wah = MULTI_WAH_GAIN * self.wah.process(blocked);
            if self.wah_on {
                x = wah;
            }
            self.modulation.process(self.eq.process(x));
            self.delay.process(self.modulation.left, self.modulation.right);
            self.multi
                .output(self.delay.left, self.delay.right, output_left, output_right, i);
        }
    }

    fn clear(&mut self) {
        self.multi.clear();
        self.dc_blocker.clear();
        self.wah.clear();
        self.eq.clear();
        self.modulation.clear();
        self.delay.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        let wah = &mut self.wah;
        wah.set_filter_type(p[0] == 1);
        wah.set_peak(p[2]);
        wah.set_manual(level_gain(self.base.controlled(0, 1)));
        wah.set_rate(rate1(p[3]));
        // The depth word is the chorus swing in ms over 16
        wah.set_depth(modulation_depth(p[4]) / 16.0);
        self.wah_on = p[5] == 1;
        self.eq.set_shelves(p[6], p[10]);
        self.eq.set_mid(p[7], p[8], p[9]);
        self.modulation.update(p[11] == 1, p[12], p[13], p[14], p[15]);
        self.delay.update(p[16], p[17], self.base.controlled(1, 18), 127);
        self.multi.gain = level_gain(p[19]) * STAGE_GAIN;
    }
}
