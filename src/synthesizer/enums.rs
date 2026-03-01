/// enums.rs
/// purpose: Synthesizer enumeration constants.
/// Ported from: src/synthesizer/enums.ts
/// Sample interpolation type constants.
/// Equivalent to: interpolationTypes
pub mod interpolation_types {
    pub const LINEAR: u8 = 0;
    pub const NEAREST_NEIGHBOR: u8 = 1;
    pub const HERMITE: u8 = 2;
}

/// Equivalent to: InterpolationType
pub type InterpolationType = u8;

/// MIDI data entry state machine states.
/// Equivalent to: dataEntryStates
pub mod data_entry_states {
    pub const IDLE: u8 = 0;
    pub const RP_COARSE: u8 = 1;
    pub const RP_FINE: u8 = 2;
    pub const NRP_COARSE: u8 = 3;
    pub const NRP_FINE: u8 = 4;
    pub const DATA_COARSE: u8 = 5;
    pub const DATA_FINE: u8 = 6;
}

/// Equivalent to: DataEntryState
pub type DataEntryState = u8;

/// Extended (custom) controller indices used internally by the synthesizer.
/// Equivalent to: customControllers
pub mod custom_controllers {
    /// Cents, RPN for fine tuning
    pub const CHANNEL_TUNING: u8 = 0;
    /// Cents, only the decimal tuning (e.g., transpose is 4.5 → tune by 50 cents)
    pub const CHANNEL_TRANSPOSE_FINE: u8 = 1;
    /// Cents, set by modulation depth RPN
    pub const MODULATION_MULTIPLIER: u8 = 2;
    /// Cents, set by system exclusive
    pub const MASTER_TUNING: u8 = 3;
    /// Semitones, for RPN coarse tuning
    pub const CHANNEL_TUNING_SEMITONES: u8 = 4;
    /// Key shift, for system exclusive
    pub const CHANNEL_KEY_SHIFT: u8 = 5;
    /// SF2 NPRN LSB for selecting a generator value
    pub const SF2_NPRN_GENERATOR_LSB: u8 = 6;
}

/// Equivalent to: CustomController
pub type CustomController = u8;

#[cfg(test)]
mod tests {
    use super::custom_controllers as cc;
    use super::data_entry_states as des;
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

    // --- data_entry_states ---

    #[test]
    fn test_idle() {
        assert_eq!(des::IDLE, 0);
    }

    #[test]
    fn test_rp_coarse() {
        assert_eq!(des::RP_COARSE, 1);
    }

    #[test]
    fn test_nrp_coarse() {
        assert_eq!(des::NRP_COARSE, 3);
    }

    #[test]
    fn test_data_fine() {
        assert_eq!(des::DATA_FINE, 6);
    }

    // --- custom_controllers ---

    #[test]
    fn test_channel_tuning() {
        assert_eq!(cc::CHANNEL_TUNING, 0);
    }

    #[test]
    fn test_master_tuning() {
        assert_eq!(cc::MASTER_TUNING, 3);
    }

    #[test]
    fn test_sf2_nprn_generator_lsb() {
        assert_eq!(cc::SF2_NPRN_GENERATOR_LSB, 6);
    }
}
