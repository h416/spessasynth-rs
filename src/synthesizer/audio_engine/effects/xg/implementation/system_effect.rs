//! The default implementation of an XG system effect block.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/system_effect.ts

use super::effect::{DefaultXGEffect, XGEffectBlockSnapshot};
use std::sync::LazyLock;

const MIN_PAN: i32 = 1;
const MAX_PAN: i32 = 127;
const PAN_RESOLUTION: i32 = MAX_PAN - MIN_PAN;

/// Pan lookup tables (left, right).
static PAN_TABLES: LazyLock<([f32; 127], [f32; 127])> = LazyLock::new(|| {
    let mut left = [0.0f32; (PAN_RESOLUTION + 1) as usize];
    let mut right = [0.0f32; (PAN_RESOLUTION + 1) as usize];
    for pan in MIN_PAN..=MAX_PAN {
        // Clamp to 0-1
        let real_pan = (pan - MIN_PAN) as f64 / PAN_RESOLUTION as f64;
        let table_index = (pan - MIN_PAN) as usize;
        left[table_index] = (std::f64::consts::FRAC_PI_2 * real_pan).cos() as f32;
        right[table_index] = (std::f64::consts::FRAC_PI_2 * real_pan).sin() as f32;
    }
    (left, right)
});

/// A snapshot of an XG system effect block.
/// Equivalent to: interface XGSystemEffectBlockSnapshot
/// (extends XGSystemEffectParameter, XGEffectBlockSnapshot)
#[derive(Clone, Debug, PartialEq)]
pub struct XGSystemEffectBlockSnapshot {
    /// Equivalent to: XGEffectBlockSnapshot
    pub effect: XGEffectBlockSnapshot,
    /// Equivalent to: return
    pub r#return: i32,
    /// Equivalent to: pan
    pub pan: i32,
}

/// The default implementation for XGSystemEffectBlock.
///
/// System effects are global and have a send level for each channel, always adding wet output into them.
///
/// Equivalent to: abstract class DefaultXGSystemEffect
pub struct DefaultXGSystemEffect {
    pub effect: DefaultXGEffect,
    /// The return (level) of the effect. 0 is silence, 64 is 0 dB and 127 is +6 dB.
    pub r#return: i32,
    /// The stereo panning of this effect. 1 is hard left, 64 is center, 127 is hard right.
    pub pan: i32,
}

impl DefaultXGSystemEffect {
    pub fn new(effect: DefaultXGEffect) -> Self {
        Self { effect, r#return: 64, pan: 64 }
    }

    pub fn reset(&mut self) {
        self.effect.reset();
        self.r#return = 64;
        self.pan = 64;
    }

    pub fn get_snapshot(&self) -> XGSystemEffectBlockSnapshot {
        XGSystemEffectBlockSnapshot {
            effect: self.effect.get_snapshot(),
            r#return: self.r#return,
            pan: self.pan,
        }
    }

    pub fn apply_snapshot(&mut self, snapshot: &XGSystemEffectBlockSnapshot) {
        self.effect.apply_snapshot(&snapshot.effect);
        self.r#return = snapshot.r#return;
        self.pan = snapshot.pan;
    }

    /// Mixes the staged wet output into the system effect destination, applying return and pan.
    pub fn mix_system_effect(
        &self,
        output_left: &mut [f32],
        output_right: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        let gain = self.r#return as f64 / 64.0;
        let pan_index = (MAX_PAN.min(MIN_PAN.max(self.pan)) - MIN_PAN) as usize;
        let (table_left, table_right) = &*PAN_TABLES;
        let gain_left = table_left[pan_index] as f64 * gain;
        let gain_right = table_right[pan_index] as f64 * gain;

        for i in 0..sample_count {
            let out_index = start_index + i;
            output_left[out_index] =
                (output_left[out_index] as f64 + self.effect.output_left[i] as f64 * gain_left) as f32;
            output_right[out_index] = (output_right[out_index] as f64
                + self.effect.output_right[i] as f64 * gain_right) as f32;
        }
    }
}
