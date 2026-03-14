/// insertion/mod.rs
/// purpose: Insertion effect trait, module declarations, and factory function.
/// Ported from: src/synthesizer/audio_engine/effects/types.ts (InsertionProcessor interface)
///              src/synthesizer/audio_engine/effects/insertion_list.ts

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

/// Creates an insertion processor for the given EFX type, or None if unsupported.
pub fn create_insertion_processor(efx_type: u16, sample_rate: f64) -> Option<Box<dyn InsertionProcessor>> {
    match efx_type {
        0x0000 => Some(Box::new(ThruFx::new(sample_rate))),
        0x0100 => Some(Box::new(StereoEqFx::new(sample_rate))),
        0x0120 => Some(Box::new(PhaserFx::new(sample_rate))),
        0x0121 => Some(Box::new(AutoWahFx::new(sample_rate))),
        0x0125 => Some(Box::new(TremoloFx::new(sample_rate))),
        0x0126 => Some(Box::new(AutoPanFx::new(sample_rate))),
        0x1108 => Some(Box::new(PhAutoWahFx::new(sample_rate))),
        _ => None,
    }
}
