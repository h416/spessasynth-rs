//! The Reverb and Delay types' reverb: a diffuser and two independent tanks, read by four taps per side.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/reverb.ts

use super::crossfade::Crossfade;
use super::delay_line::DelayLine;
use super::delay_memory::DelayMemory;
use super::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};

/// The lines of the reverb. Each holds what one stage writes: the pre delay, the diffuser's three allpasses, then per
/// tank the first allpass, its output (the first delay segment), the second allpass and its output (the second
/// segment, which feeds back). The diffuser's plain delay is part of the pre delay line (a delay in front of the
/// allpasses is the same as one after them).
/// Equivalent to: LINES
mod lines {
    pub const PRE: usize = 0;
    pub const DIFFUSER1: usize = 1;
    pub const DIFFUSER2: usize = 2;
    pub const DIFFUSER3: usize = 3;
    pub const LEFT_ALLPASS1: usize = 4;
    pub const LEFT_SEGMENT1: usize = 5;
    pub const LEFT_ALLPASS2: usize = 6;
    pub const LEFT_SEGMENT2: usize = 7;
    pub const RIGHT_ALLPASS1: usize = 8;
    pub const RIGHT_SEGMENT1: usize = 9;
    pub const RIGHT_ALLPASS2: usize = 10;
    pub const RIGHT_SEGMENT2: usize = 11;
}
use lines as l;

/// An output tap: a line and its delay in 32 kHz samples.
type Tap = (usize, f64);

/// A character (Room, Stage, Hall). Delays are in 32 kHz samples, as the hardware holds them.
struct ReverbCharacter {
    /// The gain into the pre delay.
    input_gain: f64,
    /// The pole of the "1" variant's input low pass (the "2" variant has none).
    input_pole: f64,
    /// The diffuser's three allpasses and the plain delay after them.
    diffuser: [f64; 4],
    /// Each tank: allpass, delay, allpass, delay.
    left: [f64; 4],
    right: [f64; 4],
    /// Four taps per output, spread over both tanks (and the diffuser in Room).
    out_left: [Tap; 4],
    out_right: [Tap; 4],
}

/// Room, Stage and Hall. The "1" and "2" types of each differ only in the input low pass.
const CHARACTERS: [ReverbCharacter; 3] = [
    ReverbCharacter {
        input_gain: 0.125,
        input_pole: 0.617_919_921_875,
        diffuser: [323.0, 238.0, 142.0, 1.0],
        left: [245.0, 826.0, 505.0, 1694.0],
        right: [266.0, 894.0, 466.0, 1564.0],
        out_left: [
            (l::DIFFUSER3, 142.0),
            (l::LEFT_SEGMENT2, 941.0),
            (l::RIGHT_SEGMENT1, 149.0),
            (l::RIGHT_SEGMENT2, 260.0),
        ],
        out_right: [
            (l::DIFFUSER3, 142.0),
            (l::LEFT_SEGMENT2, 282.0),
            (l::RIGHT_SEGMENT1, 496.0),
            (l::RIGHT_SEGMENT2, 868.0),
        ],
    },
    ReverbCharacter {
        input_gain: 0.25,
        input_pole: 0.617_919_921_875,
        diffuser: [917.0, 377.0, 110.0, 76.0],
        left: [665.0, 1913.0, 813.0, 2339.0],
        right: [742.0, 2136.0, 781.0, 2248.0],
        out_left: [
            (l::LEFT_SEGMENT1, 477.0),
            (l::LEFT_SEGMENT2, 584.0),
            (l::RIGHT_SEGMENT1, 533.0),
            (l::RIGHT_SEGMENT2, 561.0),
        ],
        out_right: [
            (l::LEFT_SEGMENT1, 1549.0),
            (l::LEFT_SEGMENT2, 1894.0),
            (l::RIGHT_SEGMENT1, 1729.0),
            (l::RIGHT_SEGMENT2, 1820.0),
        ],
    },
    ReverbCharacter {
        input_gain: 0.125,
        input_pole: 0.548_950_195_312_5,
        diffuser: [523.0, 339.0, 216.0, 1351.0],
        left: [993.0, 1536.0, 635.0, 3338.0],
        right: [1228.0, 2948.0, 1545.0, 1352.0],
        out_left: [
            (l::LEFT_SEGMENT1, 1106.0),
            (l::LEFT_SEGMENT2, 2314.0),
            (l::RIGHT_ALLPASS2, 777.0),
            (l::RIGHT_SEGMENT2, 1721.0),
        ],
        out_right: [
            (l::LEFT_ALLPASS2, 205.0),
            (l::LEFT_SEGMENT2, 2969.0),
            (l::RIGHT_SEGMENT2, 1403.0),
            (l::RIGHT_SEGMENT2, 983.0),
        ],
    },
];

/// The longest delay any character reads from each line, in 32 kHz samples (the pre delay up to 100 ms with the
/// diffuser's delay).
const LINE_LENGTHS: [f64; 12] = [
    4552.0, 917.0, 377.0, 216.0, 993.0, 1913.0, 813.0, 3338.0, 1228.0, 2948.0, 1545.0, 2248.0,
];

/// How many host samples later the whole forward path runs, and how much earlier the outputs read, so that even the
/// shortest pre delay is read band-limited (the interpolator needs 7 newer samples). The outputs read 141 32 kHz
/// samples or more, so they have room.
const LOOKAHEAD: f64 = 8.0;

/// The gain of every allpass.
const ALLPASS: f64 = 0.5;

/// The pre delay line's write and read gains.
const PRE_GAIN: f64 = (127.0 / 128.0) * (127.0 / 128.0);

/// The corner of the hardware's one-pole low pass `(1 - B) x + B y[n-1]` at 32 kHz (-3 dB, the inverse of
/// `FirstOrderFilter::set_damp`). `pole` is B.
fn damp_corner(pole: f64) -> f64 {
    let k = (1.0 + pole * pole) / (2.0 * pole);
    ((2.0 - k).acos() * 32_000.0) / (2.0 * std::f64::consts::PI)
}

/// The Reverb: a mono input through a DC blocker and an input low pass into a pre delay, a diffuser of three
/// Schroeder allpasses and a delay, then two independent tanks (allpass, delay, allpass, delay, a damping low pass
/// and the time gain back to the start). The stereo image comes from the output taps alone, four per side spread
/// over both tanks.
///
/// The delays are the hardware's 32 kHz sample counts, read band-limited at other rates. The left output is one
/// 32 kHz sample earlier than the program's (the shared stage reads the left input one sample late, so the program's
/// mono input is one sample behind the dry left), the right one is the program's (its output is one sample late).
pub struct Reverb {
    pub left: f64,
    pub right: f64,
    lines: Vec<DelayLine>,
    dc_blocker: FirstOrderFilter,
    input_low_pass: FirstOrderFilter,
    damp_left: FirstOrderFilter,
    damp_right: FirstOrderFilter,
    /// One 32 kHz sample in host samples.
    step: f64,
    character: usize,
    /// The pre delay read, the one set, and the one read before it while a change crossfades (32 kHz samples).
    pre_delay: f64,
    target_pre_delay: f64,
    previous_pre_delay: f64,
    fade: Crossfade,
    time: f64,
}

impl Reverb {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let step = sample_rate / 32_000.0;
        let lines = LINE_LENGTHS
            .iter()
            .map(|&length| memory.line((length * step).ceil() + LOOKAHEAD))
            .collect();
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        Self {
            left: 0.0,
            right: 0.0,
            lines,
            dc_blocker,
            input_low_pass: FirstOrderFilter::new(sample_rate),
            damp_left: FirstOrderFilter::new(sample_rate),
            damp_right: FirstOrderFilter::new(sample_rate),
            step,
            character: 0,
            pre_delay: 1.0,
            target_pre_delay: 1.0,
            previous_pre_delay: 1.0,
            fade: Crossfade::new(sample_rate),
            time: 0.0,
        }
    }

    /// `type` is the Reverb type (0-5: Room 1, Room 2, Stage 1, Stage 2, Hall 1, Hall 2).
    pub fn set_type(&mut self, r#type: u8) {
        self.character = (r#type >> 1) as usize;
        if r#type & 1 != 0 {
            self.input_low_pass.set_off();
        } else {
            self.input_low_pass
                .set_damp(damp_corner(CHARACTERS[self.character].input_pole));
        }
    }

    /// `ms` is the pre delay (the PreDly column).
    pub fn set_pre_delay(&mut self, ms: f64) {
        // One 32 kHz sample more, the program reads the line before it writes it
        if 1.0 + ms * 32.0 == self.target_pre_delay {
            return;
        }
        self.target_pre_delay = 1.0 + ms * 32.0;
        self.fade.request();
    }

    /// `gain` is the gain per pass through a tank (`reverb_time`).
    pub fn set_time(&mut self, gain: f64) {
        self.time = gain;
    }

    /// `hz` is the tanks' damping corner (the HF column), or infinite for none.
    pub fn set_damping(&mut self, hz: f64) {
        if hz == f64::INFINITY {
            self.damp_left.set_off();
            self.damp_right.set_off();
        } else {
            self.damp_left.set_damp(hz);
            self.damp_right.set_damp(hz);
        }
    }

    #[inline]
    pub fn process(&mut self, x: f64) {
        let step = self.step;
        let c = &CHARACTERS[self.character];
        let d1 = self.before(l::DIFFUSER1, c.diffuser[0]);
        let d2 = self.before(l::DIFFUSER2, c.diffuser[1]);
        let d3 = self.before(l::DIFFUSER3, c.diffuser[2]);
        let l1 = self.before(l::LEFT_ALLPASS1, c.left[0]);
        let l_mid = self.before(l::LEFT_SEGMENT1, c.left[1]);
        let l2 = self.before(l::LEFT_ALLPASS2, c.left[2]);
        let l_loop = self.before(l::LEFT_SEGMENT2, c.left[3]);
        let r1 = self.before(l::RIGHT_ALLPASS1, c.right[0]);
        let r_mid = self.before(l::RIGHT_SEGMENT1, c.right[1]);
        let r2 = self.before(l::RIGHT_ALLPASS2, c.right[2]);
        let r_loop = self.before(l::RIGHT_SEGMENT2, c.right[3]);

        if self.fade.tick() {
            self.previous_pre_delay = self.pre_delay;
            self.pre_delay = self.target_pre_delay;
        }
        let input = self.input_low_pass.process(self.dc_blocker.process(x));
        let lines = &mut self.lines;
        lines[l::PRE].write(c.input_gain * PRE_GAIN * input);
        let mut pre = lines[l::PRE].read_sinc((self.pre_delay + c.diffuser[3]) * step + LOOKAHEAD);
        if self.fade.active {
            let old = lines[l::PRE].read_sinc((self.previous_pre_delay + c.diffuser[3]) * step + LOOKAHEAD);
            pre = old + self.fade.gain * (pre - old);
        }
        // The diffuser: w = x - g w[n-N], y = w[n-N] + g w
        let w1 = pre - ALLPASS * d1;
        let w2 = d1 + ALLPASS * w1 - ALLPASS * d2;
        let w3 = d2 + ALLPASS * w2 - ALLPASS * d3;
        lines[l::DIFFUSER1].write(w1);
        lines[l::DIFFUSER2].write(w2);
        lines[l::DIFFUSER3].write(w3);
        let diffused = d3 + ALLPASS * w3;
        // The tanks, each fed by the diffused signal
        let lw1 = self.time * self.damp_left.process(l_loop) + diffused - ALLPASS * l1;
        let lw2 = l_mid - ALLPASS * l2;
        lines[l::LEFT_ALLPASS1].write(lw1);
        lines[l::LEFT_SEGMENT1].write(l1 + ALLPASS * lw1);
        lines[l::LEFT_ALLPASS2].write(lw2);
        lines[l::LEFT_SEGMENT2].write(l2 + ALLPASS * lw2);
        let rw1 = self.time * self.damp_right.process(r_loop) + diffused - ALLPASS * r1;
        let rw2 = r_mid - ALLPASS * r2;
        lines[l::RIGHT_ALLPASS1].write(rw1);
        lines[l::RIGHT_SEGMENT1].write(r1 + ALLPASS * rw1);
        lines[l::RIGHT_ALLPASS2].write(rw2);
        lines[l::RIGHT_SEGMENT2].write(r2 + ALLPASS * rw2);

        let mut left = 0.0;
        let mut right = 0.0;
        for i in 0..4 {
            left += self.output(c.out_left[i], 1.0);
            right += self.output(c.out_right[i], 0.0);
        }
        self.left = left;
        self.right = right;
    }

    pub fn clear(&mut self) {
        for line in &mut self.lines {
            line.clear();
        }
        self.dc_blocker.clear();
        self.input_low_pass.clear();
        self.damp_left.clear();
        self.damp_right.clear();
        self.fade.clear();
        self.left = 0.0;
        self.right = 0.0;
    }

    /// An output tap, read after this sample's writes. `early` is how many 32 kHz samples earlier than the program it
    /// reads.
    fn output(&self, tap: Tap, early: f64) -> f64 {
        // The taps on the diffuser's lines come before its delay, which the pre delay holds here
        let shift = if tap.0 <= l::DIFFUSER3 {
            CHARACTERS[self.character].diffuser[3]
        } else {
            0.0
        };
        self.lines[tap.0].read_sinc((tap.1 - shift - early) * self.step - LOOKAHEAD)
    }

    /// A read of what a stage wrote before this sample. `delay` is in 32 kHz samples.
    fn before(&self, line: usize, delay: f64) -> f64 {
        self.lines[line].read_sinc(delay * self.step - 1.0)
    }
}
