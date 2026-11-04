//! Bass Multi's amp simulator: a high pass, a resonant high pass and a resonant low pass.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/bass_amp_simulator.ts

use super::first_order_filter::FirstOrderFilter;
use super::state_variable_filter::StateVariableFilter;

/// Equivalent to: interface BassAmpModel
struct BassAmpModel {
    /// The first order high pass in front (corner in Hz, gain at Nyquist), or none.
    high_pass: Option<(f64, f64)>,
    /// The resonant high pass (corner in Hz, Q 4), or none.
    resonant_high_pass: Option<f64>,
    /// The resonant low pass (corner in Hz, Q).
    low_pass: (f64, f64),
    /// The amp's flat gains put together.
    gain: f64,
}

/// The three bass amps. The resonances are the hardware's Chamberlin filters at 32 kHz (their coefficient words
/// give the corners). The gains are the input gains of both filters and the output level.
const BASS_AMP_MODELS: [BassAmpModel; 3] = [
    BassAmpModel {
        high_pass: Some((39.945, 0.996_08)),
        resonant_high_pass: Some(162.43),
        low_pass: (2000.0, 4.0),
        gain: 0.5 * (127.0 / 128.0) * 0.9375,
    },
    BassAmpModel {
        high_pass: Some((80.2, 1.0)),
        resonant_high_pass: None,
        low_pass: (2000.0, 8.0),
        // The resonant high pass is off, which leaves half of its input gain
        gain: 0.25 * (127.0 / 128.0) * 2.5,
    },
    BassAmpModel {
        high_pass: None,
        resonant_high_pass: Some(39.945),
        low_pass: (4000.0, 4.0),
        gain: (127.0 / 128.0) * 0.5 * (127.0 / 128.0) * 1.875,
    },
];

/// Bass Multi's amp simulator: a first order high pass, a resonant high pass and a resonant low pass (each Q 4 or
/// more), so a boost at the bottom and the top of the bass range.
pub struct BassAmpSimulator {
    high_pass: FirstOrderFilter,
    resonant_high_pass: StateVariableFilter,
    low_pass: StateVariableFilter,
    gain: f64,
}

impl BassAmpSimulator {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            high_pass: FirstOrderFilter::new(sample_rate),
            resonant_high_pass: StateVariableFilter::new(sample_rate),
            low_pass: StateVariableFilter::new(sample_rate),
            gain: 0.0,
        }
    }

    /// The amp (0-2).
    pub fn set_type(&mut self, r#type: u8) {
        let m = &BASS_AMP_MODELS[r#type as usize];
        if let Some((frequency, gain)) = m.high_pass {
            self.high_pass.set_high_pass(frequency, gain);
        } else {
            self.high_pass.set_off();
        }
        if let Some(frequency) = m.resonant_high_pass {
            self.resonant_high_pass.set_high_pass(frequency, 4.0);
        } else {
            self.resonant_high_pass.set_off();
        }
        self.low_pass.set_low_pass(m.low_pass.0, m.low_pass.1);
        self.gain = m.gain;
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let a = self.high_pass.process(x);
        let b = self.resonant_high_pass.process(a);
        self.gain * self.low_pass.process(b)
    }

    pub fn clear(&mut self) {
        self.high_pass.clear();
        self.resonant_high_pass.clear();
        self.low_pass.clear();
    }
}
