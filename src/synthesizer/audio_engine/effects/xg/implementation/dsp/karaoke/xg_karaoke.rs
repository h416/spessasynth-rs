//! The S-YXG50 karaoke program, shared by KARAOKE 1-3: a filtered mono echo with diffusion.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/karaoke/xg_karaoke.ts
//!
//! The mono input goes through a high pass and a low pass, two diffusion allpasses and a damped feedback delay.
//! Two output allpasses decorrelate the left and right echoes.

use super::super::xg_dsp_program::{
    XGDSPProgram, XGFilterShape, XGParameterRanges, XGProgram, XGProgramPreset,
};
use super::super::xg_dsp_tables::{MIX_DRY, MIX_WET};
use super::super::xg_filter_variation_program::{CATEGORY, UNWRITTEN_CATEGORY_3};
use super::super::xg_variation_program::{variation_layout, XGVariationState, MEMORY_SIZE};
use super::karaoke_tables::KARAOKE_DELAY;

const PARAMETER_RANGES: XGParameterRanges = &[(1, 1, 127), (2, 0, 52), (3, 34, 60)];

// Input filters (b0, b1, a1), echo taps and the density allpasses (four coefficients and two addresses per line)
const LIST_HIGH_PASS: &[usize] = &[0x70, 0x6f, 0x71];
const LIST_LOW_PASS: &[usize] = &[0x73, 0x72, 0x74];
const LIST_DELAY: &[usize] = &[0x34, 0x2b, 0x2f];
const LIST_DENSITY_COEFFICIENT: &[usize] =
    &[0x79, 0x78, 0x77, 0x76, 0x7d, 0x7c, 0x7b, 0x7a, 0xb2, 0xb1, 0xb0, 0xaf, 0xa4, 0xa3, 0xa2, 0xa1];
const LIST_DENSITY_ADDRESS: &[usize] = &[0x28, 0x25, 0x29, 0x26, 0x3c, 0x37, 0x36, 0x33];

/// The converter index of the mono input level, which depends on the connection.
const MONO_INPUT: usize = 16;

const INIT_ORDER: &[usize] = &[0, 1, 2, 3, 9, 10, MONO_INPUT];
const CONNECTION_PARAMETERS: &[usize] = &[9, MONO_INPUT];

/// The S-YXG50 karaoke program.
/// Equivalent to: abstract class XGKaraokeBase
pub struct XGKaraokeBase {
    pub(crate) program: XGDSPProgram,
    pub(crate) var: XGVariationState,
    /// The auto wah LFO step (coefficient 0x7e), unused by the karaoke.
    pub(crate) wah_step: i32,
    // Input filter, echo feedback filter states
    filter_x: f64,
    filter_y: f64,
    low_y: f64,
    echo_x: f64,
    mono: Vec<f32>,
    dry_left: Vec<f32>,
    dry_right: Vec<f32>,
}

impl XGKaraokeBase {
    pub fn new(
        effect_type: u16,
        preset: &'static XGProgramPreset,
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
                variation_layout(INIT_ORDER, CONNECTION_PARAMETERS, PARAMETER_RANGES),
            ),
            var: XGVariationState::new(max_buffer_size, UNWRITTEN_CATEGORY_3),
            wah_step: 0,
            filter_x: 0.0,
            filter_y: 0.0,
            low_y: 0.0,
            echo_x: 0.0,
            mono: vec![0.0; max_buffer_size],
            dry_left: vec![0.0; max_buffer_size],
            dry_right: vec![0.0; max_buffer_size],
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
        let c = |i: usize| self.program.coefficients[i] as f64;
        let in_l = &self.var.input_left;
        let in_r = &self.var.input_right;
        let mono = &mut self.mono;
        let dry_l = &mut self.dry_left;
        let dry_r = &mut self.dry_right;
        // Dry copies, mono sum and the input filters (high pass, low pass)
        let (left_gain, right_gain, dry_left_gain, dry_right_gain) = (c(0x6d), c(0x6e), c(0xb9), c(0xac));
        let (hb1, hb0, ha1) = (c(0x6f), c(0x70), c(0x71));
        let (lb1, lb0, la1) = (c(0x72), c(0x73), c(0x74));
        let (mut filter_x, mut filter_y, mut low_y, mut echo_x) =
            (self.filter_x, self.filter_y, self.low_y, self.echo_x);
        let mut pointer = self.program.pointer;
        for i in 0..sample_count {
            let l = in_l[i] as f64;
            let r = in_r[i] as f64;
            dry_l[i] = (dry_left_gain * l) as f32;
            dry_r[i] = (dry_right_gain * r) as f32;
            let x = left_gain * l + right_gain * r;
            let y = ha1 * filter_y + hb1 * filter_x + hb0 * x;
            low_y = lb1 * filter_y + la1 * low_y + y * lb0;
            filter_x = x;
            filter_y = y;
            mono[i] = low_y as f32;
        }

        let mask = self.program.mask;
        let t = &self.program.taps;
        macro_rules! at {
            ($tap:expr) => {
                ((t[$tap] + pointer) & mask) as usize
            };
        }
        let out_l = c(0xbb) * output_gain;
        let out_r = c(0xae) * output_gain;
        for i in 0..sample_count {
            let x = mono[i] as f64;
            // Two diffusion allpasses
            let k25 = at!(0x25);
            memory[at!(0x28)] = (c(0x77) * x + memory[k25] as f64 * c(0x76)) as f32;
            let k26 = at!(0x26);
            let diffused = c(0x79) * x + memory[k25] as f64 * c(0x78);
            memory[at!(0x29)] = (diffused * c(0x7b) + memory[k26] as f64 * c(0x7a)) as f32;
            // Echo with averaged feedback
            let k34 = at!(0x34);
            memory[at!(0x38)] = (c(0xa5) * echo_x
                + (diffused * c(0x7d) + memory[k26] as f64 * c(0x7c)) * c(0xa7)
                + memory[k34] as f64 * c(0xa6)) as f32;
            echo_x = memory[k34] as f64;
            // Output allpasses
            let left = memory[at!(0x2b)] as f64 * c(0x8f);
            let k37 = at!(0x37);
            memory[at!(0x3c)] = (left * c(0xb0) + memory[k37] as f64 * c(0xaf)) as f32;
            let wet_l = left * c(0xb2) + memory[k37] as f64 * c(0xb1);
            let right = memory[at!(0x2f)] as f64 * c(0x9c);
            let k33 = at!(0x33);
            memory[at!(0x36)] = (right * c(0xa2) + memory[k33] as f64 * c(0xa1)) as f32;
            let wet_r = right * c(0xa4) + memory[k33] as f64 * c(0xa3);
            output_left[i] = (out_l * wet_l + dry_l[i] as f64 * output_gain) as f32;
            output_right[i] = (out_r * wet_r + dry_r[i] as f64 * output_gain) as f32;
            pointer = (pointer - 1) & mask;
        }
        self.filter_x = filter_x;
        self.filter_y = filter_y;
        self.low_y = low_y;
        self.echo_x = echo_x;
        self.program.pointer = pointer;
    }

    fn reset_state_impl(&mut self) {
        self.filter_x = 0.0;
        self.filter_y = 0.0;
        self.low_y = 0.0;
        self.echo_x = 0.0;
    }

    fn convert_impl(&mut self, param: usize) {
        let p = |d: &Self, i: usize| d.program.param(i);
        match param {
            0 => {
                let pr = &mut self.program;
                let delay = ((pr.to_address100us(KARAOKE_DELAY[pr.params[0] as usize] as i32) << 16) >> 16)
                    + pr.signed_address(0x3e);
                let word = (delay & 0xff_ff) as u16;
                pr.address_words[0x2f] = word;
                pr.address_words[0x2b] = word;
                pr.address_words[0x34] = word;
                pr.push_addresses_all(LIST_DELAY);
            }
            1 => {
                // Both feedback taps take half of the level (64 is none)
                let word = ((((p(self, 1) - 0x40) * 125) >> 5) & 0x3_ff) as u16;
                self.program.coefficient_words[0xa5] = word;
                self.program.coefficient_words[0xa6] = word;
                self.push_coefficients_all(&[0xa6, 0xa5]);
            }
            2 => self.design_first_order(XGFilterShape::HighPass, p(self, 2), 0x40, LIST_HIGH_PASS, 0),
            3 => self.design_first_order(XGFilterShape::LowPass, p(self, 3), 0x40, LIST_LOW_PASS, 0),
            9 => {
                let mix = self.program.mix_index();
                let w = &mut self.program.coefficient_words;
                w[0xac] = (MIX_DRY[mix] as i32 & 0x3_ff) as u16;
                w[0xb9] = w[0xac];
                w[0xae] = (MIX_WET[mix] as i32 & 0x3_ff) as u16;
                w[0xbb] = w[0xae];
                self.push_coefficients_all(&[0xb9]);
                self.push_coefficients_all(&[0xbb]);
                self.push_coefficients_all(&[0xae, 0xac]);
            }
            10 => {
                let density = p(self, 10);
                self.program.density_allpasses(
                    density,
                    10,
                    1,
                    0,
                    LIST_DENSITY_COEFFICIENT,
                    LIST_DENSITY_ADDRESS,
                );
                self.push_coefficients_all(LIST_DENSITY_COEFFICIENT);
            }
            MONO_INPUT => {
                // L+R at 0.707 in the system mode, half of that in the insertion mode
                let word = if self.program.insertion { 0xb5 } else { 0x1_6a };
                self.program.coefficient_words[0x6d] = word;
                self.program.coefficient_words[0x6e] = word;
                self.push_coefficients_all(&[0x6d, 0x6e]);
            }
            _ => {}
        }
    }
}

/// Equivalent to: XGKaraokeBase.clampParameter
fn clamp_parameter(d: &XGKaraokeBase, param: usize, value: i32) -> i32 {
    // The density is fixed per type
    if param == 10 {
        return d.program.param(10);
    }
    d.program.clamp_parameter_base(param, value)
}

crate::impl_xg_filter_variation_program!(XGKaraokeBase, clamp = clamp_parameter);
