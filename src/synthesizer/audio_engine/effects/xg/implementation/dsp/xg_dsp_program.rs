//! The base of every XG DSP program (S-YXG50 model).
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/xg_dsp_program.ts
//!
//! The TS `abstract class XGDSPProgram` is split in two:
//! - [`XGDSPProgram`]: the shared state (word RAMs, coefficients, taps, fade-in) and the
//!   helpers that never call an overridable method.
//! - [`XGProgram`]: the overridable members (`convert`, `resetState`, `pushCoefficients`,
//!   `clampParameter`, `reset`) and the base methods that call them (`setParameter`,
//!   `reset`, `updateConnection`, `designFirstOrder`, `designSecondOrder`).
//!
//! The abstract layout members (`preset`, `coefficientList`, `addressList`, `initOrder`,
//! `connectionParameters`, `ranges`) are stored in [`XGDSPProgram`] at construction.

use super::effect_processor::DefaultXGEffectProcessor;
use super::xg_dsp_tables::{
    COEFFICIENT_MODES, DENSITY_MASK, DENSITY_TABLE, FILTER_CUT, FILTER_FREQUENCIES, FILTER_GAIN,
};

/// The sample rate the XG DSP programs are designed for.
pub const XG_DSP_SAMPLE_RATE: f64 = 44_100.0;

/// The number of coefficient words and processing coefficients.
const COEFFICIENT_COUNT: usize = 192;

/// The number of address words and delay taps.
const ADDRESS_COUNT: usize = 64;

/// Seconds between the control ticks that drive the input fade-in after a type change.
const CONTROL_TICK: f64 = 0.01;

/// The input level step per control tick during the fade-in.
const FADE_STEP: f64 = 1.0 / 32.0;

/// The extra gain of the boosted low shelf.
/// Equivalent to: Math.fround(1.4142)
const BOOST: f64 = 1.4142_f32 as f64;

/// The first-order filter shapes of the XG DSP filter designer.
/// The boosted low shelf has an extra 3 dB of gain (the chorus input EQ).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XGFilterShape {
    LowPass,
    HighPass,
    LowShelf,
    LowShelfBoosted,
    HighShelf,
    Gain,
}

/// The second-order filter shapes of the XG DSP filter designer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XGBiquadShape {
    Peaking,
    First(XGFilterShape),
}

/// The level of a voice's chorus and variation send relative to its dry signal at full send.
/// The original sends half of the (panned) voice to these stereo effect inputs,
/// while the synthesizer sends the full panned voice.
pub const STEREO_SEND_LEVEL: f64 = 0.5;

/// The variation input level in the system mode (1 in the insertion mode).
pub const VARIATION_INPUT_LEVEL: f64 = 181.0 / 64.0;

/// The original chorus and variation return at its default (return 64, center pan) is 182 / 512.
/// The center pan is divided out because the block applies its own pan.
pub const RETURN_GAIN: f64 = 182.0 / 512.0 / std::f64::consts::FRAC_1_SQRT_2;

/// The 23-bit LFO phase range.
pub const PHASE_MASK: i32 = 0x7f_ff_ff;
/// The peak of a folded LFO phase (22 bits).
pub const TRIANGLE_PEAK: f64 = 4_194_304.0;
/// Scales a folded LFO phase (0 to 2^22) to 0-1.
pub const TRIANGLE_SCALE: f64 = 1.0 / TRIANGLE_PEAK;

/// Folds an LFO phase into a triangle (0 to 2^22).
/// Equivalent to: triangle = (u) => u ^ (u >> 22 ? PHASE_MASK : 0)
#[inline]
pub fn triangle(u: i32) -> i32 {
    u ^ (if (u >> 22) != 0 { PHASE_MASK } else { 0 })
}

/// Reads a 16-bit word as a signed value.
/// Equivalent to: toSigned16 = (value) => (value << 16) >> 16
#[inline]
pub fn to_signed16(value: i32) -> i32 {
    (value << 16) >> 16
}

/// Math.fround
#[inline]
pub fn fround(x: f64) -> f64 {
    x as f32 as f64
}

/// Math.round (half-up).
#[inline]
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// Stores a JS number into a Uint16Array element (ToUint16: wraps modulo 2^16).
#[inline]
pub fn to_u16(x: i32) -> u16 {
    x as u16
}

/// The delay memory length a processor needs (a power of two).
pub fn get_memory_size(sample_rate: f64, size: usize) -> usize {
    let length = ((size as f64 * sample_rate) / XG_DSP_SAMPLE_RATE).ceil() as usize;
    let mut power_of_two = 1usize;
    while power_of_two < length {
        power_of_two <<= 1;
    }
    power_of_two
}

/// Rounds toward zero to float32.
/// The original renders with the x87 FPU in 24-bit precision and truncating rounding,
/// which matters for slow control-rate accumulators (LFO smoothing).
pub fn chop32(value: f64) -> f64 {
    let mut f = value as f32;
    if (f as f64).abs() > value.abs() {
        // One step toward zero (the bits are sign and magnitude)
        f = f32::from_bits(f.to_bits().wrapping_sub(1));
    }
    f as f64
}

/// Valid parameter ranges (param, min, max), the rest is 0-127.
/// Equivalent to: type XGParameterRanges
pub type XGParameterRanges = &'static [(usize, i32, i32)];

/// The type-specific data shared by all XG DSP programs.
/// Equivalent to: interface XGProgramPreset
#[derive(Debug)]
pub struct XGProgramPreset {
    /// The coefficient words followed by the address words of the program.
    pub words: &'static [i32],
    /// The 16 default parameter values.
    pub defaults: [i32; 16],
}

/// The program layout: the TS abstract readonly members of XGDSPProgram.
#[derive(Debug, Clone, Copy)]
pub struct XGProgramLayout {
    /// Coefficient indices of the preset words.
    pub coefficient_list: &'static [usize],
    /// Address indices of the preset words.
    pub address_list: &'static [usize],
    /// The converters run when a type is loaded.
    pub init_order: &'static [usize],
    /// The converters run again when the connection (system or insertion) changes.
    pub connection_parameters: &'static [usize],
    /// Valid parameter ranges (min, max), the rest is 0-127.
    pub ranges: XGParameterRanges,
}

/// Converts an XG DSP coefficient word into a float.
/// `mode` 0: signed /512, 1: signed /128, 2: 3-bit exponent and 7-bit mantissa.
fn word_to_float(word: u16, mode: i8) -> f64 {
    let w = (word & 0x3_ff) as i32;
    if mode == 2 {
        let exponent = w >> 7;
        let mantissa = w & 0x7f;
        return if exponent == 7 {
            mantissa as f64 / 128.0
        } else {
            fround((1i32 << exponent) as f64 * 2.384_185_791_015_625e-7 * mantissa as f64)
        };
    }
    let signed = if w > 0x2_00 { w - 0x4_00 } else { w };
    if mode == 1 {
        signed as f64 / 128.0
    } else {
        signed as f64 / 512.0
    }
}

/// Rounds a scaled coefficient half away from zero, like the DLL's biased truncation.
pub fn biased_round(value: f64) -> f64 {
    fround(if value >= 0.0 { value + 0.5 } else { value - 0.5 })
}

/// The shared state of the XG DSP programs (S-YXG50 model).
///
/// The parameter converters write 16-bit coefficient and address words into word RAMs.
/// Only the words named in each converter's push list reach the DSP, exactly like the original.
/// Address words are in 44.1 kHz samples and get scaled to the actual sample rate.
///
/// The delay memory belongs to the effect block and is shared by all of its processors,
/// as only one of them runs at a time. Each processor uses the start of it, sized by `memory_size`.
///
/// Converter indices from 16 up are internal converters (such as connection-dependent input levels),
/// `set_parameter` never reaches them.
pub struct XGDSPProgram {
    /// The type of the effect.
    pub effect_type: u16,
    /// The delay memory length the program needs at 44.1 kHz.
    pub memory_size: usize,
    /// The raw XG parameter values.
    pub params: [i16; 16],
    /// The coefficient word RAM.
    pub coefficient_words: [u16; COEFFICIENT_COUNT],
    /// The address word RAM.
    pub address_words: [u16; ADDRESS_COUNT],
    /// The coefficients visible to the DSP.
    pub coefficients: [f32; COEFFICIENT_COUNT],
    /// The delay taps visible to the DSP, in samples.
    pub taps: [i32; ADDRESS_COUNT],
    pub sample_rate: f64,
    pub max_buffer_size: usize,
    /// The sample rate relative to 44.1 kHz, in float32 like the original.
    pub rate_ratio: f64,
    /// The inverse of `rate_ratio`, in float32 like the original.
    pub rate_inverse: f64,
    /// The length of this program's delay memory (the start of the block's shared memory).
    pub memory_length: usize,
    pub mask: i32,
    /// The delay memory write position, moving backwards.
    pub pointer: i32,
    /// The current input level, ramped up to the target level after a type change.
    pub input_level: f64,
    /// The insertion mode the converters last ran for.
    pub insertion: bool,
    /// When false, the converters update the word RAMs without pushing them to the DSP.
    pub push_enabled: bool,
    /// The variation category selecting the coefficient formats.
    pub category: usize,
    /// The preset (words and defaults).
    pub preset: &'static XGProgramPreset,
    /// The program layout.
    pub layout: XGProgramLayout,
    /// The first-order filter designer output (b0, b1, a1).
    first_order: [f64; 3],
    address_scale: f64,
    tick_length: usize,
    tick_counter: usize,
}

impl XGDSPProgram {
    /// Creates the program state.
    /// Equivalent to: XGDSPProgram constructor
    pub fn new(
        effect_type: u16,
        memory_size: usize,
        sample_rate: f64,
        max_buffer_size: usize,
        category: usize,
        preset: &'static XGProgramPreset,
        layout: XGProgramLayout,
    ) -> Self {
        let rate_ratio = fround(sample_rate / XG_DSP_SAMPLE_RATE);
        let length = get_memory_size(sample_rate, memory_size);
        Self {
            effect_type,
            memory_size,
            params: [0; 16],
            coefficient_words: [0; COEFFICIENT_COUNT],
            address_words: [0; ADDRESS_COUNT],
            coefficients: [0.0; COEFFICIENT_COUNT],
            taps: [0; ADDRESS_COUNT],
            sample_rate,
            max_buffer_size,
            rate_ratio,
            rate_inverse: fround(1.0 / rate_ratio),
            memory_length: length,
            mask: length as i32 - 1,
            pointer: 0,
            input_level: 0.0,
            insertion: false,
            push_enabled: true,
            category,
            preset,
            layout,
            first_order: [0.0; 3],
            address_scale: sample_rate / XG_DSP_SAMPLE_RATE,
            tick_length: js_round(sample_rate * CONTROL_TICK).max(1.0) as usize,
            tick_counter: 0,
        }
    }

    /// The parameter value as a JS number.
    #[inline]
    pub fn param(&self, index: usize) -> i32 {
        self.params[index] as i32
    }

    /// Reads a coefficient as a JS number.
    #[inline]
    pub fn c(&self, index: usize) -> f64 {
        self.coefficients[index] as f64
    }

    /// Equivalent to: getSnapshot
    pub fn get_snapshot(&self) -> [i16; 16] {
        self.params
    }

    /// Clamps a raw parameter value to its valid range.
    /// Equivalent to: XGDSPProgram.clampParameter
    pub fn clamp_parameter_base(&self, param: usize, value: i32) -> i32 {
        let (min, max) = self
            .layout
            .ranges
            .iter()
            .find(|r| r.0 == param)
            .map_or((0, 127), |r| (r.1, r.2));
        max.min(min.max(value))
    }

    /// Copies coefficient words to the DSP.
    /// Equivalent to: XGDSPProgram.pushCoefficients (the base implementation)
    pub fn push_coefficients_base(&mut self, list: &[usize], count: usize, offset: usize) {
        if !self.push_enabled {
            return;
        }
        let mode_offset = self.category * COEFFICIENT_COUNT;
        for &k in &list[offset..offset + count] {
            let mode = COEFFICIENT_MODES[mode_offset + k];
            if mode != -1 {
                self.coefficients[k] = word_to_float(self.coefficient_words[k], mode) as f32;
            }
        }
    }

    /// Converts a coefficient word to its DSP value, without the category processing rules.
    pub fn raw_coefficient(&self, index: usize) -> f64 {
        let mode = COEFFICIENT_MODES[self.category * COEFFICIENT_COUNT + index];
        if mode == -1 {
            0.0
        } else {
            word_to_float(self.coefficient_words[index], mode)
        }
    }

    /// Copies address words to the DSP as delay taps.
    pub fn push_addresses(&mut self, list: &[usize], count: usize, offset: usize) {
        if !self.push_enabled {
            return;
        }
        for &k in &list[offset..offset + count] {
            // The original only stores some address slots, depending on the variation category
            if k == 0x0a || k == 0x0f || k == 0x14 || k == 0x1a {
                continue;
            }
            if (0x24..=0x3e).contains(&k) {
                let written = if self.category == 0 {
                    k == 0x28 || k == 0x2d || k == 0x33 || k == 0x38 || k == 0x3a || k == 0x3b
                } else {
                    self.category >= 2
                };
                if !written {
                    continue;
                }
            }
            self.taps[k] = (self.address_words[k] as f64 * self.address_scale).trunc() as i32;
        }
    }

    /// Copies all the given address words to the DSP.
    pub fn push_addresses_all(&mut self, list: &[usize]) {
        self.push_addresses(list, list.len(), 0);
    }

    /// The Dry/Wet parameter in effect: the system mode is always fully wet.
    pub fn mix_index(&self) -> usize {
        if self.insertion {
            self.params[9] as usize
        } else {
            127
        }
    }

    /// The feedback level coefficient word.
    /// `level` is the feedback level parameter (1-127, 64 is zero).
    pub fn feedback_word(&self, level: i32) -> u16 {
        ((((level - 0x40) * 0x7d) >> 4) & 0x3_ff) as u16
    }

    /// The LFO phase offset word.
    /// `value` is the phase difference parameter (64 is zero).
    pub fn phase_word(&self, value: i32) -> u16 {
        // Math.trunc(((0x40 - value) * 0x2_00) / 0x3c) & 0x3_ff
        ((((0x40 - value) * 0x2_00) / 0x3c) & 0x3_ff) as u16
    }

    /// Converts a 10 microsecond length into 44.1 kHz address units.
    pub fn to_address10us(&self, value: i32) -> i32 {
        ((value & 0xff_ff) * 0x70_e5) >> 16
    }

    /// Converts a 0.1 millisecond length into 44.1 kHz address units.
    pub fn to_address100us(&self, value: i32) -> i32 {
        ((value & 0xff_ff) * 0x46_8f) >> 12
    }

    /// Reads an address word as a signed 16-bit value.
    pub fn signed_address(&self, index: usize) -> i32 {
        to_signed16(self.address_words[index] as i32)
    }

    /// Writes the four density allpass lengths and coefficients (reverb, early reflections and karaoke).
    /// - `density`: the density parameter (0-3, more lines get diffused as it rises).
    /// - `diffusion`: the diffusion parameter, the fourth line reuses the third length when it is 0.
    /// - `row`: the density table row.
    /// - `doubled`: the bit mask of lines with doubled input coefficients.
    /// - `coefficient_list`: the four coefficient indices of each line.
    /// - `address_list`: the base and density tap address indices of each line.
    pub fn density_allpasses(
        &mut self,
        density: i32,
        diffusion: i32,
        row: usize,
        mut doubled: i32,
        coefficient_list: &[usize],
        address_list: &[usize],
    ) {
        let mut bits = DENSITY_MASK[3.min(density) as usize] as i32;
        let start = row * 8;
        for line in 0..4 {
            let length_index = if line == 3 && diffusion == 0 {
                start + line * 2 - 2
            } else {
                start + line * 2
            };
            let mut length = to_signed16(self.to_address10us(DENSITY_TABLE[length_index] as i32));
            if length == 0 {
                length = 1;
            }
            self.address_words[address_list[line * 2 + 1]] =
                to_u16(length + self.signed_address(address_list[line * 2]));

            let mut u6 = DENSITY_TABLE[start + line * 2 + 1] as i32;
            let mut c0: i32;
            let mut c1: i32;
            let c2: i32;
            if (bits & 1) == 0 {
                c0 = 0x80;
                c1 = 0;
                c2 = 0;
                u6 = 0;
            } else {
                c0 = (-to_signed16(u6) >> 2) & 0xff_ff;
                c1 = (0x80 - c0) & 0xff_ff;
                c2 = (0x2_00 - u6) & 0xff_ff;
            }
            if doubled & 1 != 0 {
                c0 = (c0 << 1) & 0xff_ff;
                c1 = (c1 << 1) & 0xff_ff;
            }
            bits >>= 1;
            doubled >>= 1;
            let k = line * 4;
            let w = &mut self.coefficient_words;
            w[coefficient_list[k]] = (c0 & 0x3_ff) as u16;
            w[coefficient_list[k + 1]] = (c1 & 0x3_ff) as u16;
            w[coefficient_list[k + 2]] = (c2 & 0x3_ff) as u16;
            w[coefficient_list[k + 3]] = (u6 & 0x3_ff) as u16;
        }
    }

    /// Runs a stereo input EQ: per side, a first-order low section followed by a first-order high section.
    /// - `list`: the coefficient indices (b1, b0, a1) of the left low, left high, right low and right high sections.
    /// - `state`: the filter state (low input, low output, high output of each side).
    #[allow(clippy::too_many_arguments)]
    pub fn equalize_stereo(
        &self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
        gain: f64,
        list: &[usize],
        state: &mut [f64; 6],
    ) {
        let c = |i: usize| self.coefficients[list[i]] as f64;
        let (lb1_l, lb0_l, la1_l, hb1_l, hb0_l, ha1_l) = (c(0), c(1), c(2), c(3), c(4), c(5));
        let (lb1_r, lb0_r, la1_r, hb1_r, hb0_r, ha1_r) = (c(6), c(7), c(8), c(9), c(10), c(11));
        let [mut low_x_l, mut low_y_l, mut high_y_l, mut low_x_r, mut low_y_r, mut high_y_r] =
            *state;
        for i in 0..sample_count {
            let mut x = input_left[i] as f64 * gain;
            let mut low = lb1_l * low_x_l + lb0_l * x + la1_l * low_y_l;
            high_y_l = low * hb0_l + hb1_l * low_y_l + ha1_l * high_y_l;
            low_x_l = x;
            low_y_l = low;
            output_left[i] = high_y_l as f32;
            x = input_right[i] as f64 * gain;
            low = lb1_r * low_x_r + lb0_r * x + la1_r * low_y_r;
            high_y_r = low * hb0_r + hb1_r * low_y_r + ha1_r * high_y_r;
            low_x_r = x;
            low_y_r = low;
            output_right[i] = high_y_r as f32;
        }
        *state = [low_x_l, low_y_l, high_y_l, low_x_r, low_y_r, high_y_r];
    }

    /// Advances the post type change input fade-in by the given number of samples.
    /// `target` is the input level to ramp to (the original send prescale of the block).
    pub fn advance_fade(&mut self, sample_count: usize, target: f64) {
        if self.input_level >= target {
            self.input_level = target;
            return;
        }
        self.tick_counter += sample_count;
        while self.tick_counter >= self.tick_length && self.input_level < target {
            self.tick_counter -= self.tick_length;
            self.input_level = target.min(self.input_level + FADE_STEP);
        }
    }

    /// The prewarped frequency (tan of pi * f / fs) of an XG filter frequency index.
    fn prewarp(&self, frequency: i32) -> f64 {
        let hz = (FILTER_FREQUENCIES
            .get((frequency & 0xff) as usize)
            .map_or(0.0, |&f| f as f64))
        .min(self.sample_rate * 0.49);
        fround((std::f64::consts::PI * hz / self.sample_rate).tan())
    }

    /// Computes the coefficients (b0, b1, a1) of a first-order shape into `first_order`.
    /// `w` is the prewarped frequency, `g` the filter gain index (64 is unity).
    fn first_order_coefficients(&mut self, shape: XGFilterShape, w: f64, g: usize) {
        let gain = |i: usize| FILTER_GAIN[i] as f64;
        let cut = |i: usize| FILTER_CUT[i] as f64;
        let (b0, b1, a1);
        match shape {
            XGFilterShape::LowPass | XGFilterShape::HighPass => {
                let den = fround(w + 1.0);
                if shape == XGFilterShape::HighPass {
                    b0 = fround(gain(g) / den);
                    b1 = -b0;
                } else {
                    b0 = fround(fround(gain(g) * w) / den);
                    b1 = b0;
                }
                a1 = fround(fround(1.0 - w) / den);
            }
            XGFilterShape::LowShelf | XGFilterShape::LowShelfBoosted => {
                let zero = if g < 0x40 { w } else { fround(w * gain(g)) };
                let pole = if g < 0x40 { fround(w * cut(g)) } else { w };
                let den = fround(pole + 1.0);
                let mut lb0 = fround(fround(zero + 1.0) / den);
                let mut lb1 = fround(fround(zero - 1.0) / den);
                a1 = fround(fround(1.0 - pole) / den);
                if shape == XGFilterShape::LowShelfBoosted {
                    lb0 = fround(lb0 * BOOST);
                    lb1 = fround(lb1 * BOOST);
                }
                b0 = lb0;
                b1 = lb1;
            }
            XGFilterShape::Gain => {
                b0 = gain(g);
                b1 = 0.0;
                a1 = 0.0;
            }
            XGFilterShape::HighShelf => {
                let zero = if g < 0x40 { 1.0 } else { gain(g) };
                let pole = if g < 0x40 { cut(g) } else { 1.0 };
                let den = fround(pole + w);
                b0 = fround(fround(zero + w) / den);
                b1 = fround(fround(w - zero) / den);
                a1 = fround(fround(pole - w) / den);
            }
        }
        self.first_order = [b0, b1, a1];
    }

    /// Rounds second-order coefficients into words (the push is done by the caller).
    /// The summed rounding errors of the zeros and of the poles are kept within half a step.
    /// Equivalent to: XGDSPProgram.pushSecondOrder (without the final pushCoefficients)
    fn round_second_order(&mut self, b0: f64, b1: f64, b2: f64, a1: f64, a2: f64, list: &[usize]) {
        let (x0, x1, x2, x3, x4) = (
            fround(b0 * 128.0),
            fround(b1 * 128.0),
            fround(b2 * 128.0),
            fround(a1 * 128.0),
            fround(a2 * 128.0),
        );
        let (mut r0, mut r1, mut r2, mut r3, mut r4) = (
            biased_round(x0),
            biased_round(x1),
            biased_round(x2),
            biased_round(x3),
            biased_round(x4),
        );
        let (e0, e1, e2, e3, e4) = (
            r0.trunc() - x0,
            r1.trunc() - x1,
            r2.trunc() - x2,
            r3.trunc() - x3,
            r4.trunc() - x4,
        );
        let sum = e1 + e0 + e2;
        if sum >= -0.5 {
            if sum > 0.5 {
                if e1 <= e2 {
                    if e0 <= e2 {
                        r2 -= 1.0;
                    } else {
                        r0 -= 1.0;
                    }
                } else if e0 <= e1 {
                    r1 -= 1.0;
                } else {
                    r0 -= 1.0;
                }
            }
        } else if e1 > e2 {
            if e0 > e2 {
                r2 += 1.0;
            } else {
                r0 += 1.0;
            }
        } else if e0 > e1 {
            r1 += 1.0;
        } else {
            r0 += 1.0;
        }
        let poles = e3 + e4;
        if poles >= -0.5 {
            if poles > 0.5 {
                if e3 <= e4 {
                    r4 -= 1.0;
                } else {
                    r3 -= 1.0;
                }
            }
        } else if e3 > e4 {
            r4 += 1.0;
        } else {
            r3 += 1.0;
        }
        let w = &mut self.coefficient_words;
        w[list[0]] = word_of(r0);
        w[list[1]] = word_of(r1);
        w[list[2]] = word_of(r2);
        w[list[3]] = word_of(r3);
        w[list[4]] = word_of(r4);
    }
}

/// Math.trunc(r) & 0x3_ff
#[inline]
fn word_of(r: f64) -> u16 {
    ((r.trunc() as i32) & 0x3_ff) as u16
}

/// The overridable part of XGDSPProgram (and the base methods that call it).
pub trait XGProgram {
    fn program(&self) -> &XGDSPProgram;
    fn program_mut(&mut self) -> &mut XGDSPProgram;

    /// Runs the converter for the given parameter.
    fn convert(&mut self, param: usize);

    /// Process the effect and **OVERWRITES** it to the output.
    #[allow(clippy::too_many_arguments)]
    fn process(
        &mut self,
        memory: &mut [f32],
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
        is_insertion: bool,
    );

    /// Clears the routine's own state on reset.
    fn reset_state(&mut self) {
        // No state by default
    }

    /// Clamps a raw parameter value to its valid range.
    fn clamp_parameter(&self, param: usize, value: i32) -> i32 {
        self.program().clamp_parameter_base(param, value)
    }

    /// Copies coefficient words to the DSP.
    /// `count` is how many indices of the list to push, `offset` the first list index.
    fn push_coefficients(&mut self, list: &[usize], count: usize, offset: usize) {
        self.program_mut().push_coefficients_base(list, count, offset);
    }

    /// pushCoefficients(list) with the default count and offset.
    fn push_coefficients_all(&mut self, list: &[usize]) {
        self.push_coefficients(list, list.len(), 0);
    }

    /// The public setParameter (overridden by the flanger).
    fn set_parameter_program(&mut self, param: i32, value: i32) {
        set_parameter_base(self, param, value);
    }

    /// Clears the delay memory and the routine state, then loads the type preset and the defaults.
    /// The converters first run on the defaults without pushing, the whole program is pushed,
    /// then every parameter is sent again in order, mirroring a type change in the original.
    /// Equivalent to: XGDSPProgram.reset
    fn reset_program(&mut self, memory: &mut [f32]) {
        let len = self.program().memory_length;
        memory[..len].fill(0.0);
        self.program_mut().pointer = 0;
        self.reset_state();

        let p = self.program_mut();
        let preset = p.preset;
        let layout = p.layout;
        let coefficient_list = layout.coefficient_list;
        let address_list = layout.address_list;
        p.coefficient_words.fill(0);
        p.address_words.fill(0);
        p.coefficients.fill(0.0);
        p.taps.fill(0);
        for (i, &k) in coefficient_list.iter().enumerate() {
            p.coefficient_words[k] = to_u16(preset.words[i]);
        }
        for (i, &k) in address_list.iter().enumerate() {
            p.address_words[k] = to_u16(preset.words[coefficient_list.len() + i]);
        }
        for i in 0..16 {
            p.params[i] = preset.defaults[i] as i16;
        }
        p.push_enabled = false;
        for &param in layout.init_order {
            self.convert(param);
        }
        self.program_mut().push_enabled = true;
        self.program_mut().push_addresses_all(address_list);
        self.push_coefficients_all(coefficient_list);
        for i in 0..16 {
            self.set_parameter_program(i as i32, preset.defaults[i]);
        }
        let p = self.program_mut();
        p.input_level = 0.0;
        p.tick_counter = 0;
    }

    /// The public reset (overridden by the variation categories to clear their own state first).
    /// Equivalent to: reset() as dispatched on the processor
    fn reset_xg(&mut self, memory: &mut [f32]) {
        self.reset_program(memory);
    }

    /// Runs the connection converters when the connection (system or insertion) changes.
    fn update_connection(&mut self, is_insertion: bool) {
        if is_insertion == self.program().insertion {
            return;
        }
        self.program_mut().insertion = is_insertion;
        let list = self.program().layout.connection_parameters;
        for &param in list {
            self.convert(param);
        }
    }

    /// Designs a first-order filter into three coefficient words (b0, b1, a1) and pushes them.
    /// - `frequency`: the XG filter frequency index.
    /// - `gain_index`: the filter gain index (64 is unity).
    /// - `list`: the coefficient indices for b0, b1 and a1, from `offset`.
    fn design_first_order(
        &mut self,
        shape: XGFilterShape,
        frequency: i32,
        gain_index: i32,
        list: &[usize],
        offset: usize,
    ) {
        let p = self.program_mut();
        let w = p.prewarp(frequency);
        p.first_order_coefficients(shape, w, (gain_index & 0x7f) as usize);
        let [b0, b1, a1] = p.first_order;

        // Round b0 and b1, keeping their summed rounding error within half a step
        let e0 = fround(b0 * 128.0);
        let e1 = fround(b1 * 128.0);
        let mut r0 = biased_round(e0);
        let mut r1 = biased_round(e1);
        let err0 = r0.trunc() - e0;
        let err1 = r1.trunc() - e1;
        let sum = err0 + err1;
        if sum >= -0.5 {
            if sum > 0.5 {
                if err0 <= err1 {
                    r1 -= 1.0;
                } else {
                    r0 -= 1.0;
                }
            }
        } else if err0 > err1 {
            r1 += 1.0;
        } else {
            r0 += 1.0;
        }
        let a = biased_round(fround(a1 * 512.0));

        p.coefficient_words[list[offset]] = word_of(r0);
        p.coefficient_words[list[offset + 1]] = word_of(r1);
        p.coefficient_words[list[offset + 2]] = word_of(a);
        self.push_coefficients(list, 3, offset);
    }

    /// Designs a second-order filter into five coefficient words (b0, b1, b2, a1, a2) and pushes them.
    /// - `frequency`: the XG filter frequency index.
    /// - `gain_index`: the filter gain index (64 is unity).
    /// - `width`: the filter width (Q * 10).
    /// - `list`: the coefficient indices for b0, b1, b2, a1 and a2.
    fn design_second_order(
        &mut self,
        shape: XGBiquadShape,
        frequency: i32,
        gain_index: i32,
        width: i32,
        list: &[usize],
    ) {
        let p = self.program_mut();
        let w = p.prewarp(frequency);
        let g = (gain_index & 0x7f) as usize;
        if let XGBiquadShape::First(s) = shape {
            // A first-order shape in the second-order section
            p.first_order_coefficients(s, w, g);
            let [f0, f1, f2] = p.first_order;
            p.round_second_order(f0, f1, 0.0, f2, 0.0, list);
            self.push_coefficients(list, 5, 0);
            return;
        }
        let w2 = fround(w * w);
        let mut q = fround(fround(w * 10.0) / (width & 0xff) as f64);
        let (den, b0, b2);
        // Peaking: a cut widens the poles, a boost widens the zeros
        if g < 0x40 {
            let cut = FILTER_CUT[g] as f64;
            den = fround(fround(w2 + fround(q * cut)) + 1.0);
            b0 = fround(fround(fround(w2 + q) + 1.0) / den);
            b2 = fround(fround(fround(1.0 - q) + w2) / den);
            q = fround(q * cut);
        } else {
            let zero = fround(q * FILTER_GAIN[g] as f64);
            den = fround(fround(w2 + q) + 1.0);
            b0 = fround(fround(fround(w2 + zero) + 1.0) / den);
            b2 = fround(fround(fround(1.0 - zero) + w2) / den);
        }
        let b1 = fround(fround(fround(w2 + w2) - 2.0) / den);
        let a1 = -b1;
        let a2 = fround(fround(fround(q - 1.0) - w2) / den);
        p.round_second_order(b0, b1, b2, a1, a2, list);
        self.push_coefficients(list, 5, 0);
    }
}

/// Equivalent to: XGDSPProgram.setParameter (the base implementation)
pub fn set_parameter_base<P: XGProgram + ?Sized>(program: &mut P, param: i32, value: i32) {
    if !(0..=15).contains(&param) {
        return;
    }
    let param = param as usize;
    let v = program.clamp_parameter(param, value);
    program.program_mut().params[param] = v as i16;
    program.convert(param);
}

impl<T: XGProgram> DefaultXGEffectProcessor for T {
    fn effect_type(&self) -> u16 {
        self.program().effect_type
    }

    fn memory_size(&self) -> usize {
        self.program().memory_size
    }

    fn reset(&mut self, memory: &mut [f32]) {
        self.reset_xg(memory);
    }

    fn set_parameter(&mut self, param: i32, value: i32) {
        self.set_parameter_program(param, value);
    }

    fn process(
        &mut self,
        memory: &mut [f32],
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
        is_insertion: bool,
    ) {
        XGProgram::process(
            self,
            memory,
            input_left,
            input_right,
            output_left,
            output_right,
            sample_count,
            is_insertion,
        );
    }

    fn get_snapshot(&self) -> [i16; 16] {
        self.program().get_snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chop32_truncates_toward_zero() {
        // 0.1 rounds up to nearest f32; chop32 must go one step below
        let nearest = 0.1_f32 as f64;
        assert!(nearest > 0.1);
        let c = chop32(0.1);
        assert!(c < 0.1);
        assert_eq!(f32::from_bits((c as f32).to_bits() + 1) as f64, nearest);
        assert_eq!(chop32(-0.1), -c);
        assert_eq!(chop32(0.5), 0.5);
    }

    #[test]
    fn test_triangle_and_signed16() {
        assert_eq!(triangle(0), 0);
        assert_eq!(triangle(1 << 22), (1 << 22) ^ PHASE_MASK);
        assert_eq!(to_signed16(0xffff), -1);
        assert_eq!(to_signed16(0x7fff), 0x7fff);
    }

    #[test]
    fn test_memory_size_power_of_two() {
        assert_eq!(get_memory_size(44_100.0, 27_187), 32_768);
        assert_eq!(get_memory_size(48_000.0, 27_187), 32_768);
        assert_eq!(get_memory_size(48_000.0, 131_072), 262_144);
        assert_eq!(get_memory_size(44_100.0, 0), 1);
    }

    #[test]
    fn test_word_to_float() {
        assert_eq!(word_to_float(0x80, 1), 1.0);
        assert_eq!(word_to_float(0x200, 0), 1.0);
        assert_eq!(word_to_float(0x3ff, 0), -1.0 / 512.0);
        assert_eq!(word_to_float((7 << 7) | 64, 2), 0.5);
    }
}
