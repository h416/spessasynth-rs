//! The default implementation of the XG chorus block.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/chorus.ts

use super::dsp::chorus::{
    celeste1, celeste2, celeste3, celeste4, chorus1, chorus2, chorus3, chorus4, fb_chorus,
    flanger1, flanger2, flanger3, gm_chorus1, gm_chorus2, gm_chorus3, gm_chorus4, gm_flanger,
};
use super::dsp::effect_processor::DefaultXGEffectProcessor;
use super::dsp::no_effect::XGNoEffect;
use super::effect::DefaultXGEffect;
use super::system_effect::DefaultXGSystemEffect;

/// Equivalent to: CHORUS_MAP
fn chorus_map(sr: f64, buf: usize) -> Vec<(u16, Box<dyn DefaultXGEffectProcessor>)> {
    vec![
        (0x00_00, Box::new(XGNoEffect)),
        (0x41_00, Box::new(chorus1::new(sr, buf, false))),
        (0x41_01, Box::new(chorus2::new(sr, buf, false))),
        (0x41_02, Box::new(chorus3::new(sr, buf, false))),
        (0x41_03, Box::new(gm_chorus1::new(sr, buf, false))),
        (0x41_04, Box::new(gm_chorus2::new(sr, buf, false))),
        (0x41_05, Box::new(gm_chorus3::new(sr, buf, false))),
        (0x41_06, Box::new(gm_chorus4::new(sr, buf, false))),
        (0x41_07, Box::new(fb_chorus::new(sr, buf, false))),
        (0x41_08, Box::new(chorus4::new(sr, buf, false))),
        (0x42_00, Box::new(celeste1::new(sr, buf, false))),
        (0x42_01, Box::new(celeste2::new(sr, buf, false))),
        (0x42_02, Box::new(celeste3::new(sr, buf, false))),
        (0x42_08, Box::new(celeste4::new(sr, buf, false))),
        (0x43_00, Box::new(flanger1::new(sr, buf, false))),
        (0x43_01, Box::new(flanger2::new(sr, buf, false))),
        (0x43_07, Box::new(gm_flanger::new(sr, buf, false))),
        (0x43_08, Box::new(flanger3::new(sr, buf, false))),
    ]
}

/// The default implementation for XGChorusBlock.
/// Equivalent to: class DefaultXGChorus
pub struct DefaultXGChorus {
    pub system: DefaultXGSystemEffect,
    /// The amount of chorus being sent to the reverb effect.
    /// 0 is none, 64 is 100% and 127 is 200%.
    pub send_to_reverb: i32,
}

impl DefaultXGChorus {
    /// Constructs a new default XG chorus processor.
    pub fn new(sample_rate: f64, max_buffer_size: usize) -> Self {
        Self {
            system: DefaultXGSystemEffect::new(DefaultXGEffect::new(
                chorus_map(sample_rate, max_buffer_size),
                Box::new(XGNoEffect),
                0x41_00,
                sample_rate,
                max_buffer_size,
            )),
            send_to_reverb: 0,
        }
    }

    pub fn reset(&mut self) {
        self.system.reset();
        self.send_to_reverb = 0;
    }

    /// Process the effect and **adds** it to the output.
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        reverb_left: &mut [f32],
        reverb_right: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        self.system.effect.process_processor(input_left, input_right, sample_count, false);
        self.system.mix_system_effect(output_left, output_right, start_index, sample_count);
        // Chorus gets sent to reverb regardless of return,
        // Even at 0 and part having only chorus applied,
        // The reverb still sounds with chorus to reverb being 127.
        // Tested on MU2K

        // Common scenario
        if self.send_to_reverb == 0 {
            return;
        }
        let gain = self.send_to_reverb as f64 / 64.0;

        let out_l = &self.system.effect.output_left;
        let out_r = &self.system.effect.output_right;
        for i in 0..sample_count {
            reverb_left[i] = (reverb_left[i] as f64 + out_l[i] as f64 * gain) as f32;
            reverb_right[i] = (reverb_right[i] as f64 + out_r[i] as f64 * gain) as f32;
        }
    }
}
