//! Lo-Fi 1's quantizer: truncation, a DC offset, a sample and hold and the hardware's two-sample average.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/lofi_quantizer.ts

use super::delay_line::DelayLine;
use super::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};

#[derive(Clone, Copy)]
struct Resolution {
    steps: f64,
    offset: f64,
    sign: f64,
}

/// Lo-Fi 1's types (0-8) as three resolutions by three hold rates.
const RESOLUTIONS: [Resolution; 3] = [
    // 15 bits with the sign, a tiny DC offset, the wet inverted
    Resolution { steps: 16384.0, offset: 1.0 / 1024.0, sign: -1.0 },
    // 11 bits, a small offset, inverted
    Resolution { steps: 1024.0, offset: 1.0 / 64.0, sign: -1.0 },
    // 11 bits, a quarter of full scale as offset (the positive peaks clip at 0.75), not inverted
    Resolution { steps: 1024.0, offset: 0.25, sign: 1.0 },
];

/// The hold periods in 32 kHz samples: 16000, 10667 and 8000 Hz.
const HOLD_PERIODS: [f64; 3] = [2.0, 3.0, 4.0];

/// The newer samples the band-limited read needs around a hold instant.
const LOOKAHEAD: f64 = 8.0;

/// Lo-Fi 1's quantizer: truncation to 15 or 11 bits, a DC offset and a clamp at full scale, a DC blocker that takes
/// the offset out again, a sample and hold at 16000, 10667 or 8000 Hz and the hardware's two-sample average (a
/// first order low pass at 8 kHz).
pub struct LoFiQuantizer {
    /// How many host samples late the output is: `LOOKAHEAD` where the instants fall between samples, else 0.
    pub latency: f64,
    dc_blocker: FirstOrderFilter,
    smoothing: FirstOrderFilter,
    /// One 32 kHz sample in host samples.
    step: f64,
    resolution: Resolution,
    /// The hold period in host samples.
    period: f64,
    /// Host samples from the start of this output sample to the next hold instant.
    until_hold: f64,
    held: f64,
    /// Where the hold instants fall between host samples, the hold reads its value at the exact instant from this
    /// line (band-limited, which needs `LOOKAHEAD` newer samples).
    line: DelayLine,
}

impl LoFiQuantizer {
    pub fn new(sample_rate: f64) -> Self {
        let step = sample_rate / 32_000.0;
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        let mut smoothing = FirstOrderFilter::new(sample_rate);
        smoothing.set_low_pass(8000.0, 127.0 / 128.0);
        Self {
            latency: if step.fract() == 0.0 { 0.0 } else { LOOKAHEAD },
            dc_blocker,
            smoothing,
            step,
            resolution: RESOLUTIONS[0],
            period: 2.0,
            until_hold: 0.0,
            held: 0.0,
            line: DelayLine::new(LOOKAHEAD + 2.0),
        }
    }

    /// `type` is the Lo-Fi type (0-8).
    pub fn set_type(&mut self, r#type: u8) {
        self.resolution = RESOLUTIONS[(r#type / 3) as usize];
        self.period = HOLD_PERIODS[(r#type % 3) as usize] * self.step;
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let Resolution { steps, offset, sign } = self.resolution;
        let q = (sign * (x * steps).trunc()) / steps;
        let d = self.dc_blocker.process((-1.0f64).max(1.0f64.min(q + offset)));
        // At 32 kHz the instants are whole samples
        // Elsewhere the value is read at the exact instant, `latency` samples back, and steps on the next sample
        self.line.write(d);
        if self.until_hold < 1.0 - 1e-9 {
            self.held = if self.latency != 0.0 {
                self.line.read_sinc(self.latency - 0.0f64.max(self.until_hold))
            } else {
                d
            };
            self.until_hold += self.period;
        }
        self.until_hold -= 1.0;
        self.smoothing.process(self.held)
    }

    pub fn clear(&mut self) {
        self.dc_blocker.clear();
        self.smoothing.clear();
        // The first hold is one 32 kHz sample in, like the hardware's counter
        self.until_hold = self.step;
        self.held = 0.0;
        self.line.clear();
    }
}
