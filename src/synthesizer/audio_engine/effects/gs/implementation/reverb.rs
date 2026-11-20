//! The default GS reverb implementation, modeled on the reverb of the Sound Canvas.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/reverb.ts

use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::delay_line::DelayLine;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::first_order_filter::FirstOrderFilter;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::send_input::{
    LEVEL_UNITY, SendInput,
};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::hardware::HARDWARE_RATE;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::js_round;

/// Equivalent to: GSReverbParameter
#[derive(Clone, Debug)]
pub struct ReverbSnapshot {
    pub level: u8,
    pub pre_lowpass: u8,
    pub character: u8,
    pub time: u8,
    pub delay_feedback: u8,
    pub pre_delay_time: u8,
}

/// The lines an output tap can read: the diffuser's four allpasses, then per tank its first allpass, first delay,
/// second allpass and second delay. An allpass line holds the allpass's sum node.
/// Equivalent to: LINES
#[derive(Clone, Copy)]
enum Line {
    Diffuser4 = 3,
    LeftAllpass1 = 4,
    LeftDelay1 = 5,
    LeftDelay2 = 7,
    RightDelay1 = 9,
    RightDelay2 = 11,
}

/// An output tap: a line and its delay in 32 kHz samples.
type Tap = (Line, f64);

/// The delays of one tank in 32 kHz samples, in signal order.
#[derive(Clone, Copy)]
struct TankLengths {
    allpass1: f64,
    delay1: f64,
    allpass2: f64,
    delay2: f64,
}

#[derive(Clone, Copy)]
struct TankSetup {
    lengths: TankLengths,
    /// The pole of the damping low pass at 32 kHz.
    damping: f64,
}

/// A reverb character. Delays are in 32 kHz samples, as the Sound Canvas holds them.
struct ReverbCharacter {
    /// The gain into the pre delay.
    input_gain: f64,
    /// The diffuser's four stages.
    diffuser: [f64; 4],
    /// Whether the fourth stage is an allpass like the other three, or a plain delay.
    diffuser4_allpass: bool,
    left: TankSetup,
    right: TankSetup,
    /// Four taps per output, spread over both tanks (and the diffuser in the rooms).
    out_left: [Tap; 4],
    out_right: [Tap; 4],
}

// The poles of the three damping low passes the characters pick from
const DAMP_BRIGHT: f64 = 1.0 / 8.0;
const DAMP_MEDIUM: f64 = 2.0 / 8.0;
const DAMP_DARK: f64 = 31.0 / 64.0;

/// The tanks feed back inverted, at 0.55 of the time gain. The DLL folds this into the damping filter's gain,
/// rounded to 1/64 (at pole 1/4 that is -0.55 (1 - 1/4) = -26.4/64, stored as -26/64).
const DAMPING_GAIN: f64 = -0.55;

/// The gain g of every allpass, H(z) = (g + z^-N) / (1 + g z^-N).
const ALLPASS_GAIN: f64 = 0.5;

// Room 1 and Plate share the small tanks, Room 2, Room 3 and Hall 1 the medium ones
const SMALL_DIFFUSER: [f64; 4] = [323.0, 239.0, 143.0, 1.0];
const SMALL_LEFT: TankLengths = TankLengths {
    allpass1: 247.0,
    delay1: 827.0,
    allpass2: 506.0,
    delay2: 1695.0,
};
const SMALL_RIGHT: TankLengths = TankLengths {
    allpass1: 267.0,
    delay1: 895.0,
    allpass2: 467.0,
    delay2: 1565.0,
};
const MEDIUM_DIFFUSER: [f64; 4] = [917.0, 378.0, 111.0, 77.0];
const MEDIUM_LEFT: TankLengths = TankLengths {
    allpass1: 666.0,
    delay1: 1914.0,
    allpass2: 814.0,
    delay2: 2340.0,
};
const MEDIUM_RIGHT: TankLengths = TankLengths {
    allpass1: 743.0,
    delay1: 2137.0,
    allpass2: 782.0,
    delay2: 2249.0,
};
// Room 2 and Room 3 share their taps
const MEDIUM_TAPS_LEFT: [Tap; 4] = [
    (Line::Diffuser4, 20.0),
    (Line::LeftDelay2, 585.0),
    (Line::RightDelay1, 478.0),
    (Line::RightDelay2, 562.0),
];
const MEDIUM_TAPS_RIGHT: [Tap; 4] = [
    (Line::Diffuser4, 20.0),
    (Line::LeftDelay2, 1895.0),
    (Line::RightDelay1, 1550.0),
    (Line::RightDelay2, 1821.0),
];

const fn tank(lengths: TankLengths, damping: f64) -> TankSetup {
    TankSetup { lengths, damping }
}

/// Room 1, Room 2, Room 3, Hall 1, Hall 2 and Plate, as Sound Canvas VA sets them up.
const CHARACTERS: [ReverbCharacter; 6] = [
    // Room 1
    ReverbCharacter {
        input_gain: 1.0 / 8.0,
        diffuser: SMALL_DIFFUSER,
        diffuser4_allpass: false,
        left: tank(SMALL_LEFT, DAMP_MEDIUM),
        right: tank(SMALL_RIGHT, DAMP_DARK),
        out_left: [
            (Line::LeftAllpass1, 0.0),
            (Line::LeftDelay2, 942.0),
            (Line::RightDelay1, 150.0),
            (Line::RightDelay2, 261.0),
        ],
        out_right: [
            (Line::LeftAllpass1, 0.0),
            (Line::LeftDelay2, 283.0),
            (Line::RightDelay1, 497.0),
            (Line::RightDelay2, 869.0),
        ],
    },
    // Room 2
    ReverbCharacter {
        input_gain: 1.0 / 8.0,
        diffuser: MEDIUM_DIFFUSER,
        diffuser4_allpass: false,
        left: tank(MEDIUM_LEFT, DAMP_BRIGHT),
        right: tank(MEDIUM_RIGHT, DAMP_DARK),
        out_left: MEDIUM_TAPS_LEFT,
        out_right: MEDIUM_TAPS_RIGHT,
    },
    // Room 3
    ReverbCharacter {
        input_gain: 1.0 / 8.0,
        diffuser: MEDIUM_DIFFUSER,
        diffuser4_allpass: false,
        left: tank(MEDIUM_LEFT, DAMP_DARK),
        right: tank(MEDIUM_RIGHT, DAMP_DARK),
        out_left: MEDIUM_TAPS_LEFT,
        out_right: MEDIUM_TAPS_RIGHT,
    },
    // Hall 1, the only one with twice the input (the DLL does not say why)
    ReverbCharacter {
        input_gain: 1.0 / 4.0,
        diffuser: MEDIUM_DIFFUSER,
        diffuser4_allpass: false,
        left: tank(MEDIUM_LEFT, DAMP_DARK),
        right: tank(MEDIUM_RIGHT, DAMP_DARK),
        out_left: [
            (Line::LeftDelay1, 1550.0),
            (Line::LeftDelay2, 1895.0),
            (Line::RightDelay1, 534.0),
            (Line::RightDelay2, 562.0),
        ],
        out_right: [
            (Line::LeftDelay1, 478.0),
            (Line::LeftDelay2, 585.0),
            (Line::RightDelay1, 1730.0),
            (Line::RightDelay2, 1821.0),
        ],
    },
    // Hall 2
    ReverbCharacter {
        input_gain: 1.0 / 8.0,
        diffuser: [926.0, 381.0, 112.0, 1.0],
        diffuser4_allpass: true,
        left: tank(
            TankLengths {
                allpass1: 774.0,
                delay1: 2227.0,
                allpass2: 950.0,
                delay2: 2731.0,
            },
            DAMP_DARK,
        ),
        right: tank(
            TankLengths {
                allpass1: 860.0,
                delay1: 2474.0,
                allpass2: 909.0,
                delay2: 1989.0,
            },
            DAMP_DARK,
        ),
        out_left: [
            (Line::LeftDelay1, 1803.0),
            (Line::LeftDelay2, 2212.0),
            (Line::RightDelay1, 618.0),
            (Line::RightDelay2, 384.0),
        ],
        out_right: [
            (Line::LeftDelay1, 556.0),
            (Line::LeftDelay2, 682.0),
            (Line::RightDelay1, 2003.0),
            (Line::RightDelay2, 1734.0),
        ],
    },
    // Plate
    ReverbCharacter {
        input_gain: 1.0 / 8.0,
        diffuser: SMALL_DIFFUSER,
        diffuser4_allpass: true,
        left: tank(SMALL_LEFT, DAMP_BRIGHT),
        right: tank(SMALL_RIGHT, DAMP_MEDIUM),
        out_left: [
            (Line::LeftDelay1, 459.0),
            (Line::LeftDelay2, 942.0),
            (Line::RightDelay1, 150.0),
            (Line::RightDelay2, 261.0),
        ],
        out_right: [
            (Line::LeftDelay1, 138.0),
            (Line::LeftDelay2, 283.0),
            (Line::RightDelay1, 497.0),
            (Line::RightDelay2, 869.0),
        ],
    },
];

/// Characters 0 to 5 run the tanks, these two run a single feedback delay line.
const CHARACTER_DELAY: u8 = 6;
const CHARACTER_PANNING_DELAY: u8 = 7;

/// The longest any character makes a diffuser stage, in 32 kHz samples.
/// Equivalent to: MAX_DIFFUSER
fn max_diffuser(stage: usize) -> f64 {
    CHARACTERS
        .iter()
        .map(|c| c.diffuser[stage])
        .fold(f64::NEG_INFINITY, f64::max)
}

/// The longest any character makes a tank line, in 32 kHz samples.
/// Equivalent to: maxTank(key)
fn max_tank(key: fn(&TankLengths) -> f64) -> f64 {
    CHARACTERS
        .iter()
        .flat_map(|c| [key(&c.left.lengths), key(&c.right.lengths)])
        .fold(f64::NEG_INFINITY, f64::max)
}

/// The longest pre delay, in ms.
const MAX_PRE_DELAY: f64 = 127.0;

/// The largest gain the reverb's feedback word holds. The time gain grows to it at REVERB TIME 108, the delay
/// characters' feedback at DELAY FEEDBACK 95 (95/64 is the same 380/256).
const FEEDBACK_LIMIT: f64 = 380.0 / 256.0;
const TIME_AT_LIMIT: f64 = 108.0;

/// The delay characters' line is `DELAY_BASE + DELAY_STEP * time` 32 kHz samples (0.4 to 445 ms).
/// The panning delay's left output sits at half the step.
const DELAY_BASE: f64 = 13.0;
const DELAY_STEP: f64 = 112.0;

/// The gain of the delay characters' feedback at DELAY FEEDBACK 64, where the feedback word is 1.
/// The DLL feeds back through the nearly flat low pass `-31/64 x + 1/64 y[n-1]`, this is its gain at DC.
/// The repeats come back inverted.
const DELAY_FEEDBACK_GAIN: f64 = -31.0 / 63.0;

/// A delay of the Sound Canvas in whole host samples, at least one.
/// `samples` is the delay in 32 kHz samples, `step` one 32 kHz sample in host samples.
fn to_host_samples(samples: f64, step: f64) -> isize {
    js_round(samples * step).max(1.0) as isize
}

/// A Schroeder allpass: w = x - g w[n-N], y = w[n-N] + g w. Its line holds w, which some taps read.
struct Allpass {
    line: DelayLine,
    gain: f64,
    /// N - 1, since the read comes before the write.
    delay: isize,
}

impl Allpass {
    fn new(max_delay: f64) -> Self {
        Self {
            line: DelayLine::new(max_delay),
            gain: ALLPASS_GAIN,
            delay: 0,
        }
    }

    /// `samples` is N, in host samples.
    fn set_length(&mut self, samples: isize) {
        self.delay = samples - 1;
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let delayed = self.line.read(self.delay);
        let w = x - self.gain * delayed;
        self.line.write(w);
        delayed + self.gain * w
    }
}

/// One tank: allpass, delay, allpass, delay, then the damping low pass and the feedback gain back to its start.
struct ReverbTank {
    allpass1: Allpass,
    delay1: DelayLine,
    allpass2: Allpass,
    delay2: DelayLine,
    damping: FirstOrderFilter,
    delay1_length: isize,
    delay2_length: isize,
}

impl ReverbTank {
    fn new(sample_rate: f64, step: f64) -> Self {
        Self {
            allpass1: Allpass::new((max_tank(|t| t.allpass1) * step).ceil()),
            delay1: DelayLine::new((max_tank(|t| t.delay1) * step).ceil()),
            allpass2: Allpass::new((max_tank(|t| t.allpass2) * step).ceil()),
            delay2: DelayLine::new((max_tank(|t| t.delay2) * step).ceil()),
            damping: FirstOrderFilter::new(sample_rate),
            delay1_length: 1,
            delay2_length: 1,
        }
    }

    fn setup(&mut self, tank: &TankSetup, step: f64) {
        let t = &tank.lengths;
        self.allpass1.set_length(to_host_samples(t.allpass1, step));
        self.delay1_length = to_host_samples(t.delay1, step);
        self.allpass2.set_length(to_host_samples(t.allpass2, step));
        self.delay2_length = to_host_samples(t.delay2, step);
        self.damping.set_damp_pole(tank.damping);
    }

    /// `x` is the diffused input, `feedback` the gain of the way back, before the damping.
    #[inline]
    fn process(&mut self, x: f64, feedback: f64) {
        // The delays read before they write, so a delay of N reads N - 1
        let end = self.delay2.read(self.delay2_length - 1);
        let middle = self.delay1.read(self.delay1_length - 1);
        let damped = self.damping.process(end);
        let first = self.allpass1.process(x + feedback * damped);
        self.delay1.write(first);
        let second = self.allpass2.process(middle);
        self.delay2.write(second);
    }

    fn clear(&mut self) {
        self.allpass1.line.clear();
        self.delay1.clear();
        self.allpass2.line.clear();
        self.delay2.clear();
        self.damping.clear();
    }
}

/// The default GS Reverb implementation.
///
/// It is modeled on the reverb of the Sound Canvas (as Sound Canvas VA runs it): a mono input through a DC blocker
/// and the PRE-LPF into a pre delay, a diffuser of four Schroeder allpasses, then two independent tanks
/// (allpass, delay, allpass, delay, a damping low pass and the time gain back to the start).
/// The stereo image comes from the output taps alone, four per side.
/// The characters differ in the delays, the taps and the damping.
/// The two delay characters are a single delay line with a feedback. The DLL's 1/4 input gain and four equal 1/4
/// taps cancel there, so neither appears.
///
/// The delays are the 32 kHz sample counts of the Sound Canvas, rounded to whole samples at the host rate.
pub struct DefaultGSReverb {
    sample_rate: f64,
    /// One 32 kHz sample in host samples.
    step: f64,
    input: SendInput,
    pre_delay_line: DelayLine,
    diffuser: [Allpass; 4],
    left: ReverbTank,
    right: ReverbTank,
    /// The line each output tap reads and its delay in host samples, four left, then four right.
    taps: [Line; 8],
    tap_delays: [isize; 8],
    /// The line of the delay characters.
    delay_line: DelayLine,

    input_gain: f64,
    /// The gain per pass through a tank, before the damping.
    time_gain: f64,
    output_gain: f64,
    pre_delay_samples: isize,
    delay_character: bool,
    panning: bool,
    // The delay characters: the delay, the panning delay's left tap and the feedback gain
    delay_samples: isize,
    delay_half_samples: isize,
    delay_feedback_gain: f64,

    level: u8,
    delay_feedback: u8,
    character: u8,
    time: u8,
    pre_delay_time: u8,
    pre_lowpass: u8,
}

impl DefaultGSReverb {
    /// Constructs a new default GS reverb processor.
    /// `max_buffer_size` is unused (TS: for consistency across effects).
    pub fn new(sample_rate: f64, _max_buffer_size: usize) -> Self {
        let step = sample_rate / HARDWARE_RATE;
        let mut reverb = Self {
            sample_rate,
            step,
            input: SendInput::new(sample_rate),
            pre_delay_line: DelayLine::new(((MAX_PRE_DELAY * sample_rate) / 1000.0).ceil()),
            diffuser: std::array::from_fn(|stage| Allpass::new((max_diffuser(stage) * step).ceil())),
            left: ReverbTank::new(sample_rate, step),
            right: ReverbTank::new(sample_rate, step),
            taps: [Line::Diffuser4; 8],
            tap_delays: [0; 8],
            delay_line: DelayLine::new(((DELAY_BASE + DELAY_STEP * 127.0) * step).ceil()),
            input_gain: 0.0,
            time_gain: 0.0,
            output_gain: 1.0,
            pre_delay_samples: 0,
            delay_character: false,
            panning: false,
            delay_samples: 1,
            delay_half_samples: 1,
            delay_feedback_gain: 0.0,
            level: LEVEL_UNITY as u8,
            delay_feedback: 0,
            character: 0,
            time: 0,
            pre_delay_time: 0,
            pre_lowpass: 0,
        };
        reverb.reset();
        reverb
    }

    pub fn level(&self) -> u8 {
        self.level
    }

    pub fn set_level(&mut self, value: u8) {
        self.level = value;
        self.output_gain = value as f64 / LEVEL_UNITY;
    }

    pub fn delay_feedback(&self) -> u8 {
        self.delay_feedback
    }

    pub fn set_delay_feedback(&mut self, value: u8) {
        self.delay_feedback = value;
        // DELAY FEEDBACK is the feedback word over 64
        self.delay_feedback_gain = DELAY_FEEDBACK_GAIN * (value as f64 / 64.0).min(FEEDBACK_LIMIT);
    }

    pub fn character(&self) -> u8 {
        self.character
    }

    pub fn set_character(&mut self, value: u8) {
        // The lines hold the old character's sound
        if value != self.character {
            self.clear();
        }
        self.character = value;
        self.delay_character = value == CHARACTER_DELAY || value == CHARACTER_PANNING_DELAY;
        self.panning = value == CHARACTER_PANNING_DELAY;

        // An unknown character is the first one
        let c = CHARACTERS.get(value as usize).unwrap_or(&CHARACTERS[0]);
        let step = self.step;
        for stage in 0..4 {
            self.diffuser[stage].set_length(to_host_samples(c.diffuser[stage], step));
        }
        self.diffuser[3].gain = if c.diffuser4_allpass { ALLPASS_GAIN } else { 0.0 };
        self.left.setup(&c.left, step);
        self.right.setup(&c.right, step);
        for i in 0..8 {
            let (line, delay) = if i < 4 { c.out_left[i] } else { c.out_right[i - 4] };
            self.taps[i] = line;
            self.tap_delays[i] = js_round(delay * step) as isize;
        }
        self.input_gain = c.input_gain;
    }

    pub fn time(&self) -> u8 {
        self.time
    }

    pub fn set_time(&mut self, value: u8) {
        self.time = value;
        let value = (value as f64).clamp(0.0, 127.0);
        self.time_gain = (FEEDBACK_LIMIT * value.min(TIME_AT_LIMIT)) / TIME_AT_LIMIT;
        self.delay_samples = to_host_samples(DELAY_BASE + DELAY_STEP * value, self.step);
        self.delay_half_samples = to_host_samples(DELAY_BASE + (DELAY_STEP / 2.0) * value, self.step);
    }

    pub fn pre_delay_time(&self) -> u8 {
        self.pre_delay_time
    }

    pub fn set_pre_delay_time(&mut self, value: u8) {
        self.pre_delay_time = value;
        self.pre_delay_samples =
            js_round(((value as f64).clamp(0.0, MAX_PRE_DELAY) * self.sample_rate) / 1000.0) as isize;
    }

    pub fn pre_lowpass(&self) -> u8 {
        self.pre_lowpass
    }

    pub fn set_pre_lowpass(&mut self, value: u8) {
        self.pre_lowpass = value;
        self.input.set_pre_low_pass(value as f64);
    }

    fn line(&self, line: Line) -> &DelayLine {
        match line {
            Line::Diffuser4 => &self.diffuser[3].line,
            Line::LeftAllpass1 => &self.left.allpass1.line,
            Line::LeftDelay1 => &self.left.delay1,
            Line::LeftDelay2 => &self.left.delay2,
            Line::RightDelay1 => &self.right.delay1,
            Line::RightDelay2 => &self.right.delay2,
        }
    }

    /// Process the effect and **adds** it to the output.
    /// - input: 0-based mono input
    /// - output_left/right: start_index-based stereo output (ADDS)
    pub fn process(
        &mut self,
        input: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        if self.delay_character {
            self.process_delay(input, output_left, output_right, start_index, sample_count);
            return;
        }
        let pre_delay_samples = self.pre_delay_samples;
        let input_gain = self.input_gain;
        let output_gain = self.output_gain;
        let feedback = self.time_gain * DAMPING_GAIN;
        for i in 0..sample_count {
            let sent = input_gain * self.input.process(input[i] as f64);
            self.pre_delay_line.write(sent);
            let mut x = self.pre_delay_line.read(pre_delay_samples);
            for stage in self.diffuser.iter_mut() {
                x = stage.process(x);
            }

            self.left.process(x, feedback);
            self.right.process(x, feedback);

            let mut out_left = 0.0;
            let mut out_right = 0.0;
            for tap in 0..4 {
                out_left += self.line(self.taps[tap]).read(self.tap_delays[tap]);
                out_right += self.line(self.taps[tap + 4]).read(self.tap_delays[tap + 4]);
            }
            let o = start_index + i;
            output_left[o] = (output_left[o] as f64 + out_left * output_gain) as f32;
            output_right[o] = (output_right[o] as f64 + out_right * output_gain) as f32;
        }
    }

    pub fn reset(&mut self) {
        self.clear();
        self.set_level(LEVEL_UNITY as u8);
        self.set_pre_lowpass(0);
        self.set_character(0);
        self.set_time(0);
        self.set_delay_feedback(0);
        self.set_pre_delay_time(0);
    }

    pub fn get_snapshot(&self) -> ReverbSnapshot {
        ReverbSnapshot {
            level: self.level,
            pre_lowpass: self.pre_lowpass,
            character: self.character,
            time: self.time,
            delay_feedback: self.delay_feedback,
            pre_delay_time: self.pre_delay_time,
        }
    }

    /// Restores this effect processor instance from a snapshot.
    /// Equivalent to: applySnapshot(snapshot)
    pub fn apply_snapshot(&mut self, snapshot: &ReverbSnapshot) {
        self.set_level(snapshot.level);
        self.set_pre_lowpass(snapshot.pre_lowpass);
        self.set_character(snapshot.character);
        self.set_time(snapshot.time);
        self.set_delay_feedback(snapshot.delay_feedback);
        self.set_pre_delay_time(snapshot.pre_delay_time);
    }

    fn clear(&mut self) {
        self.input.clear();
        self.pre_delay_line.clear();
        for stage in self.diffuser.iter_mut() {
            stage.line.clear();
        }
        self.left.clear();
        self.right.clear();
        self.delay_line.clear();
    }

    /// The delay characters: one delay line that feeds back.
    /// The panning delay takes its left output from the middle of the line, so the repeats alternate.
    fn process_delay(
        &mut self,
        input: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        let delay_samples = self.delay_samples;
        let delay_half_samples = self.delay_half_samples;
        let delay_feedback_gain = self.delay_feedback_gain;
        let pre_delay_samples = self.pre_delay_samples;
        let output_gain = self.output_gain;
        let panning = self.panning;
        for i in 0..sample_count {
            let sent = self.input.process(input[i] as f64);
            self.pre_delay_line.write(sent);
            let echo = self.delay_line.read(delay_samples - 1);
            let into = self.pre_delay_line.read(pre_delay_samples) + delay_feedback_gain * echo;
            self.delay_line.write(into);
            let o = start_index + i;
            let left = if panning { self.delay_line.read(delay_half_samples) } else { echo };
            output_left[o] = (output_left[o] as f64 + output_gain * left) as f32;
            output_right[o] = (output_right[o] as f64 + output_gain * echo) as f32;
        }
    }
}
