//! The clean GS insertion (EFX) types: the blocks, the shared tables and the 64 SC-88Pro types plus Thru.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp

pub mod blocks;
pub mod conversion;
pub mod effect_list;
pub mod effects;
pub mod enums;
pub mod glue;
pub mod gs_effect;
pub mod insertion_processor;
pub mod peaking_table;

/// `Math.round`: half-up, unlike `f64::round` (half away from zero).
#[inline]
pub(crate) fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// `Math.fround`: the nearest single precision value.
#[inline]
pub(crate) fn fround(x: f64) -> f64 {
    x as f32 as f64
}
