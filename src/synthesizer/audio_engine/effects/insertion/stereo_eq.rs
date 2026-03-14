/// stereo_eq.rs
/// purpose: 4-band stereo parametric EQ insertion effect.
/// Ported from: src/synthesizer/audio_engine/effects/insertion/stereo_eq.ts

use super::utils::{
    BiquadCoeffs, BiquadState, compute_peaking_eq_coeffs, compute_shelf_coeffs, process_biquad,
};
use super::convert::InsertionValueConverter;
use super::InsertionProcessor;

use std::f64::consts::PI;

pub struct StereoEqFx {
    send_level_to_reverb: f64,
    send_level_to_chorus: f64,
    send_level_to_delay: f64,
    sample_rate: f64,
    level: f64,
    low_freq: f64,
    low_gain: f64,
    hi_freq: f64,
    hi_gain: f64,
    m1_freq: f64,
    m1_q: f64,
    m1_gain: f64,
    m2_freq: f64,
    m2_q: f64,
    m2_gain: f64,
    low_coeffs: BiquadCoeffs,
    m1_coeffs: BiquadCoeffs,
    m2_coeffs: BiquadCoeffs,
    hi_coeffs: BiquadCoeffs,
    low_state_l: BiquadState,
    low_state_r: BiquadState,
    m1_state_l: BiquadState,
    m1_state_r: BiquadState,
    m2_state_l: BiquadState,
    m2_state_r: BiquadState,
    hi_state_l: BiquadState,
    hi_state_r: BiquadState,
}

impl StereoEqFx {
    pub fn new(sample_rate: f64) -> Self {
        let mut fx = Self {
            send_level_to_reverb: 0.0,
            send_level_to_chorus: 0.0,
            send_level_to_delay: 0.0,
            sample_rate,
            level: 1.0,
            low_freq: 400.0,
            low_gain: 5.0,
            hi_freq: 8000.0,
            hi_gain: -12.0,
            m1_freq: 1600.0,
            m1_q: 0.5,
            m1_gain: 8.0,
            m2_freq: 1000.0,
            m2_q: 0.5,
            m2_gain: -8.0,
            low_coeffs: BiquadCoeffs::default(),
            m1_coeffs: BiquadCoeffs::default(),
            m2_coeffs: BiquadCoeffs::default(),
            hi_coeffs: BiquadCoeffs::default(),
            low_state_l: BiquadState::default(),
            low_state_r: BiquadState::default(),
            m1_state_l: BiquadState::default(),
            m1_state_r: BiquadState::default(),
            m2_state_l: BiquadState::default(),
            m2_state_r: BiquadState::default(),
            hi_state_l: BiquadState::default(),
            hi_state_r: BiquadState::default(),
        };
        fx.update_coefficients();
        fx
    }

    fn update_coefficients(&mut self) {
        compute_low_shelf_coeffs(&mut self.low_coeffs, self.low_freq, self.low_gain / 2.0, self.sample_rate);
        compute_peaking_eq_coeffs(&mut self.m1_coeffs, self.m1_freq, self.m1_gain, self.m1_q, self.sample_rate);
        compute_peaking_eq_coeffs(&mut self.m2_coeffs, self.m2_freq, self.m2_gain, self.m2_q, self.sample_rate);
        compute_high_shelf_coeffs(&mut self.hi_coeffs, self.hi_freq, self.hi_gain / 2.0, self.sample_rate);
    }
}

impl InsertionProcessor for StereoEqFx {
    fn effect_type(&self) -> u16 { 0x0100 }

    fn reset(&mut self) {
        self.level = 1.0;
        self.low_freq = 400.0;
        self.low_gain = 5.0;
        self.hi_gain = -12.0;
        self.hi_freq = 8000.0;
        self.m1_freq = 1600.0;
        self.m1_q = 0.5;
        self.m1_gain = 8.0;
        self.m2_freq = 1000.0;
        self.m2_q = 0.5;
        self.m2_gain = -8.0;
        self.low_state_l.reset();
        self.low_state_r.reset();
        self.m1_state_l.reset();
        self.m1_state_r.reset();
        self.m2_state_l.reset();
        self.m2_state_r.reset();
        self.hi_state_l.reset();
        self.hi_state_r.reset();
        self.update_coefficients();
    }

    fn set_parameter(&mut self, parameter: u8, value: u8) {
        match parameter {
            0x03 => { self.low_freq = if value == 1 { 400.0 } else { 200.0 }; }
            0x04 => { self.low_gain = value as f64 - 64.0; }
            0x05 => { self.hi_freq = if value == 1 { 8000.0 } else { 4000.0 }; }
            0x06 => { self.hi_gain = value as f64 - 64.0; }
            0x07 => { self.m1_freq = InsertionValueConverter::eq_freq(value); }
            0x08 => {
                const Q_TABLE: [f64; 5] = [0.5, 1.0, 2.0, 4.0, 9.0];
                self.m1_q = Q_TABLE.get(value as usize).copied().unwrap_or(1.0);
            }
            0x09 => { self.m1_gain = value as f64 - 64.0; }
            0x0a => { self.m2_freq = InsertionValueConverter::eq_freq(value); }
            0x0b => {
                const Q_TABLE: [f64; 5] = [0.5, 1.0, 2.0, 4.0, 9.0];
                self.m2_q = Q_TABLE.get(value as usize).copied().unwrap_or(1.0);
            }
            0x0c => { self.m2_gain = value as f64 - 64.0; }
            0x16 => { self.level = value as f64 / 127.0; }
            _ => {}
        }
        self.update_coefficients();
    }

    fn process(
        &mut self,
        input_l: &[f32], input_r: &[f32],
        output_l: &mut [f32], output_r: &mut [f32],
        reverb_out: &mut [f32], chorus_out: &mut [f32], delay_out: &mut [f32],
        start_index: usize, sample_count: usize,
    ) {
        let level = self.level;
        let rev = self.send_level_to_reverb;
        let chr = self.send_level_to_chorus;
        let dly = self.send_level_to_delay;

        for i in 0..sample_count {
            let mut sl = input_l[i] as f64;
            let mut sr = input_r[i] as f64;

            sl = process_biquad(sl, &self.low_coeffs, &mut self.low_state_l);
            sr = process_biquad(sr, &self.low_coeffs, &mut self.low_state_r);
            sl = process_biquad(sl, &self.m1_coeffs, &mut self.m1_state_l);
            sr = process_biquad(sr, &self.m1_coeffs, &mut self.m1_state_r);
            sl = process_biquad(sl, &self.m2_coeffs, &mut self.m2_state_l);
            sr = process_biquad(sr, &self.m2_coeffs, &mut self.m2_state_r);
            sl = process_biquad(sl, &self.hi_coeffs, &mut self.hi_state_l);
            sr = process_biquad(sr, &self.hi_coeffs, &mut self.hi_state_r);

            let idx = start_index + i;
            output_l[idx] = (output_l[idx] as f64 + sl * level) as f32;
            output_r[idx] = (output_r[idx] as f64 + sr * level) as f32;
            let mono = 0.5 * (sl + sr);
            reverb_out[i] = (reverb_out[i] as f64 + mono * rev) as f32;
            chorus_out[i] = (chorus_out[i] as f64 + mono * chr) as f32;
            delay_out[i] = (delay_out[i] as f64 + mono * dly) as f32;
        }
    }

    fn send_level_to_reverb(&self) -> f64 { self.send_level_to_reverb }
    fn send_level_to_chorus(&self) -> f64 { self.send_level_to_chorus }
    fn send_level_to_delay(&self) -> f64 { self.send_level_to_delay }
    fn set_send_level_to_reverb(&mut self, v: f64) { self.send_level_to_reverb = v; }
    fn set_send_level_to_chorus(&mut self, v: f64) { self.send_level_to_chorus = v; }
    fn set_send_level_to_delay(&mut self, v: f64) { self.send_level_to_delay = v; }
}

fn compute_low_shelf_coeffs(coeffs: &mut BiquadCoeffs, freq: f64, gain_db: f64, sample_rate: f64) {
    compute_shelf_coeffs(coeffs, gain_db, freq, sample_rate, true);
}

fn compute_high_shelf_coeffs(coeffs: &mut BiquadCoeffs, freq: f64, gain_db: f64, sample_rate: f64) {
    compute_shelf_coeffs(coeffs, gain_db, freq, sample_rate, false);
}
