//! The default implementation of the XG variation block.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/variation.ts

use super::dsp::auto_wah::auto_wah::XGAutoWah;
use super::dsp::chorus::{
    celeste1, celeste2, celeste3, celeste4, chorus1, chorus2, chorus3, chorus4, flanger1,
    flanger2, flanger3,
};
use super::dsp::delay::{cross_delay, delay_lcr, delay_lr, echo};
use super::dsp::distortion::{amp_simulator, distortion, overdrive};
use super::dsp::equalizer::three_band_eq::XGThreeBandEQ;
use super::dsp::equalizer::two_band_eq::XGTwoBandEQ;
use super::dsp::early_reflections::{early_reflection1, early_reflection2, gate_reverb, reverse_gate};
use super::dsp::effect_processor::DefaultXGEffectProcessor;
use super::dsp::karaoke::{karaoke1, karaoke2, karaoke3};
use super::dsp::no_effect::XGNoEffect;
use super::dsp::rotary::rotary_speaker::XGRotarySpeaker;
use super::dsp::phaser::{phaser1, phaser2};
use super::dsp::reverb::{hall1, hall2, plate, room1, room2, room3, stage1, stage2};
use super::dsp::symphonic::symphonic::XGSymphonic;
use super::dsp::thru::XGThru;
use super::dsp::tremolo::{auto_pan, tremolo};
use super::effect::DefaultXGEffect;
use super::system_effect::DefaultXGSystemEffect;

/// Equivalent to: VARIATION_MAP
fn variation_map(sr: f64, buf: usize) -> Vec<(u16, Box<dyn DefaultXGEffectProcessor>)> {
    vec![
        (0x00_00, Box::new(XGNoEffect)),
        (0x01_00, Box::new(hall1::new(sr, buf, true))),
        (0x01_01, Box::new(hall2::new(sr, buf, true))),
        (0x02_00, Box::new(room1::new(sr, buf, true))),
        (0x02_01, Box::new(room2::new(sr, buf, true))),
        (0x02_02, Box::new(room3::new(sr, buf, true))),
        (0x03_00, Box::new(stage1::new(sr, buf, true))),
        (0x03_01, Box::new(stage2::new(sr, buf, true))),
        (0x04_00, Box::new(plate::new(sr, buf, true))),
        (0x05_00, Box::new(delay_lcr::new(sr, buf))),
        (0x06_00, Box::new(delay_lr::new(sr, buf))),
        (0x07_00, Box::new(echo::new(sr, buf))),
        (0x08_00, Box::new(cross_delay::new(sr, buf))),
        (0x09_00, Box::new(early_reflection1::new(sr, buf))),
        (0x09_01, Box::new(early_reflection2::new(sr, buf))),
        (0x0a_00, Box::new(gate_reverb::new(sr, buf))),
        (0x0b_00, Box::new(reverse_gate::new(sr, buf))),
        (0x14_00, Box::new(karaoke1::new(sr, buf))),
        (0x14_01, Box::new(karaoke2::new(sr, buf))),
        (0x14_02, Box::new(karaoke3::new(sr, buf))),
        (0x40_00, Box::new(XGThru)),
        (0x41_00, Box::new(chorus1::new(sr, buf, true))),
        (0x41_01, Box::new(chorus2::new(sr, buf, true))),
        (0x41_02, Box::new(chorus3::new(sr, buf, true))),
        (0x41_08, Box::new(chorus4::new(sr, buf, true))),
        (0x42_00, Box::new(celeste1::new(sr, buf, true))),
        (0x42_01, Box::new(celeste2::new(sr, buf, true))),
        (0x42_02, Box::new(celeste3::new(sr, buf, true))),
        (0x42_08, Box::new(celeste4::new(sr, buf, true))),
        (0x43_00, Box::new(flanger1::new(sr, buf, true))),
        (0x43_01, Box::new(flanger2::new(sr, buf, true))),
        (0x43_08, Box::new(flanger3::new(sr, buf, true))),
        (0x44_00, Box::new(XGSymphonic::new(sr, buf))),
        (0x45_00, Box::new(XGRotarySpeaker::new(sr, buf))),
        (0x46_00, Box::new(tremolo::new(sr, buf))),
        (0x47_00, Box::new(auto_pan::new(sr, buf))),
        (0x48_00, Box::new(phaser1::new(sr, buf))),
        (0x48_08, Box::new(phaser2::new(sr, buf))),
        (0x49_00, Box::new(distortion::new(sr, buf))),
        (0x4a_00, Box::new(overdrive::new(sr, buf))),
        (0x4b_00, Box::new(amp_simulator::new(sr, buf))),
        (0x4c_00, Box::new(XGThreeBandEQ::new(sr, buf))),
        (0x4d_00, Box::new(XGTwoBandEQ::new(sr, buf))),
        (0x4e_00, Box::new(XGAutoWah::new(sr, buf))),
    ]
}

/// The default implementation for XGVariationBlock.
/// Equivalent to: class DefaultXGVariation
pub struct DefaultXGVariation {
    pub system: DefaultXGSystemEffect,
    /// Indicates if variation is in the insertion mode (CONNECTION).
    /// If `false`, then variation is in the `system` mode.
    pub insertion_mode: bool,
    /// The channel routed through the insertion effect, 0-based.
    /// 127 or a value that's above the current channel count means OFF.
    pub part_number: i32,
    /// The amount of variation being sent to the reverb effect (system mode only).
    /// 0 is none, 64 is 100% and 127 is 200%.
    pub send_to_reverb: i32,
    /// The amount of variation being sent to the chorus effect (system mode only).
    /// 0 is none, 64 is 100% and 127 is 200%.
    pub send_to_chorus: i32,
}

impl DefaultXGVariation {
    /// Constructs a new default XG variation processor.
    pub fn new(sample_rate: f64, max_buffer_size: usize) -> Self {
        Self {
            system: DefaultXGSystemEffect::new(DefaultXGEffect::new(
                variation_map(sample_rate, max_buffer_size),
                Box::new(XGThru),
                0x05_00,
                sample_rate,
                max_buffer_size,
            )),
            insertion_mode: true,
            part_number: 127,
            send_to_reverb: 0,
            send_to_chorus: 0,
        }
    }

    pub fn reset(&mut self) {
        self.system.reset();
        self.insertion_mode = true;
        self.part_number = 127;
        self.send_to_reverb = 0;
        self.send_to_chorus = 0;
    }

    /// Process the effect in _system_ mode and **adds** it to the output.
    /// Feeds the chorus and reverb buffers according to the send amounts.
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        chorus_left: &mut [f32],
        chorus_right: &mut [f32],
        reverb_left: &mut [f32],
        reverb_right: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        self.system.effect.process_processor(input_left, input_right, sample_count, false);
        self.system.mix_system_effect(output_left, output_right, start_index, sample_count);

        // Variation gets sent to chorus/reverb regardless of return,
        // For example, even at 0 and part having only variation applied,
        // The reverb still sounds with variation to reverb being 127.
        let out_l = &self.system.effect.output_left;
        let out_r = &self.system.effect.output_right;

        // Common scenario
        if self.send_to_reverb > 0 {
            let gain = self.send_to_reverb as f64 / 64.0;
            for i in 0..sample_count {
                reverb_left[i] = (reverb_left[i] as f64 + out_l[i] as f64 * gain) as f32;
                reverb_right[i] = (reverb_right[i] as f64 + out_r[i] as f64 * gain) as f32;
            }
        }
        if self.send_to_chorus > 0 {
            let gain = self.send_to_chorus as f64 / 64.0;
            for i in 0..sample_count {
                chorus_left[i] = (chorus_left[i] as f64 + out_l[i] as f64 * gain) as f32;
                chorus_right[i] = (chorus_right[i] as f64 + out_r[i] as f64 * gain) as f32;
            }
        }
    }

    /// Process the effect in insertion mode and **overwrites** it to the input.
    pub fn process_insertion(
        &mut self,
        input_left: &mut [f32],
        input_right: &mut [f32],
        sample_count: usize,
    ) {
        self.system.effect.process_processor(input_left, input_right, sample_count, true);
        // In insertion mode, the effect output replaces the part's signal (no sends)
        // See MU128 manual page 154
        input_left[..sample_count]
            .copy_from_slice(&self.system.effect.output_left[..sample_count]);
        input_right[..sample_count]
            .copy_from_slice(&self.system.effect.output_right[..sample_count]);
    }
}
