//! The S-YXG50 reverb program, shared by all reverb block types.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/reverb/xg_reverb.ts
//!
//! The TS `XGRoomReverb` subclass (white room, tunnel, basement) is the same struct with
//! `XGReverbPreset::room` set; its `convert` override is `XGReverb::convert_room`.

use super::super::xg_dsp_program::{
    fround, to_signed16, to_u16, XGDSPProgram, XGFilterShape, XGParameterRanges, XGProgram,
    XGProgramLayout, XGProgramPreset, RETURN_GAIN, STEREO_SEND_LEVEL, VARIATION_INPUT_LEVEL,
};
use super::super::xg_dsp_tables::{DELAY_TABLE, DENSITY_TABLE, MIX_DRY, MIX_WET};
use super::reverb_tables::{
    DAMP_FEEDBACK, DAMP_FILTER, DIFFUSION_SCALE, DIFFUSION_STEP, DIFFUSION_TABLE, ROOM_GAIN,
    ROOM_SIZE, ROOM_STEP, ROOM_TIME_OFFSET, TIME_OFFSETS, TIME_TABLE,
};

/// The type-specific data of an XG reverb.
/// Equivalent to: interface XGReverbPreset (+ XGRoomReverbPreset when `room` is set)
#[derive(Debug)]
pub struct XGReverbPreset {
    pub base: XGProgramPreset,
    /// The type gain index, selecting the diffusion, density and damping table rows.
    pub gain: usize,
    /// The early reflection level scale (/256).
    pub early_level: i32,
    /// The HPF gain offset from unity (in filter gain table steps).
    pub hpf_gain: i32,
    /// The room-shaped reverb data (XGRoomReverbPreset), None for the plain reverbs.
    pub room: Option<XGRoomReverbPreset>,
}

/// The room dimension a room-shaped reverb's wall vary applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XGRoomWall {
    Width,
    Height,
    Depth,
}

/// The type-specific data of a room-shaped XG reverb.
/// Equivalent to: interface XGRoomReverbPreset (without the XGReverbPreset fields)
#[derive(Debug, Clone, Copy)]
pub struct XGRoomReverbPreset {
    /// Selects the room damping interpolation row.
    pub room_select: usize,
    /// Selects the room diffusion steps.
    pub room_step: usize,
    /// The dimension the wall vary parameter is added to.
    pub wall: XGRoomWall,
}

// Program layout: which coefficient and address indices the preset words fill
const COEFFICIENT_LIST: &[usize] = &[
    0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
    0x2f, 0x30, 0x31, 0x32, 0x33, 0x38, 0x39, 0x3a, 0x3b, 0x45, 0x46, 0x47, 0x48, 0x51, 0x52, 0x53,
    0x54, 0x59, 0x5a, 0x5b, 0x5c, 0x5d, 0x5e, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67,
];
const ADDRESS_LIST: &[usize] = &[
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x0b, 0x0c, 0x0d, 0x10, 0x11, 0x12, 0x15,
    0x17, 0x18, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x3f,
];

// Converter push lists
const LIST_HPF: &[usize] = &[0x2e, 0x2d, 0x2f];
const LIST_LPF: &[usize] = &[0x31, 0x30, 0x32];
/// Diffusion allpass addresses, 4 per line: base, density tap, write, read.
const LIST_DIFFUSION: &[usize] = &[
    0x1f, 0x0c, 0x00, 0x04, 0x21, 0x1c, 0x01, 0x05, 0x22, 0x1e, 0x02, 0x06, 0x23, 0x20, 0x03, 0x07,
];
/// Damping line coefficients, 3 per line: feedback, filter, filter.
const LIST_TIME: &[usize] = &[0x5b, 0x5a, 0x59, 0x5e, 0x5d, 0x5c, 0x64, 0x63, 0x62, 0x67, 0x66, 0x65];
const LIST_DENSITY_ADDRESS: &[usize] = &[0x10, 0x0b, 0x15, 0x0d, 0x18, 0x12, 0x1d, 0x17];
const LIST_DENSITY_COEFFICIENT: &[usize] = &[
    0x2c, 0x2b, 0x2a, 0x29, 0x3b, 0x3a, 0x39, 0x38, 0x48, 0x47, 0x46, 0x45, 0x54, 0x53, 0x52, 0x51,
];
const LIST_DELAY: &[usize] = &[0x08, 0x3f];
const LIST_BALANCE: &[usize] = &[0x1b, 0x20, 0x1c, 0x1d, 0x1e, 0x1f, 0x21, 0x22, 0x23, 0x24];
const LIST_FEEDBACK: &[usize] = &[0x33];
/// The converters run when a type is loaded.
const INIT_ORDER: &[usize] = &[0, 1, 2, 3, 4, 11, 12, 14];

/// Valid parameter ranges (min, max), the rest is 0-127.
const PARAMETER_RANGES: XGParameterRanges = &[
    (0, 0, 69),
    (1, 0, 10),
    (2, 0, 63),
    (3, 0, 52),
    (4, 34, 60),
    (5, 0, 37),
    (6, 0, 73),
    (7, 0, 104),
    (8, 0, 30),
    (10, 0, 63),
];

/// The dry/wet levels depend on the connection (only in the variation).
const CONNECTION_PARAMETERS: &[usize] = &[9];

const LAYOUT: XGProgramLayout = XGProgramLayout {
    coefficient_list: COEFFICIENT_LIST,
    address_list: ADDRESS_LIST,
    init_order: INIT_ORDER,
    connection_parameters: CONNECTION_PARAMETERS,
    ranges: PARAMETER_RANGES,
};

/// Pan table value at center, part of the original output coefficients.
/// Divided out because the reverb block applies its own pan.
const CENTER_PAN: f64 = 362.0;
/// Reverb return at its default, part of the original output coefficients.
/// The block scales by return / 64 on top, giving return / 127 like the original.
/// Equivalent to: Math.fround(64 / 127)
const DEFAULT_RETURN: f64 = (64.0_f64 / 127.0) as f32 as f64;

/// The reverb input level (the original send prescale).
const INPUT_LEVEL: f64 = 2.0;
const OUTPUT_GAIN: f64 = 512.0 / CENTER_PAN;

/// The S-YXG50 reverb program, shared by all reverb block types.
///
/// Flow: mono input -> HPF -> LPF -> input node with feedback -> two diffusion allpasses
/// -> four damped delay lines -> per-side tap sums with early reflections -> output allpasses.
pub struct XGReverb {
    program: XGDSPProgram,
    /// True when running in the variation or insertion block.
    variation: bool,
    preset: &'static XGReverbPreset,

    // Input filter state
    x1: f64,
    y1: f64,
    y2: f64,
    // Damping line filter states
    z0: f64,
    z1: f64,
    z2: f64,
    z3: f64,
    // Variation dry/wet
    dry_gain: f64,
    wet_gain: f64,
}

impl XGReverb {
    /// The largest address word any reverb program can reach, plus one.
    pub const MEMORY_SIZE: usize = 27_187;

    pub fn new(
        effect_type: u16,
        preset: &'static XGReverbPreset,
        sample_rate: f64,
        max_buffer_size: usize,
        variation: bool,
    ) -> Self {
        Self {
            program: XGDSPProgram::new(
                effect_type,
                Self::MEMORY_SIZE,
                sample_rate,
                max_buffer_size,
                0,
                &preset.base,
                LAYOUT,
            ),
            variation,
            preset,
            x1: 0.0,
            y1: 0.0,
            y2: 0.0,
            z0: 0.0,
            z1: 0.0,
            z2: 0.0,
            z3: 0.0,
            dry_gain: 0.0,
            wet_gain: 1.0,
        }
    }

    /// Equivalent to: XGReverb.convert
    fn convert_reverb(&mut self, param: usize) {
        match param {
            0 => {
                self.reverb_time();
                self.push_coefficients_all(LIST_TIME);
            }

            1 => {
                self.diffusion();
                for line in 0..4 {
                    self.program.push_addresses(LIST_DIFFUSION, 2, line * 4 + 2);
                }
                self.density();
                // The variation keeps the plain density output coefficients
                if !self.variation {
                    self.output();
                }
                self.program.push_addresses(LIST_DENSITY_ADDRESS, 1, 7);
            }

            2 | 10 => {
                self.delays();
            }

            3 => {
                // The per-type HPF gain only applies in the reverb block
                let gain = if self.variation { 0x40 } else { 0x40 + self.preset.hpf_gain };
                let frequency = self.program.param(3);
                self.design_first_order(XGFilterShape::HighPass, frequency, gain, LIST_HPF, 0);
            }

            9 => {
                // Dry/wet, only in the variation (the system mode is always fully wet)
                if !self.variation {
                    return;
                }
                let mix = self.program.mix_index();
                self.dry_gain = MIX_DRY[mix] as f64 / 512.0;
                self.wet_gain = MIX_WET[mix] as f64 / 512.0;
            }

            4 => {
                let frequency = self.program.param(4);
                self.design_first_order(XGFilterShape::LowPass, frequency, 0x40, LIST_LPF, 0);
            }

            11 => {
                self.density();
                if !self.variation {
                    self.output();
                }
                self.push_coefficients_all(LIST_DENSITY_COEFFICIENT);
            }

            12 => {
                self.balance();
            }

            14 => {
                self.program.coefficient_words[0x33] =
                    self.program.feedback_word(self.program.param(14));
                self.push_coefficients_all(LIST_FEEDBACK);
            }

            _ => {}
        }
    }

    /// Writes the damping coefficients of all four lines from the reverb time.
    fn reverb_time(&mut self) {
        let base = TIME_TABLE[self.program.param(0) as usize] as i32;
        let gain = self.preset.gain;
        for line in 0..4 {
            let index = ((TIME_OFFSETS[gain * 4 + line] as i32 + base) & 0xff) as usize;
            self.write_damping(index, LIST_TIME, line * 3);
        }
    }

    /// Writes one damping line's coefficient words.
    fn write_damping(&mut self, index: usize, list: &[usize], offset: usize) {
        let w = &mut self.program.coefficient_words;
        w[list[offset]] = DAMP_FEEDBACK[index];
        let filter = if index > 0xca { 0xff } else { DAMP_FILTER[index] };
        w[list[offset + 1]] = filter;
        w[list[offset + 2]] = filter;
    }

    /// Places the diffusion allpasses from the diffusion parameter.
    fn diffusion(&mut self) {
        let scale = DIFFUSION_SCALE[self.program.param(1) as usize] as i32;
        let row = self.preset.gain * 8;
        let p = &mut self.program;
        for line in 0..4 {
            let s3 = to_signed16(p.to_address10us(DIFFUSION_TABLE[row + line * 2] as i32));
            let s4 = to_signed16(p.to_address10us(DIFFUSION_TABLE[row + line * 2 + 1] as i32));
            let step = (DIFFUSION_STEP[line] as i32 * scale) >> 7;
            let g = line * 4;
            let mut base = p.signed_address(LIST_DIFFUSION[g]);
            p.address_words[LIST_DIFFUSION[g + 1]] = to_u16(base + s4);
            if (line & 1) == 0 {
                p.address_words[LIST_DIFFUSION[g + 2]] = to_u16(base + s3);
                base += step;
            } else {
                p.address_words[LIST_DIFFUSION[g + 2]] = to_u16(base + step + s3);
            }
            p.address_words[LIST_DIFFUSION[g + 3]] = to_u16(base + s3);
        }
    }

    /// Writes the density allpass lengths and coefficients.
    fn density(&mut self) {
        let density = self.program.param(11);
        let diffusion = self.program.param(1);
        self.program.density_allpasses(
            density,
            diffusion,
            self.preset.gain,
            0xc,
            LIST_DENSITY_COEFFICIENT,
            LIST_DENSITY_ADDRESS,
        );
    }

    /// Writes the output allpass coefficients (with the default return and center pan).
    fn output(&mut self) {
        let (a, b, c, d);
        if self.program.param(11) == 0 {
            a = 0.5;
            b = 0.5;
            c = 0.0;
            d = 0.0;
        } else {
            let row = self.preset.gain * 8;
            a = fround(-(DENSITY_TABLE[row + 5] as f64 / 1024.0));
            b = fround(-(DENSITY_TABLE[row + 7] as f64 / 1024.0));
            c = fround(0.5 - a);
            d = fround(0.5 - b);
        }
        let level = fround(CENTER_PAN * DEFAULT_RETURN);
        let word = |x: f64| ((fround(level * x).trunc() as i32) & 0x3_ff) as u16;
        let w = &mut self.program.coefficient_words;
        w[0x48] = word(a);
        w[0x54] = word(b);
        w[0x47] = word(c);
        w[0x53] = word(d);
    }

    /// Places the early reflection and reverb start from the initial and reverb delays.
    fn delays(&mut self) {
        let p = &mut self.program;
        let early = p.to_address100us(DELAY_TABLE[p.param(2) as usize] as i32) + p.signed_address(0x11);
        p.address_words[0x3f] = to_u16(early);
        p.address_words[0x08] = to_u16(
            p.to_address100us(DELAY_TABLE[p.param(10) as usize] as i32) + to_signed16(early),
        );
        p.push_addresses_all(LIST_DELAY);
    }

    /// Writes the early reflection and reverb levels from the ER/Rev balance.
    fn balance(&mut self) {
        let p = self.program.param(12) as usize;
        let early = (MIX_DRY[p] as i32 * self.preset.early_level) >> 8;
        let reverb = MIX_WET[p] as i32;
        let w = &mut self.program.coefficient_words;
        w[0x1b] = (early & 0xff_ff) as u16;
        w[0x20] = (early & 0xff_ff) as u16;
        for k in 0x1c..=0x24 {
            if k != 0x20 {
                w[k] = (reverb & 0xff_ff) as u16;
            }
        }
        self.push_coefficients_all(LIST_BALANCE);
    }

    // ---- XGRoomReverb ----

    fn room(&self) -> XGRoomReverbPreset {
        self.preset.room.expect("room reverb preset")
    }

    /// Equivalent to: XGRoomReverb.convert
    fn convert_room(&mut self, param: usize) {
        let room = self.room();
        let p5 = self.program.param(5);
        let p6 = self.program.param(6);
        let p7 = self.program.param(7);
        match param {
            0 => {
                self.room_time(0, p5, 0);
                self.room_time(1, p6, 3);
                self.room_time(2, p7, 6);
                let (dimension, size) = (self.wall_dimension(), self.wall_size());
                self.room_time(dimension, size, 9);
                self.push_coefficients_all(LIST_TIME);
            }

            1 => {
                self.room_diffusion(0, p5, 0);
                self.room_diffusion(ROOM_STEP[room.room_step] as usize, p6, 4);
                self.room_diffusion(ROOM_STEP[room.room_step + 1] as usize, p7, 8);
                let size = self.wall_size();
                self.room_diffusion(ROOM_STEP[room.room_step + 2] as usize, size, 12);
                for line in 0..4 {
                    self.program.push_addresses(LIST_DIFFUSION, 2, line * 4 + 2);
                }
                self.density();
                self.program.push_addresses(LIST_DENSITY_ADDRESS, 1, 7);
            }

            3 => {
                let frequency = self.program.param(3);
                self.design_first_order(XGFilterShape::HighPass, frequency, 0x40, LIST_HPF, 0);
            }

            5 => {
                self.room_dimension(0, 0, p5, 0);
            }

            6 => {
                self.room_dimension(1, ROOM_STEP[room.room_step] as usize, p6, 1);
                if room.wall == XGRoomWall::Height {
                    self.wall();
                }
            }

            7 => {
                self.room_dimension(2, ROOM_STEP[room.room_step + 1] as usize, p7, 2);
                if room.wall == XGRoomWall::Depth {
                    self.wall();
                }
            }

            8 => {
                self.wall();
            }

            _ => {
                self.convert_reverb(param);
            }
        }
    }

    /// The room damping gain for a dimension (0 width, 1 height, 2 depth).
    fn room_gain(&self, dimension: usize) -> i32 {
        ROOM_GAIN[self.room().room_select * 6 + dimension] as i32
    }

    /// The dimension index the wall vary applies to.
    fn wall_dimension(&self) -> usize {
        match self.room().wall {
            XGRoomWall::Width => 0,
            XGRoomWall::Height => 1,
            XGRoomWall::Depth => 2,
        }
    }

    /// The size of the wall line (its dimension plus the wall vary).
    fn wall_size(&self) -> i32 {
        self.program.param(5 + self.wall_dimension()) + self.program.param(8)
    }

    /// Writes a damping line from the reverb time and a room dimension.
    fn room_time(&mut self, dimension: usize, size: i32, offset: usize) {
        let time = self.program.param(0) as usize;
        let gain = self.room_gain(dimension);
        let time_offset = ROOM_TIME_OFFSET[size as usize] as i32;
        let extra = if time_offset < 0x38 { 0 } else { time_offset - 0x38 };
        let t = |i: usize| TIME_TABLE[i] as i32;
        let base = if gain < 0 {
            if time == 0 {
                0
            } else {
                // Math.trunc(((a - b) * gain) / 10): integer division truncates like Math.trunc
                ((((t(time) - t(time - 1)) * gain) / 10) + t(time)) & 0xff
            }
        } else {
            ((((t(time + 1) - t(time)) * gain) / 10) + t(time)) & 0xff
        };
        self.write_damping(((extra + base) & 0xff) as usize, LIST_TIME, offset);
    }

    /// Places a diffusion allpass from a room dimension.
    fn room_diffusion(&mut self, step: usize, size: i32, offset: usize) {
        let scale = DIFFUSION_SCALE[self.program.param(1) as usize] as i32;
        let p = &mut self.program;
        let length = p.to_address10us(ROOM_SIZE[size as usize] as i32) & 0xff_ff;
        let base = p.signed_address(LIST_DIFFUSION[offset]);
        let half = length >> 1;
        p.address_words[LIST_DIFFUSION[offset + 1]] = to_u16(length + base);
        let shift = (DIFFUSION_STEP[step] as i32 * scale) >> 7;
        if (step & 1) == 0 {
            p.address_words[LIST_DIFFUSION[offset + 2]] = to_u16(half + base);
            p.address_words[LIST_DIFFUSION[offset + 3]] = to_u16(base + shift + half);
        } else {
            p.address_words[LIST_DIFFUSION[offset + 2]] = to_u16(shift + base + half);
            p.address_words[LIST_DIFFUSION[offset + 3]] = to_u16(base + half);
        }
    }

    /// Updates one dimension's damping line and allpass, then pushes them.
    fn room_dimension(&mut self, dimension: usize, step: usize, size: i32, line: usize) {
        self.room_time(dimension, size, line * 3);
        self.room_diffusion(step, size, line * 4);
        self.program.push_addresses(LIST_DIFFUSION, 3, line * 4 + 1);
        self.push_coefficients(LIST_TIME, 3, line * 3);
    }

    /// Updates the wall line (the fourth line).
    fn wall(&mut self) {
        let size = self.wall_size();
        let dimension = self.wall_dimension();
        self.room_time(dimension, size, 9);
        self.room_diffusion(ROOM_STEP[self.room().room_step + 2] as usize, size, 12);
        self.program.push_addresses(LIST_DIFFUSION, 3, 13);
        self.push_coefficients(LIST_TIME, 3, 9);
    }
}

impl XGProgram for XGReverb {
    fn program(&self) -> &XGDSPProgram {
        &self.program
    }

    fn program_mut(&mut self) -> &mut XGDSPProgram {
        &mut self.program
    }

    fn convert(&mut self, param: usize) {
        if self.preset.room.is_some() {
            self.convert_room(param);
        } else {
            self.convert_reverb(param);
        }
    }

    fn reset_state(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
        self.z0 = 0.0;
        self.z1 = 0.0;
        self.z2 = 0.0;
        self.z3 = 0.0;
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

        // Input filters
        let (hb1, hb0, ha1) = (c(0x2d), c(0x2e), c(0x2f));
        let (lb1, lb0, la1) = (c(0x30), c(0x31), c(0x32));
        // Input node and diffusion
        let feedback = c(0x33);
        let (d1a, d1b, d1c, d1d) = (c(0x29), c(0x2a), c(0x2b), c(0x2c));
        let (d2a, d2b, d2c, d2d) = (c(0x38), c(0x39), c(0x3a), c(0x3b));
        // Damping lines: feedback-filter, input, read
        let (k0z, k0r, k0i) = (c(0x59), c(0x5a), c(0x5b));
        let (k1z, k1r, k1i) = (c(0x5c), c(0x5d), c(0x5e));
        let (k2z, k2r, k2i) = (c(0x62), c(0x63), c(0x64));
        let (k3z, k3r, k3i) = (c(0x65), c(0x66), c(0x67));
        // Outputs
        let (er_l, rev_l, o_la, o_lb, o_lc, o_ld) =
            (c(0x1b), c(0x1c), c(0x45), c(0x46), c(0x47), c(0x48));
        let (er_r, rev_r, o_ra, o_rb, o_rc, o_rd) =
            (c(0x20), c(0x21), c(0x51), c(0x52), c(0x53), c(0x54));

        let (t_node, t_node_read, t_d1_write, t_d1_read, t_d2_write, t_d2_read) =
            (t[0x11], t[0x08], t[0x10], t[0x0b], t[0x15], t[0x0d]);
        let (t_l0_read, t_l0_write) = (t[0x0c], t[0x1f]);
        let (t_l1_read, t_l1_write) = (t[0x1c], t[0x21]);
        let (t_l2_read, t_l2_write) = (t[0x1e], t[0x22]);
        let (t_l3_read, t_l3_write) = (t[0x20], t[0x23]);
        let t_early = t[0x3f];
        let (t_l0, t_l1, t_l2, t_l3) = (t[0x00], t[0x01], t[0x02], t[0x03]);
        let (t_r0, t_r1, t_r2, t_r3) = (t[0x04], t[0x05], t[0x06], t[0x07]);
        let (t_ol_write, t_ol_read, t_or_write, t_or_read) = (t[0x18], t[0x12], t[0x1d], t[0x17]);

        // The reverb input is mono. The reverb block sums our panned stereo send,
        // The variation sums its (halved) stereo send, or half of the part in the insertion mode
        let insertion = self.variation && is_insertion;
        let input_level = self.program.input_level;
        let input_gain = input_level
            * (if self.variation {
                if insertion { 0.5 } else { STEREO_SEND_LEVEL }
            } else {
                std::f64::consts::FRAC_1_SQRT_2
            });
        let dry_gain = self.dry_gain * input_level;
        let wet_gain = self.wet_gain
            * (if self.variation {
                if insertion { 1.0 } else { RETURN_GAIN }
            } else {
                OUTPUT_GAIN
            });
        let mut pointer = self.program.pointer;
        let (mut x1, mut y1, mut y2) = (self.x1, self.y1, self.y2);
        let (mut z0, mut z1, mut z2, mut z3) = (self.z0, self.z1, self.z2, self.z3);

        macro_rules! at {
            ($tap:expr) => {
                ((($tap) + pointer) & mask) as usize
            };
        }
        macro_rules! m {
            ($index:expr) => {
                memory[$index] as f64
            };
        }

        for i in 0..sample_count {
            // HPF then LPF
            let x = (input_left[i] as f64 + input_right[i] as f64) * input_gain;
            let h = hb1 * x1 + hb0 * x + ha1 * y1;
            let l = lb1 * y1 + la1 * y2 + h * lb0;
            x1 = x;
            y1 = h;
            y2 = l;

            // Input node with feedback, then two diffusion allpasses.
            // Memory is re-read after every write, as some presets alias taps.
            let i_node_read = at!(t_node_read);
            let i_d1_read = at!(t_d1_read);
            memory[at!(t_node)] = (m!(i_node_read) * feedback + l) as f32;
            memory[at!(t_d1_write)] = (m!(i_node_read) * d1b + m!(i_d1_read) * d1a) as f32;
            let mut f = m!(i_node_read) * d1d + m!(i_d1_read) * d1c;
            let i_d2_read = at!(t_d2_read);
            memory[at!(t_d2_write)] = (f * d2b + m!(i_d2_read) * d2a) as f32;
            f = f * d2d + m!(i_d2_read) * d2c;

            // Four damped delay lines
            let mut i_read = at!(t_l0_read);
            memory[at!(t_l0_write)] = (k0z * z0 + f * k0i + m!(i_read) * k0r) as f32;
            z0 = m!(i_read);
            i_read = at!(t_l1_read);
            memory[at!(t_l1_write)] = (k1z * z1 + f * k1i + m!(i_read) * k1r) as f32;
            z1 = m!(i_read);
            i_read = at!(t_l2_read);
            memory[at!(t_l2_write)] = (k2z * z2 + f * k2i + m!(i_read) * k2r) as f32;
            z2 = m!(i_read);
            i_read = at!(t_l3_read);
            memory[at!(t_l3_write)] = (k3z * z3 + f * k3i + m!(i_read) * k3r) as f32;
            z3 = m!(i_read);

            // Outputs: early reflection plus four line taps, then an allpass
            let i_early = at!(t_early);
            let mut sum = m!(i_early) * er_l
                + (m!(at!(t_l3)) + m!(at!(t_l2)) + m!(at!(t_l1)) + m!(at!(t_l0))) * rev_l;
            let mut i_allpass = at!(t_ol_read);
            memory[at!(t_ol_write)] = (sum * o_lb + m!(i_allpass) * o_la) as f32;
            output_left[i] = ((sum * o_ld + m!(i_allpass) * o_lc) * wet_gain
                + input_left[i] as f64 * dry_gain) as f32;

            sum = m!(i_early) * er_r
                + (m!(at!(t_r2)) + m!(at!(t_r1)) + m!(at!(t_r0)) + m!(at!(t_r3))) * rev_r;
            i_allpass = at!(t_or_read);
            memory[at!(t_or_write)] = (sum * o_rb + m!(i_allpass) * o_ra) as f32;
            output_right[i] = ((sum * o_rd + m!(i_allpass) * o_rc) * wet_gain
                + input_right[i] as f64 * dry_gain) as f32;

            pointer = (pointer - 1) & mask;
        }
        self.program.pointer = pointer;
        self.x1 = x1;
        self.y1 = y1;
        self.y2 = y2;
        self.z0 = z0;
        self.z1 = z1;
        self.z2 = z2;
        self.z3 = z3;
        let target = if self.variation {
            if is_insertion { 1.0 } else { VARIATION_INPUT_LEVEL }
        } else {
            INPUT_LEVEL
        };
        self.program.advance_fade(sample_count, target);
    }
}
