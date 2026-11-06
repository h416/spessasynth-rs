//! enums.rs
//! purpose: Synthesizer enumeration constants.
//! Ported from: src/synthesizer/enums.ts

/// Sample interpolation type constants.
/// Equivalent to: InterpolationTypes
pub mod interpolation_types {
    /// Linear interpolation.
    pub const LINEAR: u8 = 0;
    /// Nearest neighbor (no interpolation).
    pub const NEAREST_NEIGHBOR: u8 = 1;
    /// Cubic Hermite interpolation. Good quality with little artifacting. (Default)
    pub const HERMITE: u8 = 2;
    /// 32-point windowed sinc interpolation.
    /// Very high quality with almost no aliasing or harmonic distortion, at a significantly higher CPU cost.
    pub const SINC32: u8 = 3;
    /// 128-point windowed sinc interpolation with antialiasing.
    /// The best quality, but extremely high CPU cost (meant for offline rendering).
    pub const SINC128: u8 = 4;
}

/// Equivalent to: InterpolationType
pub type InterpolationType = u8;

/// The special EFX control sources of a GS insertion block (EFX CONTROL SOURCE 1/2).
/// Values `1-95` select the control change with that number, other values are off.
/// Equivalent to: GSInsertionControlSources
pub mod gs_insertion_control_sources {
    /// No control.
    pub const OFF: u8 = 0;
    /// Channel aftertouch (channel pressure).
    pub const CHANNEL_PRESSURE: u8 = 0x70;
    /// Pitch bend.
    pub const PITCH_BEND: u8 = 0x71;
}

/// Equivalent to: GSInsertionControlSource
pub type GSInsertionControlSource = u8;

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
