//! Humanizer (`01 03`): `L+R -> drive (switchable) -> formant filter -> EQ -> pan`.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/humanizer.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::drive::{Drive, HUMANIZER_DRIVE};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::formant_filter::FormantFilter;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_pan::MonoPan;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::shelf_eq::{ShelfEq, shelf_corners};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::slew_time;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x30, 0x01, 0x00, 0x7f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x40, 0x7f,
];
const LIMITS: &[(u8, u8)] = &[(1, 1), (2, 4)];

/// Humanizer (`01 03`): `L+R -> drive (switchable) -> formant filter -> EQ -> pan`.
/// Parameters: 1 drive, 2 drive switch, 3 vowel (a, i, u, e, o), 4 accel, 17 low gain, 18 high gain, 19 pan,
/// 20 level. EFX control source 1 moves the vowel (up to 4 vowels either way), source 2 the level.
pub struct GSHumanizer {
    base: GSEffectBase,
    mix: MonoMix,
    drive: Drive,
    /// The drive's own DC blocker, for the path around it when the drive is off.
    dc_blocker: FirstOrderFilter,
    formants: FormantFilter,
    shelves: ShelfEq,
    pan: MonoPan,
    drive_on: bool,
}

impl GSHumanizer {
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> Self {
        let mut drive = Drive::new(sample_rate);
        drive.set_preset(&HUMANIZER_DRIVE);
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        let mut effect = Self {
            base: GSEffectBase::new(sample_rate, None, DEFAULTS, LIMITS, &[]),
            mix: MonoMix::new(sample_rate),
            drive,
            dc_blocker,
            formants: FormantFilter::new(sample_rate),
            shelves: ShelfEq::new(sample_rate),
            pan: MonoPan::new(sample_rate),
            drive_on: false,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSHumanizer {
    gs_effect_common!(0x01_03);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        for i in 0..sample_count {
            let x = self.mix.process(input_left[i] as f64, input_right[i] as f64) * MONO_INPUT;
            // Both paths run, so the switch does not click
            let driven = self.drive.process(x);
            let dry = self.dc_blocker.process(x);
            let y = self.formants.process(if self.drive_on { driven } else { dry });
            self.pan.process(self.shelves.process(y));
            output_left[i] = self.pan.left as f32;
            output_right[i] = self.pan.right as f32;
        }
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.drive.clear();
        self.dc_blocker.clear();
        self.formants.clear();
        self.shelves.clear();
        self.pan.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.drive.set_drive(p[0]);
        self.drive_on = p[1] == 1;
        self.formants.set_accel(slew_time(p[3]));
        // Math.trunc((offset * 5) / 128): integer division truncates toward zero too
        let shift = (self.base.control_offset(0) as i32 * 5) / 128;
        self.formants
            .set_vowel(0i32.max(4i32.min(p[2] as i32 + shift)) as u8);
        self.shelves.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        // The drive switch has one more 127/128
        self.pan.set(
            p[18],
            (level_gain(self.base.controlled(1, 19)) * STAGE_OUTPUT * 127.0) / 128.0,
        );
    }
}
