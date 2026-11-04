//! Fb P.Shifter (`01 61`): one shifted voice whose output feeds back into its input.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/feedback_pitch_shifter.ts

use super::pitch_shifter_effect::{GSPitchShifterEffect, PitchShifterKind};
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{feedback_gain, pitch_ratio, pitch_window, pre_delay};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x47, 0x40, 0x54, 0x5f, 0x02, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x40, 0x40, 0x3a, 0x00, 0x7f,
];
// The coarse pitch is 40-76 (-24 to +12 semitones)
const LIMITS: &[(u8, u8)] = &[(0, 76), (4, 4)];
const MINIMUMS: &[(u8, u8)] = &[(0, 40)];
const FINE_PARAMETERS: &[u8] = &[1];
const INPUT_GAIN: f64 = 127.0 / 128.0;

/// Fb P.Shifter (`01 61`): one shifted voice whose output feeds back into its input (the 20 Hz DC blocker in the
/// loop), so the pitch steps up or down with every pass.
/// Parameters: 1 coarse, 2 fine, 3 feedback, 4 pre delay, 5 mode, 6 pan, 16 balance, 17 low gain, 18 high gain,
/// 20 level. EFX control source 1 modulates the coarse pitch, source 2 the feedback.
pub struct GSFeedbackPitchShifter;

impl GSFeedbackPitchShifter {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSPitchShifterEffect {
        let mut effect = GSPitchShifterEffect::new(
            sample_rate,
            memory,
            1,
            PitchShifterKind::Feedback,
            DEFAULTS,
            LIMITS,
            MINIMUMS,
            FINE_PARAMETERS,
            INPUT_GAIN,
        );
        DefaultGSInsertionProcessor::reset(&mut effect);
        effect
    }
}

/// Equivalent to: update
pub(super) fn update(effect: &mut GSPitchShifterEffect) {
    let p = effect.base.parameters;
    let coarse = 40.max(76.min(effect.base.controlled(0, 0)));
    effect
        .shifter
        .set_voice(0, pitch_ratio(coarse as f64, p[1] as f64), pitch_window(p[4]));
    effect
        .shifter
        .set_feedback(feedback_gain(effect.base.controlled(1, 2)));
    effect.shifter.set_pre_delay(0, pre_delay(p[3]));
    effect.gains_l[0] = pan_left(p[5]);
    effect.gains_r[0] = pan_right(p[5]);
    effect.update_output();
}
