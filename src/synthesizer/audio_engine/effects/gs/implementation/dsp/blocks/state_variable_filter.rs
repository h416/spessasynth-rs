//! The resonant filters of the amp simulators: the 32 kHz Chamberlin filters redesigned at the host rate.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/state_variable_filter.ts

use super::biquad::Biquad;

/// The rate the hardware runs its filters at.
const HARDWARE_RATE: f64 = 32_000.0;

/// The resonant filters of the amp simulators. On the hardware they are Chamberlin state variable filters at
/// 32 kHz, which at 4 kHz roll off far less than their nominal analog shape. This is that 32 kHz filter turned
/// back into its analog prototype (the inverse bilinear transform) and designed again at the host rate, so it
/// sounds the same at any rate and is exact at 32 kHz.
/// TS `StateVariableFilter extends Biquad`; Rust wraps a `Biquad`.
#[derive(Clone)]
pub struct StateVariableFilter {
    pub biquad: Biquad,
}

impl StateVariableFilter {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            biquad: Biquad::new(sample_rate),
        }
    }

    /// `frequency` is the corner in Hz, `q` the resonance.
    pub fn set_high_pass(&mut self, frequency: f64, q: f64) {
        let (w, resonance, s2) = self.prototype(frequency, q);
        // The Chamberlin high pass is (1 - z^-1)^2 over its poles, 4 s^2 on the analog side
        self.biquad.set_analog(4.0 / s2, 0.0, 0.0, 1.0, w / resonance, w * w);
    }

    /// `frequency` is the corner in Hz, `q` the resonance.
    pub fn set_low_pass(&mut self, frequency: f64, q: f64) {
        let (w, resonance, _) = self.prototype(frequency, q);
        // The Chamberlin low pass is k^2 z^-1 over its poles: unity at DC and a real zero pair
        // It sits at +-1 on the analog side (8 kHz on the prewarped axis at 32 kHz)
        let zero = self.biquad.prewarp(HARDWARE_RATE / 4.0);
        self.biquad.set_analog(-(w * w) / (zero * zero), 0.0, w * w, 1.0, w / resonance, w * w);
    }

    pub fn set_off(&mut self) {
        self.biquad.set_off();
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        self.biquad.process(x)
    }

    pub fn clear(&mut self) {
        self.biquad.clear();
    }

    /// The poles of the 32 kHz Chamberlin filter on the analog side, moved to the host rate's prewarped axis.
    /// Returns `(w, resonance, s2)`.
    fn prototype(&self, frequency: f64, q: f64) -> (f64, f64, f64) {
        let k = 2.0 * ((std::f64::consts::PI * frequency) / HARDWARE_RATE).sin();
        let dk = k / q;
        // At 32 kHz the denominator is s2 s^2 + 2 dk s + k^2
        let s2 = 4.0 - 2.0 * dk - k * k;
        let w32 = k / s2.sqrt();
        (
            self.biquad.prewarp((HARDWARE_RATE / std::f64::consts::PI) * w32.atan()),
            (k * s2.sqrt()) / (2.0 * dk),
            s2,
        )
    }
}
