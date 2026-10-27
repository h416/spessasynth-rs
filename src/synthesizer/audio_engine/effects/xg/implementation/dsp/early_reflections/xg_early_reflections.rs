//! The S-YXG50 early reflection program, shared by EARLY REF 1/2, GATE REVERB and REVERSE GATE.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/early_reflections/xg_early_reflections.ts
//!
//! Fourteen reflection taps of the mono input (a pattern scaled by the room size) are summed in two groups.
//! Diffusion allpasses and a damped feedback loop thicken the reflections.
//! The two groups become the left and right outputs.

use super::super::xg_dsp_program::{
    to_u16, XGDSPProgram, XGFilterShape, XGParameterRanges, XGProgram, XGProgramPreset,
};
use super::super::xg_dsp_tables::{DELAY_TABLE, MIX_DRY, MIX_WET};
use super::super::xg_mono_variation_program::{
    mono_input_level, XGMonoState, CATEGORY, LIST_LOW_A, LIST_LOW_B, UNWRITTEN_CATEGORY_2,
};
use super::super::xg_variation_program::{variation_layout, XGVariationState, MEMORY_SIZE};
use super::early_reflection_tables::{
    DENSITY_ROW, DIFFUSION_ADDRESS, ER_PATTERNS, HIGH_DAMP_FREQUENCY, LIVENESS_OFFSET,
    REFLECTION_GAIN, ROOM_SCALE,
};

// Reflection taps (14) and their gain coefficients
const LIST_REFLECTION_ADDRESS: &[usize] =
    &[0x3b, 0x32, 0x3a, 0x31, 0x39, 0x30, 0x38, 0x2f, 0x37, 0x27, 0x36, 0x26, 0x35, 0x25];
const LIST_REFLECTION_COEFFICIENT: &[usize] =
    &[0xba, 0x9f, 0xb9, 0x9e, 0xb8, 0x9d, 0xb7, 0x9c, 0xb6, 0x9b, 0xb5, 0x9a, 0xb4, 0x99];
// Density allpasses: four coefficients and (base, tap) addresses per line
const LIST_DENSITY_COEFFICIENT: &[usize] =
    &[0x7f, 0x7e, 0x7c, 0x7b, 0x85, 0x84, 0x82, 0x81, 0x8c, 0x8b, 0x89, 0x88, 0x98, 0x97, 0x8f, 0x8e];
const LIST_DENSITY_ADDRESS: &[usize] = &[0x2a, 0x24, 0x2c, 0x28, 0x2e, 0x29, 0x34, 0x2b];
const LIST_HIGH_DAMP: &[usize] = &[0x78, 0x77, 0x79];

/// The converter index of the mono input level, which depends on the connection.
const MONO_INPUT: usize = 16;

const INIT_ORDER: &[usize] = &[0, 2, 4, 5, 6, 9, 12, MONO_INPUT];
const CONNECTION_PARAMETERS: &[usize] = &[9, MONO_INPUT];

/// The reflection gain word of a gain index.
fn reflection_gain(index: i32) -> i32 {
    REFLECTION_GAIN[191.min(0.max((index << 16) >> 16)) as usize] as i32
}

/// The parameter ranges of the early reflection types.
pub const ER_RANGES: XGParameterRanges = &[
    (0, 0, 5),
    (1, 0, 44),
    (2, 0, 10),
    (3, 0, 63),
    (4, 1, 127),
    (5, 0, 52),
    (6, 34, 60),
    (10, 0, 10),
    (11, 0, 3),
    (12, 1, 10),
];

/// The parameter ranges of the gate types.
pub const GATE_RANGES: XGParameterRanges = &[
    (0, 0, 1),
    (1, 0, 44),
    (2, 0, 10),
    (3, 0, 63),
    (4, 1, 127),
    (5, 0, 52),
    (6, 34, 60),
    (10, 0, 10),
    (11, 0, 3),
    (12, 1, 10),
];

/// The S-YXG50 early reflection program.
/// Equivalent to: abstract class XGEarlyReflectionsBase
pub struct XGEarlyReflectionsBase {
    pub(crate) program: XGDSPProgram,
    pub(crate) var: XGVariationState,
    pub(crate) mono: XGMonoState,
    /// The first reflection pattern of the type.
    pattern_offset: i32,
    /// The density row block of the type (6 rows each).
    density_block: usize,
    // The damped feedback filter state
    damp_x: f64,
    damp_y: f64,
}

impl XGEarlyReflectionsBase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        effect_type: u16,
        preset: &'static XGProgramPreset,
        pattern_offset: i32,
        density_block: usize,
        ranges: XGParameterRanges,
        sample_rate: f64,
        max_buffer_size: usize,
    ) -> Self {
        Self {
            program: XGDSPProgram::new(
                effect_type,
                MEMORY_SIZE,
                sample_rate,
                max_buffer_size,
                CATEGORY,
                preset,
                variation_layout(INIT_ORDER, CONNECTION_PARAMETERS, ranges),
            ),
            var: XGVariationState::new(max_buffer_size, UNWRITTEN_CATEGORY_2),
            mono: XGMonoState::new(max_buffer_size),
            pattern_offset,
            density_block,
            damp_x: 0.0,
            damp_y: 0.0,
        }
    }

    fn render_impl(
        &mut self,
        memory: &mut [f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
        output_gain: f64,
    ) {
        self.mono.mix_mono(&self.program, &self.var, sample_count);
        let mask = self.program.mask;
        let c = |i: usize| self.program.coefficients[i] as f64;
        let t = &self.program.taps;
        let mono = &self.mono.mono;
        let dry_l = &self.mono.dry_left;
        let dry_r = &self.mono.dry_right;
        let out_l = c(0xaf) * output_gain;
        let out_r = c(0xb2) * output_gain;
        let mut pointer = self.program.pointer;
        let (mut damp_x, mut damp_y) = (self.damp_x, self.damp_y);
        macro_rules! at {
            ($tap:expr) => {
                ((t[$tap] + pointer) & mask) as usize
            };
        }
        macro_rules! m {
            ($tap:expr) => {
                memory[at!($tap)] as f64
            };
        }
        // The original reads some lines again after writing others, mirrored here for coinciding taps
        for i in 0..sample_count {
            let x = mono[i] as f64;
            // Two diffusion allpasses on the input
            let k24 = at!(0x24);
            memory[at!(0x2a)] = (c(0x7c) * x + memory[k24] as f64 * c(0x7b)) as f32;
            let k28 = at!(0x28);
            let diffused = c(0x7f) * x + memory[k24] as f64 * c(0x7e);
            memory[at!(0x2c)] = (diffused * c(0x82) + memory[k28] as f64 * c(0x81)) as f32;
            let second = memory[k28] as f64;
            // Damped feedback into the reflection line
            let k25 = at!(0x25);
            damp_y = c(0x77) * damp_x + c(0x78) * memory[k25] as f64 + c(0x79) * damp_y;
            damp_x = memory[k25] as f64;
            memory[at!(0x2d)] = (c(0x86) * damp_y + diffused * c(0x85) + second * c(0x84)) as f32;
            // Two groups of seven reflections
            let sum_a = memory[k25] as f64 * c(0x99)
                + m!(0x26) * c(0x9a)
                + m!(0x2f) * c(0x9c)
                + m!(0x31) * c(0x9e)
                + m!(0x27) * c(0x9b)
                + m!(0x30) * c(0x9d)
                + m!(0x32) * c(0x9f);
            let sum_b = m!(0x36) * c(0xb5)
                + m!(0x38) * c(0xb7)
                + m!(0x3a) * c(0xb9)
                + m!(0x35) * c(0xb4)
                + m!(0x37) * c(0xb6)
                + m!(0x39) * c(0xb8)
                + m!(0x3b) * c(0xba);
            memory[at!(0x3d)] = sum_a as f32;
            let group_left = m!(0x3c) * c(0xbb) + sum_b;
            memory[at!(0x3e)] = sum_b as f32;
            let group_right = m!(0x33) * c(0xa0) + sum_a;
            // Output allpasses
            let k29 = at!(0x29);
            memory[at!(0x2e)] = (c(0x89) * group_left + memory[k29] as f64 * c(0x88)) as f32;
            let left = c(0x8c) * group_left + memory[k29] as f64 * c(0x8b);
            let k2b = at!(0x2b);
            memory[at!(0x34)] = (c(0x8f) * group_right + memory[k2b] as f64 * c(0x8e)) as f32;
            let right = c(0x98) * group_right + memory[k2b] as f64 * c(0x97);
            output_left[i] = (out_l * left + dry_l[i] as f64 * output_gain) as f32;
            output_right[i] = (out_r * right + dry_r[i] as f64 * output_gain) as f32;
            pointer = (pointer - 1) & mask;
        }
        self.program.pointer = pointer;
        self.damp_x = damp_x;
        self.damp_y = damp_y;
    }

    /// Note: overrides the mono resetState without calling it, like the TS code
    /// (the mono filter states are kept).
    fn reset_state_impl(&mut self) {
        self.damp_x = 0.0;
        self.damp_y = 0.0;
    }

    fn convert_impl(&mut self, param: usize) {
        let p = |d: &Self, i: usize| d.program.param(i);
        match param {
            0 => {
                self.reflections();
                self.density();
                self.push_coefficients_all(LIST_REFLECTION_COEFFICIENT);
                self.program.push_addresses_all(LIST_REFLECTION_ADDRESS);
                self.push_coefficients_all(LIST_DENSITY_COEFFICIENT);
                self.program.push_addresses_all(LIST_DENSITY_ADDRESS);
            }
            1 | 3 => {
                self.reflections();
                self.push_coefficients_all(LIST_REFLECTION_COEFFICIENT);
                self.program.push_addresses_all(LIST_REFLECTION_ADDRESS);
            }
            2 => {
                let length = DIFFUSION_ADDRESS[p(self, 2) as usize] as i32;
                let a = &mut self.program.address_words;
                a[0x3c] = ((length + a[0x3d] as i32) & 0xff_ff) as u16;
                a[0x33] = ((length + a[0x3e] as i32) & 0xff_ff) as u16;
                self.program.push_addresses_all(&[0x3c, 0x33]);
                self.density();
                self.program.push_addresses_all(&[0x2b]);
            }
            4 => {
                // Feedback level, 64 is none
                self.program.coefficient_words[0x86] = self.program.feedback_word(p(self, 4));
                self.push_coefficients_all(&[0x86]);
            }
            5 => self.design_first_order(XGFilterShape::HighPass, p(self, 5), 0x40, LIST_LOW_A, 0),
            6 => self.design_first_order(XGFilterShape::LowPass, p(self, 6), 0x40, LIST_LOW_B, 0),
            9 => {
                let mix = self.program.mix_index();
                let w = &mut self.program.coefficient_words;
                w[0xad] = (MIX_DRY[mix] as i32 & 0x3_ff) as u16;
                w[0xb1] = w[0xad];
                w[0xaf] = (MIX_WET[mix] as i32 & 0x3_ff) as u16;
                w[0xb2] = w[0xaf];
                self.push_coefficients_all(&[0xad, 0xaf, 0xb1, 0xb2]);
            }
            10 => {
                self.reflections();
                self.push_coefficients_all(LIST_REFLECTION_COEFFICIENT);
            }
            11 => {
                self.density();
                self.push_coefficients_all(LIST_DENSITY_COEFFICIENT);
            }
            12 => {
                let frequency = HIGH_DAMP_FREQUENCY[p(self, 12) as usize] as i32;
                self.design_first_order(XGFilterShape::LowPass, frequency, 0x40, LIST_HIGH_DAMP, 0);
            }
            MONO_INPUT => mono_input_level(self),
            _ => {}
        }
    }

    /// Writes the reflection taps and gains from the type, room size, initial delay and liveness.
    fn reflections(&mut self) {
        let pr = &mut self.program;
        let p = |i: usize| pr.params[i] as i32;
        let pattern = ((p(0) + self.pattern_offset) * 28) as usize;
        let scale = ROOM_SCALE[p(1) as usize] as i32;
        let liveness = LIVENESS_OFFSET[p(10) as usize] as i32 - 0x80;
        let start = (((pr.to_address100us(DELAY_TABLE[p(3) as usize] as i32) << 16) >> 16)
            + pr.signed_address(0x2d))
            & 0xff_ff;
        for k in 0..14 {
            let delay =
                (pr.to_address10us(ER_PATTERNS[pattern + k * 2] as i32 & 0xff_ff) * scale) / 10 + start;
            let mut gain = 0;
            let mut address = 0xff_ff;
            if delay <= 0xff_ff {
                address = delay;
                let level = ER_PATTERNS[pattern + k * 2 + 1] as i32;
                gain = if level < 1 {
                    -reflection_gain(liveness - level) & 0x3_ff
                } else {
                    reflection_gain(level + liveness)
                };
            }
            pr.address_words[LIST_REFLECTION_ADDRESS[k]] = to_u16(address);
            pr.coefficient_words[LIST_REFLECTION_COEFFICIENT[k]] = to_u16(gain);
        }
    }

    /// Writes the density allpasses.
    fn density(&mut self) {
        let row = DENSITY_ROW[self.density_block * 6 + self.program.param(0) as usize] as usize;
        let (density, diffusion) = (self.program.param(11), self.program.param(2));
        self.program.density_allpasses(
            density,
            diffusion,
            row,
            0,
            LIST_DENSITY_COEFFICIENT,
            LIST_DENSITY_ADDRESS,
        );
    }
}

crate::impl_xg_mono_variation_program!(XGEarlyReflectionsBase);
