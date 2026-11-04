//! The mono input of the mono wet types, with the left input one 32 kHz sample late.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/mono_mix.ts

use super::output_lag::OutputLag;

/// The mono input of the mono wet types: `0.5 L + 0.5 R`, with the left input one 32 kHz sample late.
/// Both hardware units have this skew, a centered source hears it as `(1 + z^-1) / 2` at 32 kHz (-3 dB at 8 kHz).
pub struct MonoMix {
    /// The two inputs of the last `process` as the mix takes them: the left one 32 kHz sample late. For the types
    /// that keep both channels and mix them into each output (the keyboard multis).
    pub skewed_left: f64,
    pub skewed_right: f64,
    left: OutputLag,
}

impl MonoMix {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            skewed_left: 0.0,
            skewed_right: 0.0,
            left: OutputLag::new(sample_rate, 1.0),
        }
    }

    #[inline]
    pub fn process(&mut self, left: f64, right: f64) -> f64 {
        let delayed = self.left.process(left);
        self.skewed_left = delayed;
        self.skewed_right = right;
        0.5 * (delayed + right)
    }

    pub fn clear(&mut self) {
        self.left.clear();
    }
}
