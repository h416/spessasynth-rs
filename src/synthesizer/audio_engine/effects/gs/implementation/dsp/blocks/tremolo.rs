//! Tremolo and auto pan: an LFO that moves the gain of both channels.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/tremolo.ts

use super::lfo::{Lfo, parabolic_sine};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::enums::tremolo_waves;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::js_round;

/// The corner of the smoother after the wave (it rounds the edges of the square and the saws, about 3 ms).
const SMOOTHER_HZ: f64 = 50.0;

/// The unmodulated part of a tremolo gain: (127 - 128 depth) / 2 rounded down, over 64. So the peak sits just under
/// 1 and the deepest setting closes fully. `depth` is the modulated part, 0 to 127/128.
pub fn tremolo_rest(depth: f64) -> f64 {
    ((127.0 - js_round(128.0 * depth)) / 2.0).floor() / 64.0
}

/// Tremolo and auto pan: an LFO that moves the gain of both channels, in phase (tremolo) or against each other
/// (auto pan, a linear crossfade). Call `process` once per sample, then read `left` and `right`.
pub struct Tremolo {
    /// The gains of this sample.
    pub left: f64,
    pub right: f64,
    lfo: Lfo,
    smoothing: f64,
    pan: bool,
    wave: u8,
    depth: f64,
    /// The unmodulated part of the gain.
    rest: f64,
    smoothed: f64,
}

impl Tremolo {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            left: 1.0,
            right: 1.0,
            lfo: Lfo::new(sample_rate),
            smoothing: 1.0 - ((-2.0 * std::f64::consts::PI * SMOOTHER_HZ) / sample_rate).exp(),
            pan: false,
            wave: 0,
            depth: 0.0,
            rest: 1.0,
            smoothed: 0.0,
        }
    }

    /// True for auto pan.
    pub fn set_pan(&mut self, pan: bool) {
        self.pan = pan;
    }

    /// The wave (`tremolo_waves`).
    pub fn set_wave(&mut self, wave: u8) {
        self.wave = wave;
    }

    pub fn set_rate(&mut self, hz: f64) {
        self.lfo.set_rate(hz);
    }

    /// The modulated part of the gain, 0 (none) to 127/128 (down to silence).
    pub fn set_depth(&mut self, depth: f64) {
        self.depth = depth;
        self.rest = tremolo_rest(depth);
    }

    #[inline]
    pub fn process(&mut self) {
        let phase = self.lfo.process();
        let w = match self.wave {
            tremolo_waves::TRIANGLE => 2.0 * phase.abs() - 1.0,
            tremolo_waves::SQUARE => {
                if phase >= 0.0 {
                    1.0
                } else {
                    -1.0
                }
            }
            tremolo_waves::SINE => parabolic_sine(phase),
            tremolo_waves::SAW_UP => phase,
            _ => -phase,
        };
        self.smoothed += self.smoothing * (w - self.smoothed);
        let up = 0.5 + 0.5 * self.smoothed;
        self.left = self.depth * up + self.rest;
        self.right = if self.pan {
            self.depth * (1.0 - up) + self.rest
        } else {
            self.left
        };
    }

    pub fn clear(&mut self) {
        self.lfo.clear();
        self.smoothed = 0.0;
    }
}
