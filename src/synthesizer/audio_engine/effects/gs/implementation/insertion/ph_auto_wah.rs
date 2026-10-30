//! ph_auto_wah.rs
//! purpose: Parallel Phaser + Auto-wah insertion effect.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/insertion/ph_auto_wah.ts

use super::utils::{get_pan_table_left, get_pan_table_right};
use super::phaser::PhaserFx;
use super::auto_wah::AutoWahFx;
use super::insertion_processor::DefaultGSInsertionProcessor;

const DEFAULT_LEVEL: f64 = 127.0;

pub struct PhAutoWahFx {
    ph_pan: u8,
    aw_pan: u8,
    level: f64,
    phaser: PhaserFx,
    auto_wah: AutoWahFx,
    buffer_ph_l: Vec<f32>,
    buffer_ph_r: Vec<f32>,
    buffer_aw_l: Vec<f32>,
    buffer_aw_r: Vec<f32>,
}

impl PhAutoWahFx {
    pub fn new(sample_rate: f64, max_buffer_size: usize) -> Self {
        let mut fx = Self {
            ph_pan: 0,
            aw_pan: 127,
            level: DEFAULT_LEVEL / 127.0,
            phaser: PhaserFx::new(sample_rate),
            auto_wah: AutoWahFx::new(sample_rate),
            buffer_ph_l: vec![0.0; max_buffer_size],
            buffer_ph_r: vec![0.0; max_buffer_size],
            buffer_aw_l: vec![0.0; max_buffer_size],
            buffer_aw_r: vec![0.0; max_buffer_size],
        };
        fx.reset();
        fx
    }
}

impl DefaultGSInsertionProcessor for PhAutoWahFx {
    fn effect_type(&self) -> u16 { 0x1108 }

    fn reset(&mut self) {
        self.ph_pan = 0;
        self.aw_pan = 127;
        self.level = DEFAULT_LEVEL / 127.0;
        self.phaser.reset();
        self.auto_wah.reset();
        // Level
        self.phaser.set_parameter(19, 127);
        self.auto_wah.set_parameter(19, 127);
    }

    fn set_parameter(&mut self, parameter: u8, value: u8) {
        if parameter <= 4 {
            self.phaser.set_parameter(parameter, value);
            return;
        }
        if (5..=11).contains(&parameter) {
            self.auto_wah.set_parameter(parameter - 5, value);
            return;
        }
        match parameter {
            15 => { self.ph_pan = value; }
            16 => { self.phaser.set_parameter(19, value); }
            17 => { self.aw_pan = value; }
            18 => { self.auto_wah.set_parameter(19, value); }
            19 => { self.level = value as f64 / 127.0; }
            _ => {}
        }
    }

    fn process(
        &mut self,
        input_l: &[f32], input_r: &[f32],
        output_l: &mut [f32], output_r: &mut [f32],
        sample_count: usize,
    ) {
        let level = self.level;
        // Only takes input from left!
        self.phaser.process(
            input_l, input_l,
            &mut self.buffer_ph_l, &mut self.buffer_ph_r,
            sample_count,
        );
        // Only takes input from right!
        self.auto_wah.process(
            input_r, input_r,
            &mut self.buffer_aw_l, &mut self.buffer_aw_r,
            sample_count,
        );

        let pan_l_table = get_pan_table_left();
        let pan_r_table = get_pan_table_right();
        let ph_pan = (self.ph_pan as usize).min(127);
        let ph_l = pan_l_table[ph_pan] as f64;
        let ph_r = pan_r_table[ph_pan] as f64;
        let aw_pan = (self.aw_pan as usize).min(127);
        let aw_l = pan_l_table[aw_pan] as f64;
        let aw_r = pan_r_table[aw_pan] as f64;

        for i in 0..sample_count {
            // Mix the stereo processor outputs down to mono
            let out_phaser =
                (self.buffer_ph_l[i] as f64 + self.buffer_ph_r[i] as f64) * 0.5 * level;
            let out_auto_wah =
                (self.buffer_aw_l[i] as f64 + self.buffer_aw_r[i] as f64) * 0.5 * level;

            // Pan
            output_l[i] = (out_phaser * ph_l + out_auto_wah * aw_l) as f32;
            output_r[i] = (out_phaser * ph_r + out_auto_wah * aw_r) as f32;
        }
    }
}
