//! Overdrive and Distortion: `L+R -> drive -> amp simulator -> EQ -> pan`.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/drive_effect.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::amp_simulator::AmpSimulator;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::drive::{Drive, DrivePreset};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_pan::MonoPan;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::shelf_eq::{ShelfEq, shelf_corners};

const LIMITS: &[(u8, u8)] = &[(1, 3), (2, 1)];

/// Overdrive and Distortion: `L+R -> drive -> amp simulator -> EQ -> pan`.
/// Parameters: 1 drive, 2 amp type, 3 amp switch, 17 low gain, 18 high gain, 19 pan, 20 level.
/// EFX control source 1 modulates the drive, source 2 the pan.
pub struct GSDriveEffect {
    base: GSEffectBase,
    effect_type: u16,
    mix: MonoMix,
    drive: Drive,
    amp: AmpSimulator,
    shelves: ShelfEq,
    pan: MonoPan,
    amp_on: bool,
}

impl GSDriveEffect {
    /// `preset` is the type's drive preset. The subclass constructor resets.
    pub fn new(
        sample_rate: f64,
        effect_type: u16,
        defaults: [u8; 20],
        preset: &'static DrivePreset,
    ) -> Self {
        let mut drive = Drive::new(sample_rate);
        drive.set_preset(preset);
        Self {
            base: GSEffectBase::new(sample_rate, None, defaults, LIMITS, &[]),
            effect_type,
            mix: MonoMix::new(sample_rate),
            drive,
            amp: AmpSimulator::new(sample_rate),
            shelves: ShelfEq::new(sample_rate),
            pan: MonoPan::new(sample_rate),
            amp_on: false,
        }
    }
}

impl GSEffect for GSDriveEffect {
    gs_effect_common!(dynamic);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        for i in 0..sample_count {
            let mut y = self
                .drive
                .process(self.mix.process(input_left[i] as f64, input_right[i] as f64) * MONO_INPUT);
            // The amp runs when it is switched off too, like on the hardware, so switching it on does not click
            let amp = self.amp.process(y);
            if self.amp_on {
                y = amp;
            }
            self.pan.process(self.shelves.process(y));
            output_left[i] = self.pan.left as f32;
            output_right[i] = self.pan.right as f32;
        }
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.drive.clear();
        self.amp.clear();
        self.shelves.clear();
        self.pan.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.drive.set_drive(self.base.controlled(0, 0));
        self.amp.set_type(p[1]);
        self.amp_on = p[2] == 1;
        self.shelves.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        self.pan.set(self.base.controlled(1, 18), level_gain(p[19]) * STAGE_OUTPUT);
    }
}
