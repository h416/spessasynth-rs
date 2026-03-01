/// default_dls_modulators.rs
/// purpose: Default DLS modulator constants used when loading DLS soundbanks.
/// Ported from: src/soundbank/downloadable_sounds/default_dls_modulators.ts
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

/// DLS 1 no-vibrato modulator for mod wheel.
/// sourceEnum = 0x0081, destination = vibLfoToPitch, amount = 0 (cancels SF2 default).
/// Equivalent to: DLS_1_NO_VIBRATO_MOD
pub const DLS_1_NO_VIBRATO_MOD: DecodedModulator =
    DecodedModulator::new(0x0081, 0x0, generator_types::VIB_LFO_TO_PITCH, 0, 0);

/// DLS 1 no-vibrato modulator for channel pressure.
/// sourceEnum = 0x000d, destination = vibLfoToPitch, amount = 0 (cancels SF2 default).
/// Equivalent to: DLS_1_NO_VIBRATO_PRESSURE
pub const DLS_1_NO_VIBRATO_PRESSURE: DecodedModulator =
    DecodedModulator::new(0x000d, 0x0, generator_types::VIB_LFO_TO_PITCH, 0, 0);

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

    // --- DLS_1_NO_VIBRATO_MOD ---

    #[test]
    fn test_no_vibrato_mod_source_enum() {
        assert_eq!(DLS_1_NO_VIBRATO_MOD.source_enum, 0x0081);
    }

    #[test]
    fn test_no_vibrato_mod_secondary_source_enum() {
        assert_eq!(DLS_1_NO_VIBRATO_MOD.secondary_source_enum, 0x0);
    }

    #[test]
    fn test_no_vibrato_mod_destination() {
        assert_eq!(DLS_1_NO_VIBRATO_MOD.destination, gt::VIB_LFO_TO_PITCH);
    }

    #[test]
    fn test_no_vibrato_mod_amount() {
        assert_eq!(DLS_1_NO_VIBRATO_MOD.transform_amount, 0.0);
    }

    #[test]
    fn test_no_vibrato_mod_transform_type() {
        assert_eq!(DLS_1_NO_VIBRATO_MOD.transform_type, 0);
    }

    #[test]
    fn test_no_vibrato_mod_is_not_effect_modulator() {
        assert!(!DLS_1_NO_VIBRATO_MOD.is_effect_modulator);
    }

    #[test]
    fn test_no_vibrato_mod_is_not_resonant_modulator() {
        assert!(!DLS_1_NO_VIBRATO_MOD.is_default_resonant_modulator);
    }

    // --- DLS_1_NO_VIBRATO_PRESSURE ---

    #[test]
    fn test_no_vibrato_pressure_source_enum() {
        assert_eq!(DLS_1_NO_VIBRATO_PRESSURE.source_enum, 0x000d);
    }

    #[test]
    fn test_no_vibrato_pressure_secondary_source_enum() {
        assert_eq!(DLS_1_NO_VIBRATO_PRESSURE.secondary_source_enum, 0x0);
    }

    #[test]
    fn test_no_vibrato_pressure_destination() {
        assert_eq!(DLS_1_NO_VIBRATO_PRESSURE.destination, gt::VIB_LFO_TO_PITCH);
    }

    #[test]
    fn test_no_vibrato_pressure_amount() {
        assert_eq!(DLS_1_NO_VIBRATO_PRESSURE.transform_amount, 0.0);
    }

    #[test]
    fn test_no_vibrato_pressure_transform_type() {
        assert_eq!(DLS_1_NO_VIBRATO_PRESSURE.transform_type, 0);
    }

    #[test]
    fn test_no_vibrato_pressure_is_not_effect_modulator() {
        assert!(!DLS_1_NO_VIBRATO_PRESSURE.is_effect_modulator);
    }

    #[test]
    fn test_no_vibrato_pressure_is_not_resonant_modulator() {
        assert!(!DLS_1_NO_VIBRATO_PRESSURE.is_default_resonant_modulator);
    }

    // --- summary check for common properties ---

    #[test]
    fn test_all_secondary_source_enums_are_zero() {
        assert_eq!(DEFAULT_DLS_REVERB.secondary_source_enum, 0x0);
        assert_eq!(DEFAULT_DLS_CHORUS.secondary_source_enum, 0x0);
        assert_eq!(DLS_1_NO_VIBRATO_MOD.secondary_source_enum, 0x0);
        assert_eq!(DLS_1_NO_VIBRATO_PRESSURE.secondary_source_enum, 0x0);
    }

    #[test]
    fn test_effect_modulators_only_reverb_and_chorus() {
        assert!(DEFAULT_DLS_REVERB.is_effect_modulator);
        assert!(DEFAULT_DLS_CHORUS.is_effect_modulator);
        assert!(!DLS_1_NO_VIBRATO_MOD.is_effect_modulator);
        assert!(!DLS_1_NO_VIBRATO_PRESSURE.is_effect_modulator);
    }

    #[test]
    fn test_no_vibrato_pair_share_same_destination() {
        assert_eq!(
            DLS_1_NO_VIBRATO_MOD.destination,
            DLS_1_NO_VIBRATO_PRESSURE.destination
        );
    }

    #[test]
    fn test_destinations_are_valid_generator_indices() {
        // None of the destinations should be clamped to INVALID (-1)
        use crate::soundbank::basic_soundbank::generator_types::{MAX_GENERATOR, generator_types};
        assert!(DEFAULT_DLS_REVERB.destination != generator_types::INVALID);
        assert!(DEFAULT_DLS_CHORUS.destination != generator_types::INVALID);
        assert!(DLS_1_NO_VIBRATO_MOD.destination != generator_types::INVALID);
        assert!(DLS_1_NO_VIBRATO_PRESSURE.destination != generator_types::INVALID);
        assert!(DEFAULT_DLS_REVERB.destination <= MAX_GENERATOR);
        assert!(DEFAULT_DLS_CHORUS.destination <= MAX_GENERATOR);
        assert!(DLS_1_NO_VIBRATO_MOD.destination <= MAX_GENERATOR);
        assert!(DLS_1_NO_VIBRATO_PRESSURE.destination <= MAX_GENERATOR);
    }
}
