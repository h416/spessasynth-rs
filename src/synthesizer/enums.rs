/// enums.rs
/// purpose: Synthesizer enumeration constants.
/// Ported from: src/synthesizer/enums.ts (spessasynth_core 4.3.0)
///
/// Changes from 4.2.0 (reviewed against the 4.3.0 diff): TS 4.3.0 keeps only
/// `InterpolationTypes` (renamed from `interpolationTypes` — TS casing convention only, no
/// Rust-side change) and removed `dataEntryStates` and `customControllers` entirely.
/// `dataEntryStates` is removed here too. `custom_controllers` is kept for the slots the
/// Rust channel still stores in its custom controller table: the SF2 NRPN generator LSB
/// (TS: `sf2NRPNGeneratorLSB`), the modulation multiplier reset value, and the channel
/// transpose fine tune read by the 4.2.0-shaped snapshot and channel property event.
/// Sample interpolation type constants.
/// Equivalent to: InterpolationTypes
pub mod interpolation_types {
    pub const LINEAR: u8 = 0;
    pub const NEAREST_NEIGHBOR: u8 = 1;
    pub const HERMITE: u8 = 2;
}

/// Equivalent to: InterpolationType
pub type InterpolationType = u8;

/// Extended (custom) controller indices used internally by the synthesizer.
/// Equivalent to: customControllers (removed in TS 4.3.0 — see module doc)
pub mod custom_controllers {
    /// Cents, only the decimal tuning (e.g., transpose is 4.5 → tune by 50 cents)
    pub const CHANNEL_TRANSPOSE_FINE: u8 = 1;
    /// Cents, set by modulation depth RPN
    pub const MODULATION_MULTIPLIER: u8 = 2;
    /// SF2 NPRN LSB for selecting a generator value
    pub const SF2_NPRN_GENERATOR_LSB: u8 = 6;
}

/// Equivalent to: CustomController
pub type CustomController = u8;

#[cfg(test)]
mod tests {
    use super::custom_controllers as cc;
    use super::interpolation_types as interp;

    // --- interpolation_types ---

    #[test]
    fn test_linear() {
        assert_eq!(interp::LINEAR, 0);
    }

    #[test]
    fn test_nearest_neighbor() {
        assert_eq!(interp::NEAREST_NEIGHBOR, 1);
    }

    #[test]
    fn test_hermite() {
        assert_eq!(interp::HERMITE, 2);
    }

    // --- custom_controllers ---

    #[test]
    fn test_sf2_nprn_generator_lsb() {
        assert_eq!(cc::SF2_NPRN_GENERATOR_LSB, 6);
    }
}
