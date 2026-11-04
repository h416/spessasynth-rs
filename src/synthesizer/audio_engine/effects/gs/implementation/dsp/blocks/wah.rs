//! The wah: a resonant low pass or band pass whose cutoff follows the Manual, an LFO and an envelope follower.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/wah.ts

use std::f64::consts::PI;
use std::sync::LazyLock;

use super::biquad::Biquad;
use super::first_order_filter::FirstOrderFilter;
use super::lfo::Lfo;

/// The side chain high passes: the hardware's 32 kHz one-poles (pole 53/128, gain 91/128), a corner of 4 kHz.
static SIDE_CHAIN_HZ: LazyLock<f64> =
    LazyLock::new(|| (32_000.0 / PI) * ((1.0f64 - 53.0 / 128.0) / (1.0 + 53.0 / 128.0)).atan());
const SIDE_CHAIN_GAIN: f64 = (2.0 * 91.0) / 128.0 / (1.0 + 53.0 / 128.0);

/// The follower's attack per 32 kHz sample (+0.31 dB) and the step that lets it start from silence.
const ATTACK: f64 = 1.036_621_093_75;
const ATTACK_SEED: f64 = 1.0 / 4_194_304.0; // 2 ** -22

/// The follower's release per 32 kHz sample (a 64 ms time constant).
const RELEASE: f64 = 1.0 - 1.0 / 2048.0;

/// The cutoff smoother's pole per 32 kHz sample (8 ms).
const SMOOTHER: f64 = 255.0 / 256.0;

/// The wah: a resonant low pass or band pass whose cutoff follows the Manual, a triangle LFO and an envelope
/// follower on the input.
///
/// On the hardware it is a Chamberlin state variable filter at 32 kHz. Here that 32 kHz filter is turned into its
/// analog prototype (the inverse bilinear transform) and designed again at the host rate around its pole frequency.
pub struct Wah {
    filter: Biquad,
    lfo: Lfo,
    side_chain1: FirstOrderFilter,
    side_chain2: FirstOrderFilter,
    /// 32 kHz over the host rate.
    scale: f64,
    attack: f64,
    attack_seed: f64,
    release: f64,
    smoothing: f64,
    band_pass: bool,
    peak: f64,
    peak_value: u8,
    manual: f64,
    sensitivity: f64,
    depth: f64,
    up: bool,
    envelope_on: bool,
    envelope: f64,
    coefficient: f64,
    last_coefficient: f64,
    last_damping: f64,
}

impl Wah {
    pub fn new(sample_rate: f64) -> Self {
        let mut side_chain1 = FirstOrderFilter::new(sample_rate);
        side_chain1.set_high_pass(*SIDE_CHAIN_HZ, SIDE_CHAIN_GAIN);
        let mut side_chain2 = FirstOrderFilter::new(sample_rate);
        side_chain2.set_high_pass(*SIDE_CHAIN_HZ, SIDE_CHAIN_GAIN);
        let scale = 32_000.0 / sample_rate;
        // The per sample factors of the 32 kHz hardware, moved to the host rate
        let attack = ATTACK.powf(scale);
        Self {
            filter: Biquad::new(sample_rate),
            lfo: Lfo::new(sample_rate),
            side_chain1,
            side_chain2,
            scale,
            attack,
            attack_seed: (ATTACK_SEED * (attack - 1.0)) / (ATTACK - 1.0),
            release: RELEASE.powf(scale),
            smoothing: 1.0 - SMOOTHER.powf(scale),
            band_pass: false,
            peak: 0.0,
            peak_value: 0,
            manual: 0.0,
            sensitivity: 0.0,
            depth: 0.0,
            up: true,
            envelope_on: true,
            envelope: 0.0,
            coefficient: 0.0,
            last_coefficient: -1.0,
            last_damping: -1.0,
        }
    }

    /// The band pass instead of the low pass.
    pub fn set_filter_type(&mut self, band_pass: bool) {
        self.band_pass = band_pass;
        self.update_peak();
    }

    /// The Peak parameter (0-127).
    pub fn set_peak(&mut self, value: u8) {
        self.peak_value = value;
        self.update_peak();
    }

    /// The Manual as a gain (the level law), 0 to 127/128.
    pub fn set_manual(&mut self, manual: f64) {
        self.manual = manual;
    }

    /// The Sens as a gain (the level law), 0 to 127/128.
    pub fn set_sensitivity(&mut self, sensitivity: f64) {
        // The Sens register has the wide format, the level over 32 instead of over 128
        self.sensitivity = 4.0 * sensitivity;
    }

    pub fn set_rate(&mut self, hz: f64) {
        self.lfo.set_rate(hz);
    }

    /// The LFO's share of the cutoff control (0 to 0.78).
    pub fn set_depth(&mut self, depth: f64) {
        self.depth = depth;
    }

    /// The LFO and the envelope raise the cutoff (Up) instead of lowering it (Down).
    pub fn set_polarity(&mut self, up: bool) {
        self.up = up;
    }

    /// The envelope follower (off in the multis without a Sens parameter).
    pub fn set_envelope(&mut self, on: bool) {
        self.envelope_on = on;
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let mut y = 0.0;
        if self.envelope_on {
            // A 4 kHz second order high pass, x 127/32 x Sens, rectified
            let s1 = self.side_chain1.process(x);
            let v = (self.side_chain2.process(s1) * (127.0 / 32.0) * self.sensitivity).abs();
            // The follower jumps up while the input is within 20 dB of it, so it rides at 10x the peaks
            let mut e = self.envelope;
            e = if v >= 0.1 * e {
                e * self.attack + self.attack_seed
            } else {
                e * self.release
            };
            e = e.min(1.0);
            self.envelope = e;
            y = if e < 0.1 {
                4.960_937_5 * e
            } else {
                0.555_541_992_187_5 * e + 0.444_458_007_812_5
            };
        }
        let swing = self.depth * self.lfo.process().abs() + y;
        let mut w = if self.up {
            self.manual - 0.1 + (127.0 / 128.0) * swing
        } else {
            self.manual + 0.1 - swing
        };
        w = 0.0f64.max(w.min(1.0));
        // F = 2 sin(pi fc / 32000): fc = 250 + 1500 w below about 1 kHz, 6000 w - 2000 above
        let target = if (127.0 / 128.0) * w < 0.5 {
            PI * ((3.0 * w) / 32.0 + 1.0 / 64.0)
        } else {
            PI * ((3.0 * w) / 8.0 - 1.0 / 8.0)
        };
        self.coefficient += self.smoothing * (target - self.coefficient);
        let f = self.coefficient;
        // The band pass damps (and scales its input) by k clamp(F, 0.1, 0.4)
        let mut s1 = if self.band_pass {
            self.peak * 0.1f64.max(0.4f64.min(f))
        } else {
            1.0
        };
        s1 = s1.min(1.95).min(2.0 - f);
        if f != self.last_coefficient || s1 != self.last_damping {
            self.design(f, s1);
        }
        self.filter.process(x)
    }

    pub fn clear(&mut self) {
        self.filter.clear();
        self.lfo.clear();
        self.side_chain1.clear();
        self.side_chain2.clear();
        self.envelope = 0.0;
        self.coefficient = 0.0;
        self.last_coefficient = -1.0;
    }

    /// The Peak's law depends on the filter type.
    fn update_peak(&mut self) {
        // Low pass: 1/Q from 1 to 0.04. Band pass: the damping's k from 4 to 0.1
        let value = self.peak_value as f64;
        self.peak = if self.band_pass {
            4.0 * 40f64.powf(-value / 127.0)
        } else {
            25f64.powf(-value / 127.0)
        };
        self.last_coefficient = -1.0;
    }

    /// Designs the filter from the 32 kHz Chamberlin coefficients: the frequency coefficient F and the input gain
    /// (and the band pass damping).
    fn design(&mut self, f: f64, s1: f64) {
        self.last_coefficient = f;
        self.last_damping = s1;
        let fd = f * s1 * (if self.band_pass { 1.0 } else { self.peak });
        // The 32 kHz poles on the analog side (s normalized to 64000 rad/s): s2 s^2 + 2 F d s + F^2
        let s2 = 4.0 - 2.0 * fd - f * f;
        let w32 = f / s2.sqrt();
        if self.band_pass {
            // F s1 s over the poles, out at 127/32: a peak gain of 127/64 at any Q
            // The -3 dB edges keep their place in Hz, so a wide band near the top does not spread at higher rates
            let width = (2.0 * fd) / s2;
            let mut low = (w32 * w32 + 0.25 * width * width).sqrt() - 0.5 * width;
            let mut high = low + width;
            if self.scale != 1.0 {
                low = self.filter.prewarp((32_000.0 / PI) * low.atan());
                high = self.filter.prewarp((32_000.0 / PI) * high.atan());
            }
            let band = high - low;
            self.filter.set_analog(0.0, (127.0 / 64.0) * band, 0.0, 1.0, band, low * high);
        } else {
            // F^2 (1 + s) / 2 over the poles (the zero is the input's (x + x[n-1]) / 4), out at 127/128
            // The pole frequency is prewarped for the host rate, which scales s by l
            let l = if self.scale == 1.0 {
                1.0
            } else {
                w32 / self.filter.prewarp((32_000.0 / PI) * w32.atan())
            };
            let n = (127.0 / 128.0) * 0.5 * f * f;
            self.filter.set_analog(0.0, n * l, n, s2 * l * l, 2.0 * fd * l, f * f);
        }
    }
}
