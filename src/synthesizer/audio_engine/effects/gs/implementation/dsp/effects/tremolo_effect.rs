//! Tremolo and Auto Pan: per channel `DC blocker -> LFO gain -> EQ`.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/tremolo_effect.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::shelf_eq::{ShelfEq, shelf_corners};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::tremolo::Tremolo;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::rate1;

const LIMITS: &[(u8, u8)] = &[(0, 4)];

/// Tremolo and Auto Pan: per channel `DC blocker -> LFO gain -> EQ`.
/// Parameters: 1 wave, 2 rate (Rate1 column), 3 depth, 17 low gain, 18 high gain, 20 level. EFX control source 1
/// modulates the rate, source 2 the depth.
pub struct GSTremoloEffect {
    base: GSEffectBase,
    effect_type: u16,
    tremolo: Tremolo,
    dc_blocker_l: FirstOrderFilter,
    dc_blocker_r: FirstOrderFilter,
    shelves_l: ShelfEq,
    shelves_r: ShelfEq,
    gain: f64,
}

impl GSTremoloEffect {
    /// `pan` is true for auto pan. The subclass constructor resets.
    pub fn new(sample_rate: f64, effect_type: u16, defaults: [u8; 20], pan: bool) -> Self {
        let mut tremolo = Tremolo::new(sample_rate);
        tremolo.set_pan(pan);
        let mut dc_blocker_l = FirstOrderFilter::new(sample_rate);
        dc_blocker_l.set_high_pass(DC_BLOCKER_HZ, 1.0);
        let mut dc_blocker_r = FirstOrderFilter::new(sample_rate);
        dc_blocker_r.set_high_pass(DC_BLOCKER_HZ, 1.0);
        Self {
            base: GSEffectBase::new(sample_rate, None, defaults, LIMITS, &[]),
            effect_type,
            tremolo,
            dc_blocker_l,
            dc_blocker_r,
            shelves_l: ShelfEq::new(sample_rate),
            shelves_r: ShelfEq::new(sample_rate),
            gain: 0.0,
        }
    }
}

impl GSEffect for GSTremoloEffect {
    gs_effect_common!(dynamic);

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
            self.tremolo.process();
            let l = self.dc_blocker_l.process(input_left[i] as f64) * self.tremolo.left;
            let r = self.dc_blocker_r.process(input_right[i] as f64) * self.tremolo.right;
            output_left[i] = (self.shelves_l.process(l) * gain) as f32;
            output_right[i] = (self.shelves_r.process(r) * gain) as f32;
        }
    }

    fn clear(&mut self) {
        self.tremolo.clear();
        self.dc_blocker_l.clear();
        self.dc_blocker_r.clear();
        self.shelves_l.clear();
        self.shelves_r.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.tremolo.set_wave(p[0]);
        self.tremolo.set_rate(rate1(self.base.controlled(0, 1)));
        self.tremolo.set_depth(level_gain(self.base.controlled(1, 2)));
        self.shelves_l.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves_l.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        self.shelves_r.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves_r.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        // The input and the gain stage each have one more 127/128 than the output stage
        self.gain = level_gain(p[19]) * STAGE_GAIN * ((127.0 / 128.0) * (127.0 / 128.0));
    }
}
