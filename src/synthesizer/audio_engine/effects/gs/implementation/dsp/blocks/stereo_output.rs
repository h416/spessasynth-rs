//! The output of most stereo types: balance, the 2-band EQ per side and the output gain.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/stereo_output.ts

use super::shelf_eq::{ShelfEq, shelf_corners};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::glue::{
    balance_dry, balance_wet, shelf_gain,
};

/// The output of most stereo types: dry and wet mixed by the balance, then the 2-band EQ (200 Hz and 4 kHz shelves)
/// per side and the output gain.
pub struct StereoOutput {
    pub left: f64,
    pub right: f64,
    shelves_l: ShelfEq,
    shelves_r: ShelfEq,
    dry: f64,
    wet: f64,
    gain: f64,
}

impl StereoOutput {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            shelves_l: ShelfEq::new(sample_rate),
            shelves_r: ShelfEq::new(sample_rate),
            dry: 0.0,
            wet: 0.0,
            gain: 0.0,
        }
    }

    /// `balance` (0 dry, 127 wet), the low and high gain parameters and the output gain
    /// (the level and the stage's gains).
    pub fn set(&mut self, balance: u8, low: u8, high: u8, gain: f64) {
        self.dry = balance_dry(balance);
        self.wet = balance_wet(balance);
        self.shelves_l.set_low(shelf_corners::LOW200, shelf_gain(low));
        self.shelves_l.set_high(shelf_corners::HIGH4K, shelf_gain(high));
        self.shelves_r.set_low(shelf_corners::LOW200, shelf_gain(low));
        self.shelves_r.set_high(shelf_corners::HIGH4K, shelf_gain(high));
        self.gain = gain;
    }

    /// Mixes one sample into `left` and `right`.
    #[inline]
    pub fn process(&mut self, dry_l: f64, wet_l: f64, dry_r: f64, wet_r: f64) {
        let (dry, wet, gain) = (self.dry, self.wet, self.gain);
        self.left = self.shelves_l.process(dry * dry_l + wet * wet_l) * gain;
        self.right = self.shelves_r.process(dry * dry_r + wet * wet_r) * gain;
    }

    pub fn clear(&mut self) {
        self.shelves_l.clear();
        self.shelves_r.clear();
        self.left = 0.0;
        self.right = 0.0;
    }
}
