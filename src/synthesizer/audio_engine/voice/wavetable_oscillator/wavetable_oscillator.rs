//! Plays back raw audio data at an arbitrary playback rate.
//! Ported from: src/synthesizer/audio_engine/voice/wavetable_oscillator/wavetable_oscillator.ts
//!
//! TS has one subclass per interpolation type (`linear.ts`, `nearest.ts`, `hermite.ts`);
//! Rust keeps a single struct and dispatches on `interpolation_type`. The per-type
//! `process` bodies live in the sibling files of the same names.
use crate::synthesizer::enums::{InterpolationType, interpolation_types};

/// Shared state for all wavetable oscillator variants.
/// Equivalent to: WavetableOscillator (abstract class fields)
pub struct WavetableOscillator {
    /// Interpolation mode. One of `interpolation_types` constants.
    pub interpolation_type: InterpolationType,
    /// Is the loop on?
    /// Equivalent to: isLooping
    pub is_looping: bool,
    /// Sample data of the voice.
    /// Equivalent to: sampleData
    pub sample_data: Option<Vec<f32>>,
    /// Playback step (rate) for sample pitch correction.
    /// Equivalent to: playbackStep
    pub playback_step: f64,
    /// Start position of the loop.
    /// Equivalent to: loopStart
    pub loop_start: f64,
    /// End position of the loop.
    /// Equivalent to: loopEnd
    pub loop_end: f64,
    /// Length of the loop.
    /// Equivalent to: loopLength
    pub loop_length: f64,
    /// End position of the sample.
    /// Equivalent to: end
    pub end: f64,
    /// The current cursor of the sample.
    /// Equivalent to: cursor
    pub cursor: f64,
    /// Whether the cursor has wrapped around the loop at least once.
    /// Only the hermite interpolator reads it (for the point before the cursor).
    /// Equivalent to: hasLooped
    pub has_looped: bool,
}

impl WavetableOscillator {
    /// Creates a new WavetableOscillator with default values.
    pub fn new(interpolation_type: InterpolationType) -> Self {
        Self {
            interpolation_type,
            is_looping: false,
            sample_data: None,
            playback_step: 0.0,
            loop_start: 0.0,
            loop_end: 0.0,
            loop_length: 0.0,
            end: 0.0,
            cursor: 0.0,
            has_looped: false,
        }
    }

    /// Fills the output buffer with raw sample data using the configured interpolation.
    /// Returns `true` if the voice is still active, `false` if it has finished.
    /// Equivalent to: process (abstract method dispatched per subclass)
    pub fn process(
        &mut self,
        sample_count: usize,
        tuning_ratio: f64,
        output_buffer: &mut [f64],
    ) -> bool {
        // TS asserts `sampleData!`; a voice without data has nothing to play.
        if self.sample_data.is_none() {
            return false;
        }
        match self.interpolation_type {
            interpolation_types::NEAREST_NEIGHBOR => {
                self.process_nearest(sample_count, tuning_ratio, output_buffer)
            }
            interpolation_types::HERMITE => {
                self.process_hermite(sample_count, tuning_ratio, output_buffer)
            }
            // Default: LINEAR (interpolation_types::LINEAR == 0)
            _ => self.process_linear(sample_count, tuning_ratio, output_buffer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_sample_data_returns_false_linear() {
        let mut osc = WavetableOscillator::new(interpolation_types::LINEAR);
        let mut buf = vec![0.0f64; 4];
        assert!(!osc.process(4, 1.0, &mut buf));
    }

    #[test]
    fn no_sample_data_returns_false_nearest() {
        let mut osc = WavetableOscillator::new(interpolation_types::NEAREST_NEIGHBOR);
        let mut buf = vec![0.0f64; 4];
        assert!(!osc.process(4, 1.0, &mut buf));
    }

    #[test]
    fn no_sample_data_returns_false_hermite() {
        let mut osc = WavetableOscillator::new(interpolation_types::HERMITE);
        let mut buf = vec![0.0f64; 4];
        assert!(!osc.process(4, 1.0, &mut buf));
    }
}
