/// enums.rs
/// purpose: SoundFont2 enumeration constants.
/// Ported from: src/soundbank/enums.ts
///
/// Note: TS 4.3.0 moved the DLS enums (DLSSources, DLSDestinations, DLSLoopTypes,
/// DLSTransform) to src/soundbank/downloadable_sounds/enums.ts; the Rust counterparts
/// now live in soundbank/downloadable_sounds/enums.rs.
/// TS 4.3.0 also renamed `sampleTypes` → `SampleTypes`, `modulatorSources` →
/// `ModulatorControllerSources` (type: `ModulatorSourceEnum` → `ModulatorControllerSource`),
/// `modulatorCurveTypes` → `ModulatorCurveTypes` and `modulatorTransformTypes` →
/// `ModulatorTransformTypes`. Rust keeps its idiomatic snake_case module names.
// Re-export everything from generator_types.
// Equivalent to: export * from "./basic_soundbank/generator_types"
pub use crate::soundbank::basic_soundbank::generator_types::*;

/// Sample type constants.
/// Equivalent to: SampleTypes
pub mod sample_types {
    pub const MONO_SAMPLE: u16 = 1;
    pub const RIGHT_SAMPLE: u16 = 2;
    pub const LEFT_SAMPLE: u16 = 4;
    pub const LINKED_SAMPLE: u16 = 8;
    pub const ROM_MONO_SAMPLE: u16 = 32_769;
    pub const ROM_RIGHT_SAMPLE: u16 = 32_770;
    pub const ROM_LEFT_SAMPLE: u16 = 32_772;
    pub const ROM_LINKED_SAMPLE: u16 = 32_776;
}

/// Equivalent to: SampleType
pub type SampleType = u16;

/// Modulator controller source constants.
/// Equivalent to: ModulatorControllerSources
pub mod modulator_sources {
    pub const NO_CONTROLLER: u8 = 0;
    pub const NOTE_ON_VELOCITY: u8 = 2;
    pub const NOTE_ON_KEY_NUM: u8 = 3;
    pub const POLY_PRESSURE: u8 = 10;
    pub const CHANNEL_PRESSURE: u8 = 13;
    pub const PITCH_WHEEL: u8 = 14;
    pub const PITCH_WHEEL_RANGE: u8 = 16;
    pub const LINK: u8 = 127;
}

/// Equivalent to: ModulatorControllerSource (TS 4.2.0 name: ModulatorSourceEnum)
pub type ModulatorControllerSource = u8;

/// Modulator curve type constants.
/// Equivalent to: ModulatorCurveTypes
pub mod modulator_curve_types {
    pub const LINEAR: u8 = 0;
    pub const CONCAVE: u8 = 1;
    pub const CONVEX: u8 = 2;
    pub const SWITCH: u8 = 3;
}

/// Equivalent to: ModulatorCurveType
pub type ModulatorCurveType = u8;

/// Modulator transform type constants.
/// Equivalent to: ModulatorTransformTypes
pub mod modulator_transform_types {
    pub const LINEAR: u8 = 0;
    pub const ABSOLUTE: u8 = 2;
}

/// Equivalent to: ModulatorTransformType
pub type ModulatorTransformType = u8;

#[cfg(test)]
mod tests {
    use super::modulator_curve_types as mct;
    use super::modulator_sources as ms;
    use super::modulator_transform_types as mtt;
    use super::sample_types as st;
    use super::*;

    // --- sample_types ---

    #[test]
    fn test_sample_types_mono() {
        assert_eq!(st::MONO_SAMPLE, 1);
    }

    #[test]
    fn test_sample_types_right() {
        assert_eq!(st::RIGHT_SAMPLE, 2);
    }

    #[test]
    fn test_sample_types_left() {
        assert_eq!(st::LEFT_SAMPLE, 4);
    }

    #[test]
    fn test_sample_types_linked() {
        assert_eq!(st::LINKED_SAMPLE, 8);
    }

    #[test]
    fn test_sample_types_rom_mono() {
        assert_eq!(st::ROM_MONO_SAMPLE, 32_769);
    }

    #[test]
    fn test_sample_types_rom_right() {
        assert_eq!(st::ROM_RIGHT_SAMPLE, 32_770);
    }

    #[test]
    fn test_sample_types_rom_left() {
        assert_eq!(st::ROM_LEFT_SAMPLE, 32_772);
    }

    #[test]
    fn test_sample_types_rom_linked() {
        assert_eq!(st::ROM_LINKED_SAMPLE, 32_776);
    }

    // --- modulator_sources ---

    #[test]
    fn test_modulator_sources_no_controller() {
        assert_eq!(ms::NO_CONTROLLER, 0);
    }

    #[test]
    fn test_modulator_sources_note_on_velocity() {
        assert_eq!(ms::NOTE_ON_VELOCITY, 2);
    }

    #[test]
    fn test_modulator_sources_note_on_key_num() {
        assert_eq!(ms::NOTE_ON_KEY_NUM, 3);
    }

    #[test]
    fn test_modulator_sources_poly_pressure() {
        assert_eq!(ms::POLY_PRESSURE, 10);
    }

    #[test]
    fn test_modulator_sources_channel_pressure() {
        assert_eq!(ms::CHANNEL_PRESSURE, 13);
    }

    #[test]
    fn test_modulator_sources_pitch_wheel() {
        assert_eq!(ms::PITCH_WHEEL, 14);
    }

    #[test]
    fn test_modulator_sources_pitch_wheel_range() {
        assert_eq!(ms::PITCH_WHEEL_RANGE, 16);
    }

    #[test]
    fn test_modulator_sources_link() {
        assert_eq!(ms::LINK, 127);
    }

    // --- modulator_curve_types ---

    #[test]
    fn test_modulator_curve_types_linear() {
        assert_eq!(mct::LINEAR, 0);
    }

    #[test]
    fn test_modulator_curve_types_concave() {
        assert_eq!(mct::CONCAVE, 1);
    }

    #[test]
    fn test_modulator_curve_types_convex() {
        assert_eq!(mct::CONVEX, 2);
    }

    #[test]
    fn test_modulator_curve_types_switch() {
        assert_eq!(mct::SWITCH, 3);
    }

    // --- modulator_transform_types ---

    #[test]
    fn test_modulator_transform_types_linear() {
        assert_eq!(mtt::LINEAR, 0);
    }

    #[test]
    fn test_modulator_transform_types_absolute() {
        assert_eq!(mtt::ABSOLUTE, 2);
    }

    // --- type alias consistency (compile-time check via assignment) ---

    #[test]
    fn test_type_alias_sample_type() {
        let _v: SampleType = st::ROM_LINKED_SAMPLE;
    }

    #[test]
    fn test_type_alias_modulator_controller_source() {
        let _v: ModulatorControllerSource = ms::LINK;
    }

    #[test]
    fn test_type_alias_modulator_curve_type() {
        let _v: ModulatorCurveType = mct::SWITCH;
    }

    #[test]
    fn test_type_alias_modulator_transform_type() {
        let _v: ModulatorTransformType = mtt::ABSOLUTE;
    }

    // --- re-exported generator_types items are accessible ---

    #[test]
    fn test_reexport_generators_amount() {
        assert_eq!(GENERATORS_AMOUNT, 67);
    }

    #[test]
    fn test_reexport_max_generator() {
        assert_eq!(MAX_GENERATOR, 66);
    }
}
