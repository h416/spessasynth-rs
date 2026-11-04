//! The guitar amp simulator: shelf, resonant high pass, cabinet resonance, resonant low pass and cabinet echo.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/amp_simulator.ts

use super::biquad::Biquad;
use super::delay_line::DelayLine;
use super::first_order_filter::FirstOrderFilter;
use super::state_variable_filter::StateVariableFilter;

/// The cabinet echo of the 3 Stack (27 samples on the hardware).
const COMB_SECONDS: f64 = 27.0 / 32_000.0;

/// Equivalent to: interface AmpModel
struct AmpModel {
    /// The low shelf in front of the amp (corner in Hz, gain in dB), or none.
    shelf: Option<(f64, f64)>,
    high_pass: (f64, f64),
    /// The cabinet resonance: poles in Hz and Q, zeros in Hz and Q.
    cabinet: Option<(f64, f64, f64, f64)>,
    low_pass: (f64, f64),
    gain: f64,
    /// The level of the cabinet echo.
    echo: f64,
}

/// The four amps: Small, Built-In, 2 Stack and 3 Stack. The resonances are the hardware's (Q 1.5 is a damping of
/// 85/128). The gains are each amp's flat input gain, filter input gains and output level put together.
const AMP_MODELS: [AmpModel; 4] = [
    AmpModel {
        shelf: Some((995.0, -6.0)),
        high_pass: (200.0, 128.0 / 85.0),
        // +6 dB at 500 Hz
        cabinet: Some((499.4, 2.0, 499.1, 1.0)),
        low_pass: (4000.0, 128.0 / 85.0),
        gain: 0.565,
        echo: 0.0,
    },
    AmpModel {
        shelf: None,
        high_pass: (100.0, 128.0 / 127.0),
        // -10 dB at 396 Hz
        cabinet: Some((395.7, 0.4946, 395.3, 1.5645)),
        low_pass: (4000.0, 128.0 / 127.0),
        gain: 1.7543,
        echo: 0.0,
    },
    AmpModel {
        shelf: None,
        high_pass: (100.0, 128.0 / 85.0),
        cabinet: None,
        low_pass: (3500.0, 128.0 / 85.0),
        gain: 1.0749,
        echo: 0.0,
    },
    AmpModel {
        shelf: Some((1390.0, -6.0)),
        high_pass: (100.0, 2.0),
        // A resonance at 95 Hz over a dip at 109 Hz, +2.4 dB at DC
        cabinet: Some((94.94, 2.575, 109.1, 1.85)),
        low_pass: (4000.0, 128.0 / 85.0),
        gain: 1.3038,
        echo: 0.4648,
    },
];

/// The guitar amp simulator: a low shelf, a resonant high pass, the cabinet resonance, a resonant low pass and
/// (3 Stack only) a short cabinet echo.
pub struct AmpSimulator {
    shelf: FirstOrderFilter,
    high_pass: StateVariableFilter,
    cabinet: Biquad,
    low_pass: StateVariableFilter,
    comb: DelayLine,
    comb_delay: f64,
    gain: f64,
    echo: f64,
}

impl AmpSimulator {
    pub fn new(sample_rate: f64) -> Self {
        let comb_delay = COMB_SECONDS * sample_rate;
        Self {
            shelf: FirstOrderFilter::new(sample_rate),
            high_pass: StateVariableFilter::new(sample_rate),
            cabinet: Biquad::new(sample_rate),
            low_pass: StateVariableFilter::new(sample_rate),
            comb: DelayLine::new(comb_delay.ceil() + 1.0),
            comb_delay,
            gain: 0.0,
            echo: 0.0,
        }
    }

    /// The amp (0-3: Small, Built-In, 2 Stack, 3 Stack).
    pub fn set_type(&mut self, r#type: u8) {
        let m = &AMP_MODELS[r#type as usize];
        if let Some((frequency, gain)) = m.shelf {
            self.shelf.set_low_shelf(frequency, gain);
        } else {
            self.shelf.set_off();
        }
        self.high_pass.set_high_pass(m.high_pass.0, m.high_pass.1);
        if let Some((pole, pole_q, zero, zero_q)) = m.cabinet {
            self.cabinet.set_pole_zero(pole, pole_q, zero, zero_q);
        } else {
            self.cabinet.set_off();
        }
        self.low_pass.set_low_pass(m.low_pass.0, m.low_pass.1);
        self.gain = m.gain;
        self.echo = m.echo;
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let a = self.shelf.process(x);
        let b = self.high_pass.process(a);
        let c = self.cabinet.process(b);
        let y = self.low_pass.process(c);
        self.comb.write(y);
        self.gain * y + self.echo * self.comb.read_linear(self.comb_delay)
    }

    pub fn clear(&mut self) {
        self.shelf.clear();
        self.high_pass.clear();
        self.cabinet.clear();
        self.low_pass.clear();
        self.comb.clear();
    }
}
