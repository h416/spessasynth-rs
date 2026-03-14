/// thru.rs
/// purpose: Pass-through insertion effect (no processing).
/// Ported from: src/synthesizer/audio_engine/effects/insertion/thru.ts

use super::InsertionProcessor;

pub struct ThruFx {
    send_level_to_reverb: f64,
    send_level_to_chorus: f64,
    send_level_to_delay: f64,
}

impl ThruFx {
    pub fn new(_sample_rate: f64) -> Self {
        Self {
            send_level_to_reverb: 40.0 / 127.0,
            send_level_to_chorus: 0.0,
            send_level_to_delay: 0.0,
        }
    }
}

impl InsertionProcessor for ThruFx {
    fn effect_type(&self) -> u16 {
        0x0000
    }

    fn reset(&mut self) {}

    fn set_parameter(&mut self, _parameter: u8, _value: u8) {}

    fn process(
        &mut self,
        input_l: &[f32],
        input_r: &[f32],
        output_l: &mut [f32],
        output_r: &mut [f32],
        reverb_out: &mut [f32],
        chorus_out: &mut [f32],
        delay_out: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        let rev = self.send_level_to_reverb;
        let chr = self.send_level_to_chorus;
        let dly = self.send_level_to_delay;
        for i in 0..sample_count {
            let sl = input_l[i] as f64;
            let sr = input_r[i] as f64;
            let idx = start_index + i;
            output_l[idx] = (output_l[idx] as f64 + sl) as f32;
            output_r[idx] = (output_r[idx] as f64 + sr) as f32;
            let mono = (sl + sr) * 0.5;
            reverb_out[i] = (reverb_out[i] as f64 + mono * rev) as f32;
            chorus_out[i] = (chorus_out[i] as f64 + mono * chr) as f32;
            delay_out[i] = (delay_out[i] as f64 + mono * dly) as f32;
        }
    }

    fn send_level_to_reverb(&self) -> f64 {
        self.send_level_to_reverb
    }
    fn send_level_to_chorus(&self) -> f64 {
        self.send_level_to_chorus
    }
    fn send_level_to_delay(&self) -> f64 {
        self.send_level_to_delay
    }
    fn set_send_level_to_reverb(&mut self, value: f64) {
        self.send_level_to_reverb = value;
    }
    fn set_send_level_to_chorus(&mut self, value: f64) {
        self.send_level_to_chorus = value;
    }
    fn set_send_level_to_delay(&mut self, value: f64) {
        self.send_level_to_delay = value;
    }
}
