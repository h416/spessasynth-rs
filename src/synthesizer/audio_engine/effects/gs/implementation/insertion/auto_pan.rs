//! auto_pan.rs
//! purpose: Automatic panning insertion effect.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/insertion/auto_pan.ts

use super::utils::{BiquadCoeffs, BiquadState, apply_shelves, compute_shelf_coeffs};
use super::convert::InsertionValueConverter;
use super::insertion_processor::DefaultGSInsertionProcessor;

use std::f64::consts::PI;

const PI_2: f64 = PI * 2.0;
const GAIN_LVL: f64 = 0.935;
const LEVEL_EXP: f64 = 2.0;
const PAN_SMOOTHING: f64 = 0.01;
const DEFAULT_LEVEL: f64 = 127.0;

pub struct AutoPanFx {
    sample_rate: f64,
    mod_wave: u8,
    mod_rate: f64,
    mod_depth: f64,
    low_gain: f64,
    hi_gain: f64,
    level: f64,
    current_pan: f64,
    phase: f64,
    ls_coeffs: BiquadCoeffs,
    hs_coeffs: BiquadCoeffs,
    ls_state_l: BiquadState,
    ls_state_r: BiquadState,
    hs_state_l: BiquadState,
    hs_state_r: BiquadState,
}

impl AutoPanFx {
    pub fn new(sample_rate: f64) -> Self {
        let mut fx = Self {
            sample_rate,
            mod_wave: 1,
            mod_rate: 3.05,
            mod_depth: 96.0,
            low_gain: 0.0,
            hi_gain: 0.0,
            level: DEFAULT_LEVEL / 127.0,
            current_pan: 0.0,
            phase: 0.0,
            ls_coeffs: BiquadCoeffs::default(),
            hs_coeffs: BiquadCoeffs::default(),
            ls_state_l: BiquadState::default(),
            ls_state_r: BiquadState::default(),
            hs_state_l: BiquadState::default(),
            hs_state_r: BiquadState::default(),
        };
        fx.update_shelves();
        fx
    }

    fn update_shelves(&mut self) {
        compute_shelf_coeffs(&mut self.ls_coeffs, self.low_gain, 200.0, self.sample_rate, true);
        compute_shelf_coeffs(&mut self.hs_coeffs, self.hi_gain, 4000.0, self.sample_rate, false);
    }
}

impl DefaultGSInsertionProcessor for AutoPanFx {
    fn effect_type(&self) -> u16 { 0x0126 }

    fn reset(&mut self) {
        self.mod_wave = 1;
        self.mod_rate = 3.05;
        self.mod_depth = 96.0;
        self.low_gain = 0.0;
        self.hi_gain = 0.0;
        self.level = DEFAULT_LEVEL / 127.0;
        self.current_pan = 0.0;
        self.phase = 0.0;
        self.ls_state_l.reset();
        self.ls_state_r.reset();
        self.hs_state_l.reset();
        self.hs_state_r.reset();
        self.update_shelves();
    }

    fn set_parameter(&mut self, parameter: u8, value: u8) {
        match parameter {
            0 => { self.mod_wave = value; }
            1 => { self.mod_rate = InsertionValueConverter::rate1(value); }
            2 => { self.mod_depth = value as f64; }
            16 => { self.low_gain = value as f64 - 64.0; }
            17 => { self.hi_gain = value as f64 - 64.0; }
            19 => { self.level = value as f64 / 127.0; }
            _ => {}
        }
        self.update_shelves();
    }

    fn process(
        &mut self,
        input_l: &[f32], input_r: &[f32],
        output_l: &mut [f32], output_r: &mut [f32],
        sample_count: usize,
    ) {
        let level = self.level;
        let mod_wave = self.mod_wave;
        let depth = (self.mod_depth / 127.0).powf(LEVEL_EXP);
        let scale = (2.0 / (1.0 + depth)) * GAIN_LVL;
        let rate_inc = self.mod_rate / self.sample_rate;

        let mut phase = self.phase;
        let mut current_pan = self.current_pan;

        for i in 0..sample_count {
            let sl = apply_shelves(
                input_l[i] as f64,
                &self.ls_coeffs, &self.hs_coeffs,
                &mut self.ls_state_l, &mut self.hs_state_l,
            );
            let sr = apply_shelves(
                input_r[i] as f64,
                &self.ls_coeffs, &self.hs_coeffs,
                &mut self.ls_state_r, &mut self.hs_state_r,
            );

            let lfo = match mod_wave {
                1 => {
                    // Square (half-sine, SC-VA behavior)
                    if phase > 0.5 { -1.0 } else { -((phase - 0.75) * PI_2).cos() }
                }
                2 => (PI_2 * phase).sin(),          // Sine
                3 => 1.0 - 2.0 * phase,              // Saw1
                4 => 2.0 * phase - 1.0,              // Saw2
                _ => 1.0 - 4.0 * (phase - 0.5).abs(), // 0 -> Triangle (default)
            };
            phase += rate_inc;
            if phase >= 1.0 { phase -= 1.0; }
            current_pan += (lfo - current_pan) * PAN_SMOOTHING;
            let pan = current_pan * depth;
            let gain_l = (1.0 - pan) * 0.5 * scale;
            let gain_r = (1.0 + pan) * 0.5 * scale;

            let out_l = sl * level * gain_l;
            let out_r = sr * level * gain_r;

            output_l[i] = out_l as f32;
            output_r[i] = out_r as f32;
        }
        self.current_pan = current_pan;
        self.phase = phase;
    }
}
