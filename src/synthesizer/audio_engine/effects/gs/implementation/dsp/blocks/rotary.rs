//! The rotary speaker: a crossover into a horn and a drum line, each with a turning rotor.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/rotary.ts

use std::sync::LazyLock;

use super::biquad::Biquad;
use super::delay_line::DelayLine;
use super::delay_memory::DelayMemory;
use super::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};
use super::tremolo::tremolo_rest;

/// The horn crossover, the hardware's 32 kHz biquad `b0 (1 - z^-1)^2 / (1 - a1 z^-1 - a2 z^-2)`, on the analog side
/// (the inverse bilinear transform): a second order high pass at 2000 Hz with a Q of 5 (+14 dB at 2 kHz). The
/// SC-88Pro has the resonance too.
const HORN_B0: f64 = 0.926_483_154_296_875;
const HORN_A1: f64 = 1.779_663_085_937_5;
const HORN_A2: f64 = -0.926_269_531_25;
const HORN_D2: f64 = 1.0 + HORN_A1 - HORN_A2;
const HORN_D1: f64 = 2.0 + 2.0 * HORN_A2;
const HORN_D0: f64 = 1.0 - HORN_A1 - HORN_A2;
static HORN_HZ: LazyLock<f64> =
    LazyLock::new(|| (32_000.0 / std::f64::consts::PI) * (HORN_D0 / HORN_D2).sqrt().atan());
static HORN_Q: LazyLock<f64> = LazyLock::new(|| (HORN_D0 * HORN_D2).sqrt() / HORN_D1);
const HORN_GAIN: f64 = (4.0 * HORN_B0) / HORN_D2;

/// The drum crossover: the hardware's one-pole low pass (pole 0.668, unity at DC), its corner is 2000 Hz too.
const DRUM_POLE: f64 = 0.668_151_855_468_75;
static DRUM_HZ: LazyLock<f64> =
    LazyLock::new(|| (32_000.0 / std::f64::consts::PI) * ((1.0 - DRUM_POLE) / (1.0 + DRUM_POLE)).atan());

/// The Doppler swing of the horn (a parabola over the turn) and the drum (a triangle), in 32 kHz samples.
const HORN_SWING: f64 = 33.25;
const DRUM_SWING: f64 = 20.0;

/// The fixed taps in 32 kHz samples: the horn's (3.84 and 28.2 ms, one per microphone) and the drum's (10.25 and
/// 37.1 ms).
const HORN_TAP_LEFT: f64 = 123.0;
const HORN_TAP_RIGHT: f64 = 901.0;
const DRUM_TAP_LEFT: f64 = 328.0;
const DRUM_TAP_RIGHT: f64 = 1188.0;

/// The rotary speaker: a crossover splits the input into a horn and a drum line, each with a turning rotor.
///
/// The horn is picked up by two microphones half a turn apart. Each hears the horn line through a Doppler read that
/// swings with the square of the rotor phase, mixed with a fixed tap, and an amplitude modulation (the Separation)
/// that is loud for one microphone while it is quiet for the other. The drum line has one Doppler read (a triangle
/// over the turn, on both sides) and a fixed tap per side. The left output is the first microphone, the right one the
/// second, each minus its drum tap and 1.5 x the drum's Doppler read.
///
/// The rotors run at the slow or fast rate and ramp between them with their own time constants. The right output
/// comes one 32 kHz sample after the left one, as from the shared stage, so its reads are one sample later.
pub struct Rotary {
    pub left: f64,
    pub right: f64,
    horn_line: DelayLine,
    drum_line: DelayLine,
    dc_blocker: FirstOrderFilter,
    horn_crossover: Biquad,
    drum_crossover: FirstOrderFilter,
    sample_rate: f64,
    /// One 32 kHz sample in host samples.
    step: f64,
    horn_gain: f64,
    drum_gain: f64,
    depth: f64,
    rest: f64,
    horn_target: f64,
    drum_target: f64,
    horn_speed: f64,
    drum_speed: f64,
    horn_ramp: f64,
    drum_ramp: f64,
    horn_phase: f64,
    drum_phase: f64,
}

impl Rotary {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let step = sample_rate / 32_000.0;
        // Two more 32 kHz samples for the right side's lag and the interpolation
        let horn_line = memory.line(((HORN_TAP_RIGHT + 2.0) * step).ceil());
        let drum_line = memory.line(((DRUM_TAP_RIGHT + 2.0) * step).ceil());
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        let mut horn_crossover = Biquad::new(sample_rate);
        let w = horn_crossover.prewarp(*HORN_HZ);
        horn_crossover.set_analog(HORN_GAIN, 0.0, 0.0, 1.0, w / *HORN_Q, w * w);
        let mut drum_crossover = FirstOrderFilter::new(sample_rate);
        drum_crossover.set_low_pass(*DRUM_HZ, 1.0);
        Self {
            left: 0.0,
            right: 0.0,
            horn_line,
            drum_line,
            dc_blocker,
            horn_crossover,
            drum_crossover,
            sample_rate,
            step,
            horn_gain: 0.0,
            drum_gain: 0.0,
            depth: 0.0,
            rest: 1.0,
            horn_target: 0.0,
            drum_target: 0.0,
            horn_speed: 0.0,
            drum_speed: 0.0,
            horn_ramp: 0.0,
            drum_ramp: 0.0,
            horn_phase: 0.0,
            drum_phase: 0.0,
        }
    }

    /// Wraps a rotor phase to -1 to 1 (a turn is 2).
    #[inline]
    fn wrap(phase: f64) -> f64 {
        if phase >= 1.0 {
            phase - 2.0
        } else if phase < -1.0 {
            phase + 2.0
        } else {
            phase
        }
    }

    /// `horn` is the horn rotor's rate in Hz, `drum` the drum rotor's.
    pub fn set_rates(&mut self, horn: f64, drum: f64) {
        self.horn_target = horn;
        self.drum_target = drum;
    }

    /// `horn` is the horn rotor's ramp time constant in seconds, `drum` the drum rotor's.
    pub fn set_acceleration(&mut self, horn: f64, drum: f64) {
        // The ramps' step per 32 kHz sample is 1/32000 of the time constant
        let per_sample = 32_000.0 / self.sample_rate;
        self.horn_ramp = 1.0 - (1.0 - 1.0 / (32_000.0 * horn)).powf(per_sample);
        self.drum_ramp = 1.0 - (1.0 - 1.0 / (32_000.0 * drum)).powf(per_sample);
    }

    /// `horn` is the horn line's gain (the high level), `drum` the drum line's (the low level).
    pub fn set_levels(&mut self, horn: f64, drum: f64) {
        self.horn_gain = horn;
        self.drum_gain = drum;
    }

    /// `depth` is the horn's amplitude modulation, 0 to 127/128 (the Separation).
    pub fn set_separation(&mut self, depth: f64) {
        self.depth = depth;
        self.rest = tremolo_rest(depth);
    }

    /// Puts both rotors at their target rates, as after a type select.
    pub fn settle(&mut self) {
        self.horn_speed = self.horn_target;
        self.drum_speed = self.drum_target;
    }

    #[inline]
    pub fn process(&mut self, x: f64) {
        let step = self.step;
        let u = 0.5 * self.dc_blocker.process(x);
        let horn_in = self.horn_gain * self.horn_crossover.process(u);
        self.horn_line.write(horn_in);
        let drum_in = self.drum_gain * self.drum_crossover.process(u);
        self.drum_line.write(drum_in);

        self.horn_speed += self.horn_ramp * (self.horn_target - self.horn_speed);
        self.drum_speed += self.drum_ramp * (self.drum_target - self.drum_speed);
        self.horn_phase = Self::wrap(self.horn_phase + (2.0 * self.horn_speed) / self.sample_rate);
        let horn_phase = self.horn_phase;
        self.drum_phase = Self::wrap(self.drum_phase + (2.0 * self.drum_speed) / self.sample_rate);
        let drum_phase = self.drum_phase;
        // The right side runs a 32 kHz sample behind: the phases of then, every read one sample later
        let p = Self::wrap(horn_phase - self.horn_speed / 16_000.0);
        let d = Self::wrap(drum_phase - self.drum_speed / 16_000.0);

        let horn_line = &self.horn_line;
        let drum_line = &self.drum_line;
        // Microphone 1 hears the horn as the rotor phase says, microphone 2 half a turn later
        let left = Self::wrap(horn_phase + 0.5);
        let mic1 = (self.rest + self.depth * left.abs())
            * 0.5
            * (horn_line.read_grid(HORN_SWING * horn_phase * horn_phase, step, 0.0)
                + horn_line.read_grid(HORN_TAP_LEFT, step, 0.0));
        let later = Self::wrap(p + 1.0);
        let mic2 = (self.rest + self.depth * (1.0 - Self::wrap(p + 0.5).abs()))
            * 0.5
            * (horn_line.read_grid(HORN_SWING * later * later + 1.0, step, 0.0)
                + horn_line.read_grid(HORN_TAP_RIGHT + 1.0, step, 0.0));
        let drum_left = drum_line.read_grid(DRUM_SWING * drum_phase.abs(), step, 0.0);
        let drum_right = drum_line.read_grid(DRUM_SWING * d.abs() + 1.0, step, 0.0);
        self.left = (127.0 / 32.0) * mic1 - drum_line.read_grid(DRUM_TAP_LEFT, step, 0.0) - 1.5 * drum_left;
        self.right =
            (127.0 / 32.0) * mic2 - drum_line.read_grid(DRUM_TAP_RIGHT + 1.0, step, 0.0) - 1.5 * drum_right;
    }

    pub fn clear(&mut self) {
        self.horn_line.clear();
        self.drum_line.clear();
        self.dc_blocker.clear();
        self.horn_crossover.clear();
        self.drum_crossover.clear();
        self.horn_phase = 0.0;
        self.drum_phase = 0.0;
        self.left = 0.0;
        self.right = 0.0;
    }
}
