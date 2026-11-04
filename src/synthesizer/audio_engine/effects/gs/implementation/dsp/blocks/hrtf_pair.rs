//! The near and far ear filters of the 3D types, between two direction sets.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/hrtf_pair.ts

use super::delay_line::DelayLine;

/// One direction of the 3D model: the near ear `N = a x + b2 N[62.5 us] + b4 N[125 us]` and the far ear
/// `F = f F[31.25 us] + g N[d1] + h N[d2]`, the near signal delayed by two taps through a one-pole (the time and the
/// head shadow between the ears). `d1` and `d2` are in 32 kHz samples.
/// Equivalent to: interface HRTFSet
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HrtfSet {
    pub a: f64,
    pub b2: f64,
    pub b4: f64,
    pub f: f64,
    pub g: f64,
    pub d1: f64,
    pub h: f64,
    pub d2: f64,
}

/// The near and far ear filters of the 3D types, between two direction sets.
///
/// Both sets run on one shared history: the near coefficients mix directly (their delays are the same in every set),
/// the far ear is the weighted sum of both sets' taps through the weighted one-pole, as on the hardware. The delays
/// keep their length in seconds, read with a Lagrange interpolator where they fall between samples.
pub struct HrtfPair {
    pub near: f64,
    pub far: f64,
    near_line: DelayLine,
    far_line: DelayLine,
    /// One 32 kHz sample in host samples.
    step: f64,
    set_a: HrtfSet,
    set_b: HrtfSet,
    weight: f64,
}

impl HrtfPair {
    /// `set` is the starting set.
    pub fn new(sample_rate: f64, set: HrtfSet) -> Self {
        let step = sample_rate / 32_000.0;
        Self {
            near: 0.0,
            far: 0.0,
            // The far taps reach 29 samples back, plus the interpolator's points
            near_line: DelayLine::new((32.0 * step).ceil() + 4.0),
            far_line: DelayLine::new(step.ceil() + 4.0),
            step,
            set_a: set,
            set_b: set,
            weight: 1.0,
        }
    }

    /// `a` is the set at weight 1, `b` the set at weight 0, `weight` the share of set A.
    pub fn set_sets(&mut self, a: HrtfSet, b: HrtfSet, weight: f64) {
        self.set_a = a;
        self.set_b = b;
        self.weight = weight;
    }

    #[inline]
    pub fn process(&mut self, x: f64) {
        let step = self.step;
        let a = self.set_a;
        let b = self.set_b;
        let w = self.weight;
        let v = 1.0 - w;
        // Reads of earlier samples (before this sample's writes)
        let n2 = self.near_line.read_lagrange(2.0 * step - 1.0);
        let n4 = self.near_line.read_lagrange(4.0 * step - 1.0);
        let f1 = self.far_line.read_lagrange(step - 1.0);
        let near = (w * a.a + v * b.a) * x + (w * a.b2 + v * b.b2) * n2 + (w * a.b4 + v * b.b4) * n4;
        self.near_line.write(near);
        let far = (w * a.f + v * b.f) * f1
            + w * (a.g * self.tap(a.d1) + a.h * self.tap(a.d2))
            + v * (b.g * self.tap(b.d1) + b.h * self.tap(b.d2));
        self.far_line.write(far);
        self.near = near;
        self.far = far;
    }

    pub fn clear(&mut self) {
        self.near_line.clear();
        self.far_line.clear();
        self.near = 0.0;
        self.far = 0.0;
    }

    /// The near signal some 32 kHz samples back (0 is this sample's).
    #[inline]
    fn tap(&self, samples: f64) -> f64 {
        self.near_line.read_lagrange(samples * self.step)
    }
}
