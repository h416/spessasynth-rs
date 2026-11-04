//! St. Chorus and St. Flanger: per channel `tone filter -> modulated delay`, then the balance and the 2-band EQ.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/stereo_chorus_flanger.ts

use super::prelude::*;
use super::stereo_modulated_delay::GSStereoModulatedDelay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{chorus_phase, cutoff, feedback_gain, modulation_depth, pre_delay, rate1};

/// The line write's gain, inside the loop.
const LINE_GAIN: f64 = 127.0 / 128.0;

const LIMITS: &[(u8, u8)] = &[(0, 2)];

/// St. Chorus and St. Flanger: per channel `tone filter -> modulated delay`, then the balance and the 2-band EQ.
/// Parameters: 1 filter type (off, LPF, HPF), 2 cutoff (Cut column), 3 pre delay, 4 rate (Rate1 column), 5 depth,
/// 6 feedback (flanger), 7 phase, 16 balance, 17 low gain, 18 high gain, 20 level. EFX control source 1 modulates the
/// rate, source 2 the balance (chorus) or the feedback (flanger).
pub struct GSStereoChorusFlanger {
    md: GSStereoModulatedDelay,
    effect_type: u16,
    flanger: bool,
}

impl GSStereoChorusFlanger {
    /// The subclass constructor resets.
    pub fn new(
        sample_rate: f64,
        memory: &mut DelayMemory,
        effect_type: u16,
        defaults: [u8; 20],
        flanger: bool,
    ) -> Self {
        Self {
            md: GSStereoModulatedDelay::new(sample_rate, memory, 1, defaults, LIMITS),
            effect_type,
            flanger,
        }
    }
}

impl GSEffect for GSStereoChorusFlanger {
    fn effect_type(&self) -> u16 {
        self.effect_type
    }

    gs_effect_common!(@base md.base);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        self.md
            .process(input_left, input_right, output_left, output_right, sample_count, |lfo| {
                lfo.process()
            });
    }

    fn clear(&mut self) {
        self.md.clear();
    }

    fn update(&mut self) {
        let md = &mut self.md;
        let p = md.base.parameters;
        md.lfo.set_rate(rate1(md.base.controlled(0, 3)));
        let feedback = if self.flanger {
            feedback_gain(md.base.controlled(1, 5))
        } else {
            0.0
        };
        for delay in md.delays() {
            delay.set_curve(self.flanger);
            delay.set_pre_delay(0, pre_delay(p[2]));
            delay.set_depth(0, modulation_depth(p[4]));
            delay.set_feedback(feedback);
            // Off is one more 127/128
            if p[0] == 0 {
                delay.tone.set_gain(LINE_GAIN * (127.0 / 128.0));
            } else {
                delay.tone.set_tone(p[0] == 2, cutoff(p[1]), LINE_GAIN);
            }
        }
        md.delay_r.set_phase(0, chorus_phase(p[6]));
        let balance = if self.flanger { p[15] } else { md.base.controlled(1, 15) };
        md.set_output(balance, p[16], p[17], p[19]);
    }
}
