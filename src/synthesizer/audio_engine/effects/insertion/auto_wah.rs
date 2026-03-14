/// auto_wah.rs
/// purpose: Auto-wah (envelope-following filter) insertion effect.
/// Ported from: src/synthesizer/audio_engine/effects/insertion/auto_wah.ts

use super::utils::{
    BiquadCoeffs, BiquadState, apply_shelves, compute_highpass_coeffs, compute_lowpass_coeffs,
    compute_shelf_coeffs, get_pan_table_left, get_pan_table_right, process_biquad,
};
use super::convert::InsertionValueConverter;
use super::InsertionProcessor;

use std::f64::consts::PI;

const DEFAULT_LEVEL: f64 = 96.0;
const ATTACK_TIME: f64 = 0.1;
const RELEASE_TIME: f64 = 0.1;
const SENS_COEFF: f64 = 27.0;
const PEAK_DB: f64 = 28.0;
const HPF_Q: f64 = -28.0;
const HPF_FC: f64 = 400.0;
const MANUAL_SCALE: f64 = 0.62;
const FC_SMOOTH: f64 = 0.005;
const DEPTH_MUL: f64 = 5.0;
const LFO_SMOOTH_FRAC: f64 = DEPTH_MUL * 0.5;

pub struct AutoWahFx {
    send_level_to_reverb: f64,
    send_level_to_chorus: f64,
    send_level_to_delay: f64,
    sample_rate: f64,
    fil_type: u8,
    sens: f64,
    manual: f64,
    peak: f64,
    rate: f64,
    depth: f64,
    polarity: u8,
    pan: f64,
    low_gain: f64,
    hi_gain: f64,
    level: f64,
    phase: f64,
    last_fc: f64,
    envelope: f64,
    attack_coeff: f64,
    release_coeff: f64,
    coeffs: BiquadCoeffs,
    state: BiquadState,
    hp_coeffs: BiquadCoeffs,
    hp_state: BiquadState,
    ls_coeffs: BiquadCoeffs,
    hs_coeffs: BiquadCoeffs,
    ls_state: BiquadState,
    hs_state: BiquadState,
}

impl AutoWahFx {
    pub fn new(sample_rate: f64) -> Self {
        let attack_coeff = (-1.0 / (ATTACK_TIME * sample_rate)).exp();
        let release_coeff = (-1.0 / (RELEASE_TIME * sample_rate)).exp();

        let mut fx = Self {
            send_level_to_reverb: 40.0 / 127.0,
            send_level_to_chorus: 0.0,
            send_level_to_delay: 0.0,
            sample_rate,
            fil_type: 1,
            sens: 0.0,
            manual: 0.0,
            peak: 62.0,
            rate: 2.05,
            depth: 72.0,
            polarity: 1,
            pan: 0.0,
            low_gain: 0.0,
            hi_gain: 0.0,
            level: DEFAULT_LEVEL / 127.0,
            phase: 0.2,
            last_fc: 0.0,
            envelope: 0.0,
            attack_coeff,
            release_coeff,
            coeffs: BiquadCoeffs::default(),
            state: BiquadState::default(),
            hp_coeffs: BiquadCoeffs::default(),
            hp_state: BiquadState::default(),
            ls_coeffs: BiquadCoeffs::default(),
            hs_coeffs: BiquadCoeffs::default(),
            ls_state: BiquadState::default(),
            hs_state: BiquadState::default(),
        };
        fx.set_manual_value(68);
        fx.last_fc = fx.manual;
        fx.update_shelves();
        fx
    }

    fn set_manual_value(&mut self, value: u8) {
        let target = value as f64 * MANUAL_SCALE;
        let floor_idx = target.floor() as u8;
        let ceil_idx = target.ceil().min(127.0) as u8;
        let floor_val = InsertionValueConverter::manual(floor_idx);
        let ceil_val = InsertionValueConverter::manual(ceil_idx);
        let frac = target - target.floor();
        self.manual = floor_val + (ceil_val - floor_val) * frac;
    }

    fn update_shelves(&mut self) {
        compute_shelf_coeffs(&mut self.ls_coeffs, self.low_gain, 200.0, self.sample_rate, true);
        compute_shelf_coeffs(&mut self.hs_coeffs, self.hi_gain, 4000.0, self.sample_rate, false);
    }
}

impl InsertionProcessor for AutoWahFx {
    fn effect_type(&self) -> u16 { 0x0121 }

    fn reset(&mut self) {
        self.fil_type = 1;
        self.sens = 0.0;
        self.set_manual_value(68);
        self.peak = 62.0;
        self.rate = 2.05;
        self.depth = 72.0;
        self.polarity = 1;
        self.low_gain = 0.0;
        self.hi_gain = 0.0;
        self.pan = 0.0;
        self.level = DEFAULT_LEVEL / 127.0;
        self.phase = 0.2;
        self.last_fc = self.manual;
        self.hs_state.reset();
        self.ls_state.reset();
        self.state.reset();
        self.hp_state.reset();
        self.update_shelves();
    }

    fn set_parameter(&mut self, parameter: u8, value: u8) {
        match parameter {
            0x03 => { self.fil_type = value; }
            0x04 => { self.sens = value as f64; }
            0x05 => { self.set_manual_value(value); }
            0x06 => { self.peak = value as f64; }
            0x07 => { self.rate = InsertionValueConverter::rate1(value); }
            0x08 => { self.depth = value as f64; }
            0x09 => { self.polarity = value; }
            0x13 => { self.low_gain = value as f64 - 64.0; }
            0x14 => { self.hi_gain = value as f64 - 64.0; }
            0x15 => { self.pan = value as f64 - 64.0; }
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
        let fil_type = self.fil_type;
        let manual = self.manual;
        let sample_rate = self.sample_rate;
        let attack_coeff = self.attack_coeff;
        let release_coeff = self.release_coeff;
        let rev = self.send_level_to_reverb;
        let chr = self.send_level_to_chorus;
        let dly = self.send_level_to_delay;

        let rate_inc = self.rate / self.sample_rate;
        let peak = 10.0_f64.powf((self.peak / 127.0) * PEAK_DB / 20.0);
        let hpf_peak = 10.0_f64.powf((self.peak / 127.0) * HPF_Q / 20.0);
        let pol = if self.polarity == 0 { -1.0 } else { DEPTH_MUL };
        let depth = (self.depth / 127.0) * pol;
        let sens = self.sens / 127.0;

        let pan_index = (self.pan as i32 + 64).clamp(0, 127) as usize;
        let gain_l = get_pan_table_left()[pan_index] as f64;
        let gain_r = get_pan_table_right()[pan_index] as f64;

        let mut phase = self.phase;
        let mut last_fc = self.last_fc;
        let mut envelope = self.envelope;

        for i in 0..sample_count {
            // Mono input
            let s = apply_shelves(
                (input_l[i] as f64 + input_r[i] as f64) * 0.5,
                &self.ls_coeffs, &self.hs_coeffs,
                &mut self.ls_state, &mut self.hs_state,
            );

            let rectified = s.abs();
            envelope = if rectified > envelope {
                attack_coeff * envelope + (1.0 - attack_coeff) * rectified
            } else {
                release_coeff * envelope + (1.0 - release_coeff) * rectified
            };

            // Triangle LFO
            let lfo = 2.0 * (phase - 0.5).abs() * depth;
            phase += rate_inc;
            if phase >= 1.0 { phase -= 1.0; }
            let lfo_mul = if lfo >= LFO_SMOOTH_FRAC || pol < 0.0 {
                1.0
            } else {
                (lfo * PI / (2.0 * LFO_SMOOTH_FRAC)).sin()
            };
            let base = manual * (1.0 + sens * envelope * SENS_COEFF);
            let fc = (base * (1.0 + lfo_mul * lfo)).max(20.0);
            let target = fc.max(10.0);
            last_fc += (target - last_fc) * FC_SMOOTH;
            compute_lowpass_coeffs(&mut self.coeffs, last_fc, peak, sample_rate);

            let mut processed = s;
            if fil_type == 1 {
                compute_highpass_coeffs(&mut self.hp_coeffs, HPF_FC, hpf_peak, sample_rate);
                processed = process_biquad(processed, &self.hp_coeffs, &mut self.hp_state);
            }

            let mono = process_biquad(processed, &self.coeffs, &mut self.state) * level;

            let out_l = mono * gain_l;
            let out_r = mono * gain_r;

            let idx = start_index + i;
            output_l[idx] = (output_l[idx] as f64 + out_l) as f32;
            output_r[idx] = (output_r[idx] as f64 + out_r) as f32;
            reverb_out[i] = (reverb_out[i] as f64 + mono * rev) as f32;
            chorus_out[i] = (chorus_out[i] as f64 + mono * chr) as f32;
            delay_out[i] = (delay_out[i] as f64 + mono * dly) as f32;
        }
        self.phase = phase;
        self.last_fc = last_fc;
        self.envelope = envelope;
    }

    fn send_level_to_reverb(&self) -> f64 { self.send_level_to_reverb }
    fn send_level_to_chorus(&self) -> f64 { self.send_level_to_chorus }
    fn send_level_to_delay(&self) -> f64 { self.send_level_to_delay }
    fn set_send_level_to_reverb(&mut self, v: f64) { self.send_level_to_reverb = v; }
    fn set_send_level_to_chorus(&mut self, v: f64) { self.send_level_to_chorus = v; }
    fn set_send_level_to_delay(&mut self, v: f64) { self.send_level_to_delay = v; }
}
