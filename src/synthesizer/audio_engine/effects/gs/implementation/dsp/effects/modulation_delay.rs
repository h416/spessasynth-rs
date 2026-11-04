//! Mod Delay (`01 51`): Stereo Delay's lines, each damped tap written into a modulation line.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/modulation_delay.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::delay::Delay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::lfo::Lfo;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::modulated_delay::ModulatedDelay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::stereo_output::StereoOutput;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{SWING_MS, chorus_phase, delay3, feedback_gain, hf_damp, modulation_depth, rate1};

/// The modulation swings four times as far as a chorus.
const DEPTH_SCALE: f64 = 4.0;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x5a, 0x6c, 0x58, 0x01, 0x0c, 0x15, 0x5a, 0x7f, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x28, 0x40, 0x40, 0x00, 0x7f,
];
const LIMITS: &[(u8, u8)] = &[(3, 1)];

/// Mod Delay (`01 51`): Stereo Delay's lines (feedback normal or cross, HF damp in the loop), each damped tap written
/// into a modulation line that an LFO reads (the right one offset by the phase). True stereo, as on the SC-88Pro.
/// Parameters: 1 delay left, 2 delay right (Dly3 column), 3 feedback, 4 feedback mode (normal, cross), 5 rate (Rate1
/// column), 6 depth, 7 phase, 8 HF damp, 16 balance, 17 low gain, 18 high gain, 20 level. EFX control source 1
/// modulates the rate, source 2 the balance.
pub struct GSModulationDelay {
    base: GSEffectBase,
    delay_l: Delay,
    delay_r: Delay,
    lfo: Lfo,
    modulation_l: ModulatedDelay,
    modulation_r: ModulatedDelay,
    output: StereoOutput,
    cross: bool,
    feedback: f64,
}

impl GSModulationDelay {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), DEFAULTS, LIMITS, &[]);
        let mut delay_l = Delay::new(sample_rate, memory, 1, 500.0);
        let mut delay_r = Delay::new(sample_rate, memory, 1, 500.0);
        delay_l.set_damp_taps(true);
        delay_r.set_damp_taps(true);
        let mut modulation_l = ModulatedDelay::new(sample_rate, memory, 1, DEPTH_SCALE * SWING_MS);
        let mut modulation_r = ModulatedDelay::new(sample_rate, memory, 1, DEPTH_SCALE * SWING_MS);
        modulation_l.set_dc_blocker(false);
        modulation_r.set_dc_blocker(false);
        let mut effect = Self {
            base,
            delay_l,
            delay_r,
            lfo: Lfo::new(sample_rate),
            modulation_l,
            modulation_r,
            output: StereoOutput::new(sample_rate),
            cross: false,
            feedback: 0.0,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSModulationDelay {
    gs_effect_common!(0x01_51);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let feedback = self.feedback;
        for i in 0..sample_count {
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            if self.cross {
                // The left echo reaches the right line two 32 kHz samples later, as in Stereo Delay
                // The right echo reaches the left line in the same sample
                let fb = self.delay_l.feedback(2.0);
                self.delay_r.process(r * LINE_INPUT + fb * feedback);
                self.delay_l
                    .process(l * LINE_INPUT + self.delay_r.output[0] * feedback);
            } else {
                let fb = self.delay_l.feedback(1.0);
                self.delay_l.process(l * LINE_INPUT + fb * feedback);
                let fb = self.delay_r.feedback(1.0);
                self.delay_r.process(r * LINE_INPUT + fb * feedback);
            }
            let phase = self.lfo.process();
            self.modulation_l.process(self.delay_l.output[0], phase);
            self.modulation_r.process(self.delay_r.output[0], phase);
            self.output.process(
                l,
                self.modulation_l.output[0],
                r,
                self.modulation_r.output[0],
            );
            output_left[i] = self.output.left as f32;
            output_right[i] = self.output.right as f32;
        }
    }

    fn clear(&mut self) {
        self.delay_l.clear();
        self.delay_r.clear();
        self.lfo.clear();
        self.modulation_l.clear();
        self.modulation_r.clear();
        self.output.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.delay_l.set_tap(0, delay3(p[0]));
        self.delay_r.set_tap(0, delay3(p[1]));
        self.feedback = feedback_gain(p[2]);
        self.cross = p[3] == 1;
        self.lfo.set_rate(rate1(self.base.controlled(0, 4)));
        let depth = DEPTH_SCALE * modulation_depth(p[5]);
        self.modulation_l.set_depth(0, depth);
        self.modulation_r.set_depth(0, depth);
        self.modulation_r.set_phase(0, chorus_phase(p[6]));
        self.delay_l.set_hf_damp(hf_damp(p[7]));
        self.delay_r.set_hf_damp(hf_damp(p[7]));
        self.output.set(
            self.base.controlled(1, 15),
            p[16],
            p[17],
            level_gain(p[19]) * STAGE_GAIN,
        );
    }
}
