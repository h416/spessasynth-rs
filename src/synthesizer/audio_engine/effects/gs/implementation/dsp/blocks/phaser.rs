//! The phaser: per channel a resonance feedback, a DC blocker and eight first order all-pass sections.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/phaser.ts

use super::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};
use super::lfo::Lfo;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::resonance_gain;

/// The all-pass pole at 32 kHz per Manual value, over 8192 (x 0.45). The Man column of the conversion table is only a
/// label: the real first notch runs from 206 Hz (value 0) through 2 kHz (65, pole 0) to 6.9 kHz (125-127).
#[rustfmt::skip]
const MANUAL_POLES: [i16; 128] = [
    14_842, 14_696, 14_551, 14_407, 14_265, 14_124, 13_985, 13_846, 13_709, 13_573, 13_439, 13_305, 13_173, 13_042,
    12_912, 12_783, 12_656, 12_529, 12_404, 12_279, 12_156, 11_912, 11_673, 11_438, 11_206, 10_978, 10_754, 10_534,
    10_317, 10_103, 9893, 9686, 9483, 9282, 9084, 8890, 8698, 8509, 8323, 8140, 7959, 7781, 7606, 7433, 7262, 7094,
    6928, 6764, 6603, 6444, 6286, 6131, 5978, 5828, 5679, 5532, 4824, 4159, 3534, 2943, 2386, 1858, 1357, 882, 430,
    0, -410, -802, -1177, -1536, -1880, -2210, -2526, -2831, -3124, -3406, -3678, -3941, -4194, -4439, -4676, -4905,
    -5127, -5342, -5550, -5753, -5949, -6140, -6326, -6506, -6682, -6853, -7020, -7183, -7341, -7496, -7647, -7795,
    -7939, -8080, -8218, -8353, -8486, -8615, -8742, -8866, -8988, -9108, -9225, -9340, -9453, -9564, -9674, -9781,
    -9887, -9990, -10_093, -10_193, -10_292, -10_390, -10_486, -10_581, -10_674, -10_766, -10_857, -10_947,
    -10_947, -10_947,
];

/// How far the LFO may raise the pole per Manual value, over 128. Low values sweep less, so the pole stays under
/// about 0.9 at full depth.
#[rustfmt::skip]
const MANUAL_SWEEPS: [u8; 59] = [
    16, 17, 19, 20, 22, 23, 25, 26, 28, 29, 30, 32, 33, 34, 36, 37, 38, 40, 41, 42, 44, 47, 49, 51, 53, 55, 57, 60,
    62, 64, 66, 69, 71, 73, 75, 77, 79, 81, 83, 85, 87, 89, 91, 92, 94, 96, 97, 99, 101, 102, 103, 104, 106, 107,
    108, 110, 118, 123, 127,
];

/// The number of first order all-pass sections per channel.
const SECTIONS: usize = 8;

/// The phaser: per channel the input plus the resonance feedback, a DC blocker and eight equal first order
/// all-pass sections. Mixed with the dry signal (by the effect) the chain's 180 degree points become notches.
///
/// One LFO moves the pole of every section and channel. It raises the pole from the Manual position, so the notches
/// sweep down. The sweep is linear in the 32 kHz pole, so the pole is worked out at 32 kHz and moved to the host rate
/// with the same delay at low frequencies.
pub struct Phaser {
    lfo: Lfo,
    /// 32 kHz over the host rate.
    scale: f64,
    /// The feedback read, one 32 kHz sample back: the whole host samples (at least the one of the loop) and the
    /// fraction toward the next older output.
    feedback_delay: usize,
    feedback_fraction: f64,
    /// The outputs kept per channel, enough for the feedback read (two up to 64 kHz).
    history_length: usize,
    dc_blockers: Vec<FirstOrderFilter>,
    /// The all-pass states, `SECTIONS` per channel.
    states: Vec<f64>,
    /// The chain outputs of the last samples per channel, newest first.
    history: Vec<f64>,
    coefficient: f64,
    manual: f64,
    sweep: f64,
    depth: f64,
    resonance: f64,
}

impl Phaser {
    /// `channels` is the number of chains.
    pub fn new(sample_rate: f64, channels: usize) -> Self {
        let delay = 1.0f64.max(sample_rate / 32_000.0);
        let feedback_delay = delay.floor();
        let history_length = feedback_delay as usize + 1;
        let dc_blockers = (0..channels)
            .map(|_| {
                let mut f = FirstOrderFilter::new(sample_rate);
                f.set_high_pass(DC_BLOCKER_HZ, 1.0);
                f
            })
            .collect();
        Self {
            lfo: Lfo::new(sample_rate),
            scale: 32_000.0 / sample_rate,
            feedback_delay: feedback_delay as usize,
            feedback_fraction: delay - feedback_delay,
            history_length,
            dc_blockers,
            states: vec![0.0; channels * SECTIONS],
            history: vec![0.0; channels * history_length],
            coefficient: 0.0,
            manual: 0.0,
            sweep: 0.0,
            depth: 0.0,
            resonance: 0.0,
        }
    }

    /// The Manual parameter (0-127).
    pub fn set_manual(&mut self, value: u8) {
        self.manual = (0.45 * MANUAL_POLES[value as usize] as f64) / 8192.0;
        self.sweep = (0.9 * MANUAL_SWEEPS[value.min(58) as usize] as f64) / 128.0;
    }

    pub fn set_rate(&mut self, hz: f64) {
        self.lfo.set_rate(hz);
    }

    /// The depth, 0 to 1 (the share of the Manual's sweep).
    pub fn set_depth(&mut self, depth: f64) {
        self.depth = depth;
    }

    /// The Resonance parameter (0-127).
    pub fn set_resonance(&mut self, value: u8) {
        self.resonance = resonance_gain(value);
    }

    /// Moves the LFO and the pole on by one sample. Call once per sample before `process`.
    #[inline]
    pub fn step(&mut self) {
        let pole = self.manual + self.sweep * self.depth * self.lfo.process().abs();
        // A section delays low frequencies by 1 / (fs t) seconds, so t scales with the rate
        let t = ((1.0 - pole) / (1.0 + pole)) * self.scale;
        self.coefficient = (t - 1.0) / (t + 1.0);
    }

    /// Runs one sample through a chain and returns the chain output.
    #[inline]
    pub fn process(&mut self, channel: usize, x: f64) -> f64 {
        let a = self.coefficient;
        let h = channel * self.history_length;
        // The feedback is one 32 kHz sample old: between two outputs at higher rates
        let newer = self.history[h + self.feedback_delay - 1];
        let feedback = newer + (self.history[h + self.feedback_delay] - newer) * self.feedback_fraction;
        let mut y = self.dc_blockers[channel].process(x + self.resonance * feedback);
        let end = (channel + 1) * SECTIONS;
        for i in channel * SECTIONS..end {
            let out = a * y + self.states[i];
            self.states[i] = y - a * out;
            y = out;
        }
        for i in (h + 1..h + self.history_length).rev() {
            self.history[i] = self.history[i - 1];
        }
        self.history[h] = y;
        y
    }

    pub fn clear(&mut self) {
        self.lfo.clear();
        self.states.fill(0.0);
        self.history.fill(0.0);
        for dc_blocker in &mut self.dc_blockers {
            dc_blocker.clear();
        }
    }
}
