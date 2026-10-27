//! The S-YXG50 chorus program, shared by chorus, celeste and flanger.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/chorus/xg_chorus.ts
//!
//! The TS class tree `XGChorusBase` -> `XGChorus` -> `XGCeleste` / `XGFlanger` is one struct
//! with an [`XGChorusKind`] selecting the converters (`initOrder`, `ranges`, `depth`, `feedback`,
//! `threeTaps` and the flanger's `setParameter`/`convert` overrides).

use super::super::xg_dsp_program::{
    fround, set_parameter_base, to_signed16, to_u16, triangle, XGDSPProgram, XGFilterShape,
    XGParameterRanges, XGProgram, XGProgramLayout, XGProgramPreset, PHASE_MASK, RETURN_GAIN,
    STEREO_SEND_LEVEL, TRIANGLE_PEAK, TRIANGLE_SCALE, VARIATION_INPUT_LEVEL,
};
use super::super::xg_dsp_tables::{DELAY_OFFSET, LFO_RATE, PAN_LEFT, PAN_RIGHT};

/// The type-specific data of an XG chorus block effect.
/// The words are 28 coefficient words (including the output levels) followed by 10 address words.
/// Equivalent to: interface XGChorusPreset
#[derive(Debug)]
pub struct XGChorusPreset {
    pub base: XGProgramPreset,
    /// The wet level in the variation block (/64).
    pub variation_gain: i32,
}

/// The converter family of a chorus program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XGChorusKind {
    /// Equivalent to: abstract class XGChorus
    Chorus,
    /// Equivalent to: abstract class XGCeleste (the chorus converters with a third modulated tap)
    Celeste,
    /// Equivalent to: abstract class XGFlanger
    Flanger,
}

// Program layout: which coefficient and address indices the preset words fill
const COEFFICIENT_LIST: &[usize] = &[
    0x09, 0x0a, 0x1a, 0x25, 0x28, 0x34, 0x37, 0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x41, 0x42, 0x49, 0x4a,
    0x4b, 0x4c, 0x4d, 0x4e, 0x4f, 0x50, 0x5f, 0x60, 0x61, 0x68, 0x69, 0x6a,
];
const ADDRESS_LIST: &[usize] = &[0x09, 0x0a, 0x0e, 0x0f, 0x13, 0x14, 0x16, 0x19, 0x1a, 0x1b];

// Converter push lists
const LIST_LOW_LEFT: &[usize] = &[0x4a, 0x49, 0x4b];
const LIST_LOW_RIGHT: &[usize] = &[0x3d, 0x3c, 0x3e];
const LIST_HIGH_LEFT: &[usize] = &[0x4d, 0x4c, 0x4e];
const LIST_HIGH_RIGHT: &[usize] = &[0x40, 0x3f, 0x41];
const LIST_FEEDBACK: &[usize] = &[0x4f, 0x42];
const LIST_DEPTH: &[usize] = &[0x1a, 0x28, 0x37];
const LIST_LEFT_TAPS: &[usize] = &[0x09, 0x0a, 0x13, 0x14];
const LIST_RIGHT_TAPS: &[usize] = &[0x16, 0x0e, 0x0f, 0x19, 0x1a];
const LIST_DRY_WET: &[usize] = &[0x5f, 0x60, 0x61, 0x68, 0x69, 0x6a];
// Input EQ coefficients (b1, b0, a1): left low, left high, right low, right high
const LIST_EQUALIZER: &[usize] =
    &[0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x41];

/// The right line base in the stereo input mode (chorus block).
/// The variation places it further away in its larger memory.
const STEREO_BASE: i32 = 4096;
const VARIATION_STEREO_BASE: i32 = 16_384;

/// Valid parameter ranges (min, max), the rest is 0-127.
const PARAMETER_RANGES: XGParameterRanges =
    &[(2, 1, 127), (5, 8, 40), (6, 52, 76), (7, 28, 58), (8, 52, 76), (14, 0, 1)];
const FLANGER_RANGES: XGParameterRanges = &[
    (2, 1, 127),
    (5, 8, 40),
    (6, 52, 76),
    (7, 28, 58),
    (8, 52, 76),
    (14, 0, 1),
    (3, 0, 63),
    (13, 4, 124),
];

/// The EQ and the dry/wet levels depend on the connection (only in the variation).
const CONNECTION_PARAMETERS: &[usize] = &[5, 9];
const CHORUS_INIT_ORDER: &[usize] = &[0, 1, 2, 3];
const FLANGER_INIT_ORDER: &[usize] = &[0, 1, 2, 3, 13];

/// The chorus input level (the original send prescale).
const INPUT_LEVEL: f64 = 103.0 / 32.0;

/// The S-YXG50 chorus program, shared by chorus, celeste and flanger.
///
/// Flow: two-band input EQ per side -> delay line(s) read by triangle LFO modulated taps with feedback.
/// In the mono input mode both sides feed one line and the right tap is a second LFO phase of it.
/// Celeste adds a third modulated tap per side.
///
/// Equivalent to: abstract class XGChorusBase
pub struct XGChorusBase {
    program: XGDSPProgram,
    /// True when running in the variation or insertion block.
    variation: bool,
    preset: &'static XGChorusPreset,
    kind: XGChorusKind,
    /// True for the three-tap celeste routine.
    three_taps: bool,

    // LFO and the tap modulations (updated every 4 samples)
    phase: i32,
    step: i32,
    right_offset: i32,
    third_offset: i32,
    depth_left: i32,
    depth_right: i32,
    depth_third: i32,
    lfo_counter: u32,
    int_left: i32,
    frac_left: f64,
    int_right: i32,
    frac_right: f64,
    int_third: i32,
    frac_third: f64,
    /// The EQ'd input of the current block.
    input_left: Vec<f32>,
    input_right: Vec<f32>,
    equalizer_state: [f64; 6],
}

impl XGChorusBase {
    /// The delay memory length at 44.1 kHz.
    pub const MEMORY_SIZE: usize = 32_768;

    pub fn new(
        effect_type: u16,
        preset: &'static XGChorusPreset,
        kind: XGChorusKind,
        sample_rate: f64,
        max_buffer_size: usize,
        variation: bool,
    ) -> Self {
        let flanger = kind == XGChorusKind::Flanger;
        let layout = XGProgramLayout {
            coefficient_list: COEFFICIENT_LIST,
            address_list: ADDRESS_LIST,
            init_order: if flanger { FLANGER_INIT_ORDER } else { CHORUS_INIT_ORDER },
            connection_parameters: CONNECTION_PARAMETERS,
            ranges: if flanger { FLANGER_RANGES } else { PARAMETER_RANGES },
        };
        Self {
            program: XGDSPProgram::new(
                effect_type,
                Self::MEMORY_SIZE,
                sample_rate,
                max_buffer_size,
                0,
                &preset.base,
                layout,
            ),
            variation,
            preset,
            kind,
            three_taps: kind == XGChorusKind::Celeste,
            phase: 0,
            step: 0,
            right_offset: 0,
            third_offset: 0,
            depth_left: 0,
            depth_right: 0,
            depth_third: 0,
            lfo_counter: 0,
            int_left: 0,
            frac_left: 0.0,
            int_right: 0,
            frac_right: 0.0,
            int_third: 0,
            frac_third: 0.0,
            input_left: vec![0.0; max_buffer_size],
            input_right: vec![0.0; max_buffer_size],
            equalizer_state: [0.0; 6],
        }
    }

    /// Equivalent to: XGChorusBase.convert
    fn convert_base(&mut self, param: usize) {
        let p = |s: &Self, i: usize| s.program.param(i);
        match param {
            0 => {
                self.program.coefficient_words[0x09] = LFO_RATE[p(self, 0) as usize];
                self.push_coefficients_all(&[0x09]);
            }

            1 => {
                self.depth();
            }

            2 => {
                let word = self.feedback();
                self.program.coefficient_words[0x4f] = word;
                self.program.coefficient_words[0x42] = word;
                self.push_coefficients_all(LIST_FEEDBACK);
            }

            3 | 14 => {
                self.place_taps();
                self.convert(5);
                self.convert(7);
                self.convert(9);
            }

            5 | 6 => {
                // The variation drops the +3 dB low shelf in the mono insertion mode
                let shape = if self.variation && self.program.insertion && p(self, 14) != 1 {
                    XGFilterShape::LowShelf
                } else {
                    XGFilterShape::LowShelfBoosted
                };
                let (f, g) = (p(self, 5), p(self, 6));
                self.design_first_order(shape, f, g, LIST_LOW_LEFT, 0);
                self.design_first_order(shape, f, g, LIST_LOW_RIGHT, 0);
            }

            7 | 8 => {
                let (f, g) = (p(self, 7), p(self, 8));
                self.design_first_order(XGFilterShape::HighShelf, f, g, LIST_HIGH_LEFT, 0);
                self.design_first_order(XGFilterShape::HighShelf, f, g, LIST_HIGH_RIGHT, 0);
            }

            9 => {
                // Dry/wet, only in the variation (the system mode is always fully wet)
                if !self.variation {
                    return;
                }
                let mix = self.program.mix_index();
                let stereo = p(self, 14) == 1;
                let mut dry = PAN_LEFT[mix] as i32;
                if stereo {
                    dry = (dry * 7) / 10;
                }
                let mut wet = PAN_RIGHT[mix] as i32;
                if !stereo && self.program.insertion {
                    wet = (wet * 7) / 10;
                }
                wet = (self.preset.variation_gain * wet) >> 6;
                let w = &mut self.program.coefficient_words;
                w[0x5f] = (dry & 0x3_ff) as u16;
                w[0x68] = (dry & 0x3_ff) as u16;
                for k in [0x60, 0x69, 0x61, 0x6a] {
                    w[k] = (wet & 0x3_ff) as u16;
                }
                self.push_coefficients_all(LIST_DRY_WET);
            }

            _ => {}
        }
    }

    /// Writes and pushes the LFO depth coefficient words.
    fn depth(&mut self) {
        if self.kind == XGChorusKind::Flanger {
            // XGFlanger.depth
            let word = ((self.program.param(1) * 0x80) / 0x7f) as u16;
            self.program.coefficient_words[0x1a] = word;
            self.program.coefficient_words[0x28] = word;
            self.push_coefficients(LIST_DEPTH, 2, 0);
        } else {
            // XGChorus.depth
            let word = (((self.program.param(1) << 1) * 0x80) / 0x7f) as u16;
            for &k in LIST_DEPTH {
                self.program.coefficient_words[k] = word;
            }
            self.push_coefficients_all(LIST_DEPTH);
        }
    }

    /// The feedback coefficient word.
    fn feedback(&self) -> u16 {
        if self.kind == XGChorusKind::Flanger {
            // XGFlanger.feedback
            self.program.feedback_word(self.program.param(2))
        } else {
            // XGChorus.feedback
            ((((self.program.param(2) - 0x40) * 0x5e) >> 4) & 0x3_ff) as u16
        }
    }

    /// Places the write and modulated read taps from the delay offset and input mode.
    fn place_taps(&mut self) {
        let variation = self.variation;
        let p = &mut self.program;
        let delay = to_signed16(p.to_address100us(DELAY_OFFSET[p.param(3) as usize] as i32));
        let mut base = p.signed_address(0x1b);
        p.address_words[0x09] = to_u16(base + 1 + delay);
        p.address_words[0x0a] = to_u16(base + 2 + delay);
        p.address_words[0x13] = to_u16(base + 0x41 + delay);
        p.address_words[0x14] = to_u16(base + 0x42 + delay);
        if p.param(14) == 1 {
            // Stereo: the right side gets its own line in the upper half
            p.coefficient_words[0x50] = 0;
            base = if variation { VARIATION_STEREO_BASE } else { STEREO_BASE };
            p.address_words[0x16] = to_u16(base);
        } else {
            p.coefficient_words[0x50] = 0x2_00;
            p.address_words[0x16] = 0;
        }
        p.address_words[0x0e] = to_u16(base + 1 + delay);
        p.address_words[0x0f] = to_u16(base + 2 + delay);
        p.address_words[0x19] = to_u16(base + 0x41 + delay);
        p.address_words[0x1a] = to_u16(base + 0x42 + delay);
        self.push_coefficients_all(&[0x50]);
        self.program.push_addresses_all(LIST_LEFT_TAPS);
        self.program.push_addresses_all(LIST_RIGHT_TAPS);
    }
}

impl XGProgram for XGChorusBase {
    fn program(&self) -> &XGDSPProgram {
        &self.program
    }

    fn program_mut(&mut self) -> &mut XGDSPProgram {
        &mut self.program
    }

    fn set_parameter_program(&mut self, param: i32, value: i32) {
        // XGFlanger.setParameter: the input mode of a flanger cannot be changed
        if self.kind == XGChorusKind::Flanger && param == 14 {
            return;
        }
        set_parameter_base(self, param, value);
    }

    fn convert(&mut self, param: usize) {
        // XGFlanger.convert
        if self.kind == XGChorusKind::Flanger && param == 13 {
            self.program.coefficient_words[0x25] =
                self.program.phase_word(self.program.param(13));
            self.push_coefficients_all(&[0x25]);
            return;
        }
        self.convert_base(param);
    }

    fn reset_state(&mut self) {
        self.phase = 0;
        self.lfo_counter = 0;
        self.int_left = 0;
        self.int_right = 0;
        self.int_third = 0;
        self.frac_left = 0.0;
        self.frac_right = 0.0;
        self.frac_third = 0.0;
        self.equalizer_state = [0.0; 6];
    }

    fn push_coefficients(&mut self, list: &[usize], count: usize, offset: usize) {
        self.program.push_coefficients_base(list, count, offset);
        // The LFO runs on integers derived from the coefficients
        let inverse = self.program.rate_inverse;
        let depth_scale = fround(self.program.rate_ratio * 2048.0);
        let c = |k: usize| self.program.coefficients[k] as f64;
        for &k in &list[offset..offset + count] {
            match k {
                0x09 => self.step = (c(0x09) * inverse * 16_777_216.0).trunc() as i32,
                0x25 => self.right_offset = (c(0x25) * 4_194_304.0).trunc() as i32,
                0x34 => self.third_offset = (c(0x34) * 4_194_304.0).trunc() as i32,
                0x1a => self.depth_left = (c(0x1a) * depth_scale).trunc() as i32,
                0x28 => self.depth_right = (c(0x28) * depth_scale).trunc() as i32,
                0x37 => self.depth_third = (c(0x37) * depth_scale).trunc() as i32,
                _ => {}
            }
        }
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
        self.update_connection(is_insertion);
        let memory = &mut memory[..self.program.memory_length];
        let mask = self.program.mask;
        let c = |i: usize| self.program.coefficients[i] as f64;
        let t = &self.program.taps;
        let (feedback_l, feedback_r, right_to_left) = (c(0x4f), c(0x42), c(0x50));
        let (out_l, out_r, out_third_l, out_third_r, dry_l, dry_r) =
            (c(0x60), c(0x69), c(0x61), c(0x6a), c(0x5f), c(0x68));
        // Insertion output goes straight to the part, the system output gets the block return and pan
        let output_gain = if self.variation && is_insertion { 1.0 } else { RETURN_GAIN };
        let (t_read_l, t_read_r, t_third_l, t_third_r, t_write_l, t_write_r) =
            (t[0x09], t[0x0e], t[0x13], t[0x19], t[0x1b], t[0x16]);
        let (step, right_offset, third_offset) = (self.step, self.right_offset, self.third_offset);
        let depth_left = self.depth_left as f64;
        let depth_right = self.depth_right as f64;
        let depth_third = self.depth_third as f64;
        let three_taps = self.three_taps;
        // Two first-order EQ sections per side.
        // System sends are halved like the original, the insertion input is the part itself
        let gain = self.program.input_level
            * (if self.variation && is_insertion { 1.0 } else { STEREO_SEND_LEVEL });
        self.program.equalize_stereo(
            input_left,
            input_right,
            &mut self.input_left,
            &mut self.input_right,
            sample_count,
            gain,
            LIST_EQUALIZER,
            &mut self.equalizer_state,
        );
        let eq_left = &self.input_left;
        let eq_right = &self.input_right;

        let mut pointer = self.program.pointer;
        let mut phase = self.phase;
        let mut lfo_counter = self.lfo_counter;
        let (mut int_left, mut frac_left) = (self.int_left, self.frac_left);
        let (mut int_right, mut frac_right) = (self.int_right, self.frac_right);
        let (mut int_third, mut frac_third) = (self.int_third, self.frac_third);
        macro_rules! m {
            ($index:expr) => {
                memory[(($index) & mask) as usize] as f64
            };
        }
        for i in 0..sample_count {
            let in_l = eq_left[i] as f64;
            let in_r = eq_right[i] as f64;

            // The LFO updates every 4 samples
            let counter = lfo_counter;
            lfo_counter = lfo_counter.wrapping_add(1);
            if (counter & 3) == 0 {
                phase = (phase + step) & PHASE_MASK;
                let mut v = triangle(phase) as f64 * depth_left;
                int_left = (v * TRIANGLE_SCALE).floor() as i32;
                frac_left = (v % TRIANGLE_PEAK) * TRIANGLE_SCALE;
                v = triangle((phase - right_offset) & PHASE_MASK) as f64 * depth_right;
                int_right = (v * TRIANGLE_SCALE).floor() as i32;
                frac_right = (v % TRIANGLE_PEAK) * TRIANGLE_SCALE;
                if three_taps {
                    v = triangle((phase - third_offset) & PHASE_MASK) as f64 * depth_third;
                    int_third = (v * TRIANGLE_SCALE).floor() as i32;
                    frac_third = (v % TRIANGLE_PEAK) * TRIANGLE_SCALE;
                }
            }

            // Modulated taps with linear interpolation
            let mut k = (t_read_l + pointer + int_left) & mask;
            let tap_l = (m!(k + 1) - m!(k)) * frac_left + m!(k);
            k = (t_read_r + pointer + int_right) & mask;
            let tap_r = (m!(k + 1) - m!(k)) * frac_right + m!(k);
            let mut wet_l = tap_l * out_l;
            let mut wet_r = tap_r * out_r;
            if three_taps {
                k = (t_third_l + pointer + int_third) & mask;
                wet_l += ((m!(k + 1) - m!(k)) * frac_third + m!(k)) * out_third_l;
                k = (t_third_r + pointer + int_third) & mask;
                wet_r += ((m!(k + 1) - m!(k)) * frac_third + m!(k)) * out_third_r;
            }
            memory[((t_write_l + pointer) & mask) as usize] =
                (right_to_left * in_r + tap_l * feedback_l + in_l) as f32;
            memory[((t_write_r + pointer) & mask) as usize] = (tap_r * feedback_r + in_r) as f32;
            output_left[i] = ((wet_l + in_l * dry_l) * output_gain) as f32;
            output_right[i] = ((wet_r + in_r * dry_r) * output_gain) as f32;

            pointer = (pointer - 1) & mask;
        }
        self.program.pointer = pointer;
        self.phase = phase;
        self.lfo_counter = lfo_counter & 3;
        self.int_left = int_left;
        self.frac_left = frac_left;
        self.int_right = int_right;
        self.frac_right = frac_right;
        self.int_third = int_third;
        self.frac_third = frac_third;
        let target = if self.variation {
            if is_insertion { 1.0 } else { VARIATION_INPUT_LEVEL }
        } else {
            INPUT_LEVEL
        };
        self.program.advance_fade(sample_count, target);
    }
}
