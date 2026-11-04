//! The Humanizer's vowel filter: two banks of two formants with a crossfade between them.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/formant_filter.ts

use super::biquad::Biquad;
use super::first_order_filter::FirstOrderFilter;

/// One vowel: two formants, each a 32 kHz Chamberlin state variable filter given by its frequency coefficient
/// `F = 2 sin(pi f / 32000)` and damping `q` (about 1/Q), with a tap on its low pass (formant 1) or high pass
/// (formant 2) and one on its band pass.
#[derive(Clone, Copy)]
struct Vowel {
    f1: f64,
    q1: f64,
    low1: f64,
    band1: f64,
    f2: f64,
    q2: f64,
    high2: f64,
    band2: f64,
}

/// The vowels a, i, u, e, o as the hardware's coefficient words. Formant 1 and 2 peak at about 1028 and 1837 Hz (a),
/// 288 and 3470 Hz (i), 157 and 918 Hz (u), 550 and 2428 Hz (e), 550 and 525 Hz (o).
const VOWEL_WORDS: [[u16; 8]; 5] = [
    [0x19_a2, 0x0f_80, 0x18_00, 0x3f_80, 0x0b_52, 0x13_00, 0x09_00, 0x18_00],
    [0x07_3c, 0x1c_00, 0x18_00, 0x20_00, 0x14_90, 0x1b_00, 0x0f_80, 0x1e_80],
    [0x03_f2, 0x10_00, 0x30_00, 0x02_d0, 0x05_c0, 0x07_00, 0x20_00, 0x30_00],
    [0x0d_cf, 0x1e_00, 0x18_00, 0x20_00, 0x0e_be, 0x17_00, 0x16_00, 0x21_80],
    [0x0d_cf, 0x15_80, 0x2a_80, 0x1e_00, 0x03_49, 0x09_80, 0x16_00, 0x1a_80],
];

fn vowel(index: usize) -> Vowel {
    let [f1, q1, low1, band1, f2, q2, high2, band2] = VOWEL_WORDS[index].map(f64::from);
    Vowel {
        f1: f1 / 32_768.0,
        q1: q1 / 32_768.0,
        low1: low1 / 8192.0,
        band1: band1 / 8192.0,
        f2: f2 / 8192.0,
        q2: q2 / 32_768.0,
        high2: high2 / 8192.0,
        band2: band2 / 8192.0,
    }
}

/// The gain of each formant's taps.
const TAP_GAIN: f64 = 1.25;

/// One bank: the two formants of a vowel, summed.
struct FormantBank {
    formant1: Biquad,
    formant2: Biquad,
    /// Formant 2's share of the input's `(x + x[n-1]) / 2` that its high pass does not cancel.
    average: FirstOrderFilter,
    /// 32 kHz over the host rate.
    scale: f64,
}

impl FormantBank {
    fn new(sample_rate: f64) -> Self {
        let mut average = FirstOrderFilter::new(sample_rate);
        average.set_low_pass(8000.0, 1.0);
        Self {
            formant1: Biquad::new(sample_rate),
            formant2: Biquad::new(sample_rate),
            average,
            scale: 32_000.0 / sample_rate,
        }
    }

    /// `vowel` is the vowel (0-4).
    fn load(&mut self, vowel_index: usize) {
        let v = vowel(vowel_index);
        // The Chamberlin filter on its input (x + x[n-1]) / 2, on the analog side (s normalized to 64000 rad/s)
        // Poles s2 s^2 + 2 F q s + F^2, over them the low pass q F^2 (1 - s), the band pass 2 q F s
        // And the high pass 4 q s^2 / (1 + s)
        {
            let (f, s2, l) = self.prototype(v.f1, v.q1);
            let k = TAP_GAIN * v.q1 * f;
            self.formant1.set_analog(
                0.0,
                k * (2.0 * v.band1 - v.low1 * f) * l,
                k * v.low1 * f,
                s2 * l * l,
                2.0 * f * v.q1 * l,
                f * f,
            );
        }
        {
            let (f, s2, l) = self.prototype(v.f2, v.q2);
            let k = 2.0 * TAP_GAIN * v.q2;
            // 1 / (1 + s) is the average filter
            self.formant2.set_analog(
                k * (2.0 * v.high2 + f * v.band2) * l * l,
                k * f * v.band2 * l,
                0.0,
                s2 * l * l,
                2.0 * f * v.q2 * l,
                f * f,
            );
        }
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        self.formant1.process(x) + self.average.process(self.formant2.process(x))
    }

    fn clear(&mut self) {
        self.formant1.clear();
        self.formant2.clear();
        self.average.clear();
    }

    /// The poles of a formant and the scale of s that keeps their frequency at the host rate.
    /// `f` is the frequency coefficient, `q` the damping. Returns `(f, s2, l)`.
    fn prototype(&self, f: f64, q: f64) -> (f64, f64, f64) {
        let s2 = 4.0 - 2.0 * f * q - f * f;
        let w32 = f / s2.sqrt();
        let l = if self.scale == 1.0 {
            1.0
        } else {
            w32 / self
                .formant1
                .prewarp((32_000.0 / std::f64::consts::PI) * w32.atan())
        };
        (f, s2, l)
    }
}

/// The Humanizer's vowel filter: two banks of two formants. A new vowel goes into the idle bank and a ramp
/// crossfades to it. The output is `(1 + r) A + r B` with r from 0 (bank A) to -1 (bank B), so a vowel in bank B
/// comes out inverted, and halfway the banks subtract. The SC-88Pro does the same.
pub struct FormantFilter {
    bank_a: FormantBank,
    bank_b: FormantBank,
    sample_rate: f64,
    vowel: u8,
    /// The bank the crossfade moves to (false for A).
    on_b: bool,
    target: f64,
    fade: f64,
    rate: f64,
}

impl FormantFilter {
    pub fn new(sample_rate: f64) -> Self {
        let mut filter = Self {
            bank_a: FormantBank::new(sample_rate),
            bank_b: FormantBank::new(sample_rate),
            sample_rate,
            vowel: 0,
            on_b: false,
            target: 0.0,
            fade: 0.0,
            rate: 0.0,
        };
        filter.clear();
        filter
    }

    /// Loads a vowel into the idle bank and crossfades to it, if it is not the current one.
    /// `vowel` is the vowel (0-4: a, i, u, e, o).
    pub fn set_vowel(&mut self, vowel: u8) {
        if vowel == self.vowel {
            return;
        }
        self.vowel = vowel;
        self.on_b = !self.on_b;
        if self.on_b {
            self.bank_b.load(vowel as usize);
        } else {
            self.bank_a.load(vowel as usize);
        }
        self.target = if self.on_b { -1.0 } else { 0.0 };
    }

    /// `seconds` is the crossfade's time constant.
    pub fn set_accel(&mut self, seconds: f64) {
        self.rate = 1.0 - (1.0 - 1.0 / (32_000.0 * seconds)).powf(32_000.0 / self.sample_rate);
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        self.fade += self.rate * (self.target - self.fade);
        let r = self.fade;
        (1.0 + r) * self.bank_a.process(x) + r * self.bank_b.process(x)
    }

    /// Clears the filters and starts on vowel a in bank A (bank B holds i, as after a type select).
    pub fn clear(&mut self) {
        self.bank_a.clear();
        self.bank_b.clear();
        self.bank_a.load(0);
        self.bank_b.load(1);
        self.vowel = 0;
        self.on_b = false;
        self.target = 0.0;
        self.fade = 0.0;
    }
}
