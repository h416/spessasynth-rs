//! ph_auto_wah.rs
//! purpose: Parallel Phaser + Auto-wah insertion effect.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/insertion/ph_auto_wah.ts

use super::utils::{get_pan_table_left, get_pan_table_right};
use super::phaser::PhaserFx;
use super::auto_wah::AutoWahFx;
use super::InsertionProcessor;
use crate::synthesizer::audio_engine::synth_constants::EFX_SENDS_GAIN_CORRECTION;

const DEFAULT_LEVEL: f64 = 127.0;

pub struct PhAutoWahFx {
    send_level_to_reverb: f64,
    send_level_to_chorus: f64,
    send_level_to_delay: f64,
    ph_pan: u8,
    aw_pan: u8,
    level: f64,
    phaser: PhaserFx,
    auto_wah: AutoWahFx,
    buffer_ph: Vec<f32>,
    buffer_aw: Vec<f32>,
}

impl PhAutoWahFx {
    pub fn new(sample_rate: f64, max_buffer_size: usize) -> Self {
        let mut phaser = PhaserFx::new(sample_rate);
        let mut auto_wah = AutoWahFx::new(sample_rate);
        phaser.set_send_level_to_reverb(0.0);
        phaser.set_send_level_to_chorus(0.0);
        phaser.set_send_level_to_delay(0.0);
        auto_wah.set_send_level_to_reverb(0.0);
        auto_wah.set_send_level_to_chorus(0.0);
        auto_wah.set_send_level_to_delay(0.0);

        let mut fx = Self {
            send_level_to_reverb: 40.0 / 127.0 * EFX_SENDS_GAIN_CORRECTION,
            send_level_to_chorus: 0.0,
            send_level_to_delay: 0.0,
            ph_pan: 0,
            aw_pan: 127,
            level: DEFAULT_LEVEL / 127.0,
            phaser,
            auto_wah,
            buffer_ph: vec![0.0; max_buffer_size],
            buffer_aw: vec![0.0; max_buffer_size],
        };
        fx.phaser.set_parameter(0x16, 127);
        fx.auto_wah.set_parameter(0x16, 127);
        fx
    }
}

impl InsertionProcessor for PhAutoWahFx {
    fn effect_type(&self) -> u16 { 0x1108 }

    fn reset(&mut self) {
        self.ph_pan = 0;
        self.aw_pan = 127;
        self.level = DEFAULT_LEVEL / 127.0;
        self.phaser.reset();
        self.auto_wah.reset();
        self.phaser.set_parameter(0x16, 127);
        self.auto_wah.set_parameter(0x16, 127);
    }

    fn set_parameter(&mut self, parameter: u8, value: u8) {
        if parameter >= 0x03 && parameter <= 0x07 {
            self.phaser.set_parameter(parameter, value);
            return;
        }
        if parameter >= 0x08 && parameter <= 0x0e {
            self.auto_wah.set_parameter(parameter - 5, value);
            return;
        }
        match parameter {
            0x12 => { self.ph_pan = value; }
            0x13 => { self.phaser.set_parameter(0x16, value); }
            0x14 => { self.aw_pan = value; }
            0x15 => { self.auto_wah.set_parameter(0x16, value); }
            0x16 => { self.level = value as f64 / 127.0; }
            _ => {}
        }
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

        // Process phaser
        // TS passes `bufferPh` as every output, so both channels and the (level 0) sends sum into it
        let buffer_ph = &mut self.buffer_ph;
        buffer_ph.fill(0.0);
        let (ph_rev, ph_chr, ph_dly) = (
            self.phaser.send_level_to_reverb(),
            self.phaser.send_level_to_chorus(),
            self.phaser.send_level_to_delay(),
        );
        // Only takes input from left!
        self.phaser.process_with(input_l, input_l, sample_count, |i, out_l, out_r| {
            buffer_ph[i] = (buffer_ph[i] as f64 + out_l) as f32;
            buffer_ph[i] = (buffer_ph[i] as f64 + out_r) as f32;
            let mono = (out_l + out_r) * 0.5;
            buffer_ph[i] = (buffer_ph[i] as f64 + mono * ph_rev) as f32;
            buffer_ph[i] = (buffer_ph[i] as f64 + mono * ph_chr) as f32;
            buffer_ph[i] = (buffer_ph[i] as f64 + mono * ph_dly) as f32;
        });

        // Process auto wah
        let buffer_aw = &mut self.buffer_aw;
        buffer_aw.fill(0.0);
        let (aw_rev, aw_chr, aw_dly) = (
            self.auto_wah.send_level_to_reverb(),
            self.auto_wah.send_level_to_chorus(),
            self.auto_wah.send_level_to_delay(),
        );
        // Only takes input from right!
        self.auto_wah.process_with(input_r, input_r, sample_count, |i, out_l, out_r, mono| {
            buffer_aw[i] = (buffer_aw[i] as f64 + out_l) as f32;
            buffer_aw[i] = (buffer_aw[i] as f64 + out_r) as f32;
            buffer_aw[i] = (buffer_aw[i] as f64 + mono * aw_rev) as f32;
            buffer_aw[i] = (buffer_aw[i] as f64 + mono * aw_chr) as f32;
            buffer_aw[i] = (buffer_aw[i] as f64 + mono * aw_dly) as f32;
        });

        let pan_l_table = get_pan_table_left();
        let pan_r_table = get_pan_table_right();
        let ph_pan = (self.ph_pan as usize).min(127);
        let ph_l = pan_l_table[ph_pan] as f64;
        let ph_r = pan_r_table[ph_pan] as f64;
        let aw_pan = (self.aw_pan as usize).min(127);
        let aw_l = pan_l_table[aw_pan] as f64;
        let aw_r = pan_r_table[aw_pan] as f64;

        for i in 0..sample_count {
            // Divide by 2 since processor mixes both left and right into it
            let out_phaser = self.buffer_ph[i] as f64 * 0.5 * level;
            let out_auto_wah = self.buffer_aw[i] as f64 * 0.5 * level;

            let out_l = out_phaser * ph_l + out_auto_wah * aw_l;
            let out_r = out_phaser * ph_r + out_auto_wah * aw_r;

            let idx = start_index + i;
            output_l[idx] = (output_l[idx] as f64 + out_l) as f32;
            output_r[idx] = (output_r[idx] as f64 + out_r) as f32;
            let mono = (out_l + out_r) * 0.5;
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
