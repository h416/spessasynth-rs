/// synth_constants.rs
/// purpose: Synthesizer-wide constants.
/// Ported from: src/synthesizer/audio_engine/engine_components/synth_constants.ts
use std::sync::OnceLock;

use crate::synthesizer::types::{SynthMethodOptions, SynthSystem};

/// Synthesizer's default voice cap.
/// Equivalent to: VOICE_CAP
pub const VOICE_CAP: u32 = 350;

/// Default MIDI drum channel (0-indexed).
/// Equivalent to: DEFAULT_PERCUSSION
pub const DEFAULT_PERCUSSION: u8 = 9;

/// MIDI channel count.
/// Equivalent to: MIDI_CHANNEL_COUNT
pub const MIDI_CHANNEL_COUNT: u8 = 16;

/// Default bank select and SysEx mode.
/// Equivalent to: DEFAULT_SYNTH_MODE
pub const DEFAULT_SYNTH_MODE: SynthSystem = SynthSystem::Gs;

/// Sentinel value meaning "all channels" or "a different action applies".
/// Equivalent to: ALL_CHANNELS_OR_DIFFERENT_ACTION
pub const ALL_CHANNELS_OR_DIFFERENT_ACTION: i32 = -1;

/// A process-unique identifier for the embedded sound bank,
/// used to prevent it from being accidentally deleted.
/// Equivalent to: EMBEDDED_SOUND_BANK_ID (uses Math.random() in TS)
static EMBEDDED_SOUND_BANK_ID_STORAGE: OnceLock<String> = OnceLock::new();

pub fn embedded_sound_bank_id() -> &'static str {
    EMBEDDED_SOUND_BANK_ID_STORAGE.get_or_init(|| {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        format!("SPESSASYNTH_EMBEDDED_BANK_{}_DO_NOT_DELETE", nanos)
    })
}

/// Generator value sentinel meaning "no override / no change".
/// Matches i16::MAX (32767), the same as Int16Array sentinel in TS.
/// Equivalent to: GENERATOR_OVERRIDE_NO_CHANGE_VALUE
pub const GENERATOR_OVERRIDE_NO_CHANGE_VALUE: i16 = 32_767;

/// Default SynthMethodOptions (schedule at time 0).
/// Equivalent to: DEFAULT_SYNTH_METHOD_OPTIONS
pub const DEFAULT_SYNTH_METHOD_OPTIONS: SynthMethodOptions = SynthMethodOptions { time: 0.0 };

/// Minimum note length in seconds.
/// Notes released faster than this are extended to this duration
/// (prevents instant note-off issues on drum channels).
/// Equivalent to: MIN_NOTE_LENGTH
pub const MIN_NOTE_LENGTH: f64 = 0.03;

/// Minimum exclusive class cutoff length in seconds.
/// Equivalent to: MIN_EXCLUSIVE_LENGTH
pub const MIN_EXCLUSIVE_LENGTH: f64 = 0.07;

/// Overall synthesizer output gain.
/// Equivalent to: SYNTHESIZER_GAIN
pub const SYNTHESIZER_GAIN: f64 = 1.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthesizer::enums::interpolation_types;

    // --- VOICE_CAP ---

    #[test]
    fn test_voice_cap() {
        assert_eq!(VOICE_CAP, 350);
    }

    // --- DEFAULT_PERCUSSION ---

    #[test]
    fn test_default_percussion() {
        assert_eq!(DEFAULT_PERCUSSION, 9);
    }

    // --- MIDI_CHANNEL_COUNT ---

    #[test]
    fn test_midi_channel_count() {
        assert_eq!(MIDI_CHANNEL_COUNT, 16);
    }

    // --- DEFAULT_SYNTH_MODE ---

    #[test]
    fn test_default_synth_mode_is_gs() {
        assert_eq!(DEFAULT_SYNTH_MODE, SynthSystem::Gs);
    }

    // --- ALL_CHANNELS_OR_DIFFERENT_ACTION ---

    #[test]
    fn test_all_channels_or_different_action() {
        assert_eq!(ALL_CHANNELS_OR_DIFFERENT_ACTION, -1);
    }

    // --- embedded_sound_bank_id ---

    #[test]
    fn test_embedded_sound_bank_id_format() {
        let id = embedded_sound_bank_id();
        assert!(id.starts_with("SPESSASYNTH_EMBEDDED_BANK_"));
        assert!(id.ends_with("_DO_NOT_DELETE"));
    }

    #[test]
    fn test_embedded_sound_bank_id_stable() {
        // OnceLock: calling twice returns the same value
        let id1 = embedded_sound_bank_id();
        let id2 = embedded_sound_bank_id();
        assert_eq!(id1, id2);
    }

    // --- GENERATOR_OVERRIDE_NO_CHANGE_VALUE ---

    #[test]
    fn test_generator_override_no_change_value() {
        assert_eq!(GENERATOR_OVERRIDE_NO_CHANGE_VALUE, i16::MAX);
    }

    // --- DEFAULT_SYNTH_METHOD_OPTIONS ---

    #[test]
    fn test_default_synth_method_options_time_is_zero() {
        assert_eq!(DEFAULT_SYNTH_METHOD_OPTIONS.time, 0.0);
    }

    // --- MIN_NOTE_LENGTH ---

    #[test]
    fn test_min_note_length() {
        assert!((MIN_NOTE_LENGTH - 0.03).abs() < f64::EPSILON);
    }

    // --- MIN_EXCLUSIVE_LENGTH ---

    #[test]
    fn test_min_exclusive_length() {
        assert!((MIN_EXCLUSIVE_LENGTH - 0.07).abs() < f64::EPSILON);
    }

    #[test]
    fn test_min_exclusive_length_greater_than_min_note_length() {
        assert!(MIN_EXCLUSIVE_LENGTH > MIN_NOTE_LENGTH);
    }

    // --- SYNTHESIZER_GAIN ---

    #[test]
    fn test_synthesizer_gain() {
        assert!((SYNTHESIZER_GAIN - 1.0).abs() < f64::EPSILON);
    }

    // --- consistency checks ---

    #[test]
    fn test_default_percussion_within_channel_count() {
        assert!((DEFAULT_PERCUSSION as u32) < (MIDI_CHANNEL_COUNT as u32));
    }

    #[test]
    fn test_default_synth_mode_matches_synth_system_default() {
        // DEFAULT_SYNTH_MODE must equal SynthSystem::default() (Gs)
        assert_eq!(DEFAULT_SYNTH_MODE, SynthSystem::default());
    }

    // verify SynthMethodOptions is usable as a const
    const _OPTS: SynthMethodOptions = DEFAULT_SYNTH_METHOD_OPTIONS;
    #[test]
    fn test_default_synth_method_options_is_const_usable() {
        assert_eq!(_OPTS.time, 0.0);
    }
}
