//! Hexa Chorus (`01 40`): six chorus taps on one mono line, one LFO, the taps 60 degrees apart.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/hexa_chorus.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::lfo::Lfo;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::modulated_delay::ModulatedDelay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::stereo_output::StereoOutput;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{STEP_MS, SWING_MS, modulation_depth, pre_delay, rate1};

const TAPS: usize = 6;

/// The pan of one deviation step: a word of 136 over 2^15, so the outer taps reach 0.249 at a pan deviation of 20.
const PAN_STEP: f64 = 136.0 / 32_768.0;

/// The pan spread of each tap, in steps of the pan deviation (the left side, the right is the mirror image).
const SPREAD: [f64; TAPS] = [3.0, 2.0, 1.0, -1.0, -2.0, -3.0];

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x18, 0x08, 0x7f, 0x05, 0x42, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x40, 0x40, 0x40, 0x00, 0x70,
];
const LIMITS: &[(u8, u8)] = &[(3, 20), (4, 84), (5, 20)];
// The depth deviation is -20 to +20 (44-84)
const MINIMUMS: &[(u8, u8)] = &[(4, 44)];

/// Hexa Chorus (`01 40`): six chorus taps on one mono line, one LFO, the taps 60 degrees apart. The deviation
/// parameters spread the taps' pre delays, depths and pans.
/// The left output reads every tap one 32 kHz sample before the right output (the shared stage's timing).
/// Parameters: 1 pre delay, 2 rate (Rate1 column), 3 depth, 4 pre delay deviation (0-20 ms), 5 depth deviation
/// (-20 to +20), 6 pan deviation (0-20), 16 balance, 17 low gain, 18 high gain, 20 level. EFX control source 1
/// modulates the rate, source 2 the balance.
pub struct GSHexaChorus {
    base: GSEffectBase,
    mix: MonoMix,
    lfo: Lfo,
    /// Reads 0-5 are the taps for the left output, 6-11 the same taps for the right.
    delay: ModulatedDelay,
    output: StereoOutput,
    pans_l: [f64; TAPS],
    pans_r: [f64; TAPS],
}

impl GSHexaChorus {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), DEFAULTS, LIMITS, MINIMUMS);
        // 100 ms of pre delay plus 5 x 20 ms of deviation
        let mut delay = ModulatedDelay::new(sample_rate, memory, 2 * TAPS, 200.0 + SWING_MS);
        for tap in 0..TAPS {
            delay.set_phase(tap, tap as f64 * 60.0);
            delay.set_phase(tap + TAPS, tap as f64 * 60.0);
        }
        let mut effect = Self {
            base,
            mix: MonoMix::new(sample_rate),
            lfo: Lfo::new(sample_rate),
            delay,
            output: StereoOutput::new(sample_rate),
            pans_l: [0.0; TAPS],
            pans_r: [0.0; TAPS],
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSHexaChorus {
    gs_effect_common!(0x01_40);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        for i in 0..sample_count {
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            let x = self.mix.process(l, r) * LINE_INPUT;
            let phase = self.lfo.process();
            self.delay.process(x, phase);
            let taps = &self.delay.output;
            let mut left = 0.0;
            let mut right = 0.0;
            for tap in 0..TAPS {
                left += self.pans_l[tap] * taps[tap];
                right += self.pans_r[tap] * taps[tap + TAPS];
            }
            self.output.process(l, left, r, right);
            output_left[i] = self.output.left as f32;
            output_right[i] = self.output.right as f32;
        }
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.lfo.clear();
        self.delay.clear();
        self.output.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.lfo.set_rate(rate1(self.base.controlled(0, 1)));
        for tap in 0..TAPS {
            let pre = pre_delay(p[0]) + tap as f64 * p[3] as f64;
            let depth = modulation_depth(
                0i32.max(127i32.min(p[2] as i32 + tap as i32 * (p[4] as i32 - 64))) as u8,
            );
            // The left output's read is one 32 kHz sample earlier
            self.delay.set_pre_delay(tap, pre - STEP_MS);
            self.delay.set_pre_delay(tap + TAPS, pre);
            self.delay.set_depth(tap, depth);
            self.delay.set_depth(tap + TAPS, depth);
            // Each tap's pans add up to 0.5
            let pan = p[5] as f64 * SPREAD[tap] * PAN_STEP;
            self.pans_l[tap] = 0.25 + pan;
            self.pans_r[tap] = 0.25 - pan;
        }
        self.output.set(
            self.base.controlled(1, 15),
            p[16],
            p[17],
            level_gain(p[19]) * STAGE_GAIN,
        );
    }
}
