//! One effect of the series and parallel combinations: the chorus, flanger and delay stages.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/combination_stage.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::delay::Delay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::lfo::Lfo;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::modulated_delay::ModulatedDelay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{STEP_MS, SWING_MS, delay3, feedback_gain, hf_damp, modulation_depth, pre_delay, rate1};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::enums::combination_stages;

/// A line fed by a mono mix of both sides: a quarter of each side (the mono mix halves them) and the 127/128 into
/// the line.
pub const STAGE_MIX_GAIN: f64 = (0.5 * 127.0) / 128.0;

/// One effect of the series and parallel combinations, with an output for the left and the right side. Its
/// parameters start at 1 or 6 (`base`), and the type may hand it one of them moved by an EFX control source.
/// Equivalent to: abstract class CombinationStage
pub trait CombinationStage {
    /// The left output of this sample (the same as the right for the mono effects).
    fn left(&self) -> f64;

    /// The right output of this sample.
    fn right(&self) -> f64;

    /// `parameters` is the type's parameters, `base` the index of the stage's first parameter (0 or 5),
    /// `control` the parameter the type's EFX control source moves in this stage, with the offset added
    /// (`None` where TS leaves it out).
    fn update(&mut self, parameters: &[u8; 20], base: usize, control: Option<u8>);

    /// `x` is the input, without the feedback.
    fn process(&mut self, x: f64);

    fn clear(&mut self);

    /// Called after a type select.
    fn settle(&mut self) {
        // Only the rotary has something to settle
    }
}

/// The chorus or flanger: pre delay, rate (Rate1 column), depth, feedback (flanger) from `base` on.
/// Read 0 sits at the pre delay and feeds back, the skewed side's read (or pair) is next in the read order.
///
/// In the series, drive and enhancer combinations each side has one read. The right one is 180 degrees off
/// (chorus) or in phase (flanger, on the hardware too). The right read feeds back, which is the left one on the
/// hardware: only the flanger has feedback and its reads are in phase.
/// In the parallel combinations each side mixes two reads 180 degrees apart at half level (`paired`), the flanger
/// too.
pub struct ModulationStage {
    left: f64,
    right: f64,
    lfo: Lfo,
    delay: ModulatedDelay,
    flanger: bool,
    paired: bool,
    /// Each read's offset from the pre delay in ms.
    offsets: Vec<f64>,
    /// The first read of each side.
    left_read: usize,
    right_read: usize,
}

impl ModulationStage {
    /// `flanger` is the flanger (the squared curve, the feedback) instead of the chorus, `skew` the skewed side
    /// (see `create_combination_stage`), `paired` two reads for each side.
    pub fn new(sample_rate: f64, memory: &mut DelayMemory, flanger: bool, skew: i32, paired: bool) -> Self {
        let per_side = if paired { 2 } else { 1 };
        // A paired stage that is not skewed has one pair for both sides
        let skewed = if paired && skew == 0 { 0 } else { per_side };
        let mut offsets = vec![0.0; skewed + per_side];
        offsets[skewed..].fill(skew as f64 * STEP_MS);
        let left_read = if skew > 0 { 0 } else { skewed };
        let right_read = if skew > 0 { skewed } else { 0 };
        let mut delay = ModulatedDelay::new(sample_rate, memory, offsets.len(), 101.0 + SWING_MS);
        delay.set_curve(flanger);
        if paired {
            let mut read = 1;
            while read < offsets.len() {
                delay.set_phase(read, 180.0);
                read += 2;
            }
        } else {
            delay.set_phase(right_read, if flanger { 0.0 } else { 180.0 });
        }
        Self {
            left: 0.0,
            right: 0.0,
            lfo: Lfo::new(sample_rate),
            delay,
            flanger,
            paired,
            offsets,
            left_read,
            right_read,
        }
    }
}

impl CombinationStage for ModulationStage {
    fn left(&self) -> f64 {
        self.left
    }

    fn right(&self) -> f64 {
        self.right
    }

    /// `control` is the feedback parameter (FL -> Delay moves it with EFX control source 1), `parameters[base + 3]`
    /// when `None`.
    fn update(&mut self, parameters: &[u8; 20], base: usize, control: Option<u8>) {
        let feedback = control.unwrap_or(parameters[base + 3]);
        let pre = pre_delay(parameters[base]);
        let depth = modulation_depth(parameters[base + 2]);
        self.lfo.set_rate(rate1(parameters[base + 1]));
        for read in 0..self.offsets.len() {
            self.delay.set_pre_delay(read, pre + self.offsets[read]);
            self.delay.set_depth(read, depth);
        }
        self.delay
            .set_feedback(if self.flanger { feedback_gain(feedback) } else { 0.0 });
    }

    #[inline]
    fn process(&mut self, x: f64) {
        let phase = self.lfo.process();
        self.delay.process(x, phase);
        let reads = &self.delay.output;
        let left_read = self.left_read;
        let right_read = self.right_read;
        if self.paired {
            self.left = (reads[left_read] + reads[left_read + 1]) * 0.5;
            self.right = (reads[right_read] + reads[right_read + 1]) * 0.5;
        } else {
            self.left = reads[left_read];
            self.right = reads[right_read];
        }
    }

    fn clear(&mut self) {
        self.lfo.clear();
        self.delay.clear();
    }
}

/// The delay: delay (Dly3 column), feedback, HF damp (on the echoes and in the loop) from `base` on.
/// Tap 0 sits at the delay and feeds back, tap 1 is the skewed side's.
struct DelayStage {
    left: f64,
    right: f64,
    delay: Delay,
    /// The skewed tap's offset in ms.
    skew: f64,
    left_tap: usize,
    right_tap: usize,
    feedback: f64,
}

impl DelayStage {
    fn new(sample_rate: f64, memory: &mut DelayMemory, skew: i32) -> Self {
        let mut delay = Delay::new(sample_rate, memory, 2, 500.0);
        delay.set_damp_taps(true);
        Self {
            left: 0.0,
            right: 0.0,
            delay,
            skew: skew as f64 * STEP_MS,
            left_tap: if skew > 0 { 0 } else { 1 },
            right_tap: if skew > 0 { 1 } else { 0 },
            feedback: 0.0,
        }
    }
}

impl CombinationStage for DelayStage {
    fn left(&self) -> f64 {
        self.left
    }

    fn right(&self) -> f64 {
        self.right
    }

    fn update(&mut self, parameters: &[u8; 20], base: usize, _control: Option<u8>) {
        let time = delay3(parameters[base]);
        self.delay.set_tap(0, time);
        // The shortest delay is one 32 kHz sample, so an earlier tap is never ahead of the input
        self.delay.set_tap(1, time + self.skew);
        self.feedback = feedback_gain(parameters[base + 1]);
        self.delay.set_hf_damp(hf_damp(parameters[base + 2]));
    }

    #[inline]
    fn process(&mut self, x: f64) {
        let fb = self.delay.feedback(1.0);
        self.delay.process(x + fb * self.feedback);
        self.left = self.delay.output[self.left_tap];
        self.right = self.delay.output[self.right_tap];
    }

    fn clear(&mut self) {
        self.delay.clear();
    }
}

/// Builds a chorus, flanger or delay stage. Call it after the type's `GSEffectBase` constructor, which rewinds the
/// delay memory.
///
/// The shared stage reads the left input one 32 kHz sample late and writes the right output one sample late, so
/// the two sides read the line a sample apart: -1 for the left read one sample earlier (a line fed from both
/// inputs), 1 for the right read one sample later (the parallel combinations' effect 2, whose chorus/flanger mixes
/// two reads per side), 0 for neither (the drive combinations, whose dry signal is as late as the line's input).
/// `r#type` is the stage, `skew` the skewed side: -1, 0 or 1.
pub fn create_combination_stage(
    r#type: u8,
    sample_rate: f64,
    memory: &mut DelayMemory,
    skew: i32,
) -> Box<dyn CombinationStage> {
    if r#type == combination_stages::DELAY {
        Box::new(DelayStage::new(sample_rate, memory, skew))
    } else {
        Box::new(ModulationStage::new(
            sample_rate,
            memory,
            r#type == combination_stages::FLANGER,
            skew,
            skew > 0,
        ))
    }
}
