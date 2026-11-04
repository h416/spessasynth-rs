//! A fixed delay of whole 32 kHz samples: a delay line or a Thiran allpass.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/output_lag.ts

use super::delay_line::DelayLine;
use super::first_order_filter::FLUSH;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::js_round;

/// A fixed delay of whole 32 kHz samples. The shared stage writes the right output one 32 kHz sample late, so every
/// type with a mono or a summed output delays its right side by one of these, and the mono mix its left input.
///
/// Whole host samples read the line. Between them a Thiran allpass of order `floor(d) + 1` (stable, flat in
/// magnitude) gives the fractional delay.
pub struct OutputLag {
    line: Option<DelayLine>,
    whole: isize,
    /// The allpass: `a` (a0 = 1 left out), the numerator is `a` reversed. Inputs and outputs, newest first.
    a: Vec<f64>,
    inputs: Vec<f64>,
    outputs: Vec<f64>,
}

impl OutputLag {
    /// `samples` is the delay in 32 kHz samples (1 by default in TS).
    pub fn new(sample_rate: f64, samples: f64) -> Self {
        let delay = (samples * sample_rate) / 32_000.0;
        let whole = js_round(delay);
        let line;
        let mut a;
        if (delay - whole).abs() < 1e-9 {
            line = Some(DelayLine::new(whole));
            a = Vec::new();
        } else {
            line = None;
            let order = delay.floor() as usize + 1;
            a = vec![0.0; order];
            // The coefficients a_k = (-1)^k C(order, k) prod_n (d - order + n) / (d - order + k + n)
            let mut binomial = 1.0;
            for k in 1..=order {
                binomial = (binomial * (order - k + 1) as f64) / k as f64;
                let mut p = 1.0;
                for n in 0..=order {
                    p *= (delay - order as f64 + n as f64) / (delay - order as f64 + k as f64 + n as f64);
                }
                a[k - 1] = (if k % 2 == 0 { 1.0 } else { -1.0 }) * binomial * p;
            }
        }
        let len = a.len();
        Self {
            line,
            whole: whole as isize,
            a,
            inputs: vec![0.0; len + 1],
            outputs: vec![0.0; len],
        }
    }

    /// Returns the sample of the delay ago.
    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        if let Some(line) = &mut self.line {
            line.write(x);
            return line.read(self.whole);
        }
        let order = self.a.len();
        let inputs = &mut self.inputs;
        let outputs = &mut self.outputs;
        for k in (1..=order).rev() {
            inputs[k] = inputs[k - 1];
        }
        inputs[0] = x;
        // The numerator is the denominator reversed: b_k = a_(order - k), with a_0 = 1
        let mut y = inputs[order];
        for k in 1..=order {
            y += self.a[k - 1] * (inputs[order - k] - outputs[k - 1]);
        }
        if y.abs() < FLUSH {
            y = 0.0;
        }
        for k in (1..order).rev() {
            outputs[k] = outputs[k - 1];
        }
        outputs[0] = y;
        y
    }

    pub fn clear(&mut self) {
        if let Some(line) = &mut self.line {
            line.clear();
        }
        self.inputs.fill(0.0);
        self.outputs.fill(0.0);
    }
}
