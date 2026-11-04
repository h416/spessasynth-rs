//! Equalizer (`01 00`): a stereo 4-band EQ.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/equalizer.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::peaking_band::PeakingBand;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::shelf_eq::{ShelfEq, shelf_corners};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x01, 0x45, 0x01, 0x34, 0x48, 0x00, 0x48, 0x38, 0x00, 0x38, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7f,
];
const LIMITS: &[(u8, u8)] = &[(0, 1), (2, 1), (5, 4), (8, 4)];

/// Equalizer (`01 00`): a stereo 4-band EQ. Each channel runs the low and high shelves and two peaking bands.
/// Parameters: 1 low frequency (200/400 Hz), 2 low gain, 3 high frequency (4k/8k Hz), 4 high gain, 5-7 mid 1
/// frequency, Q and gain, 8-10 mid 2, 20 level. EFX control source 1 modulates the level.
pub struct GSEqualizer {
    base: GSEffectBase,
    shelves_l: ShelfEq,
    shelves_r: ShelfEq,
    mid1_l: PeakingBand,
    mid1_r: PeakingBand,
    mid2_l: PeakingBand,
    mid2_r: PeakingBand,
    gain: f64,
}

impl GSEqualizer {
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> Self {
        let mut effect = Self {
            base: GSEffectBase::new(sample_rate, None, DEFAULTS, LIMITS, &[]),
            shelves_l: ShelfEq::new(sample_rate),
            shelves_r: ShelfEq::new(sample_rate),
            mid1_l: PeakingBand::new(sample_rate),
            mid1_r: PeakingBand::new(sample_rate),
            mid2_l: PeakingBand::new(sample_rate),
            mid2_r: PeakingBand::new(sample_rate),
            gain: 0.0,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSEqualizer {
    gs_effect_common!(0x01_00);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let gain = self.gain;
        for i in 0..sample_count {
            let l = self
                .mid2_l
                .process(self.mid1_l.process(self.shelves_l.process(input_left[i] as f64)));
            let r = self
                .mid2_r
                .process(self.mid1_r.process(self.shelves_r.process(input_right[i] as f64)));
            output_left[i] = (l * gain) as f32;
            output_right[i] = (r * gain) as f32;
        }
    }

    fn clear(&mut self) {
        self.shelves_l.clear();
        self.shelves_r.clear();
        self.mid1_l.clear();
        self.mid1_r.clear();
        self.mid2_l.clear();
        self.mid2_r.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        let low = if p[0] != 0 { shelf_corners::LOW400 } else { shelf_corners::LOW200 };
        let high = if p[2] != 0 { shelf_corners::HIGH8K } else { shelf_corners::HIGH4K };
        self.shelves_l.set_low(low, shelf_gain(p[1]));
        self.shelves_l.set_high(high, shelf_gain(p[3]));
        self.shelves_r.set_low(low, shelf_gain(p[1]));
        self.shelves_r.set_high(high, shelf_gain(p[3]));
        self.mid1_l.set_parameters(p[4], p[5], p[6]);
        self.mid1_r.set_parameters(p[4], p[5], p[6]);
        self.mid2_l.set_parameters(p[7], p[8], p[9]);
        self.mid2_r.set_parameters(p[7], p[8], p[9]);
        // The EQ input has one more 127/128 than the output stage
        self.gain = (level_gain(self.base.controlled(0, 19)) * STAGE_GAIN * 127.0) / 128.0;
    }
}
