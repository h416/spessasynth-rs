//! The 3D stage of 3D Chorus and 3D Delay: two sources at left and right 90 degrees.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/crosstalk_3d.ts

use super::hrtf_pair::{HrtfPair, HrtfSet};
use super::output_mode_3d::OutputMode3D;

/// The fixed set: a direction tuned for 90 degrees (between the positioner's 84 and 96, with its own far taps).
const CROSSTALK: HrtfSet = HrtfSet {
    a: 1.531_25,
    b2: -0.445_312_5,
    b4: 0.281_25,
    f: 0.742_187_5,
    g: 0.109_375,
    d1: 22.0,
    h: 0.046_875,
    d2: 27.0,
};

/// The 3D stage of 3D Chorus and 3D Delay: two sources placed at left and right 90 degrees through the fixed near and
/// far ear pair (the first source's near ear is the left one, the second's the right one), then the output mode
/// (the speaker network with its 562.5 us numerator tap, switched hard).
pub struct Crosstalk3D {
    pub left: f64,
    pub right: f64,
    first: HrtfPair,
    second: HrtfPair,
    output: OutputMode3D,
}

impl Crosstalk3D {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            first: HrtfPair::new(sample_rate, CROSSTALK),
            second: HrtfPair::new(sample_rate, CROSSTALK),
            output: OutputMode3D::new(sample_rate, 18.0, false),
        }
    }

    /// `phones` is phones instead of speakers.
    pub fn set_phones(&mut self, phones: bool) {
        self.output.set_phones(phones);
    }

    /// `a` is the first (left) source, `b` the second (right) one.
    #[inline]
    pub fn process(&mut self, a: f64, b: f64) {
        self.first.process(a);
        self.second.process(b);
        self.output
            .process(self.first.near + self.second.far, self.first.far + self.second.near);
        self.left = self.output.left;
        self.right = self.output.right;
    }

    pub fn clear(&mut self) {
        self.first.clear();
        self.second.clear();
        self.output.clear();
        self.left = 0.0;
        self.right = 0.0;
    }
}
