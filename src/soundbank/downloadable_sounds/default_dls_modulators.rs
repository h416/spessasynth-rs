//! default_dls_modulators.rs
//! purpose: Default DLS modulator constants used when loading DLS soundbanks.
//! Ported from: src/soundbank/downloadable_sounds/default_dls_modulators.ts

use crate::soundbank::basic_soundbank::generator_types::generator_types;
use crate::soundbank::basic_soundbank::modulator::DecodedModulator;

/// DLS reverb effect modulator.
/// sourceEnum = 0x00db, destination = reverbEffectsSend, amount = 1000.
/// Equivalent to: DEFAULT_DLS_REVERB
pub const DEFAULT_DLS_REVERB: DecodedModulator =
    DecodedModulator::new(0x00db, 0x0, generator_types::REVERB_EFFECTS_SEND, 1000, 0);

/// DLS chorus effect modulator.
/// sourceEnum = 0x00dd, destination = chorusEffectsSend, amount = 1000.
/// Equivalent to: DEFAULT_DLS_CHORUS
pub const DEFAULT_DLS_CHORUS: DecodedModulator =
    DecodedModulator::new(0x00dd, 0x0, generator_types::CHORUS_EFFECTS_SEND, 1000, 0);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;

    // --- DEFAULT_DLS_REVERB ---

    #[test]
    fn test_reverb_source_enum() {
        assert_eq!(DEFAULT_DLS_REVERB.source_enum, 0x00db);
    }

    #[test]
    fn test_reverb_secondary_source_enum() {
        assert_eq!(DEFAULT_DLS_REVERB.secondary_source_enum, 0x0);
    }

    #[test]
    fn test_reverb_destination() {
        assert_eq!(DEFAULT_DLS_REVERB.destination, gt::REVERB_EFFECTS_SEND);
    }

    #[test]
    fn test_reverb_amount() {
        assert_eq!(DEFAULT_DLS_REVERB.transform_amount, 1000.0);
    }

    #[test]
    fn test_reverb_transform_type() {
        assert_eq!(DEFAULT_DLS_REVERB.transform_type, 0);
    }

    #[test]
    fn test_reverb_is_effect_modulator() {
        assert!(DEFAULT_DLS_REVERB.is_effect_modulator);
    }

    #[test]
    fn test_reverb_is_not_resonant_modulator() {
        assert!(!DEFAULT_DLS_REVERB.is_default_resonant_modulator);
    }

    // --- DEFAULT_DLS_CHORUS ---

    #[test]
    fn test_chorus_source_enum() {
        assert_eq!(DEFAULT_DLS_CHORUS.source_enum, 0x00dd);
    }

    #[test]
    fn test_chorus_secondary_source_enum() {
        assert_eq!(DEFAULT_DLS_CHORUS.secondary_source_enum, 0x0);
    }

    #[test]
    fn test_chorus_destination() {
        assert_eq!(DEFAULT_DLS_CHORUS.destination, gt::CHORUS_EFFECTS_SEND);
    }

    #[test]
    fn test_chorus_amount() {
        assert_eq!(DEFAULT_DLS_CHORUS.transform_amount, 1000.0);
    }

    #[test]
    fn test_chorus_transform_type() {
        assert_eq!(DEFAULT_DLS_CHORUS.transform_type, 0);
    }

    #[test]
    fn test_chorus_is_effect_modulator() {
        assert!(DEFAULT_DLS_CHORUS.is_effect_modulator);
    }

    #[test]
    fn test_chorus_is_not_resonant_modulator() {
        assert!(!DEFAULT_DLS_CHORUS.is_default_resonant_modulator);
    }
}
