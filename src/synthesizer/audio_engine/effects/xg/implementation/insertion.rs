//! The default implementation of an XG insertion block.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/insertion.ts

use super::dsp::chorus::{
    celeste1, celeste2, celeste3, celeste4, chorus1, chorus2, chorus3, chorus4, flanger1,
    flanger2, flanger3,
};
use super::dsp::delay::{cross_delay, delay_lcr, delay_lr, echo};
use super::dsp::distortion::{amp_simulator, distortion, overdrive};
use super::dsp::equalizer::three_band_eq::XGThreeBandEQ;
use super::dsp::equalizer::two_band_eq::XGTwoBandEQ;
use super::dsp::effect_processor::DefaultXGEffectProcessor;
use super::dsp::rotary::rotary_speaker::XGRotarySpeaker;
use super::dsp::phaser::phaser1;
use super::dsp::reverb::{hall1, hall2, plate, room1, room2, room3, stage1, stage2};
use super::dsp::symphonic::symphonic::XGSymphonic;
use super::dsp::thru::XGThru;
use super::dsp::tremolo::{auto_pan, tremolo};
use super::effect::DefaultXGEffect;

/// Equivalent to: INSERTION_MAP
fn insertion_map(sr: f64, buf: usize) -> Vec<(u16, Box<dyn DefaultXGEffectProcessor>)> {
    vec![
        (0x00_00, Box::new(XGThru)),
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
        // TODO(XG effects): 0x1400-0x1402 KARAOKE 1-3
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
        // The XG spec's EFFECT 2 list only has PHASER 1.
        // PHASER 2 (48 08) is variation only and falls back to PHASER 1 here.
        (0x48_00, Box::new(phaser1::new(sr, buf))),
        (0x49_00, Box::new(distortion::new(sr, buf))),
        (0x4a_00, Box::new(overdrive::new(sr, buf))),
        (0x4b_00, Box::new(amp_simulator::new(sr, buf))),
        (0x4c_00, Box::new(XGThreeBandEQ::new(sr, buf))),
        (0x4d_00, Box::new(XGTwoBandEQ::new(sr, buf))),
        // TODO(XG effects): 0x4e00 AUTO WAH
    ]
}

/// The default implementation for XGInsertionBlock.
/// Equivalent to: class DefaultXGInsertion
pub struct DefaultXGInsertion {
    pub effect: DefaultXGEffect,
    /// The channel routed through the insertion effect, 0-based.
    /// 127 or a value that's above the current channel count means OFF.
    pub part_number: i32,
}

impl DefaultXGInsertion {
    /// Constructs a new default XG insertion processor.
    pub fn new(sample_rate: f64, max_buffer_size: usize) -> Self {
        Self {
            effect: DefaultXGEffect::new(
                insertion_map(sample_rate, max_buffer_size),
                Box::new(XGThru),
                0x49_00,
                sample_rate,
                max_buffer_size,
            ),
            part_number: 127,
        }
    }

    pub fn reset(&mut self) {
        self.effect.reset();
        self.part_number = 127;
    }

    /// Process the effect in insertion mode and **overwrites** it to the input.
    /// The DSP honors Dry/Wet in this case.
    pub fn process_insertion(
        &mut self,
        input_left: &mut [f32],
        input_right: &mut [f32],
        sample_count: usize,
    ) {
        self.effect.process_processor(input_left, input_right, sample_count, true);
        // Copy without creating subarray views on the audio thread
        input_left[..sample_count].copy_from_slice(&self.effect.output_left[..sample_count]);
        input_right[..sample_count].copy_from_slice(&self.effect.output_right[..sample_count]);
    }
}
