//! Lo-Fi 2's noise sources (white or pink, disc, hum) and the Cut/LPF column filters.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/noise_generator.ts

use super::first_order_filter::FirstOrderFilter;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::fround;

/// The LPF column's low pass per step (`value >> 3`): the hardware's bytes (gain A over 128 on both taps, pole B over
/// 128). The last step (13.5 kHz) is off.
const CUTOFF_A: [f64; 16] = [
    3.0, 4.0, 5.0, 6.0, 7.0, 9.0, 11.0, 14.0, 18.0, 21.0, 26.0, 31.0, 37.0, 45.0, 53.0, 64.0,
];
const CUTOFF_B: [f64; 16] = [
    122.0, 120.0, 118.0, 116.0, 113.0, 109.0, 105.0, 100.0, 93.0, 86.0, 77.0, 66.0, 53.0, 39.0, 22.0, 0.0,
];

/// Sets a first order low pass from the Cut and LPF columns' bytes (the corner of the pole, the DC gain of the taps).
/// `value` is the parameter (0-127), `off` whether the last step bypasses the filter (the noise low passes).
pub fn set_column_low_pass(filter: &mut FirstOrderFilter, value: u8, off: bool) {
    let step = (value >> 3) as usize;
    if off && step == 15 {
        filter.set_gain(127.0 / 128.0);
        return;
    }
    let pole = CUTOFF_B[step] / 128.0;
    filter.set_low_pass(
        (32_000.0 / std::f64::consts::PI) * ((1.0 - pole) / (1.0 + pole)).atan(),
        (2.0 * CUTOFF_A[step]) / 128.0 / (1.0 - pole),
    );
}

/// Sets a first order high pass from the Cut column's bytes (its taps are 128 - A over 128, the same pole).
/// `value` is the parameter (0-127).
pub fn set_column_high_pass(filter: &mut FirstOrderFilter, value: u8) {
    let step = (value >> 3) as usize;
    let pole = CUTOFF_B[step] / 128.0;
    filter.set_high_pass(
        (32_000.0 / std::f64::consts::PI) * ((1.0 - pole) / (1.0 + pole)).atan(),
        (2.0 * (128.0 - CUTOFF_A[step])) / 128.0 / (1.0 + pole),
    );
}

/// The disc's turn words per type (LP, EP, SP, RND): 0.555, 0.750 and 1.299 turns per second.
const DISC_WORDS: [f64; 4] = [0x04_8c as f64, 0x06_24 as f64, 0x0a_a4 as f64, 0x04_8c as f64];

/// The hum's words per type: 50 and 60 Hz.
const HUM_WORDS: [f64; 2] = [0x06_66 as f64, 0x07_ae as f64];

/// The disc's click window: the click sounds while the rotation phase is this close to its wrap.
const CLICK: f64 = 0.5 / (0.500_061_035_156_25 + 1e-5);

/// The hardware program's tiny offset against denormals, added after its clamps and wraps.
const OFFSET: f64 = 1e-8_f64 as f32 as f64;

/// The memory the hardware program starts from.
const SEED: f64 = 1e-5_f64 as f32 as f64;

/// The 1/8 that the white noise recursion adds through two delayed copies (0.297 in all). The copies still hold the
/// memory's seed for the first two samples.
const WHITE_EIGHTH: f64 = (0.125 + OFFSET) as f32 as f64;

/// Wraps a phase to -1 to 1 in float precision with the offset, as the hardware program does.
fn wrap32(x: f64) -> f64 {
    let mut v = fround(fround(x) + OFFSET);
    while v < -1.0 {
        v = fround(v + 2.0);
    }
    while v >= 1.0 {
        v = fround(v - 2.0);
    }
    v
}

/// The program's saturating step: in float with the offset, clamped to full scale.
fn clamp32(x: f64) -> f64 {
    (-1.0f64).max(1.0f64.min(fround(fround(x) + OFFSET)))
}

/// Lo-Fi 2's noise sources, each through its own low pass and level:
///
/// - White (x 1/32) or "pink": the white noise times a 500 Hz saw (on the SC-88Pro too).
/// - Disc: one click per turn of a record at 33 1/3, 45 or 78 rpm, 0.125 + 0.25 abs(saw) while the turn's phase is
///   within 0.012 % of its wrap. RND adds a second train whose phase is offset by twice the hum's (pseudo-random
///   times).
/// - Hum: a unipolar 50 or 60 Hz square, 0 or 1/16 (it carries DC, on the hardware too).
///
/// The phases run the hardware program's own arithmetic, in float, on a 32 kHz clock: the click lengths (8, 6 and 4
/// samples) and the RND train's times depend on where the phases land. The disc phase takes a step every second
/// sample (the program reads it two samples back, as two interleaved phases). The white noise is the program's
/// chaotic recursion at 32 kHz, uniform and white. It loops every 89843 samples, which gets short at high rates, so
/// other rates draw the same distribution from xorshift32 instead.
pub struct NoiseGenerator {
    pub output: f64,
    white_filter: FirstOrderFilter,
    disc_filter: FirstOrderFilter,
    hum_filter: FirstOrderFilter,
    /// The 32 kHz clock: 32 kHz samples per host sample, and the time to the next one.
    scale: f64,
    until_tick: f64,
    saw: f64,
    hum: f64,
    /// The two interleaved disc phases, and which one this 32 kHz sample steps.
    disc: [f32; 2],
    disc_track: usize,
    saw_word: f64,
    hum_word: f64,
    disc_word: f64,
    pink: bool,
    random: bool,
    white_level: f64,
    disc_level: f64,
    hum_level: f64,
    /// The white noise recursion's last two values, and its gain at the host rate (the same noise density as at
    /// 32 kHz).
    white1: f64,
    white2: f64,
    white_age: u8,
    white_gain: f64,
    exact_white: bool,
    random32: i32,
}

impl NoiseGenerator {
    pub fn new(sample_rate: f64) -> Self {
        let scale = 32_000.0 / sample_rate;
        Self {
            output: 0.0,
            white_filter: FirstOrderFilter::new(sample_rate),
            disc_filter: FirstOrderFilter::new(sample_rate),
            hum_filter: FirstOrderFilter::new(sample_rate),
            scale,
            until_tick: 0.0,
            saw: 0.0,
            hum: 0.0,
            disc: [0.0; 2],
            disc_track: 0,
            saw_word: 0.031_25,
            hum_word: 0.0,
            disc_word: 0.0,
            pink: false,
            random: false,
            white_level: 0.0,
            disc_level: 0.0,
            hum_level: 0.0,
            white1: 0.0,
            white2: 0.0,
            white_age: 0,
            white_gain: (1.0 / scale).sqrt() / 32.0,
            exact_white: sample_rate == 32_000.0,
            random32: 1,
        }
    }

    /// `pink` is the pink noise instead of the white, `low_pass` the LPF column value, `level` the level as a gain.
    pub fn set_white(&mut self, pink: bool, low_pass: u8, level: f64) {
        self.pink = pink;
        set_column_low_pass(&mut self.white_filter, low_pass, true);
        self.white_level = level;
    }

    /// `type` is the disc type (0-3: LP, EP, SP, RND), `low_pass` the LPF column value, `level` the level as a gain.
    pub fn set_disc(&mut self, r#type: u8, low_pass: u8, level: f64) {
        let word = DISC_WORDS[r#type as usize];
        self.disc_word = fround(fround(word / 8192.0) * 2f64.powi(-11));
        self.random = r#type == 3;
        set_column_low_pass(&mut self.disc_filter, low_pass, true);
        self.disc_level = level;
    }

    /// `type` is the hum type (0 50 Hz, 1 60 Hz), `low_pass` the LPF column value, `level` the level as a gain.
    pub fn set_hum(&mut self, r#type: u8, low_pass: u8, level: f64) {
        let word = HUM_WORDS[r#type as usize];
        self.hum_word = fround(fround(word / 8192.0) * 2f64.powi(-6));
        set_column_low_pass(&mut self.hum_filter, low_pass, true);
        self.hum_level = level;
    }

    #[inline]
    pub fn process(&mut self) -> f64 {
        // The phases step on the 32 kHz clock, with the program's own constants (its seeds included)
        self.until_tick -= 1.0;
        while self.until_tick < 1e-9 {
            self.saw = wrap32(self.saw + self.saw_word);
            self.hum = wrap32(self.hum + 1.5625e-7 + self.hum_word);
            let track = self.disc_track;
            self.disc[track] = wrap32(self.disc[track] as f64 + 4.8828e-9 + self.disc_word) as f32;
            self.disc_track = 1 - track;
            self.until_tick += 1.0 / self.scale;
        }
        let disc = self.disc[1 - self.disc_track] as f64;
        let noise = (if self.pink { self.saw } else { 1.0 }) * self.white() * self.white_gain;
        let saw = clamp32(0.5 * self.saw.abs());
        let mut click = if clamp32(disc.abs()) >= CLICK {
            0.5 * saw + 0.125
        } else {
            0.0
        };
        if self.random {
            let offset = 2.0 * clamp32((127.0 / 128.0) * self.hum.abs()) - disc + 5e-6;
            if clamp32(wrap32(offset).abs()) >= CLICK {
                click += 0.5 * saw;
            }
        }
        let hum = if self.hum < 0.0 { 1.0 / 16.0 } else { 0.0 };
        self.output = self.white_level * self.white_filter.process(noise)
            + self.disc_level * self.disc_filter.process(click)
            + self.hum_level * self.hum_filter.process(hum);
        self.output
    }

    pub fn clear(&mut self) {
        self.white_filter.clear();
        self.disc_filter.clear();
        self.hum_filter.clear();
        self.saw = SEED;
        self.hum = SEED;
        self.disc = [SEED as f32; 2];
        self.disc_track = 0;
        self.until_tick = 1.0;
        self.white1 = SEED;
        self.white2 = SEED;
        self.white_age = 0;
        self.random32 = 1;
        self.output = 0.0;
    }

    /// The white noise of this sample, -1 to 1.
    fn white(&mut self) -> f64 {
        if !self.exact_white {
            let mut r = self.random32;
            r ^= r.wrapping_shl(13);
            r ^= ((r as u32) >> 17) as i32;
            r ^= r.wrapping_shl(5);
            self.random32 = r;
            return r as f64 / 2_147_483_648.0;
        }
        // The hardware's white noise: a wrapped second order recursion, chaotic and close to uniform
        // It amplifies any rounding, so it runs in float precision in the hardware's order of operations
        let f = fround;
        let first = if self.white_age > 0 { WHITE_EIGHTH } else { SEED };
        let second = if self.white_age > 1 { WHITE_EIGHTH } else { SEED };
        self.white_age = (self.white_age + 1).min(2);
        let constant = f(f(0.25 * WHITE_EIGHTH) + f(f(first * 1.5) + f(0.625 * second)));
        let white = wrap32(f(f(f(3.0 * self.white1) + constant) + f(0.25 * self.white2)));
        self.white2 = self.white1;
        self.white1 = white;
        white
    }
}
