/// basic_global_zone.rs
/// purpose: Global zone within an instrument or preset.
/// Ported from: src/soundbank/basic_soundbank/basic_global_zone.ts
///
/// TypeScript: `class BasicGlobalZone extends BasicZone {}` — empty body.
/// Since Rust has no inheritance, this is expressed as a type alias.
/// The distinction in TypeScript is only about "different instance" semantics;
/// there are no additional fields or methods.
use crate::soundbank::basic_soundbank::basic_zone::BasicZone;

/// Global zone within an instrument or preset.
/// Equivalent to: class BasicGlobalZone extends BasicZone {}
pub type BasicGlobalZone = BasicZone;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator::Generator;
    use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;

    // BasicGlobalZone is a type alias for BasicZone, so it can be created with
    // BasicZone::new() and all BasicZone methods are available as-is.

    #[test]
    fn test_basic_global_zone_is_default_constructible() {
        let _z: BasicGlobalZone = BasicZone::new();
    }

    #[test]
    fn test_basic_global_zone_default_has_no_generators() {
        let z: BasicGlobalZone = BasicZone::new();
        assert!(z.generators.is_empty());
    }

    #[test]
    fn test_basic_global_zone_default_has_no_key_range() {
        let z: BasicGlobalZone = BasicZone::new();
        assert!(!z.has_key_range());
    }

    #[test]
    fn test_basic_global_zone_add_generators_works() {
        let mut z: BasicGlobalZone = BasicZone::new();
        z.add_generators(&[Generator::new(gt::PAN, 0.0)]);
        assert_eq!(z.generators.len(), 1);
    }

    #[test]
    fn test_basic_global_zone_add_modulators_works() {
        use crate::soundbank::basic_soundbank::modulator::Modulator;
        let mut z: BasicGlobalZone = BasicZone::new();
        z.add_modulators(&[Modulator::default()]);
        assert_eq!(z.modulators.len(), 1);
    }

    #[test]
    fn test_basic_global_zone_copy_from_works() {
        let mut src: BasicGlobalZone = BasicZone::new();
        src.set_generator(gt::PAN, Some(42.0), true);
        let mut dst: BasicGlobalZone = BasicZone::new();
        dst.copy_from(&src);
        assert_eq!(dst.get_generator(gt::PAN, -999), 42);
    }
}
