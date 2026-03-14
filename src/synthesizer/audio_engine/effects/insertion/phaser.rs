/// phaser.rs
/// purpose: 8-stage all-pass phaser insertion effect.
/// Ported from: src/synthesizer/audio_engine/effects/insertion/phaser.ts

use super::utils::{
    BiquadCoeffs, BiquadState, apply_shelves, compute_shelf_coeffs,
};
use super::convert::InsertionValueConverter;
use super::InsertionProcessor;

use std::f64::consts::PI;

const ALL_PASS_STAGES: usize = 8;
const DEPTH_DIV: f64 = 128.0;
const MANUAL_MULTIPLIER: f64 = 4.0;
const MANUAL_OFFSET: f64 = 600.0;
const FEEDBACK: f64 = 0.9;
const PHASE_START: f64 = 0.35;

pub struct PhaserFx {
    send_level_to_reverb: f64,
    send_level_to_chorus: f64,
    send_level_to_delay: f64,
    sample_rate: f64,
    manual: f64,
    manual_offset: f64,
    rate: f64,
    depth: f64,
    reso: f64,
    mix: f64,
    low_gain: f64,
    hi_gain: f64,
    level: f64,
    phase: f64,
    prev_l: f64,
    prev_r: f64,
    prev_in_l: [f64; ALL_PASS_STAGES],
    prev_out_l: [f64; ALL_PASS_STAGES],
    prev_in_r: [f64; ALL_PASS_STAGES],
    prev_out_r: [f64; ALL_PASS_STAGES],
    low_shelf_coef: BiquadCoeffs,
    high_shelf_coef: BiquadCoeffs,
    low_shelf_state_l: BiquadState,
    low_shelf_state_r: BiquadState,
    high_shelf_state_l: BiquadState,
    high_shelf_state_r: BiquadState,
}

impl PhaserFx {
    pub fn new(sample_rate: f64) -> Self {
        let mut fx = Self {
            send_level_to_reverb: 40.0 / 127.0,
            send_level_to_chorus: 0.0,
            send_level_to_delay: 0.0,
            sample_rate,
            manual: 620.0,
            manual_offset: MANUAL_OFFSET,
            rate: 0.85,
            depth: 64.0 / DEPTH_DIV,
            reso: 16.0 / 127.0,
            mix: 1.0,
            low_gain: 0.0,
            hi_gain: 0.0,
            level: 104.0 / 127.0,
            phase: PHASE_START,
            prev_l: 0.0,
            prev_r: 0.0,
            prev_in_l: [0.0; ALL_PASS_STAGES],
            prev_out_l: [0.0; ALL_PASS_STAGES],
            prev_in_r: [0.0; ALL_PASS_STAGES],
            prev_out_r: [0.0; ALL_PASS_STAGES],
            low_shelf_coef: BiquadCoeffs::default(),
            high_shelf_coef: BiquadCoeffs::default(),
            low_shelf_state_l: BiquadState::default(),
            low_shelf_state_r: BiquadState::default(),
            high_shelf_state_l: BiquadState::default(),
            high_shelf_state_r: BiquadState::default(),
        };
        fx.update_shelves();
        fx
    }

    fn set_manual(&mut self, manual_in: f64) {
        if manual_in > 1000.0 {
            self.manual_offset = MANUAL_OFFSET * 1.5 * MANUAL_MULTIPLIER;
            self.manual = manual_in;
        } else {
            self.manual_offset = MANUAL_OFFSET;
            self.manual = manual_in * MANUAL_MULTIPLIER;
        }
    }

    fn clear_all_pass(&mut self) {
        self.prev_l = 0.0;
        self.prev_r = 0.0;
        self.prev_in_l = [0.0; ALL_PASS_STAGES];
        self.prev_out_l = [0.0; ALL_PASS_STAGES];
        self.prev_in_r = [0.0; ALL_PASS_STAGES];
        self.prev_out_r = [0.0; ALL_PASS_STAGES];
    }

    fn update_shelves(&mut self) {
        compute_shelf_coeffs(&mut self.low_shelf_coef, self.low_gain, 200.0, self.sample_rate, true);
        compute_shelf_coeffs(&mut self.high_shelf_coef, self.hi_gain, 4000.0, self.sample_rate, false);
    }
}

impl InsertionProcessor for PhaserFx {
    fn effect_type(&self) -> u16 { 0x0120 }

    fn reset(&mut self) {
        self.phase = PHASE_START;
        self.set_manual(620.0);
        self.rate = 0.85;
        self.depth = 64.0 / DEPTH_DIV;
        self.reso = 16.0 / 127.0;
        self.mix = 1.0;
        self.low_gain = 0.0;
        self.hi_gain = 0.0;
        self.level = 104.0 / 127.0;
        self.low_shelf_state_l.reset();
        self.low_shelf_state_r.reset();
        self.high_shelf_state_l.reset();
        self.high_shelf_state_r.reset();
        self.update_shelves();
        self.clear_all_pass();
    }

    fn set_parameter(&mut self, parameter: u8, value: u8) {
        match parameter {
            0x03 => { self.set_manual(InsertionValueConverter::manual(value)); }
            0x04 => { self.rate = InsertionValueConverter::rate1(value); }
            0x05 => { self.depth = value as f64 / DEPTH_DIV; }
            0x06 => { self.reso = value as f64 / 127.0; }
            0x07 => { self.mix = value as f64 / 127.0; }
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
        let manual = self.manual;
        let manual_offset = self.manual_offset;
        let mix = self.mix;
        let depth = self.depth;
        let sample_rate = self.sample_rate;
        let rate_inc = self.rate / self.sample_rate;
        let fb = self.reso * FEEDBACK;
        let rev = self.send_level_to_reverb;
        let chr = self.send_level_to_chorus;
        let dly = self.send_level_to_delay;

        let mut prev_l = self.prev_l;
        let mut prev_r = self.prev_r;
        let mut phase = self.phase;

        for i in 0..sample_count {
            let sl = apply_shelves(
                input_l[i] as f64,
                &self.low_shelf_coef, &self.high_shelf_coef,
                &mut self.low_shelf_state_l, &mut self.high_shelf_state_l,
            );
            let sr = apply_shelves(
                input_r[i] as f64,
                &self.low_shelf_coef, &self.high_shelf_coef,
                &mut self.low_shelf_state_r, &mut self.high_shelf_state_r,
            );

            // Triangle LFO
            let lfo = 2.0 * (phase - 0.5).abs();
            phase += rate_inc;
            if phase >= 1.0 { phase -= 1.0; }
            let lfo_mul = 1.0 - depth * lfo;

            let fc = manual_offset + manual * lfo_mul;
            let tan_term = (PI * fc / sample_rate).tan();
            let a = ((1.0 - tan_term) / (1.0 + tan_term)).clamp(-0.9999, 0.9999);

            // Process all-pass stages
            let mut ap_l = sl + fb * prev_l;
            let mut ap_r = sr + fb * prev_r;
            for stage in 0..ALL_PASS_STAGES {
                let out_l = -a * ap_l + self.prev_in_l[stage] + a * self.prev_out_l[stage];
                self.prev_in_l[stage] = ap_l;
                self.prev_out_l[stage] = out_l;
                ap_l = out_l;
                let out_r = -a * ap_r + self.prev_in_r[stage] + a * self.prev_out_r[stage];
                self.prev_in_r[stage] = ap_r;
                self.prev_out_r[stage] = out_r;
                ap_r = out_r;
            }
            prev_l = ap_l;
            prev_r = ap_r;

            let out_l_val = (sl + ap_l * mix) * level;
            let out_r_val = (sr + ap_r * mix) * level;

            let idx = start_index + i;
            output_l[idx] = (output_l[idx] as f64 + out_l_val) as f32;
            output_r[idx] = (output_r[idx] as f64 + out_r_val) as f32;
            let mono = (out_l_val + out_r_val) * 0.5;
            reverb_out[i] = (reverb_out[i] as f64 + mono * rev) as f32;
            chorus_out[i] = (chorus_out[i] as f64 + mono * chr) as f32;
            delay_out[i] = (delay_out[i] as f64 + mono * dly) as f32;
        }
        self.phase = phase;
        self.prev_l = prev_l;
        self.prev_r = prev_r;
    }

    fn send_level_to_reverb(&self) -> f64 { self.send_level_to_reverb }
    fn send_level_to_chorus(&self) -> f64 { self.send_level_to_chorus }
    fn send_level_to_delay(&self) -> f64 { self.send_level_to_delay }
    fn set_send_level_to_reverb(&mut self, v: f64) { self.send_level_to_reverb = v; }
    fn set_send_level_to_chorus(&mut self, v: f64) { self.send_level_to_chorus = v; }
    fn set_send_level_to_delay(&mut self, v: f64) { self.send_level_to_delay = v; }
}
