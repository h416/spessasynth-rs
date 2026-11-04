//! The "EQ" box of the manuals: a first order low shelf, then a first order high shelf.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/shelf_eq.ts

use super::first_order_filter::FirstOrderFilter;

/// The corners of the shelves. The labels are what the manuals print, the values are where the hardware's corners
/// really are (the low shelves sit above their labels).
/// Equivalent to: ShelfCorners
pub mod shelf_corners {
    pub const LOW200: f64 = 224.5;
    pub const LOW400: f64 = 425.0;
    pub const HIGH4K: f64 = 3969.0;
    pub const HIGH8K: f64 = 8012.0;
}

/// The "EQ" box of the manuals: a first order low shelf, then a first order high shelf, each -12 to +12 dB.
/// At 0 dB both are exactly unity.
#[derive(Clone)]
pub struct ShelfEq {
    low: FirstOrderFilter,
    high: FirstOrderFilter,
}

impl ShelfEq {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            low: FirstOrderFilter::new(sample_rate),
            high: FirstOrderFilter::new(sample_rate),
        }
    }

    /// `frequency` is the corner in Hz (`shelf_corners`).
    pub fn set_low(&mut self, frequency: f64, gain_db: f64) {
        self.low.set_low_shelf(frequency, gain_db);
    }

    /// `frequency` is the corner in Hz (`shelf_corners`).
    pub fn set_high(&mut self, frequency: f64, gain_db: f64) {
        self.high.set_high_shelf(frequency, gain_db);
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let low = self.low.process(x);
        self.high.process(low)
    }

    pub fn clear(&mut self) {
        self.low.clear();
        self.high.clear();
    }
}
