//! enums.rs
//! purpose: Synthesizer enumeration constants.
//! Ported from: src/synthesizer/enums.ts (spessasynth_core 4.3.0)
//!
//! Changes from 4.2.0 (reviewed against the 4.3.0 diff): TS 4.3.0 keeps only
//! `InterpolationTypes` (renamed from `interpolationTypes` — TS casing convention only, no
//! Rust-side change) and removed `dataEntryStates` and `customControllers` entirely.
//! Both are removed here too (the SF2 NRPN generator selector is the channel field
//! `sf2_nrpn_generator_lsb`, TS `sf2NRPNGeneratorLSB`).

/// Sample interpolation type constants.
/// Equivalent to: InterpolationTypes
pub mod interpolation_types {
    pub const LINEAR: u8 = 0;
    pub const NEAREST_NEIGHBOR: u8 = 1;
    pub const HERMITE: u8 = 2;
}

/// Equivalent to: InterpolationType
pub type InterpolationType = u8;

#[cfg(test)]
mod tests {
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

}
