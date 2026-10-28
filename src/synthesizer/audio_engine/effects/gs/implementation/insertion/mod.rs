//! insertion/mod.rs
//! purpose: Insertion effect trait, module declarations, and the list of insertion types.
//! Ported from: src/synthesizer/audio_engine/effects/gs/interface/gs_insertion_processor.ts
//!              src/synthesizer/audio_engine/effects/gs/implementation/insertion_list.ts

pub mod auto_pan;
pub mod auto_wah;
pub mod convert;
pub mod ph_auto_wah;
pub mod phaser;
pub mod stereo_eq;
pub mod thru;
pub mod tremolo;
pub mod utils;

use self::auto_pan::AutoPanFx;
use self::auto_wah::AutoWahFx;
use self::ph_auto_wah::PhAutoWahFx;
use self::phaser::PhaserFx;
use self::stereo_eq::StereoEqFx;
use self::thru::ThruFx;
use self::tremolo::TremoloFx;

/// Trait for insertion effect processors.
/// Equivalent to: InsertionProcessor interface in types.ts
pub trait InsertionProcessor {
    /// The EFX type of this processor (MSB << 8 | LSB).
    fn effect_type(&self) -> u16;

    /// Resets parameters to defaults (does not reset send levels).
    fn reset(&mut self);

    /// Sets an EFX parameter (0x03-0x16).
    fn set_parameter(&mut self, parameter: u8, value: u8);

    /// Process the effect: reads from zero-indexed stereo input buffers,
    /// ADDS to start_index-based stereo output buffers,
    /// and ADDS to zero-indexed mono effect send buffers.
    #[allow(clippy::too_many_arguments)]
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
    );

    fn send_level_to_reverb(&self) -> f64;
    fn send_level_to_chorus(&self) -> f64;
    fn send_level_to_delay(&self) -> f64;
    fn set_send_level_to_reverb(&mut self, value: f64);
    fn set_send_level_to_chorus(&mut self, value: f64);
    fn set_send_level_to_delay(&mut self, value: f64);
}

/// Creates one processor of every supported insertion type, in the TS list order.
/// Equivalent to: GS_INSERTION_EFFECT_LIST (each entry constructed by registerInsertionProcessor)
pub fn gs_insertion_effect_list(
    sample_rate: f64,
    max_buffer_size: usize,
) -> Vec<Box<dyn InsertionProcessor>> {
    vec![
        Box::new(ThruFx::new(sample_rate)),
        Box::new(StereoEqFx::new(sample_rate)),
        Box::new(PhaserFx::new(sample_rate)),
        Box::new(AutoPanFx::new(sample_rate)),
        Box::new(AutoWahFx::new(sample_rate)),
        Box::new(PhAutoWahFx::new(sample_rate, max_buffer_size)),
        Box::new(TremoloFx::new(sample_rate)),
    ]
}
