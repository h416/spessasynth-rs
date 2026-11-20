//! The input stage of the GS system effects (reverb, chorus, delay).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/send_input.ts

use super::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};

/// The system effects' LEVEL is a gain over 64, so 64 is unity and 127 almost doubles.
pub const LEVEL_UNITY: f64 = 64.0;

/// Their sends, tap levels and the chorus feedback are gain bytes over 128, so 127 never quite reaches unity.
pub const GAIN_SCALE: f64 = 128.0;

/// The input of the system effects (reverb, chorus, delay): a DC blocker, then the PRE-LPF.
///
/// The PRE-LPF is the hardware's one-pole `y = (1 - B) x + B y[n-1]` at 32 kHz with B = value / 8.
pub struct SendInput {
    dc_blocker: FirstOrderFilter,
    low_pass: FirstOrderFilter,
}

impl SendInput {
    pub fn new(sample_rate: f64) -> Self {
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        Self {
            dc_blocker,
            low_pass: FirstOrderFilter::new(sample_rate),
        }
    }

    /// `value` is the PRE-LPF parameter, 0 (off) to 7.
    pub fn set_pre_low_pass(&mut self, value: f64) {
        if value <= 0.0 {
            self.low_pass.set_off();
        } else {
            self.low_pass.set_damp_pole(value.min(7.0) / 8.0);
        }
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.dc_blocker.process(x);
        self.low_pass.process(y)
    }

    pub fn clear(&mut self) {
        self.dc_blocker.clear();
        self.low_pass.clear();
    }
}
