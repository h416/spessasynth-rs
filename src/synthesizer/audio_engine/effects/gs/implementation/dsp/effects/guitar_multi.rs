//! The stages shared by the multis and their base: compressor, drive section, phaser, 3-band EQ, chorus/flanger and
//! delay.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/guitar_multi.ts

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::amp_simulator::AmpSimulator;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::bass_amp_simulator::BassAmpSimulator;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::delay::Delay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::drive::{Drive, DrivePreset};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::dynamics::{Compressor, detector_gain, recovery_rate};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::lfo::Lfo;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::modulated_delay::ModulatedDelay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::output_lag::OutputLag;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::peaking_band::PeakingBand;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::phaser::Phaser;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::shelf_eq::{ShelfEq, shelf_corners};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{
    SWING_MS, delay4, feedback_gain, hf_damp, modulation_depth, rate2, resonance_gain,
};

/// The fixed pre delay of the multis' chorus and flanger, in ms (16 samples at 32 kHz).
const MODULATION_PRE_DELAY: f64 = 0.5;

/// The dry gain of the chorus/flanger and delay stages, and the gain into their lines and into the phaser.
const STAGE_DRY: f64 = 127.0 / 128.0;

/// The gain after the wah of the multis.
pub const MULTI_WAH_GAIN: f64 = (127.0 / 32.0) * (127.0 / 128.0);

/// The multis' drives see their input 128/127 hotter than the drive types do and give back 127/128 less after the
/// clipper (the same linear gain, a level 0.07 dB lower once they saturate).
const DRIVE_INPUT: f64 = 128.0 / 127.0;
const DRIVE_OUTPUT: f64 = 127.0 / 128.0;

/// The compressor of the multis (parameters 1-4: sustain, attack, level, switch): the Compressor's dynamics with
/// its post gain at +6 dB and a level after it. The compressor runs when it is switched off too.
pub struct MultiCompressor {
    dynamics: Compressor,
    on: bool,
}

impl MultiCompressor {
    /// `lookahead` is the lookahead in seconds (the Compressor's is 1 ms).
    pub fn new(sample_rate: f64, lookahead: f64) -> Self {
        Self {
            dynamics: Compressor::new(sample_rate, lookahead),
            on: false,
        }
    }

    /// `parameters` are the type's parameters (0-3 are the compressor's).
    pub fn update(&mut self, parameters: &[u8; 20]) {
        let d = &mut self.dynamics;
        d.set_recovery_rate(recovery_rate(parameters[0]));
        d.set_makeup(detector_gain(parameters[1]));
        d.set_post_gain(2.0 * level_gain(parameters[2]));
        self.on = parameters[3] == 1;
    }

    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.dynamics.process(x);
        if self.on { y } else { x }
    }

    pub fn clear(&mut self) {
        self.dynamics.clear();
    }
}

/// The amp simulators of the drive section (the guitar and the bass one).
pub trait MultiAmp {
    /// `type` is the amp type parameter.
    fn set_type(&mut self, r#type: u8);
    fn process(&mut self, x: f64) -> f64;
    fn clear(&mut self);
}

impl MultiAmp for AmpSimulator {
    fn set_type(&mut self, r#type: u8) {
        AmpSimulator::set_type(self, r#type);
    }
    fn process(&mut self, x: f64) -> f64 {
        AmpSimulator::process(self, x)
    }
    fn clear(&mut self) {
        AmpSimulator::clear(self);
    }
}

impl MultiAmp for BassAmpSimulator {
    fn set_type(&mut self, r#type: u8) {
        BassAmpSimulator::set_type(self, r#type);
    }
    fn process(&mut self, x: f64) -> f64 {
        BassAmpSimulator::process(self, x)
    }
    fn clear(&mut self) {
        BassAmpSimulator::clear(self);
    }
}

/// How a multi builds its drive section.
pub struct MultiDriveOptions {
    /// The drive of drive type 0.
    pub overdrive: &'static DrivePreset,
    /// The drive of drive type 1.
    pub distortion: &'static DrivePreset,
    pub amp: Box<dyn MultiAmp>,
    /// The gain of the path around the section when the drive is off (1 in TS when missing).
    pub bypass_gain: f64,
    /// A 2-band EQ (200 Hz and 4 kHz shelves) between the drive and the amp.
    pub shelves: bool,
}

/// The drive section of the guitar and bass multis: overdrive or distortion, in some of them the 2-band EQ, then
/// the amp simulator (switchable). The drive switch bypasses the whole section, the path around it goes through the
/// drive's DC blocker.
pub struct MultiDrive {
    drive: Drive,
    /// The gain of the path around the drive, on the drive's DC blocked input.
    bypass_gain: f64,
    amp: Box<dyn MultiAmp>,
    shelves: Option<ShelfEq>,
    overdrive: &'static DrivePreset,
    distortion: &'static DrivePreset,
    preset: Option<&'static DrivePreset>,
    drive_on: bool,
    amp_on: bool,
}

impl MultiDrive {
    pub fn new(sample_rate: f64, options: MultiDriveOptions) -> Self {
        Self {
            drive: Drive::new(sample_rate),
            // The drive's input is DRIVE_INPUT times the section's
            bypass_gain: options.bypass_gain / DRIVE_INPUT,
            amp: options.amp,
            shelves: if options.shelves { Some(ShelfEq::new(sample_rate)) } else { None },
            overdrive: options.overdrive,
            distortion: options.distortion,
            preset: None,
            drive_on: false,
            amp_on: false,
        }
    }

    /// `distortion` selects the distortion instead of the overdrive, then the drive parameter, the amp type
    /// parameter, the amp switch and the drive switch.
    pub fn update(&mut self, distortion: bool, drive: u8, amp_type: u8, amp_on: bool, drive_on: bool) {
        let preset = if distortion { self.distortion } else { self.overdrive };
        // The filters only need a new design when the drive type changes
        if !self.preset.is_some_and(|p| std::ptr::eq(p, preset)) {
            self.preset = Some(preset);
            self.drive.set_preset(preset);
        }
        self.drive.set_drive(drive);
        self.amp.set_type(amp_type);
        self.amp_on = amp_on;
        self.drive_on = drive_on;
    }

    /// The low and high gain parameters.
    pub fn set_shelves(&mut self, low: u8, high: u8) {
        if let Some(shelves) = &mut self.shelves {
            shelves.set_low(shelf_corners::LOW200, shelf_gain(low));
            shelves.set_high(shelf_corners::HIGH4K, shelf_gain(high));
        }
    }

    pub fn process(&mut self, x: f64) -> f64 {
        let mut driven = DRIVE_OUTPUT * self.drive.process(x * DRIVE_INPUT);
        if let Some(shelves) = &mut self.shelves {
            driven = shelves.process(driven);
        }
        // The amp runs when it is switched off too, so switching it on does not click
        let amp = self.amp.process(driven);
        if self.amp_on {
            driven = amp;
        }
        // Both paths run, so the drive switch does not click either
        if self.drive_on { driven } else { self.drive.input * self.bypass_gain }
    }

    pub fn clear(&mut self) {
        self.drive.clear();
        if let Some(shelves) = &mut self.shelves {
            shelves.clear();
        }
        self.amp.clear();
    }
}

/// The phaser of the keyboard multis: the mono sum of both channels into the phaser, its output added to each
/// channel at the mix level. Rate on the Rate2 column.
pub struct MultiPhaser {
    pub left: f64,
    pub right: f64,
    phaser: Phaser,
    wet: f64,
}

impl MultiPhaser {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            phaser: Phaser::new(sample_rate, 1),
            wet: 0.0,
        }
    }

    /// The manual, rate (Rate2 column), depth, resonance and mix level parameters.
    pub fn update(&mut self, manual: u8, rate: u8, depth: u8, resonance: u8, mix: u8) {
        let phaser = &mut self.phaser;
        phaser.set_manual(manual);
        phaser.set_rate(rate2(rate));
        // The depth word is the chorus swing in ms over 16
        phaser.set_depth(modulation_depth(depth) / 16.0);
        phaser.set_resonance(resonance);
        self.wet = level_gain(mix);
    }

    pub fn process(&mut self, left: f64, right: f64) {
        let phaser = &mut self.phaser;
        phaser.step();
        // The chain runs at 1/8 of the 127/128 input and comes out x 8 the mix level
        let phased = self.wet * phaser.process(0, 0.5 * (left + right) * STAGE_DRY);
        self.left = left + phased;
        self.right = right + phased;
    }

    pub fn clear(&mut self) {
        self.phaser.clear();
    }
}

/// The 3-band EQ of the multis: the low and high shelves (200 Hz and 4 kHz) and a peaking band.
pub struct MultiEq {
    /// The peaking band, for Keyboard Multi's right band after a type select.
    pub mid: PeakingBand,
    shelves: ShelfEq,
}

impl MultiEq {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            shelves: ShelfEq::new(sample_rate),
            mid: PeakingBand::new(sample_rate),
        }
    }

    /// The low and high gain parameters.
    pub fn set_shelves(&mut self, low: u8, high: u8) {
        self.shelves.set_low(shelf_corners::LOW200, shelf_gain(low));
        self.shelves.set_high(shelf_corners::HIGH4K, shelf_gain(high));
    }

    /// The mid frequency (EQ column, one table every 8 values), the Q index (0-4) and the gain parameter
    /// (0x34-0x4c is -12 to +12 dB).
    pub fn set_mid(&mut self, frequency: u8, q: u8, gain: u8) {
        self.mid.set_parameters(frequency, q, gain);
    }

    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.shelves.process(x);
        self.mid.process(y)
    }

    pub fn clear(&mut self) {
        self.shelves.clear();
        self.mid.clear();
    }
}

/// The chorus or flanger of the multis: the mono chain into a line with two reads, the left one also feeds back
/// (flanger). Each output is the chain plus its read at the wet level. Pre delay 0.5 ms, rate on the Rate2 column.
/// The chorus reads are half a cycle apart, the flanger's together.
pub struct MultiModulation {
    pub left: f64,
    pub right: f64,
    lfo: Lfo,
    delay: ModulatedDelay,
    wet: f64,
}

impl MultiModulation {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let mut delay = ModulatedDelay::new(sample_rate, memory, 2, MODULATION_PRE_DELAY + SWING_MS);
        delay.set_pre_delay(0, MODULATION_PRE_DELAY);
        delay.set_pre_delay(1, MODULATION_PRE_DELAY);
        Self {
            left: 0.0,
            right: 0.0,
            lfo: Lfo::new(sample_rate),
            delay,
            wet: 0.0,
        }
    }

    /// `flanger` selects the flanger instead of the chorus, then the rate (Rate2 column), depth, flanger feedback
    /// and wet level parameters.
    pub fn update(&mut self, flanger: bool, rate: u8, depth: u8, feedback: u8, wet: u8) {
        let delay = &mut self.delay;
        delay.set_curve(flanger);
        delay.set_phase(1, if flanger { 0.0 } else { 180.0 });
        delay.set_depth(0, modulation_depth(depth));
        delay.set_depth(1, modulation_depth(depth));
        delay.set_feedback(if flanger { feedback_gain(feedback) } else { 0.0 });
        self.lfo.set_rate(rate2(rate));
        self.wet = level_gain(wet);
    }

    /// Puts a high pass in the line (in the loop, like the tone filters of the choruses): the corner in Hz and the
    /// gain at Nyquist.
    pub fn set_high_pass(&mut self, frequency: f64, gain: f64) {
        self.delay.tone.set_high_pass(frequency, gain);
    }

    pub fn process(&mut self, x: f64) {
        let phase = self.lfo.process();
        let delay = &mut self.delay;
        delay.process(x * STAGE_DRY, phase);
        self.left = STAGE_DRY * x + self.wet * delay.output[0];
        self.right = STAGE_DRY * x + self.wet * delay.output[1];
    }

    pub fn clear(&mut self) {
        self.lfo.clear();
        self.delay.clear();
    }
}

/// The delay of the multis: the mean of both channels plus the feedback into one line, its echo added to each
/// channel at the wet level. Time on the Dly4 column, feedback on the resonance table, optional HF damp.
pub struct MultiDelay {
    pub left: f64,
    pub right: f64,
    delay: Delay,
    feedback: f64,
    wet: f64,
}

impl MultiDelay {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let mut delay = Delay::new(sample_rate, memory, 1, 640.0);
        // The echo is the damped tap, like the other single tap delays
        delay.set_damp_taps(true);
        Self {
            left: 0.0,
            right: 0.0,
            delay,
            feedback: 0.0,
            wet: 0.0,
        }
    }

    /// The delay time (Dly4 column), feedback, wet level and HF damp (HF column, 127 = Bypass) parameters.
    pub fn update(&mut self, time: u8, feedback: u8, wet: u8, damp: u8) {
        self.delay.set_tap(0, delay4(time));
        self.delay.set_hf_damp(hf_damp(damp));
        self.feedback = resonance_gain(feedback) * STAGE_DRY;
        self.wet = level_gain(wet) * STAGE_DRY;
    }

    pub fn process(&mut self, left: f64, right: f64) {
        let delay = &mut self.delay;
        let fed = self.feedback * delay.feedback(1.0);
        delay.process(0.5 * (left + right) + fed);
        let echo = self.wet * delay.output[0];
        self.left = STAGE_DRY * left + echo;
        self.right = STAGE_DRY * right + echo;
    }

    pub fn clear(&mut self) {
        self.delay.clear();
    }
}

/// The multis on the mono mix: the guitar and bass multis (a mono chain on `L+R`, compressor or wah, drive, amp, EQ
/// in some order, then a chorus or flanger that gives two outputs, and in some of them a delay), and the keyboard
/// multis, which keep both channels and mix them into their mono stages.
///
/// Like the other mono input types the right output comes one 32 kHz sample after the left one.
/// TS `GSMulti` is an abstract `GSEffect`; here it is the shared part the multis hold next to their `GSEffectBase`.
pub struct GSMulti {
    pub mix: MonoMix,
    pub gain: f64,
    lag: OutputLag,
}

impl GSMulti {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            mix: MonoMix::new(sample_rate),
            gain: 0.0,
            lag: OutputLag::new(sample_rate, 1.0),
        }
    }

    pub fn clear(&mut self) {
        self.mix.clear();
        self.lag.clear();
    }

    /// Writes one output sample, the right one late.
    pub fn output(
        &mut self,
        left: f64,
        right: f64,
        output_left: &mut [f32],
        output_right: &mut [f32],
        i: usize,
    ) {
        output_left[i] = (left * self.gain) as f32;
        output_right[i] = self.lag.process(right * self.gain) as f32;
    }
}
