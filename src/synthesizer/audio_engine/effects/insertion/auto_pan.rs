/// auto_pan.rs
/// purpose: Automatic panning insertion effect.
/// Ported from: src/synthesizer/audio_engine/effects/insertion/auto_pan.ts

use super::utils::{BiquadCoeffs, BiquadState, apply_shelves, compute_shelf_coeffs};
use super::convert::InsertionValueConverter;
use super::InsertionProcessor;

use std::f64::consts::PI;

const PI_2: f64 = PI * 2.0;
const GAIN_LVL: f64 = 0.935;
const LEVEL_EXP: f64 = 2.0;
const PAN_SMOOTHING: f64 = 0.01;
const DEFAULT_LEVEL: f64 = 127.0;

pub struct AutoPanFx {
    send_level_to_reverb: f64,
    send_level_to_chorus: f64,
    send_level_to_delay: f64,
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
            send_level_to_reverb: 40.0 / 127.0,
            send_level_to_chorus: 0.0,
            send_level_to_delay: 0.0,
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

impl InsertionProcessor for AutoPanFx {
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
            0x03 => { self.mod_wave = value; }
            0x04 => { self.mod_rate = InsertionValueConverter::rate1(value); }
            0x05 => { self.mod_depth = value as f64; }
            0x13 => { self.low_gain = value as f64 - 64.0; }
            0x14 => { self.hi_gain = value as f64 - 64.0; }
            0x16 => { self.level = value as f64 / 127.0; }
            _ => {}
        }
        self.update_shelves();
    }

    fn process(
        &mut self,
        input_l: &[f32], input_r: &[f32],
        output_l: &mut [f32], output_r: &mut [f32],
        reverb_out: &mut [f32], chorus_out: &mut [f32], delay_out: &mut [f32],
        start_index: usize, sample_count: usize,
    ) {
        let level = self.level;
        let mod_wave = self.mod_wave;
        let depth = (self.mod_depth / 127.0).powf(LEVEL_EXP);
        let scale = (2.0 / (1.0 + depth)) * GAIN_LVL;
        let rate_inc = self.mod_rate / self.sample_rate;
        let rev = self.send_level_to_reverb;
        let chr = self.send_level_to_chorus;
        let dly = self.send_level_to_delay;

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

            let idx = start_index + i;
            output_l[idx] = (output_l[idx] as f64 + out_l) as f32;
            output_r[idx] = (output_r[idx] as f64 + out_r) as f32;
            let mono = (out_l + out_r) * 0.5;
            reverb_out[i] = (reverb_out[i] as f64 + mono * rev) as f32;
            chorus_out[i] = (chorus_out[i] as f64 + mono * chr) as f32;
            delay_out[i] = (delay_out[i] as f64 + mono * dly) as f32;
        }
        self.current_pan = current_pan;
        self.phase = phase;
    }

    fn send_level_to_reverb(&self) -> f64 { self.send_level_to_reverb }
    fn send_level_to_chorus(&self) -> f64 { self.send_level_to_chorus }
    fn send_level_to_delay(&self) -> f64 { self.send_level_to_delay }
    fn set_send_level_to_reverb(&mut self, v: f64) { self.send_level_to_reverb = v; }
    fn set_send_level_to_chorus(&mut self, v: f64) { self.send_level_to_chorus = v; }
    fn set_send_level_to_delay(&mut self, v: f64) { self.send_level_to_delay = v; }
}
