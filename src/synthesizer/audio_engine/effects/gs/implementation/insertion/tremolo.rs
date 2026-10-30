//! tremolo.rs
//! purpose: Tremolo (volume modulation) insertion effect.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/insertion/tremolo.ts

use super::utils::{BiquadCoeffs, BiquadState, apply_shelves, compute_shelf_coeffs};
use super::convert::InsertionValueConverter;
use super::insertion_processor::DefaultGSInsertionProcessor;

use std::f64::consts::PI;

const PI_2: f64 = PI * 2.0;
const GAIN_SMOOTHING: f64 = 0.01;
const DEFAULT_LEVEL: f64 = 127.0;

pub struct TremoloFx {
    sample_rate: f64,
    mod_wave: u8,
    mod_rate: f64,
    mod_depth: f64,
    low_gain: f64,
    hi_gain: f64,
    level: f64,
    phase: f64,
    current_gain: f64,
    ls_coeffs: BiquadCoeffs,
    hs_coeffs: BiquadCoeffs,
    ls_state_l: BiquadState,
    ls_state_r: BiquadState,
    hs_state_l: BiquadState,
    hs_state_r: BiquadState,
}

impl TremoloFx {
    pub fn new(sample_rate: f64) -> Self {
        let mut fx = Self {
            sample_rate,
            mod_wave: 1,
            mod_rate: 3.05,
            mod_depth: 96.0,
            low_gain: 0.0,
            hi_gain: 0.0,
            level: DEFAULT_LEVEL / 127.0,
            phase: 0.0,
            current_gain: 1.0,
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

impl DefaultGSInsertionProcessor for TremoloFx {
    fn effect_type(&self) -> u16 { 0x0125 }

    fn reset(&mut self) {
        self.mod_wave = 1;
        self.mod_rate = 3.05;
        self.mod_depth = 96.0;
        self.low_gain = 0.0;
        self.hi_gain = 0.0;
        self.level = DEFAULT_LEVEL / 127.0;
        self.phase = 0.0;
        self.current_gain = 1.0;
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
        let mod_depth = self.mod_depth;
        let mod_wave = self.mod_wave;
        let rate_inc = self.mod_rate / self.sample_rate;

        let mut phase = self.phase;
        let mut current_gain = self.current_gain;

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
                    // Square (half-sine)
                    if phase > 0.5 { -1.0 } else { -((phase - 0.75) * PI_2).cos() }
                }
                2 => (PI_2 * phase).sin(),          // Sine
                3 => 1.0 - 2.0 * phase,              // Saw1
                4 => 2.0 * phase - 1.0,              // Saw2
                _ => 1.0 - 4.0 * (phase - 0.5).abs(), // 0 -> Triangle (default)
            };
            phase += rate_inc;
            if phase >= 1.0 { phase -= 1.0; }

            let tremolo_level = 1.0 - (lfo / 2.0 + 0.5) * (mod_depth / 127.0);
            current_gain += (tremolo_level - current_gain) * GAIN_SMOOTHING;

            let out_l = sl * level * current_gain;
            let out_r = sr * level * current_gain;

            output_l[i] = out_l as f32;
            output_r[i] = out_r as f32;
        }
        self.phase = phase;
        self.current_gain = current_gain;
    }
}
