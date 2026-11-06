//! The default GS equalizer: a low shelf into a high shelf, both first order, per side.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/equalizer.ts
//!              src/synthesizer/audio_engine/effects/gs/interface/gs_equalizer_processor.ts

use std::f64::consts::PI;

// Measured on the SC-88Pro.
const LOW_FREQUENCIES: [f64; 2] = [225.0, 425.0];
const HIGH_FREQUENCIES: [f64; 2] = [3000.0, 6000.0];

/// The GS equalizer parameters (`40 02 xx`).
/// Equivalent to: interface GSEqualizerParameter
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GSEqualizerParameter {
    /// `0-1`: 200 Hz or 400 Hz. Equivalent to: lowFrequency
    pub low_frequency: u8,
    /// `0x34-0x4c`: -12 dB to +12 dB. Equivalent to: lowGain
    pub low_gain: u8,
    /// `0-1`: 3 kHz or 6 kHz. Equivalent to: highFrequency
    pub high_frequency: u8,
    /// `0x34-0x4c`: -12 dB to +12 dB. Equivalent to: highGain
    pub high_gain: u8,
}

/// The default GS equalizer parameters (SC-8850 manual p.236).
/// Equivalent to: DEFAULT_GS_EQUALIZER (processor.ts)
pub const DEFAULT_GS_EQUALIZER: GSEqualizerParameter = GSEqualizerParameter {
    low_frequency: 0,
    low_gain: 0x40,
    high_frequency: 0,
    high_gain: 0x40,
};

/// One shelf band's design values.
struct Band {
    k: f64,
    g: f64,
    cut: bool,
}

/// The default GS equalizer implementation.
/// Equivalent to: DefaultGSEqualizer
pub struct DefaultGSEqualizer {
    sample_rate: f64,
    // Normalized first order coefficients (a0 = 1)
    low_b0: f64,
    low_b1: f64,
    low_a1: f64,
    high_b0: f64,
    high_b1: f64,
    high_a1: f64,
    // Transposed direct form II state
    low_zl: f64,
    low_zr: f64,
    high_zl: f64,
    high_zr: f64,
    low_frequency: u8,
    low_gain: u8,
    high_frequency: u8,
    high_gain: u8,
}

impl DefaultGSEqualizer {
    /// Constructs a new default GS equalizer processor.
    /// Equivalent to: constructor(sampleRate)
    pub fn new(sample_rate: f64) -> Self {
        let mut eq = Self {
            sample_rate,
            low_b0: 1.0,
            low_b1: 0.0,
            low_a1: 0.0,
            high_b0: 1.0,
            high_b1: 0.0,
            high_a1: 0.0,
            low_zl: 0.0,
            low_zr: 0.0,
            high_zl: 0.0,
            high_zr: 0.0,
            low_frequency: 0,
            low_gain: 64,
            high_frequency: 0,
            high_gain: 64,
        };
        eq.update_low();
        eq.update_high();
        eq
    }

    /// Equivalent to: get lowFrequency
    pub fn low_frequency(&self) -> u8 {
        self.low_frequency
    }

    /// Equivalent to: set lowFrequency
    pub fn set_low_frequency(&mut self, value: u8) {
        self.low_frequency = value;
        self.update_low();
    }

    /// Equivalent to: get lowGain
    pub fn low_gain(&self) -> u8 {
        self.low_gain
    }

    /// Equivalent to: set lowGain
    pub fn set_low_gain(&mut self, value: u8) {
        self.low_gain = value;
        self.update_low();
    }

    /// Equivalent to: get highFrequency
    pub fn high_frequency(&self) -> u8 {
        self.high_frequency
    }

    /// Equivalent to: set highFrequency
    pub fn set_high_frequency(&mut self, value: u8) {
        self.high_frequency = value;
        self.update_high();
    }

    /// Equivalent to: get highGain
    pub fn high_gain(&self) -> u8 {
        self.high_gain
    }

    /// Equivalent to: set highGain
    pub fn set_high_gain(&mut self, value: u8) {
        self.high_gain = value;
        self.update_high();
    }

    /// Sets a parameter by its address (`40 02 a3`): 0 low frequency, 1 low gain,
    /// 2 high frequency, 3 high gain. Returns false for an unknown address.
    /// Equivalent to: `this.gsEqualizerProcessor[parameter] = value` (roland.ts / processor.ts)
    pub fn set_parameter(&mut self, address: u8, value: u8) -> bool {
        match address {
            0 => self.set_low_frequency(value),
            1 => self.set_low_gain(value),
            2 => self.set_high_frequency(value),
            3 => self.set_high_gain(value),
            _ => return false,
        }
        true
    }

    /// Processes the equalizer: reads the zero-indexed stereo input
    /// and ADDS to the `start_index`-based stereo output.
    /// Equivalent to: process(inputLeft, inputRight, outputLeft, outputRight, startIndex, sampleCount)
    pub fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        let (low_b0, low_b1, low_a1) = (self.low_b0, self.low_b1, self.low_a1);
        let (high_b0, high_b1, high_a1) = (self.high_b0, self.high_b1, self.high_a1);
        let mut low_zl = self.low_zl;
        let mut low_zr = self.low_zr;
        let mut high_zl = self.high_zl;
        let mut high_zr = self.high_zr;
        for i in 0..sample_count {
            let o = start_index + i;

            // Left: low shelf into high shelf
            let mut x = input_left[i] as f64;
            let mut y = low_b0 * x + low_zl;
            low_zl = low_b1 * x - low_a1 * y;
            x = y;
            y = high_b0 * x + high_zl;
            high_zl = high_b1 * x - high_a1 * y;
            output_left[o] = (output_left[o] as f64 + y) as f32;

            // Right
            x = input_right[i] as f64;
            y = low_b0 * x + low_zr;
            low_zr = low_b1 * x - low_a1 * y;
            x = y;
            y = high_b0 * x + high_zr;
            high_zr = high_b1 * x - high_a1 * y;
            output_right[o] = (output_right[o] as f64 + y) as f32;
        }
        self.low_zl = low_zl;
        self.low_zr = low_zr;
        self.high_zl = high_zl;
        self.high_zr = high_zr;
    }

    /// Equivalent to: getSnapshot()
    pub fn get_snapshot(&self) -> GSEqualizerParameter {
        GSEqualizerParameter {
            low_frequency: self.low_frequency,
            low_gain: self.low_gain,
            high_frequency: self.high_frequency,
            high_gain: self.high_gain,
        }
    }

    /// Equivalent to: applySnapshot(snapshot)
    pub fn apply_snapshot(&mut self, snapshot: &GSEqualizerParameter) {
        self.set_low_frequency(snapshot.low_frequency);
        self.set_low_gain(snapshot.low_gain);
        self.set_high_frequency(snapshot.high_frequency);
        self.set_high_gain(snapshot.high_gain);
    }

    /// Equivalent to: getBand(frequency, gain) (private)
    fn get_band(&self, frequency: f64, gain: u8) -> Band {
        // Keep the corner below Nyquist for low sample rates
        let fc = frequency.min(self.sample_rate * 0.45);
        let db = (-12.0f64).max(12.0f64.min(gain as f64 - 64.0));
        Band {
            k: ((PI * fc) / self.sample_rate).tan(),
            g: 10f64.powf(db.abs() / 20.0),
            cut: db < 0.0,
        }
    }

    /// Equivalent to: updateLow() (private)
    fn update_low(&mut self) {
        let Band { k, g, cut } =
            self.get_band(LOW_FREQUENCIES[(self.low_frequency & 1) as usize], self.low_gain);
        let mut b0 = 1.0 + k;
        let mut b1 = k - 1.0;
        let mut a0 = 1.0 + k / g;
        let mut a1 = k / g - 1.0;
        if cut {
            (b0, b1, a0, a1) = (a0, a1, b0, b1);
        }
        self.low_b0 = b0 / a0;
        self.low_b1 = b1 / a0;
        self.low_a1 = a1 / a0;
    }

    /// Equivalent to: updateHigh() (private)
    fn update_high(&mut self) {
        let Band { k, g, cut } =
            self.get_band(HIGH_FREQUENCIES[(self.high_frequency & 1) as usize], self.high_gain);
        let mut b0 = g * (1.0 + k);
        let mut b1 = g * (k - 1.0);
        let mut a0 = 1.0 + g * k;
        let mut a1 = g * k - 1.0;
        if cut {
            (b0, b1, a0, a1) = (a0, a1, b0, b1);
        }
        self.high_b0 = b0 / a0;
        self.high_b1 = b1 / a0;
        self.high_a1 = a1 / a0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_identity() {
        let mut eq = DefaultGSEqualizer::new(48_000.0);
        let input: Vec<f32> = (0..64).map(|i| ((i as f32) * 0.37).sin()).collect();
        let mut l = vec![0.0f32; 64];
        let mut r = vec![0.0f32; 64];
        eq.process(&input, &input, &mut l, &mut r, 0, 64);
        assert_eq!(l, input);
        assert_eq!(r, input);
    }

    #[test]
    fn test_set_parameter_addresses() {
        let mut eq = DefaultGSEqualizer::new(44_100.0);
        assert!(eq.set_parameter(0, 1));
        assert!(eq.set_parameter(1, 0x4c));
        assert!(eq.set_parameter(2, 1));
        assert!(eq.set_parameter(3, 0x34));
        assert!(!eq.set_parameter(4, 0));
        assert_eq!(
            eq.get_snapshot(),
            GSEqualizerParameter { low_frequency: 1, low_gain: 0x4c, high_frequency: 1, high_gain: 0x34 }
        );
    }
}
