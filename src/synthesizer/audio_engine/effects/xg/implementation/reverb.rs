//! The default implementation of the XG reverb block.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/reverb.ts

use super::dsp::effect_processor::DefaultXGEffectProcessor;
use super::dsp::no_effect::XGNoEffect;
use super::dsp::reverb::{
    basement, gm_plate, hall1, hall2, hall_l, hall_m, plate, room1, room2, room3, room_l, room_m,
    room_s, stage1, stage2, tunnel, white_room,
};
use super::effect::DefaultXGEffect;
use super::system_effect::{DefaultXGSystemEffect, XGSystemEffectBlockSnapshot};

/// Equivalent to: REVERB_MAP
fn reverb_map(sr: f64, buf: usize) -> Vec<(u16, Box<dyn DefaultXGEffectProcessor>)> {
    vec![
        (0x00_00, Box::new(XGNoEffect)),
        (0x01_00, Box::new(hall1::new(sr, buf, false))),
        (0x01_01, Box::new(hall2::new(sr, buf, false))),
        (0x01_06, Box::new(hall_m::new(sr, buf, false))),
        (0x01_07, Box::new(hall_l::new(sr, buf, false))),
        (0x02_00, Box::new(room1::new(sr, buf, false))),
        (0x02_01, Box::new(room2::new(sr, buf, false))),
        (0x02_02, Box::new(room3::new(sr, buf, false))),
        (0x02_05, Box::new(room_s::new(sr, buf, false))),
        (0x02_06, Box::new(room_m::new(sr, buf, false))),
        (0x02_07, Box::new(room_l::new(sr, buf, false))),
        (0x03_00, Box::new(stage1::new(sr, buf, false))),
        (0x03_01, Box::new(stage2::new(sr, buf, false))),
        (0x04_00, Box::new(plate::new(sr, buf, false))),
        (0x04_07, Box::new(gm_plate::new(sr, buf, false))),
        (0x10_00, Box::new(white_room::new(sr, buf, false))),
        (0x11_00, Box::new(tunnel::new(sr, buf, false))),
        (0x13_00, Box::new(basement::new(sr, buf, false))),
    ]
}

/// The default implementation for XGReverbBlock.
/// Equivalent to: class DefaultXGReverb
pub struct DefaultXGReverb {
    pub system: DefaultXGSystemEffect,
}

impl DefaultXGReverb {
    /// Constructs a new default XG reverb processor.
    pub fn new(sample_rate: f64, max_buffer_size: usize) -> Self {
        Self {
            system: DefaultXGSystemEffect::new(DefaultXGEffect::new(
                reverb_map(sample_rate, max_buffer_size),
                Box::new(XGNoEffect),
                0x01_00,
                sample_rate,
                max_buffer_size,
            )),
        }
    }

    pub fn reset(&mut self) {
        self.system.reset();
    }

    pub fn get_snapshot(&self) -> XGSystemEffectBlockSnapshot {
        self.system.get_snapshot()
    }

    pub fn apply_snapshot(&mut self, snapshot: &XGSystemEffectBlockSnapshot) {
        self.system.apply_snapshot(snapshot);
    }

    /// Process the effect and **adds** it to the output.
    pub fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        self.system.effect.process_processor(input_left, input_right, sample_count, false);
        self.system.mix_system_effect(output_left, output_right, start_index, sample_count);
    }
}
