//! The Gate Reverb: a diffuser into a line that twelve taps per side read, spread over the gate time.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/gate_reverb.ts

use std::sync::LazyLock;

use super::crossfade::Crossfade;
use super::delay_line::DelayLine;
use super::delay_memory::DelayMemory;
use super::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::js_round;

/// The diffuser's allpasses in 32 kHz samples (the Reverb's Stage lengths, all four allpasses here).
const DIFFUSER: [f64; 4] = [917.0, 377.0, 110.0, 76.0];

/// The diffuser's allpass gain.
const ALLPASS: f64 = 77.0 / 128.0;

/// Where the taps sit along the gate time, shortest first. The hardware rounds each to a whole 32 kHz sample, which
/// reproduces its tap table exactly.
#[rustfmt::skip]
const TAPS_LEFT: [f64; 12] = [
    0.000_212, 0.142_675, 0.237_933, 0.291_515, 0.377_206, 0.450_139, 0.527_11, 0.618_966, 0.664_044, 0.780_565,
    0.884_33, 1.0
];
#[rustfmt::skip]
const TAPS_RIGHT: [f64; 12] = [
    0.000_212, 0.149_053, 0.236_019, 0.302_147, 0.357_006_2, 0.448_864, 0.538_167, 0.614_715, 0.663_62, 0.789_071,
    0.882_628, 1.0
];

/// The right taps start 3.625 ms (116 samples) later.
const RIGHT_OFFSET: f64 = 116.0;

/// The tap gains per type (over 128, left then right, shortest tap first): Normal, Reverse (rising), Sweep 1 (left
/// rising, right falling) and Sweep 2 (the mirror image).
#[rustfmt::skip]
const PATTERN_BYTES: [[u8; 24]; 4] = [
    [64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64],
    [4, 5, 7, 10, 14, 19, 24, 30, 37, 45, 54, 64, 4, 5, 7, 10, 14, 19, 24, 30, 37, 45, 54, 64],
    [0, 11, 23, 34, 46, 58, 69, 81, 93, 104, 116, 127, 127, 116, 104, 93, 81, 69, 58, 46, 34, 23, 11, 0],
    [127, 116, 104, 93, 81, 69, 58, 46, 34, 23, 11, 0, 0, 11, 23, 34, 46, 58, 69, 81, 93, 104, 116, 127]
];
static PATTERNS: LazyLock<[[f64; 24]; 4]> =
    LazyLock::new(|| PATTERN_BYTES.map(|gains| gains.map(|g| g as f64 / 128.0)));

/// The longest read of the gate line in 32 kHz samples: the longest pre delay and the right side's last tap at 500 ms.
const LONGEST: f64 = 3200.0 + RIGHT_OFFSET + 16_000.0;

/// The Gate Reverb: a mono input through a DC blocker and a diffuser of four Schroeder allpasses into a line that
/// twelve taps per side read, spread over the gate time. There is no feedback, so the sound stops at the gate time.
///
/// Everything is linear, so the pre delay is part of the taps. The left taps read one 32 kHz sample earlier than the
/// program's (the shared stage reads the left input one sample late), except where that would need a future sample
/// (pre delay 0 with a gate time under 73 ms, the first left tap).
pub struct GateReverb {
    pub left: f64,
    pub right: f64,
    dc_blocker: FirstOrderFilter,
    diffuser: Vec<DelayLine>,
    line: DelayLine,
    /// One 32 kHz sample in host samples.
    step: f64,
    /// The tap delays in host samples: the ones read, the ones set, and the ones read before them while a change
    /// crossfades.
    delays_left: [f64; 12],
    delays_right: [f64; 12],
    targets_left: [f64; 12],
    targets_right: [f64; 12],
    previous_left: [f64; 12],
    previous_right: [f64; 12],
    fade: Crossfade,
    pattern: usize,
    pre_delay: f64,
    gate_time: f64,
}

impl GateReverb {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let step = sample_rate / 32_000.0;
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        let diffuser = DIFFUSER.iter().map(|&n| memory.line((n * step).ceil())).collect();
        let line = memory.line((LONGEST * step).ceil());
        Self {
            left: 0.0,
            right: 0.0,
            dc_blocker,
            diffuser,
            line,
            step,
            delays_left: [0.0; 12],
            delays_right: [0.0; 12],
            targets_left: [0.0; 12],
            targets_right: [0.0; 12],
            previous_left: [0.0; 12],
            previous_right: [0.0; 12],
            fade: Crossfade::new(sample_rate),
            pattern: 0,
            pre_delay: 0.0,
            gate_time: 0.0,
        }
    }

    /// `pattern` is the type (0-3: Normal, Reverse, Sweep 1, Sweep 2).
    pub fn set_pattern(&mut self, pattern: u8) {
        self.pattern = pattern as usize;
    }

    /// `ms` is the pre delay (the PreDly column).
    pub fn set_pre_delay(&mut self, ms: f64) {
        if ms * 32.0 == self.pre_delay {
            return;
        }
        self.pre_delay = ms * 32.0;
        self.place();
    }

    /// `ms` is the gate time, 5 to 500 ms.
    pub fn set_gate_time(&mut self, ms: f64) {
        if ms * 32.0 == self.gate_time {
            return;
        }
        self.gate_time = ms * 32.0;
        self.place();
    }

    #[inline]
    pub fn process(&mut self, x: f64) {
        let step = self.step;
        if self.fade.tick() {
            self.previous_left = self.delays_left;
            self.previous_right = self.delays_right;
            self.delays_left = self.targets_left;
            self.delays_right = self.targets_right;
        }
        let gains = &PATTERNS[self.pattern];
        let mut v = self.dc_blocker.process(x);
        for (k, line) in self.diffuser.iter_mut().enumerate() {
            // The allpass: w = x - g w[n-N], y = w[n-N] + g w
            let d = line.read_sinc(DIFFUSER[k] * step - 1.0);
            let w = v - ALLPASS * d;
            line.write(w);
            v = d + ALLPASS * w;
        }
        let line = &mut self.line;
        line.write(v);
        let mut left = 0.0;
        let mut right = 0.0;
        for i in 0..12 {
            left += gains[i] * line.read_sinc(self.delays_left[i]);
            right += gains[i + 12] * line.read_sinc(self.delays_right[i]);
        }
        if self.fade.active {
            let mut old_left = 0.0;
            let mut old_right = 0.0;
            for i in 0..12 {
                old_left += gains[i] * line.read_sinc(self.previous_left[i]);
                old_right += gains[i + 12] * line.read_sinc(self.previous_right[i]);
            }
            left = old_left + self.fade.gain * (left - old_left);
            right = old_right + self.fade.gain * (right - old_right);
        }
        self.left = left;
        self.right = right;
    }

    pub fn clear(&mut self) {
        self.dc_blocker.clear();
        for line in &mut self.diffuser {
            line.clear();
        }
        self.line.clear();
        self.fade.clear();
        self.left = 0.0;
        self.right = 0.0;
    }

    /// Places the taps for the pre delay and the gate time.
    fn place(&mut self) {
        for i in 0..12 {
            let left = self.pre_delay + js_round(TAPS_LEFT[i] * self.gate_time) - 1.0;
            let right = self.pre_delay + RIGHT_OFFSET + js_round(TAPS_RIGHT[i] * self.gate_time);
            self.targets_left[i] = 0.0f64.max(left * self.step);
            self.targets_right[i] = 0.0f64.max(right * self.step);
        }
        self.fade.request();
    }
}
