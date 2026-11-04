//! Reverb and Gate Reverb: `L+R -> reverb`, then per side the balance and the EQ.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/reverb_effect.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::shelf_eq::{ShelfEq, shelf_corners};

/// Reverb and Gate Reverb: `L+R -> reverb`, then per side the balance and the EQ.
/// The level goes in with the balance, in front of the EQ (not `StereoOutput`, which applies it after).
///
/// TS has this as an abstract class with the reverb block in it; the Rust types hold this and their own reverb
/// block, and clear both.
pub struct GSReverbEffect {
    pub base: GSEffectBase,
    pub mix: MonoMix,
    pub shelves_l: ShelfEq,
    pub shelves_r: ShelfEq,
    /// The balance gains with the level in them.
    pub dry: f64,
    pub wet: f64,
}

impl GSReverbEffect {
    pub fn new(
        sample_rate: f64,
        memory: &mut DelayMemory,
        defaults: [u8; 20],
        limits: &'static [(u8, u8)],
    ) -> Self {
        Self {
            base: GSEffectBase::new(sample_rate, Some(memory), defaults, limits, &[]),
            mix: MonoMix::new(sample_rate),
            shelves_l: ShelfEq::new(sample_rate),
            shelves_r: ShelfEq::new(sample_rate),
            dry: 0.0,
            wet: 0.0,
        }
    }

    /// Clears all but the reverb block.
    pub fn clear(&mut self) {
        self.mix.clear();
        self.shelves_l.clear();
        self.shelves_r.clear();
    }

    /// Sets the balance and the 2-band EQ.
    /// `balance` is the balance parameter, `gain` the output gain, `wet_gain` the wet signal's own gain (1 by
    /// default in TS).
    pub fn set_output(&mut self, balance: u8, gain: f64, wet_gain: f64) {
        let p = self.base.parameters;
        self.shelves_l.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves_l.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        self.shelves_r.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves_r.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        self.dry = balance_dry(balance) * gain;
        self.wet = balance_wet(balance) * gain * wet_gain;
    }
}
