//! Lo-Fi 1 (`01 72`): compressor, low pass, quantizer, high pass and dip, mixed with the dry input.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/lo_fi_1.ts

use std::sync::LazyLock;

use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::biquad::Biquad;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::delay_line::DelayLine;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::dynamics::{Compressor, detector_gain, recovery_rate};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::lofi_quantizer::LoFiQuantizer;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_mix::MonoMix;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::mono_pan::MonoPan;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::shelf_eq::{ShelfEq, shelf_corners};

/// The gain after the compressor per pre filter (1 bypasses the compressor).
const PRE_GAINS: [f64; 6] = [1.0, 2.0, 2.0, 4.0, 4.0, (2.0 * 127.0) / 32.0];

/// The pre filters' low pass: the hardware's bytes (b = 37/128, pole 53/128), 4.0 kHz with -0.11 dB at DC.
const PRE_POLE: f64 = 53.0 / 128.0;
static PRE_HZ: LazyLock<f64> =
    LazyLock::new(|| (32_000.0 / std::f64::consts::PI) * ((1.0 - PRE_POLE) / (1.0 + PRE_POLE)).atan());
const PRE_GAIN: f64 = (2.0 * 37.0) / 128.0 / (1.0 - PRE_POLE);

/// A post filter: the high pass as its bytes (b0, pole) and the dip at 1 kHz (-12 dB), its poles and zeros at one
/// frequency with the Q of each.
struct PostFilter {
    b0: f64,
    pole: f64,
    hz: f64,
    pole_q: f64,
    zero_q: f64,
}

/// The post filters (1-6).
const POST_FILTERS: [Option<PostFilter>; 6] = [
    None,
    Some(PostFilter { b0: 121.0, pole: 113.0, hz: 978.065, pole_q: 1.0018, zero_q: 4.115_52 }),
    Some(PostFilter { b0: 110.0, pole: 93.0, hz: 978.065, pole_q: 1.0018, zero_q: 4.115_52 }),
    Some(PostFilter { b0: 121.0, pole: 113.0, hz: 997.345, pole_q: 0.504_22, zero_q: 1.961_36 }),
    Some(PostFilter { b0: 110.0, pole: 93.0, hz: 997.345, pole_q: 0.504_22, zero_q: 1.961_36 }),
    Some(PostFilter { b0: 121.0, pole: 113.0, hz: 992.619, pole_q: 0.255_73, zero_q: 1.022_92 }),
];

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x01, 0x05, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x7f, 0x40, 0x40, 0x40, 0x7f,
];
const LIMITS: &[(u8, u8)] = &[(0, 5), (1, 8), (2, 5)];

/// Lo-Fi 1 (`01 72`): `L+R -> compressor (pre filter) -> low pass -> quantizer -> high pass and dip (post filter)`,
/// mixed with the mono input by the balance, then the EQ and the pan.
/// Parameters: 1 pre filter (1 dry, 2-6 the compressor with a gain of 2 to 7.94, 3 and 5 with a 4 kHz low pass),
/// 2 lo-fi type (15 or 11 bits at 16000, 10667 or 8000 Hz), 3 post filter (high pass at 633 or 1595 Hz and a dip at
/// 1 kHz of Q 2, 1 or 0.5), 16 balance (the pan law), 17 low gain, 18 high gain, 19 pan, 20 level. EFX control source
/// 1 modulates the balance, source 2 the pan.
pub struct GSLoFi1 {
    base: GSEffectBase,
    mix: MonoMix,
    compressor: Compressor,
    /// The compressor's own DC blocker, for the dry path and the pre filter that bypasses it.
    dc_blocker: FirstOrderFilter,
    low_pass: FirstOrderFilter,
    quantizer: LoFiQuantizer,
    high_pass: FirstOrderFilter,
    dip: Biquad,
    shelves: ShelfEq,
    pan: MonoPan,
    dry_line: DelayLine,
    compressed: bool,
    post_on: bool,
    dry: f64,
    wet: f64,
}

impl GSLoFi1 {
    pub fn new(sample_rate: f64, _memory: &mut DelayMemory) -> Self {
        let mut compressor = Compressor::new(sample_rate, 0.001);
        // The Compressor's defaults: sustain 72, attack 100
        compressor.set_recovery_rate(recovery_rate(72));
        compressor.set_makeup(detector_gain(100));
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        let quantizer = LoFiQuantizer::new(sample_rate);
        let dry_line = DelayLine::new(quantizer.latency);
        let mut effect = Self {
            base: GSEffectBase::new(sample_rate, None, DEFAULTS, LIMITS, &[]),
            mix: MonoMix::new(sample_rate),
            compressor,
            dc_blocker,
            low_pass: FirstOrderFilter::new(sample_rate),
            quantizer,
            high_pass: FirstOrderFilter::new(sample_rate),
            dip: Biquad::new(sample_rate),
            shelves: ShelfEq::new(sample_rate),
            pan: MonoPan::new(sample_rate),
            dry_line,
            compressed: false,
            post_on: false,
            dry: 0.0,
            wet: 0.0,
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSLoFi1 {
    gs_effect_common!(0x01_72);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        for i in 0..sample_count {
            // The program saturates its stage like the Compressor and the Limiter
            let x = self.mix.process(
                saturate_at(input_left[i] as f64, SATURATED_INPUT),
                saturate_at(input_right[i] as f64, SATURATED_INPUT),
            ) * MONO_INPUT;
            let now = self.dc_blocker.process(x);
            // The dry path waits for the quantizer's latency
            self.dry_line.write(now);
            let h = self.dry_line.read(self.quantizer.latency as isize);
            // Both paths run, so a pre filter change does not click
            let c = self.compressor.process(x);
            let filtered = self.low_pass.process(if self.compressed { c } else { now });
            let mut y = self.quantizer.process((127.0 / 128.0) * filtered);
            if self.post_on {
                // Every operation saturates: after pre filter 6 the dip clips
                y = saturate(self.dip.process(saturate(self.high_pass.process(y))));
            }
            self.pan.process(self.shelves.process(self.wet * y + self.dry * h));
            output_left[i] = saturate_at(self.pan.left, SATURATED_OUTPUT) as f32;
            output_right[i] = saturate_at(self.pan.right, SATURATED_OUTPUT) as f32;
        }
    }

    fn clear(&mut self) {
        self.mix.clear();
        self.compressor.clear();
        self.dc_blocker.clear();
        self.dry_line.clear();
        self.low_pass.clear();
        self.quantizer.clear();
        self.high_pass.clear();
        self.dip.clear();
        self.shelves.clear();
        self.pan.clear();
    }

    fn update(&mut self) {
        let p = self.base.parameters;
        self.compressed = p[0] > 0;
        self.compressor.set_post_gain(PRE_GAINS[p[0] as usize]);
        if p[0] == 2 || p[0] == 4 {
            self.low_pass.set_low_pass(*PRE_HZ, PRE_GAIN);
        } else {
            self.low_pass.set_gain(127.0 / 128.0);
        }
        self.quantizer.set_type(p[1]);
        let post = &POST_FILTERS[p[2] as usize];
        self.post_on = post.is_some();
        if let Some(post) = post {
            let pole = post.pole / 128.0;
            self.high_pass.set_high_pass(
                (32_000.0 / std::f64::consts::PI) * ((1.0 - pole) / (1.0 + pole)).atan(),
                (2.0 * post.b0) / 128.0 / (1.0 + pole),
            );
            self.dip.set_pole_zero(post.hz, post.pole_q, post.hz, post.zero_q);
        }
        let balance = self.base.controlled(0, 15);
        self.dry = pan_left(balance);
        self.wet = pan_right(balance);
        self.shelves.set_low(shelf_corners::LOW200, shelf_gain(p[16]));
        self.shelves.set_high(shelf_corners::HIGH4K, shelf_gain(p[17]));
        self.pan.set(self.base.controlled(1, 18), level_gain(p[19]) * STAGE_OUTPUT);
    }
}
