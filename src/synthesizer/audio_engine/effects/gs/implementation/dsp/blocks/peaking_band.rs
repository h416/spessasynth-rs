//! A constant Q peaking band of the hardware's EQs.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/peaking_band.ts

use super::biquad::Biquad;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::peaking_table::{
    PEAKING_TABLE, peaking_entry,
};

/// The gain index (0-24, -12 to +12 dB) of an EQ gain parameter (0x34-0x4c, clamped).
pub fn peaking_gain(value: u8) -> usize {
    (value as i32 - 0x34).clamp(0, 24) as usize
}

/// A constant Q peaking band of the hardware's EQs.
/// The width is given as the lower edge of the band in Hz, so the center and the lower skirt stay put at any rate.
/// TS `PeakingBand extends Biquad`; Rust wraps a `Biquad`.
#[derive(Clone)]
pub struct PeakingBand {
    pub biquad: Biquad,
}

impl PeakingBand {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            biquad: Biquad::new(sample_rate),
        }
    }

    /// `frequency` is the center in Hz, `lower_edge` the lower edge of the band in Hz (below the center),
    /// `gain_db` the gain at the center.
    pub fn set(&mut self, frequency: f64, lower_edge: f64, gain_db: f64) {
        let t = self.biquad.prewarp(frequency);
        // The edges sit geometrically around the center on the prewarped axis, and their distance is center / Q
        let edge = self.biquad.prewarp(lower_edge);
        self.biquad.set_peaking(frequency, t / ((t * t) / edge - edge), gain_db);
    }

    /// Sets one of the hardware's peaking entries (`PEAKING_TABLE`): the frequency table (0-16), the Q index (0-4)
    /// and the gain index (0-24, -12 to +12 dB).
    pub fn set_entry(&mut self, table: usize, q: usize, gain: usize) {
        let e = peaking_entry(table, q, gain);
        let t = &*PEAKING_TABLE;
        self.set(t[e] as f64, t[e + 1] as f64, t[e + 2] as f64);
    }

    /// Sets the band from its parameters: the EQ column value (one table every 8 values), the Q index (0-4) and
    /// the gain parameter (0x34-0x4c is -12 to +12 dB).
    pub fn set_parameters(&mut self, frequency: u8, q: u8, gain: u8) {
        self.set_entry((frequency >> 3) as usize, q as usize, peaking_gain(gain));
    }

    pub fn set_off(&mut self) {
        self.biquad.set_off();
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        self.biquad.process(x)
    }

    pub fn clear(&mut self) {
        self.biquad.clear();
    }
}
