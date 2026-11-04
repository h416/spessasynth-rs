//! One effect of the parallel combinations with a drive, phaser, auto wah or rotary.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/parallel_stage.ts

use super::combination_stage::CombinationStage;
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::amp_simulator::AmpSimulator;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::drive::{Drive, GUITAR_DISTORTION_HIGH_PASS, GUITAR_OVERDRIVE};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::phaser::Phaser;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::rotary::Rotary;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::wah::Wah;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{modulation_depth, rate1, slew_time};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::enums::parallel_stages;

/// One effect of the parallel combinations with a drive, phaser, auto wah or rotary. One of its parameters is moved
/// by the effect's EFX control source (always passed to `update`).
/// Equivalent to: abstract class ParallelStage
pub trait ParallelStage: CombinationStage {
    /// The parameter the EFX control source moves, counted from the stage's first.
    fn target(&self) -> usize;

    /// The right output is one 32 kHz sample late already (the rotary's reads), else the type delays it.
    fn lagged(&self) -> bool {
        false
    }
}

const CONTROL: &str = "a parallel stage is updated with its control";

/// The drive with the amp simulator: type (OD, DS), drive (EFX control), amp type, amp switch.
struct DriveStage {
    left: f64,
    right: f64,
    drive: Drive,
    amp: AmpSimulator,
    amp_on: bool,
}

impl DriveStage {
    fn new(sample_rate: f64) -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            drive: Drive::new(sample_rate),
            amp: AmpSimulator::new(sample_rate),
            amp_on: false,
        }
    }
}

impl CombinationStage for DriveStage {
    fn left(&self) -> f64 {
        self.left
    }

    fn right(&self) -> f64 {
        self.right
    }

    fn update(&mut self, parameters: &[u8; 20], base: usize, control: Option<u8>) {
        self.drive.set_preset(if parameters[base] == 1 {
            &GUITAR_DISTORTION_HIGH_PASS
        } else {
            &GUITAR_OVERDRIVE
        });
        self.drive.set_drive(control.expect(CONTROL));
        self.amp.set_type(parameters[base + 2]);
        self.amp_on = parameters[base + 3] == 1;
    }

    #[inline]
    fn process(&mut self, x: f64) {
        let mut y = self.drive.process(x);
        // The amp runs when it is switched off too, so switching it on does not click
        let amp = self.amp.process(y);
        if self.amp_on {
            y = amp;
        }
        self.left = y;
        self.right = y;
    }

    fn clear(&mut self) {
        self.drive.clear();
        self.amp.clear();
    }
}

impl ParallelStage for DriveStage {
    fn target(&self) -> usize {
        1
    }
}

/// The phaser mixed with its input: manual, rate (Rate1 column, EFX control), depth, resonance, mix.
struct PhaserStage {
    left: f64,
    right: f64,
    phaser: Phaser,
    wet: f64,
}

impl PhaserStage {
    fn new(sample_rate: f64) -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            phaser: Phaser::new(sample_rate, 1),
            wet: 0.0,
        }
    }
}

impl CombinationStage for PhaserStage {
    fn left(&self) -> f64 {
        self.left
    }

    fn right(&self) -> f64 {
        self.right
    }

    fn update(&mut self, parameters: &[u8; 20], base: usize, control: Option<u8>) {
        let phaser = &mut self.phaser;
        phaser.set_manual(parameters[base]);
        phaser.set_rate(rate1(control.expect(CONTROL)));
        // The depth word is the chorus swing in ms over 16
        phaser.set_depth(modulation_depth(parameters[base + 2]) / 16.0);
        phaser.set_resonance(parameters[base + 3]);
        // The chain runs at 1/8 of the input and comes out x 8 the mix level
        self.wet = level_gain(parameters[base + 4]);
    }

    #[inline]
    fn process(&mut self, x: f64) {
        self.phaser.step();
        let y = x + self.wet * self.phaser.process(0, x);
        self.left = y;
        self.right = y;
    }

    fn clear(&mut self) {
        self.phaser.clear();
    }
}

impl ParallelStage for PhaserStage {
    fn target(&self) -> usize {
        1
    }
}

/// The auto wah: filter type (LPF, BPF), sens, manual (EFX control), peak, rate (Rate1 column), depth, polarity
/// (down, up).
struct WahStage {
    left: f64,
    right: f64,
    wah: Wah,
}

impl WahStage {
    fn new(sample_rate: f64) -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            wah: Wah::new(sample_rate),
        }
    }
}

impl CombinationStage for WahStage {
    fn left(&self) -> f64 {
        self.left
    }

    fn right(&self) -> f64 {
        self.right
    }

    fn update(&mut self, parameters: &[u8; 20], base: usize, control: Option<u8>) {
        let wah = &mut self.wah;
        wah.set_filter_type(parameters[base] == 1);
        wah.set_peak(parameters[base + 3]);
        wah.set_sensitivity(level_gain(parameters[base + 1]));
        wah.set_manual(level_gain(control.expect(CONTROL)));
        wah.set_rate(rate1(parameters[base + 4]));
        // The depth word is the chorus swing in ms over 16
        wah.set_depth(modulation_depth(parameters[base + 5]) / 16.0);
        wah.set_polarity(parameters[base + 6] == 1);
    }

    #[inline]
    fn process(&mut self, x: f64) {
        // The wah's output stage has a gain of 4
        let y = 4.0 * self.wah.process(x);
        self.left = y;
        self.right = y;
    }

    fn clear(&mut self) {
        self.wah.clear();
    }
}

impl ParallelStage for WahStage {
    fn target(&self) -> usize {
        2
    }
}

/// The rotary: low slow, low fast, low accel, low level, high slow, high fast, high accel, high level (rates on the
/// Rate1 column), separation, speed (slow, fast from 64, EFX control). Its two microphones go to the left and the
/// right output with the effect's pan.
struct RotaryStage {
    left: f64,
    right: f64,
    rotary: Rotary,
}

impl RotaryStage {
    fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            rotary: Rotary::new(sample_rate, memory),
        }
    }
}

impl CombinationStage for RotaryStage {
    fn left(&self) -> f64 {
        self.left
    }

    fn right(&self) -> f64 {
        self.right
    }

    fn update(&mut self, parameters: &[u8; 20], base: usize, control: Option<u8>) {
        let rotary = &mut self.rotary;
        let fast = control.expect(CONTROL) >= 64;
        rotary.set_rates(
            rate1(parameters[base + if fast { 5 } else { 4 }]),
            rate1(parameters[base + if fast { 1 } else { 0 }]),
        );
        rotary.set_acceleration(slew_time(parameters[base + 6]), slew_time(parameters[base + 2]));
        rotary.set_levels(level_gain(parameters[base + 7]), level_gain(parameters[base + 3]));
        rotary.set_separation(level_gain(parameters[base + 8]));
    }

    #[inline]
    fn process(&mut self, x: f64) {
        self.rotary.process(x);
        self.left = self.rotary.left;
        self.right = self.rotary.right;
    }

    fn clear(&mut self) {
        self.rotary.clear();
    }

    fn settle(&mut self) {
        self.rotary.settle();
    }
}

impl ParallelStage for RotaryStage {
    fn target(&self) -> usize {
        9
    }

    fn lagged(&self) -> bool {
        true
    }
}

/// Builds an effect of a parallel combination. Call it after the type's `GSEffectBase` constructor, which rewinds
/// the delay memory.
pub fn create_parallel_stage(r#type: u8, sample_rate: f64, memory: &mut DelayMemory) -> Box<dyn ParallelStage> {
    match r#type {
        parallel_stages::DRIVE => Box::new(DriveStage::new(sample_rate)),
        parallel_stages::PHASER => Box::new(PhaserStage::new(sample_rate)),
        parallel_stages::WAH => Box::new(WahStage::new(sample_rate)),
        parallel_stages::ROTARY => Box::new(RotaryStage::new(sample_rate, memory)),
        _ => unreachable!("unknown parallel stage {type}"),
    }
}
