//! Spectrum (`01 01`): a mono 8-band graphic EQ, panned.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/spectrum.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_pan::MonoPan;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::peaking_band::{PeakingBand, peaking_gain};

/// The peaking tables of the eight bands: 250, 500, 1000, 1250, 2000, 3150, 4000 and 8000 Hz.
const BAND_TABLES: [usize; 8] = [1, 4, 7, 8, 10, 12, 13, 16];

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x3c, 0x41, 0x43, 0x46, 0x42, 0x3f, 0x3c, 0x3b, 0x02, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x7f,
];
const LIMITS: &[(u8, u8)] = &[(8, 4)];

/// Spectrum (`01 01`): a mono 8-band graphic EQ, panned.
/// Parameters: 1-8 band gains, 9 Q, 19 pan, 20 level. EFX control source 1 modulates the pan, source 2 the level.
pub struct GSSpectrum {
    base: GSEffectBase,
    mix: MonoMix,
    bands: Vec<PeakingBand>,
    pan: MonoPan,
}

impl GSSpectrum {
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> Self {
        let mut effect = Self {
            base: GSEffectBase::new(sample_rate, None, DEFAULTS, LIMITS, &[]),
            mix: MonoMix::new(sample_rate),
            bands: BAND_TABLES.iter().map(|_| PeakingBand::new(sample_rate)).collect(),
            pan: MonoPan::new(sample_rate),
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSSpectrum {
    gs_effect_common!(0x01_01);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        for i in 0..sample_count {
            let mut x = self.mix.process(input_left[i] as f64, input_right[i] as f64);
            for band in &mut self.bands {
                x = band.process(x);
            }
            self.pan.process(x);
            output_left[i] = self.pan.left as f32;
            output_right[i] = self.pan.right as f32;
        }
    }

    fn clear(&mut self) {
        self.mix.clear();
        for band in &mut self.bands {
            band.clear();
        }
        self.pan.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        for (b, band) in self.bands.iter_mut().enumerate() {
            band.set_entry(BAND_TABLES[b], p[8] as usize, peaking_gain(p[b]));
        }
        self.pan.set(
            self.base.controlled(0, 18),
            level_gain(self.base.controlled(1, 19)) * STAGE_GAIN,
        );
    }
}
