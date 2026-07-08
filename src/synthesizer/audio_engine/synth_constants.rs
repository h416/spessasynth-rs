/// synth_constants.rs
/// purpose: Synthesizer-wide constants.
/// Ported from: src/synthesizer/audio_engine/synth_constants.ts (spessasynth_core 4.3.0;
/// moved out of engine_components/ in the upstream 4.3.0 restructuring)
///
/// Changes from 4.2.0 (reviewed against the 4.3.0 diff):
/// - Removed: `MIDI_CHANNEL_COUNT` (callers now use a literal 16), and
///   `ALL_CHANNELS_OR_DIFFERENT_ACTION` (callers now use a literal -1, matching the
///   `deviceID: -1` default in `parameters/system.ts`).
/// - `SYNTHESIZER_GAIN` (1) was replaced by `SPESSASYNTH_GAIN_FACTOR` (0.6), which is applied
///   inside the channel's `updateInternalParams()` gain computation. TODO(Task 21, channel
///   restructuring): the constant is defined here but not yet wired into the render path —
///   doing so requires the 4.3.0 channel gain model and WILL change WAV output.
/// - Added: `EFX_SENDS_GAIN_CORRECTION`, `CONTROLLER_TABLE_SIZE` (128 — NOT the same constant
///   as the 4.2.0-era `channel/parameters/midi.rs::CONTROLLER_TABLE_SIZE` = 147, which appends
///   non-CC modulator-source slots and remains in use by the pre-4.3.0 channel code until
///   Task 21), and `SPESSA_BUFSIZE` documentation updates.
use std::sync::OnceLock;

use crate::soundbank::types::MIDISystem;
use crate::synthesizer::types::SynthMethodOptions;

/// Synthesizer's default voice cap.
/// Equivalent to: VOICE_CAP
pub const VOICE_CAP: u32 = 350;

/// Default MIDI drum channel (0-indexed).
/// Equivalent to: DEFAULT_PERCUSSION
pub const DEFAULT_PERCUSSION: u8 = 9;

/// Default bank select and SysEx mode.
/// Equivalent to: DEFAULT_SYNTH_MODE
pub const DEFAULT_SYNTH_MODE: MIDISystem = MIDISystem::Gs;

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
/// If the note is released faster than that, it is forced to last that long.
/// This is used mostly for drum channels, where a lot of midis like to send instant note off
/// after a note on.
/// Equivalent to: MIN_NOTE_LENGTH
pub const MIN_NOTE_LENGTH: f64 = 0.03;

/// This sounds way nicer for an instant hi-hat cutoff.
/// Equivalent to: MIN_EXCLUSIVE_LENGTH
pub const MIN_EXCLUSIVE_LENGTH: f64 = 0.07;

/// This panning factor ensures that spessasynth doesn't stay too loud.
/// You can set the `gain` system parameter to an inverse of it to negate the effect.
/// Equivalent to: SPESSASYNTH_GAIN_FACTOR (new in TS 4.3.0; see module doc TODO — not yet
/// wired into the render path)
pub const SPESSASYNTH_GAIN_FACTOR: f64 = 0.6;

/// The default buffer size for the synthesizer, in samples.
/// Equivalent to: SPESSA_BUFSIZE
pub const SPESSA_BUFSIZE: usize = 128;

/// This is needed because effects (regular ones) are sent straight from the mono signal,
/// whereas insertion effects receive the panned audio (twice), which reduces gain by a factor
/// of cos(pi/4) * cos(pi/4) (master pan + voice pan). This reverses it.
/// Equivalent to: EFX_SENDS_GAIN_CORRECTION = 1 / Math.cos(Math.PI / 4) ** 2 (= 2.0)
pub const EFX_SENDS_GAIN_CORRECTION: f64 = 2.0;

/// The amount of MIDI controllers (the 128 real MIDI CCs).
/// Equivalent to: CONTROLLER_TABLE_SIZE (new in TS 4.3.0's synth_constants.ts; distinct from
/// the 4.2.0-era 147-entry extended table in `channel/parameters/midi.rs` — see module doc)
pub const CONTROLLER_TABLE_SIZE: usize = 128;

/// RPN NULL per MIDI spec.
/// Equivalent to: DEFAULT_RPN
pub const DEFAULT_RPN: u8 = 0x7f;

/// No NRPN is bound to 0 0, while 0x7f MSB is AWE32!
/// Equivalent to: DEFAULT_NRPN
pub const DEFAULT_NRPN: u8 = 0;

#[cfg(test)]
mod tests {
    use super::*;

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

    // --- DEFAULT_SYNTH_MODE ---

    #[test]
    fn test_default_synth_mode_is_gs() {
        assert_eq!(DEFAULT_SYNTH_MODE, MIDISystem::Gs);
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

    // --- MIN_NOTE_LENGTH / MIN_EXCLUSIVE_LENGTH ---

    #[test]
    fn test_min_note_length() {
        assert!((MIN_NOTE_LENGTH - 0.03).abs() < f64::EPSILON);
    }

    #[test]
    fn test_min_exclusive_length() {
        assert!((MIN_EXCLUSIVE_LENGTH - 0.07).abs() < f64::EPSILON);
    }

    #[test]
    fn test_min_exclusive_length_greater_than_min_note_length() {
        assert!(MIN_EXCLUSIVE_LENGTH > MIN_NOTE_LENGTH);
    }

    // --- SPESSASYNTH_GAIN_FACTOR ---

    #[test]
    fn test_spessasynth_gain_factor() {
        assert!((SPESSASYNTH_GAIN_FACTOR - 0.6).abs() < f64::EPSILON);
    }

    // --- SPESSA_BUFSIZE ---

    #[test]
    fn test_spessa_bufsize() {
        assert_eq!(SPESSA_BUFSIZE, 128);
    }

    // --- EFX_SENDS_GAIN_CORRECTION ---

    #[test]
    fn test_efx_sends_gain_correction_matches_formula() {
        let expected = 1.0 / (std::f64::consts::PI / 4.0).cos().powi(2);
        assert!((EFX_SENDS_GAIN_CORRECTION - expected).abs() < 1e-12);
        assert!((EFX_SENDS_GAIN_CORRECTION - 2.0).abs() < 1e-12);
    }

    // --- CONTROLLER_TABLE_SIZE ---

    #[test]
    fn test_controller_table_size_is_128() {
        assert_eq!(CONTROLLER_TABLE_SIZE, 128);
    }

    // --- DEFAULT_RPN / DEFAULT_NRPN ---

    #[test]
    fn test_default_rpn() {
        assert_eq!(DEFAULT_RPN, 0x7f);
    }

    #[test]
    fn test_default_nrpn() {
        assert_eq!(DEFAULT_NRPN, 0);
    }

    // --- consistency checks ---

    #[test]
    fn test_default_percussion_within_16_channels() {
        assert!((DEFAULT_PERCUSSION as u32) < 16);
    }

    #[test]
    fn test_default_synth_mode_matches_synth_system_default() {
        // DEFAULT_SYNTH_MODE must equal MIDISystem::default() (Gs)
        assert_eq!(DEFAULT_SYNTH_MODE, MIDISystem::default());
    }

    // verify SynthMethodOptions is usable as a const
    const _OPTS: SynthMethodOptions = DEFAULT_SYNTH_METHOD_OPTIONS;
    #[test]
    fn test_default_synth_method_options_is_const_usable() {
        assert_eq!(_OPTS.time, 0.0);
    }
}
