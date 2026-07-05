/// enums.rs
/// purpose: DLS enumeration constants (moved out of soundbank/enums.ts in TS 4.3.0).
/// Ported from: src/soundbank/downloadable_sounds/enums.ts
///
/// Note: TS 4.3.0 renamed `dlsSources` → `DLSSources` and `dlsDestinations` →
/// `DLSDestinations` (naming-only). Rust keeps the idiomatic snake_case module names.
use crate::soundbank::enums::ModulatorCurveType;

/// Source curve type maps to a soundfont curve type (section 2.10, table 9).
/// Equivalent to: DLSTransform = ModulatorCurveType
pub type DLSTransform = ModulatorCurveType;

/// DLS source constants.
/// Equivalent to: DLSSources
pub mod dls_sources {
    pub const NONE: u16 = 0x0;
    pub const MOD_LFO: u16 = 0x1;
    pub const VELOCITY: u16 = 0x2;
    pub const KEY_NUM: u16 = 0x3;
    pub const VOL_ENV: u16 = 0x4;
    pub const MOD_ENV: u16 = 0x5;
    pub const PITCH_WHEEL: u16 = 0x6;
    pub const POLY_PRESSURE: u16 = 0x7;
    pub const CHANNEL_PRESSURE: u16 = 0x8;
    pub const VIBRATO_LFO: u16 = 0x9;
    pub const MODULATION_WHEEL: u16 = 0x81;
    pub const VOLUME: u16 = 0x87;
    pub const PAN: u16 = 0x8a;
    pub const EXPRESSION: u16 = 0x8b;
    // Note: these are flipped unintentionally in DLS2 table 9. Argh!
    pub const CHORUS: u16 = 0xdd;
    pub const REVERB: u16 = 0xdb;
    pub const PITCH_WHEEL_RANGE: u16 = 0x1_00;
    pub const FINE_TUNE: u16 = 0x1_01;
    pub const COARSE_TUNE: u16 = 0x1_02;
}

/// Equivalent to: DLSSource
pub type DLSSource = u16;

/// DLS destination constants.
/// Equivalent to: DLSDestinations
pub mod dls_destinations {
    pub const NONE: u16 = 0x0; // No destination
    pub const GAIN: u16 = 0x1; // Linear gain
    pub const RESERVED: u16 = 0x2; // Reserved
    pub const PITCH: u16 = 0x3; // Pitch in cents
    pub const PAN: u16 = 0x4; // Pan 10ths of a percent
    pub const KEY_NUM: u16 = 0x5; // MIDI key number
    // Nuh uh, the channel controllers are not supported!
    pub const CHORUS_SEND: u16 = 0x80; // Chorus send level 10ths of a percent
    pub const REVERB_SEND: u16 = 0x81; // Reverb send level 10ths of a percent
    pub const MOD_LFO_FREQ: u16 = 0x1_04; // Modulation LFO frequency
    pub const MOD_LFO_DELAY: u16 = 0x1_05; // Modulation LFO delay
    pub const VIB_LFO_FREQ: u16 = 0x1_14; // Vibrato LFO frequency
    pub const VIB_LFO_DELAY: u16 = 0x1_15; // Vibrato LFO delay
    pub const VOL_ENV_ATTACK: u16 = 0x2_06; // Volume envelope attack
    pub const VOL_ENV_DECAY: u16 = 0x2_07; // Volume envelope decay
    pub const RESERVED_EG1: u16 = 0x2_08; // Reserved
    pub const VOL_ENV_RELEASE: u16 = 0x2_09; // Volume envelope release
    pub const VOL_ENV_SUSTAIN: u16 = 0x2_0a; // Volume envelope sustain
    pub const VOL_ENV_DELAY: u16 = 0x2_0b; // Volume envelope delay
    pub const VOL_ENV_HOLD: u16 = 0x2_0c; // Volume envelope hold
    pub const MOD_ENV_ATTACK: u16 = 0x3_0a; // Modulation envelope attack
    pub const MOD_ENV_DECAY: u16 = 0x3_0b; // Modulation envelope decay
    pub const RESERVED_EG2: u16 = 0x3_0c; // Reserved
    pub const MOD_ENV_RELEASE: u16 = 0x3_0d; // Modulation envelope release
    pub const MOD_ENV_SUSTAIN: u16 = 0x3_0e; // Modulation envelope sustain
    pub const MOD_ENV_DELAY: u16 = 0x3_0f; // Modulation envelope delay
    pub const MOD_ENV_HOLD: u16 = 0x3_10; // Modulation envelope hold
    pub const FILTER_CUTOFF: u16 = 0x5_00; // Low pass filter cutoff frequency
    pub const FILTER_Q: u16 = 0x5_01; // Low pass filter resonance
}

/// Equivalent to: DLSDestination
pub type DLSDestination = u16;

/// DLS loop type constants.
/// Equivalent to: DLSLoopTypes
pub mod dls_loop_types {
    pub const FORWARD: u16 = 0x00_00;
    pub const LOOP_AND_RELEASE: u16 = 0x00_01;
}

/// Equivalent to: DLSLoopType
pub type DLSLoopType = u16;

#[cfg(test)]
mod tests {
    use super::dls_destinations as dd;
    use super::dls_loop_types as dlt;
    use super::dls_sources as ds;
    use super::*;
    use crate::soundbank::enums::modulator_curve_types as mct;

    // --- dls_sources ---

    #[test]
    fn test_dls_sources_none() {
        assert_eq!(ds::NONE, 0x0);
    }

    #[test]
    fn test_dls_sources_mod_lfo() {
        assert_eq!(ds::MOD_LFO, 0x1);
    }

    #[test]
    fn test_dls_sources_vibrato_lfo() {
        assert_eq!(ds::VIBRATO_LFO, 0x9);
    }

    #[test]
    fn test_dls_sources_modulation_wheel() {
        assert_eq!(ds::MODULATION_WHEEL, 0x81);
    }

    #[test]
    fn test_dls_sources_pan() {
        assert_eq!(ds::PAN, 0x8a);
    }

    #[test]
    fn test_dls_sources_chorus() {
        // Note: flipped unintentionally in DLS2 table 9
        assert_eq!(ds::CHORUS, 0xdd);
    }

    #[test]
    fn test_dls_sources_reverb() {
        assert_eq!(ds::REVERB, 0xdb);
    }

    #[test]
    fn test_dls_sources_pitch_wheel_range() {
        assert_eq!(ds::PITCH_WHEEL_RANGE, 0x100);
    }

    #[test]
    fn test_dls_sources_fine_tune() {
        assert_eq!(ds::FINE_TUNE, 0x101);
    }

    #[test]
    fn test_dls_sources_coarse_tune() {
        assert_eq!(ds::COARSE_TUNE, 0x102);
    }

    // --- dls_destinations ---

    #[test]
    fn test_dls_destinations_none() {
        assert_eq!(dd::NONE, 0x0);
    }

    #[test]
    fn test_dls_destinations_gain() {
        assert_eq!(dd::GAIN, 0x1);
    }

    #[test]
    fn test_dls_destinations_pitch() {
        assert_eq!(dd::PITCH, 0x3);
    }

    #[test]
    fn test_dls_destinations_chorus_send() {
        assert_eq!(dd::CHORUS_SEND, 0x80);
    }

    #[test]
    fn test_dls_destinations_reverb_send() {
        assert_eq!(dd::REVERB_SEND, 0x81);
    }

    #[test]
    fn test_dls_destinations_mod_lfo_freq() {
        assert_eq!(dd::MOD_LFO_FREQ, 0x104);
    }

    #[test]
    fn test_dls_destinations_vib_lfo_delay() {
        assert_eq!(dd::VIB_LFO_DELAY, 0x115);
    }

    #[test]
    fn test_dls_destinations_vol_env_attack() {
        assert_eq!(dd::VOL_ENV_ATTACK, 0x206);
    }

    #[test]
    fn test_dls_destinations_vol_env_hold() {
        assert_eq!(dd::VOL_ENV_HOLD, 0x20c);
    }

    #[test]
    fn test_dls_destinations_mod_env_attack() {
        assert_eq!(dd::MOD_ENV_ATTACK, 0x30a);
    }

    #[test]
    fn test_dls_destinations_mod_env_hold() {
        assert_eq!(dd::MOD_ENV_HOLD, 0x310);
    }

    #[test]
    fn test_dls_destinations_filter_cutoff() {
        assert_eq!(dd::FILTER_CUTOFF, 0x500);
    }

    #[test]
    fn test_dls_destinations_filter_q() {
        assert_eq!(dd::FILTER_Q, 0x501);
    }

    // --- dls_loop_types ---

    #[test]
    fn test_dls_loop_types_forward() {
        assert_eq!(dlt::FORWARD, 0);
    }

    #[test]
    fn test_dls_loop_types_loop_and_release() {
        assert_eq!(dlt::LOOP_AND_RELEASE, 1);
    }

    // --- type aliases (compile-time check via assignment) ---

    #[test]
    fn test_type_alias_dls_transform_equals_modulator_curve_type() {
        let _v: DLSTransform = mct::CONCAVE;
    }

    #[test]
    fn test_type_alias_dls_source() {
        let _v: DLSSource = ds::COARSE_TUNE;
    }

    #[test]
    fn test_type_alias_dls_destination() {
        let _v: DLSDestination = dd::FILTER_Q;
    }

    #[test]
    fn test_type_alias_dls_loop_type() {
        let _v: DLSLoopType = dlt::LOOP_AND_RELEASE;
    }
}
