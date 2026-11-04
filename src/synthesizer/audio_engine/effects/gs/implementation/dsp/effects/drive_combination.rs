//! OD and DS -> Chorus, Flanger, Delay: `L+R -> drive -> amp simulator -> pan`, then the second stage.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/drive_combination.ts

use super::combination_stage::{CombinationStage, create_combination_stage};
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::amp_simulator::AmpSimulator;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::drive::{Drive, DrivePreset};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::output_lag::OutputLag;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::stereo_output::StereoOutput;

const LIMITS: &[(u8, u8)] = &[(2, 3), (3, 1)];

/// OD and DS -> Chorus, Flanger, Delay: `L+R -> drive -> amp simulator -> pan`, the panned drive is the dry signal
/// and its mono sum feeds the chorus, flanger or delay. Then the balance and the 2-band EQ per side.
/// Parameters: 1 drive, 2 drive pan, 3 amp type, 4 amp switch, 6-9 the second stage, 10 balance, 17 low gain,
/// 18 high gain, 20 level. EFX control source 1 modulates the drive pan, source 2 the balance.
///
/// Like the other mono input types the right output comes one 32 kHz sample after the left one.
pub struct GSDriveCombination {
    base: GSEffectBase,
    effect_type: u16,
    preset: &'static DrivePreset,
    mix: MonoMix,
    drive: Drive,
    amp: AmpSimulator,
    stage: Box<dyn CombinationStage>,
    output: StereoOutput,
    lag: OutputLag,
    amp_on: bool,
    pan_l: f64,
    pan_r: f64,
    line_gain: f64,
}

impl GSDriveCombination {
    /// `stage` is the second stage. The subclass constructor resets.
    pub fn new(
        sample_rate: f64,
        memory: &mut DelayMemory,
        effect_type: u16,
        defaults: [u8; 20],
        preset: &'static DrivePreset,
        stage: u8,
    ) -> Self {
        let base = GSEffectBase::new(sample_rate, Some(memory), defaults, LIMITS, &[]);
        // The dry signal is the drive's output, as late as the line's input, so the reads take nothing off
        let stage = create_combination_stage(stage, sample_rate, memory, 0);
        Self {
            base,
            effect_type,
            preset,
            mix: MonoMix::new(sample_rate),
            drive: Drive::new(sample_rate),
            amp: AmpSimulator::new(sample_rate),
            stage,
            output: StereoOutput::new(sample_rate),
            lag: OutputLag::new(sample_rate, 1.0),
            amp_on: false,
            pan_l: 0.0,
            pan_r: 0.0,
            line_gain: 0.0,
        }
    }
}

impl GSEffect for GSDriveCombination {
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
            let x = self.mix.process(input_left[i] as f64, input_right[i] as f64) * MONO_INPUT;
            let mut y = self.drive.process(x);
            // The amp runs when it is switched off too, so switching it on does not click
            let amp = self.amp.process(y);
            if self.amp_on {
                y = amp;
            }
            self.stage.process(y * self.line_gain);
            self.output
                .process(self.pan_l * y, self.stage.left(), self.pan_r * y, self.stage.right());
            output_left[i] = self.output.left as f32;
            output_right[i] = self.lag.process(self.output.right) as f32;
        }
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.drive.clear();
        self.amp.clear();
        self.stage.clear();
        self.output.clear();
        self.lag.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.drive.set_preset(self.preset);
        self.drive.set_drive(p[0]);
        self.amp.set_type(p[2]);
        self.amp_on = p[3] == 1;
        let pan = self.base.controlled(0, 1);
        self.pan_l = pan_left(pan);
        self.pan_r = pan_right(pan);
        // The line takes the mono sum of the panned drive
        self.line_gain = ((self.pan_l + self.pan_r) / 2.0) * LINE_INPUT;
        self.stage.update(&p, 5, None);
        self.output.set(
            self.base.controlled(1, 9),
            p[16],
            p[17],
            level_gain(p[19]) * STAGE_OUTPUT,
        );
    }
}
