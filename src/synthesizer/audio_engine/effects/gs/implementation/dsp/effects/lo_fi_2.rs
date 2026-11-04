//! Lo-Fi 2 (`01 73`): per chain `DC blocker -> shaper -> filter -> radio`, plus the noise sources.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/lo_fi_2.ts

use std::sync::LazyLock;

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_pan::MonoPan;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::noise_generator::{NoiseGenerator, set_column_high_pass, set_column_low_pass};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::shelf_eq::{ShelfEq, shelf_corners};

/// The chains run on the doubled input with the shared stage's 127/128 twice (this type uses the alternate routing)
/// and one more 127/128.
static INPUT_GAIN: LazyLock<f64> = LazyLock::new(|| 2.0 * (127.0f64 / 128.0).powf(3.0));

/// The output half: the shared stage halves the output again, with its last 127/128 (the chains' own 127/128 is not
/// part of the stage).
static OUTPUT_GAIN: LazyLock<f64> = LazyLock::new(|| (STAGE_GAIN * 127.0) / 128.0 / *INPUT_GAIN);

/// The shaper per Lo-Fi type (1-6): `a x + b saturate(4 x abs(x)) + c saturate(16 x^3)` over 128, per chain (the
/// hardware's tables differ by a step here and there). Type 1 is linear, 6 the hardest.
#[rustfmt::skip]
const SHAPE_BYTES: [[[u8; 3]; 6]; 2] = [
    [[92, 0, 0], [72, 23, 3], [66, 19, 12], [55, 26, 17], [37, 37, 37], [38, 75, 15]],
    [[92, 0, 0], [73, 22, 4], [67, 18, 13], [56, 25, 18], [38, 36, 38], [39, 74, 16]]
];
static SHAPES: LazyLock<[[[f64; 3]; 6]; 2]> =
    LazyLock::new(|| SHAPE_BYTES.map(|chain| chain.map(|shape| shape.map(|v| v as f64 / 128.0))));

/// The radio's modulator (the SC-88Pro's, from detune 125): one sine at 32000 / 17 Hz.
const RADIO_HZ: f64 = 32_000.0 / 17.0;

/// The modulator's amplitude.
const RADIO_DEPTH: f64 = 0.156;

/// The radio noise's gain after its low pass.
const RADIO_NOISE_GAIN: f64 = 0.078;

/// One chain: DC blocker, shaper, x 2 into the filter.
struct LoFiShaperChain {
    dry: f64,
    dc_blocker: FirstOrderFilter,
    filter: FirstOrderFilter,
    shape: [f64; 3],
}

impl LoFiShaperChain {
    fn new(sample_rate: f64) -> Self {
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        Self {
            dry: 0.0,
            dc_blocker,
            filter: FirstOrderFilter::new(sample_rate),
            shape: [1.0, 0.0, 0.0],
        }
    }

    /// `shape` is the shaper's a, b and c, `type` the filter type (0 off, 1 LPF, 2 HPF), `cutoff` the Cut column
    /// value.
    fn set(&mut self, shape: [f64; 3], r#type: u8, cutoff: u8) {
        self.shape = shape;
        if r#type == 1 {
            set_column_low_pass(&mut self.filter, cutoff, false);
        } else if r#type == 2 {
            set_column_high_pass(&mut self.filter, cutoff);
        } else {
            self.filter.set_gain(127.0 / 128.0);
        }
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        self.dry = saturate(self.dc_blocker.process(x));
        let h = self.dry;
        let shape = self.shape;
        let y = saturate(
            shape[0] * h + shape[1] * saturate(4.0 * h * h.abs()) + shape[2] * saturate(16.0 * h * h * h),
        );
        saturate((127.0 / 128.0) * saturate(self.filter.process(saturate(2.0 * y))))
    }

    fn clear(&mut self) {
        self.dc_blocker.clear();
        self.filter.clear();
        self.dry = 0.0;
    }
}

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x01, 0x01, 0x24, 0x00, 0x40, 0x01, 0x7f, 0x00, 0x00, 0x7f, 0x00, 0x00,
    0x7f, 0x00, 0x01, 0x7f, 0x40, 0x40, 0x40, 0x7f,
];
const LIMITS: &[(u8, u8)] = &[(0, 5), (1, 2), (5, 1), (8, 3), (11, 1), (14, 1)];

/// Lo-Fi 2 (`01 73`): per chain `DC blocker -> shaper -> filter -> radio`, plus the noise sources, mixed with the
/// input by the balance, then the EQ. Stereo runs a chain per side, mono one chain on `L+R` through the pan.
/// Parameters: 1 lo-fi type, 2 filter type (off, LPF, HPF), 3 cutoff (Cut column), 4 radio detune, 5 radio noise
/// level, 6 W/P select, 7 W/P low pass (LPF column), 8 W/P level, 9 disc type (LP, EP, SP, RND), 10 disc low pass,
/// 11 disc level, 12 hum type (50, 60 Hz), 13 hum low pass, 14 hum level, 15 mono/stereo, 16 balance (the pan law),
/// 17 low gain, 18 high gain, 19 pan (mono only), 20 level. EFX control source 1 modulates the detune, source 2 the
/// balance.
pub struct GSLoFi2 {
    base: GSEffectBase,
    mix: MonoMix,
    chain_l: LoFiShaperChain,
    chain_r: LoFiShaperChain,
    noise: NoiseGenerator,
    radio_noise: FirstOrderFilter,
    shelves_l: ShelfEq,
    shelves_r: ShelfEq,
    pan: MonoPan,
    radio_step: f64,
    /// The radio noise's spread at this rate: drawn per host sample, so scaled to the 32 kHz density under its low
    /// pass.
    radio_spread: f64,
    /// The radio's noise of this sample (`radio()` returns the modulator).
    radio_noise_value: f64,
    stereo: bool,
    detune: f64,
    noise_level: f64,
    radio_on: bool,
    radio_phase: f64,
    seed: i32,
    dry: f64,
    wet: f64,
    gain: f64,
}

impl GSLoFi2 {
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> Self {
        // The radio noise: a chaotic map through a one-pole of pole 0.5 at 32 kHz (3.5 kHz)
        let mut radio_noise = FirstOrderFilter::new(sample_rate);
        radio_noise.set_damp(3500.0);
        let mut effect = Self {
            base: GSEffectBase::new(sample_rate, None, DEFAULTS, LIMITS, &[]),
            mix: MonoMix::new(sample_rate),
            chain_l: LoFiShaperChain::new(sample_rate),
            chain_r: LoFiShaperChain::new(sample_rate),
            noise: NoiseGenerator::new(sample_rate),
            radio_noise,
            shelves_l: ShelfEq::new(sample_rate),
            shelves_r: ShelfEq::new(sample_rate),
            pan: MonoPan::new(sample_rate),
            radio_step: RADIO_HZ / sample_rate,
            // RMS 0.49 from a uniform draw of -1 to 1, whose RMS is 1/sqrt(3)
            radio_spread: 0.49 * (3.0 * (sample_rate / 32_000.0)).sqrt(),
            radio_noise_value: 0.0,
            stereo: true,
            detune: 0.0,
            noise_level: 0.0,
            radio_on: false,
            radio_phase: 0.0,
            seed: 7,
            dry: 0.0,
            wet: 0.0,
            gain: 0.0,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }

    /// Steps the radio's modulator and noise, returns the modulator.
    fn radio(&mut self) -> f64 {
        if !self.radio_on {
            return 0.0;
        }
        self.radio_phase += self.radio_step;
        if self.radio_phase >= 1.0 {
            self.radio_phase -= 1.0;
        }
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        // A noise of mean -0.27 and RMS 0.49 like the chaotic map (at 32 kHz), then the 3.5 kHz low pass
        self.radio_noise_value = RADIO_NOISE_GAIN
            * self
                .radio_noise
                .process(self.radio_spread * (self.seed as f64 / 2_147_483_648.0) - 0.27);
        RADIO_DEPTH * (2.0 * std::f64::consts::PI * self.radio_phase).sin()
    }

    /// The radio: below detune 125 a crossfade to silence, from 125 the modulator on the faded signal and the noise.
    /// `x` is the chain's output, `modulator` the modulator's sample.
    fn tune(&self, x: f64, modulator: f64) -> f64 {
        let faded = (1.0 - self.detune) * x;
        if !self.radio_on {
            return faded;
        }
        faded + self.detune * saturate(4.0 * modulator * (faded + self.noise_level * self.radio_noise_value))
    }
}

impl GSEffect for GSLoFi2 {
    gs_effect_common!(0x01_73);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let dry = self.dry;
        let wet = self.wet;
        let gain = self.gain;
        let input_gain = *INPUT_GAIN;
        for i in 0..sample_count {
            let noise = self.noise.process();
            let radio = self.radio();
            if self.stereo {
                let cl = self.chain_l.process(input_left[i] as f64 * input_gain);
                let l = self.tune(cl, radio);
                let cr = self.chain_r.process(input_right[i] as f64 * input_gain);
                let r = self.tune(cr, radio);
                output_left[i] = (self
                    .shelves_l
                    .process(wet * saturate(l + noise) + dry * self.chain_l.dry)
                    * gain) as f32;
                output_right[i] = (self
                    .shelves_r
                    .process(wet * saturate(r + noise) + dry * self.chain_r.dry)
                    * gain) as f32;
            } else {
                let x = self.mix.process(input_left[i] as f64, input_right[i] as f64) * input_gain;
                let c = self.chain_l.process(x);
                let y = self.tune(c, radio);
                let s = self
                    .shelves_l
                    .process(wet * saturate(y + noise) + dry * self.chain_l.dry);
                self.pan.process(s);
                output_left[i] = self.pan.left as f32;
                output_right[i] = self.pan.right as f32;
            }
        }
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.chain_l.clear();
        self.chain_r.clear();
        self.noise.clear();
        self.radio_noise.clear();
        self.shelves_l.clear();
        self.shelves_r.clear();
        self.pan.clear();
        self.radio_phase = 0.0;
        self.seed = 7;
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.chain_l.set(SHAPES[0][p[0] as usize], p[1], p[2]);
        self.chain_r.set(SHAPES[1][p[0] as usize], p[1], p[2]);
        let detune = self.base.controlled(0, 3);
        self.detune = detune as f64 / 128.0;
        // The SC-88Pro only modulates from 125 on, SC-VA sweeps two oscillators with the detune
        self.radio_on = detune >= 125;
        self.noise_level = p[4] as f64 / 128.0;
        self.noise.set_white(p[5] == 1, p[6], level_gain(p[7]));
        self.noise.set_disc(p[8], p[9], level_gain(p[10]));
        self.noise.set_hum(p[11], p[12], level_gain(p[13]));
        self.stereo = p[14] == 1;
        let balance = self.base.controlled(1, 15);
        self.dry = pan_left(balance);
        self.wet = pan_right(balance);
        self.shelves_l.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves_l.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        self.shelves_r.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves_r.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        let level = level_gain(p[19]) * *OUTPUT_GAIN;
        self.pan.set(p[18], level);
        // Mono has the level in the pan
        self.gain = if self.stereo { level } else { 1.0 };
    }
}
