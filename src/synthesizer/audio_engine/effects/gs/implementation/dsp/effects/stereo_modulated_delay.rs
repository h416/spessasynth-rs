//! The stereo choruses and flangers: per channel a modulated delay on its own line, one LFO for both.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/stereo_modulated_delay.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::lfo::Lfo;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::modulated_delay::ModulatedDelay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::stereo_output::StereoOutput;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::SWING_MS;

/// The longest read in ms: the 100 ms pre delay and the swing, with a ms to spare (Space D's right line reads up to
/// half a ms later).
const LONGEST_MS: f64 = 101.0 + SWING_MS;

/// The stereo choruses and flangers: per channel a modulated delay on its own line, one LFO for both (the right
/// read offset by the phase), then the balance and the 2-band EQ.
///
/// TS has this as an abstract class; the Rust types hold it and implement `GSEffect` themselves.
pub struct GSStereoModulatedDelay {
    pub base: GSEffectBase,
    pub lfo: Lfo,
    pub delay_l: ModulatedDelay,
    pub delay_r: ModulatedDelay,
    pub output: StereoOutput,
}

impl GSStereoModulatedDelay {
    /// `reads` is the reads on each line (1 by default in TS).
    pub fn new(
        sample_rate: f64,
        memory: &mut DelayMemory,
        reads: usize,
        defaults: [u8; 20],
        limits: &'static [(u8, u8)],
    ) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), defaults, limits, &[]);
        let delay_l = ModulatedDelay::new(sample_rate, memory, reads, LONGEST_MS);
        let delay_r = ModulatedDelay::new(sample_rate, memory, reads, LONGEST_MS);
        Self {
            base,
            lfo: Lfo::new(sample_rate),
            delay_l,
            delay_r,
            output: StereoOutput::new(sample_rate),
        }
    }

    /// Both lines, for the settings they share.
    /// Equivalent to: delays
    pub fn delays(&mut self) -> [&mut ModulatedDelay; 2] {
        [&mut self.delay_l, &mut self.delay_r]
    }

    /// The process loop, with the LFO phase of each sample from `next_phase` (the step flanger holds it).
    #[inline]
    pub fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
        mut next_phase: impl FnMut(&mut Lfo) -> f64,
    ) {
        for i in 0..sample_count {
            let phase = next_phase(&mut self.lfo);
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            self.delay_l.process(l * LINE_INPUT, phase);
            self.delay_r.process(r * LINE_INPUT, phase);
            self.output
                .process(l, self.delay_l.output[0], r, self.delay_r.output[0]);
            output_left[i] = self.output.left as f32;
            output_right[i] = self.output.right as f32;
        }
    }

    pub fn clear(&mut self) {
        self.lfo.clear();
        self.delay_l.clear();
        self.delay_r.clear();
        self.output.clear();
    }

    /// `balance`, `low`, `high` and `level` are the balance, low gain, high gain and level parameters.
    pub fn set_output(&mut self, balance: u8, low: u8, high: u8, level: u8) {
        self.output.set(balance, low, high, level_gain(level) * STAGE_GAIN);
    }
}
