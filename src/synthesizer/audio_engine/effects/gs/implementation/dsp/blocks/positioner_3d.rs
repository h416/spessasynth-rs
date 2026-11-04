//! The 3D positioner of 3D Auto and 3D Manual: a mono source at an azimuth through the near and far ear.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/positioner_3d.ts

use super::hrtf_pair::{HrtfPair, HrtfSet};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::js_round;

/// A set whose far ear hears what the near ear hears (straight ahead and straight behind).
const fn same(a: f64, b2: f64, b4: f64) -> HrtfSet {
    HrtfSet { a, b2, b4, f: 0.0, g: 1.0, d1: 0.0, h: 0.0, d2: 0.0 }
}

/// The 16 directions of the hardware, 12 degrees apart from behind (0) to the front (15). The near ear gets an 8 kHz
/// resonance and a low pass, the far ear the near signal 0 to 0.9 ms later through a one-pole (the ITD reaches about
/// 800 us at 84-96 degrees, the level difference grows with frequency).
#[rustfmt::skip]
const BREAKPOINTS: [HrtfSet; 16] = [
    same(0.789_062_5, 0.242_187_5, 0.046_875),
    HrtfSet { a: 0.820_312_5, b2: 0.093_75, b4: 0.25, f: 0.335_937_5, g: 0.578_125, d1: 6.0, h: 0.015_625, d2: 4.0 },
    HrtfSet { a: 0.859_375, b2: -0.015_625, b4: 0.375, f: 0.468_75, g: 0.414_062_5, d1: 12.0, h: 0.023_437_5, d2: 7.0 },
    HrtfSet { a: 0.921_875, b2: -0.101_562_5, b4: 0.445_312_5, f: 0.539_062_5, g: 0.3125, d1: 16.0, h: 0.039_062_5, d2: 10.0 },
    HrtfSet { a: 1.031_25, b2: -0.1875, b4: 0.476_562_5, f: 0.601_562_5, g: 0.226_562_5, d1: 19.0, h: 0.054_687_5, d2: 13.0 },
    HrtfSet { a: 1.156_25, b2: -0.273_437_5, b4: 0.476_562_5, f: 0.656_25, g: 0.156_25, d1: 19.0, h: 0.070_312_5, d2: 16.0 },
    HrtfSet { a: 1.3125, b2: -0.351_562_5, b4: 0.429_687_5, f: 0.703_125, g: 0.101_562_5, d1: 21.0, h: 0.085_937_5, d2: 19.0 },
    HrtfSet { a: 1.468_75, b2: -0.421_875, b4: 0.343_75, f: 0.734_375, g: 0.0625, d1: 25.0, h: 0.101_562_5, d2: 21.0 },
    HrtfSet { a: 1.593_75, b2: -0.460_937_5, b4: 0.234_375, f: 0.742_187_5, g: 0.039_062_5, d1: 28.0, h: 0.117_187_5, d2: 21.0 },
    HrtfSet { a: 1.625, b2: -0.476_562_5, b4: 0.156_25, f: 0.734_375, g: 0.031_25, d1: 29.0, h: 0.132_812_5, d2: 19.0 },
    HrtfSet { a: 1.625, b2: -0.476_562_5, b4: 0.101_562_5, f: 0.718_75, g: 0.039_062_5, d1: 27.0, h: 0.148_437_5, d2: 16.0 },
    HrtfSet { a: 1.593_75, b2: -0.460_937_5, b4: 0.054_687_5, f: 0.695_312_5, g: 0.039_062_5, d1: 25.0, h: 0.171_875, d2: 13.0 },
    HrtfSet { a: 1.5, b2: -0.421_875, b4: 0.031_25, f: 0.664_062_5, g: 0.046_875, d1: 22.0, h: 0.210_937_5, d2: 10.0 },
    HrtfSet { a: 1.375, b2: -0.3125, b4: 0.015_625, f: 0.593_75, g: 0.054_687_5, d1: 18.0, h: 0.281_25, d2: 7.0 },
    HrtfSet { a: 1.1875, b2: -0.179_687_5, b4: 0.007_812_5, f: 0.3125, g: 0.070_312_5, d1: 13.0, h: 0.546_875, d2: 4.0 },
    same(1.0, 0.0, 0.0),
];

/// A Manual azimuth change crossfades over 46 32 kHz samples (1.44 ms).
const MANUAL_FADE: f64 = 46.0 / 32_000.0;

/// The 3D positioner of 3D Auto and 3D Manual: a mono source at an azimuth, heard through the near and far ear of the
/// direction (the near ear is the left one on the left side).
///
/// The position is a phase over one turn, -1 to 1: its magnitude runs from behind (0) to the front (1), its sign is
/// the side (negative on the left). Turning moves it at the speed in turns per second, clockwise (the azimuth
/// increasing) or counterclockwise. Turned off it runs on until it reaches the azimuth and stops there.
///
/// Between two directions it mixes their sets: while turning, toward the next direction in the way it moves, done
/// after half the 12 degrees. A Manual change crossfades from the current set in 1.44 ms.
pub struct Positioner3D {
    pub left: f64,
    pub right: f64,
    pair: HrtfPair,
    sample_rate: f64,
    position: f64,
    target: f64,
    increment: f64,
    turning: bool,
    moving: bool,
    /// The Manual crossfade: from `from` to `to`, its progress and step per sample.
    from: HrtfSet,
    to: HrtfSet,
    fade: f64,
    fade_step: f64,
}

impl Positioner3D {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            pair: HrtfPair::new(sample_rate, BREAKPOINTS[15]),
            sample_rate,
            position: 0.0,
            target: 0.0,
            increment: 0.0,
            turning: false,
            moving: false,
            from: BREAKPOINTS[15],
            to: BREAKPOINTS[15],
            fade: 1.0,
            fade_step: 1.0 / (MANUAL_FADE * sample_rate),
        }
    }

    /// `degrees` is the azimuth (the Azim column), negative on the left. `jump` starts there at once (a reset)
    /// instead of crossfading or turning there (false by default in TS).
    pub fn set_azimuth(&mut self, degrees: f64, jump: bool) {
        // The front is -1 (the wrap's end), the right side positive
        let target = (if degrees > 0.0 { 1.0 } else { -1.0 }) * (1.0 - degrees.abs() / 180.0);
        if target == self.target && !jump {
            return;
        }
        self.target = target;
        if jump {
            self.position = target;
            self.to = Self::direction_at(target);
            self.from = self.to;
            self.fade = 1.0;
        } else if !self.turning && !self.moving {
            // A Manual change: crossfade from the nearer end of a running fade
            self.from = if self.fade < 0.5 { self.from } else { self.to };
            self.to = Self::direction_at(target);
            self.position = target;
            self.fade = 0.0;
        }
    }

    /// `hz` is the turning speed in turns per second, `clockwise` the azimuth increasing.
    pub fn set_speed(&mut self, hz: f64, clockwise: bool) {
        // The phase moves by 2 per turn, clockwise makes it smaller
        self.increment = ((if clockwise { -2.0 } else { 2.0 }) * hz) / self.sample_rate;
    }

    pub fn set_turning(&mut self, on: bool) {
        self.turning = on;
        if on {
            self.moving = true;
        }
    }

    #[inline]
    pub fn process(&mut self, x: f64) {
        if self.moving {
            let mut next = self.position + self.increment;
            // Turned off, it stops where it passes the azimuth
            if !self.turning && self.crosses(self.position, next) {
                next = self.target;
                self.moving = false;
                // The azimuth may have changed while turning, so the still set is the one at the stop
                self.to = Self::direction_at(next);
                self.from = self.to;
                self.fade = 1.0;
            }
            self.position = if next >= 1.0 {
                next - 2.0
            } else if next < -1.0 {
                next + 2.0
            } else {
                next
            };
            self.mix_moving();
        } else if self.fade < 1.0 {
            self.fade = 1.0f64.min(self.fade + self.fade_step);
            self.pair.set_sets(self.to, self.from, self.fade);
        } else {
            self.pair.set_sets(self.to, self.to, 1.0);
        }
        self.pair.process(x);
        // The near ear is on the side of the source
        if self.position < 0.0 {
            self.left = self.pair.near;
            self.right = self.pair.far;
        } else {
            self.left = self.pair.far;
            self.right = self.pair.near;
        }
    }

    pub fn clear(&mut self) {
        self.pair.clear();
        self.left = 0.0;
        self.right = 0.0;
    }

    /// The direction set at a position that sits on a breakpoint.
    fn direction_at(position: f64) -> HrtfSet {
        BREAKPOINTS[js_round(position.abs() * 15.0) as usize]
    }

    /// Whether a step from `a` to `b` (not wrapped) passes the target in the direction of turning.
    fn crosses(&self, a: f64, b: f64) -> bool {
        // The distance to the target ahead of the turning, over the wrap
        let ahead = if self.increment < 0.0 {
            (((a - self.target) % 2.0) + 2.0) % 2.0
        } else {
            (((self.target - a) % 2.0) + 2.0) % 2.0
        };
        ahead <= (b - a).abs()
    }

    /// While turning: the mix toward the next breakpoint in the way the position moves, complete half way through the
    /// segment.
    fn mix_moving(&mut self) {
        let level = self.position.abs() * 15.0;
        let segment = 14.0f64.min(level.floor());
        let into = level - segment;
        let segment = segment as usize;
        // The magnitude grows when the position moves away from 0 (toward the front)
        // Math.sign(this.position || 1): 0 counts as positive
        let p = if self.position == 0.0 { 1.0 } else { self.position };
        let sign = if p > 0.0 { 1.0 } else { -1.0 };
        let rising = sign * self.increment > 0.0;
        if rising {
            self.pair.set_sets(BREAKPOINTS[segment + 1], BREAKPOINTS[segment], 1.0f64.min(2.0 * into));
        } else {
            self.pair.set_sets(BREAKPOINTS[segment], BREAKPOINTS[segment + 1], 1.0f64.min(2.0 * (1.0 - into)));
        }
    }
}
