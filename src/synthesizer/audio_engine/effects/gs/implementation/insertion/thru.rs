//! thru.rs
//! purpose: Pass-through insertion effect (no processing).
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/insertion/thru.ts

use super::insertion_processor::DefaultGSInsertionProcessor;

pub struct ThruFx;

impl ThruFx {
    pub fn new(_sample_rate: f64) -> Self {
        Self
    }
}

impl DefaultGSInsertionProcessor for ThruFx {
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
        sample_count: usize,
    ) {
        output_l[..sample_count].copy_from_slice(&input_l[..sample_count]);
        output_r[..sample_count].copy_from_slice(&input_r[..sample_count]);
    }
}
