//! Tremolo Chorus (`01 41`): a cascaded chorus, then a tremolo.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/tremolo_chorus.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::lfo::Lfo;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::modulated_delay::ModulatedDelay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::stereo_output::StereoOutput;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::tremolo::tremolo_rest;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{STEP_MS, SWING_MS, chorus_phase, modulation_depth, pre_delay, rate1};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x10, 0x08, 0x28, 0x28, 0x3c, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x7f, 0x40, 0x40, 0x00, 0x7f,
];

/// Tremolo Chorus (`01 41`): a cascaded chorus, then a tremolo. Taps 1 and 2 read the mono line half a cycle apart.
/// Tap 1 with the left input feeds a second line, tap 2 with the right input a third, and taps 3 and 4 read those
/// (90 and 270 degrees). Left is 2 x tap 1 + tap 3, right 2 x tap 2 + tap 4, each through its tremolo (triangle,
/// the right one offset by the tremolo phase). The first taps count twice on the SC-88Pro too (its wet level agrees
/// within 0.4 dB, with them once it would be 5 dB lower).
/// The left chain runs one 32 kHz sample before the right one (the shared stage's timing).
/// Parameters: 1 pre delay, 2 chorus rate (Rate1 column), 3 chorus depth, 4 tremolo phase, 5 tremolo rate (Rate1
/// column), 6 tremolo depth, 16 balance, 17 low gain, 18 high gain, 20 level. EFX control source 1 modulates the
/// tremolo rate, source 2 the balance.
pub struct GSTremoloChorus {
    base: GSEffectBase,
    mix: MonoMix,
    chorus_lfo: Lfo,
    tremolo_lfo: Lfo,
    /// Taps 1 (for the left chain) and 2.
    line1: ModulatedDelay,
    /// Tap 3.
    line2: ModulatedDelay,
    /// Tap 4.
    line3: ModulatedDelay,
    output: StereoOutput,
    tremolo_offset: f64,
    depth: f64,
    rest: f64,
}

impl GSTremoloChorus {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), DEFAULTS, &[], &[]);
        let mut line1 = ModulatedDelay::new(sample_rate, memory, 2, 100.0 + SWING_MS);
        line1.set_phase(1, 180.0);
        let mut line2 = ModulatedDelay::new(sample_rate, memory, 1, SWING_MS);
        line2.set_dc_blocker(false);
        line2.set_phase(0, 90.0);
        let mut line3 = ModulatedDelay::new(sample_rate, memory, 1, SWING_MS);
        line3.set_dc_blocker(false);
        line3.set_phase(0, 270.0);
        let mut effect = Self {
            base,
            mix: MonoMix::new(sample_rate),
            chorus_lfo: Lfo::new(sample_rate),
            tremolo_lfo: Lfo::new(sample_rate),
            line1,
            line2,
            line3,
            output: StereoOutput::new(sample_rate),
            tremolo_offset: 0.0,
            depth: 0.0,
            rest: 0.0,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSTremoloChorus {
    gs_effect_common!(0x01_41);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let depth = self.depth;
        let rest = self.rest;
        for i in 0..sample_count {
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            let phase = self.chorus_lfo.process();
            let x = self.mix.process(l, r) * LINE_INPUT;
            self.line1.process(x, phase);
            let tap1 = self.line1.output[0];
            let tap2 = self.line1.output[1];
            self.line2.process(0.25 * (tap1 + l), phase);
            self.line3.process(0.25 * (tap2 + r), phase);
            let tremolo = self.tremolo_lfo.process();
            let mut right = tremolo + self.tremolo_offset;
            if right >= 1.0 {
                right -= 2.0;
            }
            let gain_l = depth * tremolo.abs() + rest;
            let gain_r = depth * right.abs() + rest;
            self.output.process(
                l,
                gain_l * (2.0 * tap1 + self.line2.output[0]),
                r,
                gain_r * (2.0 * tap2 + self.line3.output[0]),
            );
            output_left[i] = self.output.left as f32;
            output_right[i] = self.output.right as f32;
        }
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.chorus_lfo.clear();
        self.tremolo_lfo.clear();
        self.line1.clear();
        self.line2.clear();
        self.line3.clear();
        self.output.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.chorus_lfo.set_rate(rate1(p[1]));
        let pre = pre_delay(p[0]);
        let depth = modulation_depth(p[2]);
        // The left chain's tap is one 32 kHz sample earlier
        self.line1.set_pre_delay(0, pre - STEP_MS);
        self.line1.set_pre_delay(1, pre);
        self.line1.set_depth(0, depth);
        self.line1.set_depth(1, depth);
        self.line2.set_depth(0, depth);
        self.line3.set_depth(0, depth);
        self.tremolo_lfo.set_rate(rate1(self.base.controlled(0, 4)));
        self.tremolo_offset = chorus_phase(p[3]) / 180.0;
        self.depth = level_gain(p[5]);
        self.rest = tremolo_rest(self.depth);
        self.output.set(
            self.base.controlled(1, 15),
            p[16],
            p[17],
            level_gain(p[19]) * STAGE_GAIN,
        );
    }
}
