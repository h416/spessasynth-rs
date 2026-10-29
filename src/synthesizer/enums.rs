//! enums.rs
//! purpose: Synthesizer enumeration constants.
//! Ported from: src/synthesizer/enums.ts

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
