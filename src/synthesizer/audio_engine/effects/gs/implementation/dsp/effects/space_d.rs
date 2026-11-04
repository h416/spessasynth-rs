//! Space D (`01 43`): a multi tap chorus whose outputs are the difference of the two lines.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/space_d.ts

use super::prelude::*;
use super::stereo_modulated_delay::GSStereoModulatedDelay;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{STEP_MS, chorus_phase, modulation_depth, pre_delay, rate1};

/// The right line's extra delay: 16 samples at 32 kHz.
const RIGHT_MS: f64 = 16.0 * STEP_MS;

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x20, 0x08, 0x7f, 0x5a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x40, 0x40, 0x40, 0x00, 0x60,
];

/// Space D (`01 43`): a multi tap chorus. One LFO moves a read on each channel's line, the right one 0.5 ms later
/// and offset by the phase. Each output is the difference of the two lines, so a centered input partly cancels.
///
/// The shared stage reads the left input one 32 kHz sample late and writes the right output one sample late, on
/// the hardware too (the SC-88Pro's right output lags 27 us with a left input). Space D mixes both inputs into each
/// output, so that shows: each line has a second read one sample later for the right output, and the left output
/// reads the right line one sample early.
/// Parameters: 1 pre delay, 2 rate (Rate1 column), 3 depth, 4 phase, 16 balance, 17 low gain, 18 high gain,
/// 20 level. EFX control source 1 modulates the rate, source 2 the balance.
pub struct GSSpaceD {
    md: GSStereoModulatedDelay,
}

impl GSSpaceD {
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> Self {
        let mut effect = Self {
            md: GSStereoModulatedDelay::new(sample_rate, memory, 2, DEFAULTS, &[]),
        };
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

impl GSEffect for GSSpaceD {
    gs_effect_common!(0x01_43; md.base);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    ) {
        let md = &mut self.md;
        for i in 0..sample_count {
            let phase = md.lfo.process();
            let l = input_left[i] as f64;
            let r = input_right[i] as f64;
            md.delay_l.process(l * LINE_INPUT, phase);
            md.delay_r.process(r * LINE_INPUT, phase);
            md.output.process(
                l,
                md.delay_l.output[0] - md.delay_r.output[0],
                r,
                md.delay_r.output[1] - md.delay_l.output[1],
            );
            output_left[i] = md.output.left as f32;
            output_right[i] = md.output.right as f32;
        }
    }

    fn clear(&mut self) {
        self.md.clear();
    }

    fn update(&mut self) {
        let md = &mut self.md;
        let p = md.base.parameters;
        md.lfo.set_rate(rate1(md.base.controlled(0, 1)));
        // A quarter of the chorus depth
        let depth = modulation_depth(p[2]) / 4.0;
        let pre = pre_delay(p[0]);
        for read in 0..2 {
            md.delay_l.set_pre_delay(read, pre + read as f64 * STEP_MS);
            md.delay_r
                .set_pre_delay(read, pre + RIGHT_MS + (read as f64 - 1.0) * STEP_MS);
            md.delay_l.set_depth(read, depth);
            md.delay_r.set_depth(read, depth);
            md.delay_r.set_phase(read, chorus_phase(p[3]));
        }
        let balance = md.base.controlled(1, 15);
        md.set_output(balance, p[16], p[17], p[19]);
    }
}
