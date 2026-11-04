//! 2Voice Pitch Shifter (`01 60`): two shifted voices with their own pitch, pre delay and pan.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effects/two_voice_pitch_shifter.ts

use super::pitch_shifter_effect::{GSPitchShifterEffect, PitchShifterKind};
use super::prelude::*;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::conversion::{pitch_ratio, pitch_window, pre_delay};

#[rustfmt::skip]
const DEFAULTS: [u8; 20] = [
    0x47, 0x3e, 0x00, 0x7f, 0x3b, 0x42, 0x00, 0x00, 0x02, 0x40, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x30, 0x40, 0x40, 0x00, 0x5f,
];
// The coarse pitches are 40-76 (-24 to +12 semitones)
const LIMITS: &[(u8, u8)] = &[(0, 76), (4, 76), (8, 4)];
const MINIMUMS: &[(u8, u8)] = &[(0, 40), (4, 40)];
const FINE_PARAMETERS: &[u8] = &[1, 5];
/// Twice Fb P.Shifter's: the program adds the 127/128 input to itself, on the SC-88Pro too (its wet is 6 dB
/// above Fb's).
const INPUT_GAIN: f64 = 1.0 + 127.0 / 128.0;

/// 2Voice Pitch Shifter (`01 60`): two shifted voices with their own pitch, pre delay and pan, mixed by the level
/// balance.
/// Parameters: 1 coarse A, 2 fine A, 3 pre delay A, 4 pan A, 5 coarse B, 6 fine B, 7 pre delay B, 8 pan B, 9 mode,
/// 10 level balance (A to B), 16 balance, 17 low gain, 18 high gain, 20 level. EFX control source 1 modulates the
/// coarse pitch of A, source 2 that of B.
pub struct GSTwoVoicePitchShifter;

impl GSTwoVoicePitchShifter {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f64, memory: &mut DelayMemory) -> GSPitchShifterEffect {
        let mut effect = GSPitchShifterEffect::new(
            sample_rate,
            memory,
            2,
            PitchShifterKind::TwoVoice,
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
    let window = pitch_window(p[8]);
    for voice in 0..2 {
        let base = 4 * voice;
        let coarse = 40.max(76.min(effect.base.controlled(voice, base)));
        effect
            .shifter
            .set_voice(voice, pitch_ratio(coarse as f64, p[base + 1] as f64), window);
        effect.shifter.set_pre_delay(voice, pre_delay(p[base + 2]));
        // The level balance: A on its dry side, B on its wet side
        let level = if voice == 0 { balance_dry(p[9]) } else { balance_wet(p[9]) };
        effect.gains_l[voice] = level * pan_left(p[base + 3]);
        effect.gains_r[voice] = level * pan_right(p[base + 3]);
    }
    effect.update_output();
}
