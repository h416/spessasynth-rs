//! Step Flanger (`01 24`): St. Flanger without the tone filter, whose LFO phase is held in steps.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/step_flanger.ts

use super::prelude::*;
use super::stereo_modulated_delay::GSStereoModulatedDelay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{chorus_phase, feedback_gain, modulation_depth, pre_delay, rate1};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x0a, 0x05, 0x5f, 0x4f, 0x5a, 0x36, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x40, 0x40, 0x40, 0x00, 0x60,
];

/// Step Flanger (`01 24`): St. Flanger without the tone filter, whose LFO phase is held in steps (twice per cycle of
/// the step rate) and smoothed.
/// Parameters: 1 pre delay, 2 rate (Rate1 column), 3 depth, 4 feedback, 5 phase, 6 step rate (Rate1 column),
/// 16 balance, 17 low gain, 18 high gain, 20 level. EFX control source 1 modulates the feedback, source 2 the step
/// rate.
pub struct GSStepFlanger {
    md: GSStereoModulatedDelay,
    /// The step clock: it climbs from -1 and takes a new LFO phase at 0.
    step_increment: f64,
    step_clock: f64,
    held_phase: f64,
}

impl GSStepFlanger {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let mut md = GSStereoModulatedDelay::new(sample_rate, memory, 1, DEFAULTS, &[]);
        md.delay_l.set_curve(true);
        md.delay_r.set_curve(true);
        md.delay_l.set_step_smoothing(true);
        md.delay_r.set_step_smoothing(true);
        let mut effect = Self {
            md,
            step_increment: 0.0,
            step_clock: 0.0,
            held_phase: 0.0,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSStepFlanger {
    gs_effect_common!(0x01_24; md.base);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let step_increment = self.step_increment;
        let step_clock = &mut self.step_clock;
        let held_phase = &mut self.held_phase;
        // Holds the LFO phase twice per cycle of the step rate (TS nextPhase)
        self.md
            .process(input_left, input_right, output_left, output_right, sample_count, |lfo| {
                let phase = lfo.process();
                *step_clock += step_increment;
                if *step_clock >= 0.0 {
                    *held_phase = phase;
                    *step_clock = -1.0;
                }
                *held_phase
            });
    }

    fn clear(&mut self) {
        self.md.clear();
        self.step_clock = 0.0;
        self.held_phase = 0.0;
    }

    fn update(&mut self) {
        let md = &mut self.md;
        let p = md.base.parameters;
        md.lfo.set_rate(rate1(p[1]));
        self.step_increment = (2.0 * rate1(md.base.controlled(1, 5))) / md.base.sample_rate;
        let feedback = feedback_gain(md.base.controlled(0, 3));
        for delay in md.delays() {
            delay.set_pre_delay(0, pre_delay(p[0]));
            delay.set_depth(0, modulation_depth(p[2]));
            delay.set_feedback(feedback);
        }
        md.delay_r.set_phase(0, chorus_phase(p[4]));
        md.set_output(p[15], p[16], p[17], p[19]);
    }
}
