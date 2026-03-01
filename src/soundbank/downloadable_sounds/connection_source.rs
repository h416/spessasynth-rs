/// connection_source.rs
/// purpose: ConnectionSource struct - DLS modulator source with SF2 ↔ DLS conversion.
/// Ported from: src/soundbank/downloadable_sounds/connection_source.ts
use std::fmt;

use crate::midi::enums::midi_controllers;
use crate::soundbank::basic_soundbank::modulator_source::ModulatorSource;
use crate::soundbank::enums::{
    DLSSource, DLSTransform, dls_sources, modulator_curve_types, modulator_sources,
};
use crate::soundbank::types::ModulatorSourceIndex;

// ---------------------------------------------------------------------------
// ConnectionSource
// ---------------------------------------------------------------------------

/// A DLS modulator connection source, with bipolar and invert flags.
/// Can be converted to/from a SF2 ModulatorSource.
/// Equivalent to: class ConnectionSource
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ConnectionSource {
    /// The DLS source enum value.
    /// Equivalent to: source: DLSSource
    pub source: DLSSource,
    /// The curve/transform type applied to the source value.
    /// Equivalent to: transform: DLSTransform
    pub transform: DLSTransform,
    /// Unipolar (false) or Bipolar (true) polarity.
    /// Equivalent to: bipolar
    pub bipolar: bool,
    /// If true, the source direction is inverted (negative).
    /// Equivalent to: invert
    pub invert: bool,
}

impl ConnectionSource {
    /// Creates a new ConnectionSource with explicit parameters.
    /// Equivalent to: constructor(source, transform, bipolar, invert)
    pub fn new(source: DLSSource, transform: DLSTransform, bipolar: bool, invert: bool) -> Self {
        Self {
            source,
            transform,
            bipolar,
            invert,
        }
    }

    /// Returns a copy of the given ConnectionSource.
    /// Equivalent to: static copyFrom(inputSource)
    pub fn copy_from(input: &ConnectionSource) -> Self {
        input.clone()
    }

    /// Converts an SF2 ModulatorSource into a DLS ConnectionSource.
    /// Returns `None` if the SF2 source has no DLS equivalent.
    /// Equivalent to: static fromSFSource(source): ConnectionSource | undefined
    pub fn from_sf_source(source: &ModulatorSource) -> Option<ConnectionSource> {
        let source_enum: DLSSource = if source.is_cc {
            // DLS only supports a specific set of MIDI controllers.
            match source.index {
                midi_controllers::MODULATION_WHEEL => dls_sources::MODULATION_WHEEL,
                midi_controllers::MAIN_VOLUME => dls_sources::VOLUME,
                midi_controllers::PAN => dls_sources::PAN,
                midi_controllers::EXPRESSION_CONTROLLER => dls_sources::EXPRESSION,
                midi_controllers::CHORUS_DEPTH => dls_sources::CHORUS,
                midi_controllers::REVERB_DEPTH => dls_sources::REVERB,
                _ => return None,
            }
        } else {
            match source.index {
                modulator_sources::NO_CONTROLLER => dls_sources::NONE,
                modulator_sources::NOTE_ON_KEY_NUM => dls_sources::KEY_NUM,
                modulator_sources::NOTE_ON_VELOCITY => dls_sources::VELOCITY,
                modulator_sources::PITCH_WHEEL => dls_sources::PITCH_WHEEL,
                modulator_sources::PITCH_WHEEL_RANGE => dls_sources::PITCH_WHEEL_RANGE,
                modulator_sources::POLY_PRESSURE => dls_sources::POLY_PRESSURE,
                modulator_sources::CHANNEL_PRESSURE => dls_sources::CHANNEL_PRESSURE,
                _ => return None,
            }
        };

        Some(ConnectionSource::new(
            source_enum,
            source.curve_type,
            source.is_bipolar,
            source.is_negative,
        ))
    }

    /// Encodes the transform and polarity flags into a DLS transform flag byte.
    ///
    /// Bit layout: `transform[1:0] | (bipolar << 4) | (invert << 5)`.
    ///
    /// Equivalent to: toTransformFlag()
    pub fn to_transform_flag(&self) -> u16 {
        (self.transform as u16) | ((self.bipolar as u16) << 4) | ((self.invert as u16) << 5)
    }

    /// Converts this DLS ConnectionSource into an SF2 ModulatorSource.
    /// Returns `None` if this DLS source has no SF2 equivalent
    /// (e.g., `modLfo`, `vibratoLfo`, `coarseTune`, `fineTune`, `modEnv`, or any
    /// unrecognised source).
    ///
    /// Equivalent to: toSFSource(): ModulatorSource | undefined
    pub fn to_sf_source(&self) -> Option<ModulatorSource> {
        let (source_index, is_cc): (ModulatorSourceIndex, bool) = match self.source {
            dls_sources::NONE => (modulator_sources::NO_CONTROLLER, false),
            dls_sources::KEY_NUM => (modulator_sources::NOTE_ON_KEY_NUM, false),
            dls_sources::VELOCITY => (modulator_sources::NOTE_ON_VELOCITY, false),
            dls_sources::POLY_PRESSURE => (modulator_sources::POLY_PRESSURE, false),
            dls_sources::CHANNEL_PRESSURE => (modulator_sources::CHANNEL_PRESSURE, false),
            dls_sources::PITCH_WHEEL => (modulator_sources::PITCH_WHEEL, false),
            dls_sources::PITCH_WHEEL_RANGE => (modulator_sources::PITCH_WHEEL_RANGE, false),
            dls_sources::MODULATION_WHEEL => (midi_controllers::MODULATION_WHEEL, true),
            dls_sources::VOLUME => (midi_controllers::MAIN_VOLUME, true),
            dls_sources::PAN => (midi_controllers::PAN, true),
            dls_sources::EXPRESSION => (midi_controllers::EXPRESSION_CONTROLLER, true),
            dls_sources::CHORUS => (midi_controllers::CHORUS_DEPTH, true),
            dls_sources::REVERB => (midi_controllers::REVERB_DEPTH, true),
            // modLfo(0x1), volEnv(0x4), modEnv(0x5), vibratoLfo(0x9),
            // fineTune(0x101), coarseTune(0x102), and any unrecognised source
            // have no SF2 equivalent.
            _ => return None,
        };

        Some(ModulatorSource::new(
            source_index,
            self.transform,
            is_cc,
            self.bipolar,
            self.invert,
        ))
    }
}

impl fmt::Display for ConnectionSource {
    /// Equivalent to: toString()
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let src_name = dls_source_name(self.source);
        let tfm_name = curve_type_name(self.transform);
        let polarity = if self.bipolar { "bipolar" } else { "unipolar" };
        let direction = if self.invert { "inverted" } else { "positive" };
        write!(f, "{} {} {} {}", src_name, tfm_name, polarity, direction)
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// Equivalent to: private get sourceName() / private get transformName()
// ---------------------------------------------------------------------------

fn dls_source_name(source: DLSSource) -> String {
    let name: Option<&str> = match source {
        dls_sources::NONE => Some("none"),
        dls_sources::MOD_LFO => Some("mod_lfo"),
        dls_sources::VELOCITY => Some("velocity"),
        dls_sources::KEY_NUM => Some("key_num"),
        dls_sources::VOL_ENV => Some("vol_env"),
        dls_sources::MOD_ENV => Some("mod_env"),
        dls_sources::PITCH_WHEEL => Some("pitch_wheel"),
        dls_sources::POLY_PRESSURE => Some("poly_pressure"),
        dls_sources::CHANNEL_PRESSURE => Some("channel_pressure"),
        dls_sources::VIBRATO_LFO => Some("vibrato_lfo"),
        dls_sources::MODULATION_WHEEL => Some("modulation_wheel"),
        dls_sources::VOLUME => Some("volume"),
        dls_sources::PAN => Some("pan"),
        dls_sources::EXPRESSION => Some("expression"),
        dls_sources::CHORUS => Some("chorus"),
        dls_sources::REVERB => Some("reverb"),
        dls_sources::PITCH_WHEEL_RANGE => Some("pitch_wheel_range"),
        dls_sources::FINE_TUNE => Some("fine_tune"),
        dls_sources::COARSE_TUNE => Some("coarse_tune"),
        _ => None,
    };
    name.map(|s| s.to_string())
        .unwrap_or_else(|| source.to_string())
}

fn curve_type_name(transform: DLSTransform) -> &'static str {
    match transform {
        modulator_curve_types::LINEAR => "linear",
        modulator_curve_types::CONCAVE => "concave",
        modulator_curve_types::CONVEX => "convex",
        modulator_curve_types::SWITCH => "switch",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::enums::midi_controllers as cc;
    use crate::soundbank::enums::{
        dls_sources as ds, modulator_curve_types as mct, modulator_sources as ms,
    };

    // ── Default / new ─────────────────────────────────────────────────────────

    #[test]
    fn test_default_fields() {
        let s = ConnectionSource::default();
        assert_eq!(s.source, dls_sources::NONE);
        assert_eq!(s.transform, modulator_curve_types::LINEAR);
        assert!(!s.bipolar);
        assert!(!s.invert);
    }

    #[test]
    fn test_new_sets_all_fields() {
        let s = ConnectionSource::new(ds::VELOCITY, mct::CONCAVE, true, false);
        assert_eq!(s.source, ds::VELOCITY);
        assert_eq!(s.transform, mct::CONCAVE);
        assert!(s.bipolar);
        assert!(!s.invert);
    }

    // ── copy_from ─────────────────────────────────────────────────────────────

    #[test]
    fn test_copy_from_is_equal() {
        let orig = ConnectionSource::new(ds::PAN, mct::SWITCH, false, true);
        let copy = ConnectionSource::copy_from(&orig);
        assert_eq!(orig, copy);
    }

    #[test]
    fn test_copy_from_is_independent() {
        let orig = ConnectionSource::new(ds::PAN, mct::SWITCH, false, true);
        let mut copy = ConnectionSource::copy_from(&orig);
        copy.source = ds::VELOCITY;
        assert_eq!(copy.source, ds::VELOCITY);
        assert_eq!(orig.source, ds::PAN); // original unchanged
    }

    // ── to_transform_flag ────────────────────────────────────────────────────

    #[test]
    fn test_transform_flag_linear_no_flags() {
        let s = ConnectionSource::new(ds::NONE, mct::LINEAR, false, false);
        assert_eq!(s.to_transform_flag(), 0);
    }

    #[test]
    fn test_transform_flag_concave_no_flags() {
        let s = ConnectionSource::new(ds::NONE, mct::CONCAVE, false, false);
        // concave=1, no flags → 1
        assert_eq!(s.to_transform_flag(), 1);
    }

    #[test]
    fn test_transform_flag_bipolar_only() {
        let s = ConnectionSource::new(ds::NONE, mct::LINEAR, true, false);
        // bipolar=1 << 4 = 16
        assert_eq!(s.to_transform_flag(), 16);
    }

    #[test]
    fn test_transform_flag_invert_only() {
        let s = ConnectionSource::new(ds::NONE, mct::LINEAR, false, true);
        // invert=1 << 5 = 32
        assert_eq!(s.to_transform_flag(), 32);
    }

    #[test]
    fn test_transform_flag_all_flags() {
        // switch(3), bipolar(16), invert(32) → 3|16|32 = 51
        let s = ConnectionSource::new(ds::NONE, mct::SWITCH, true, true);
        assert_eq!(s.to_transform_flag(), 51);
    }

    #[test]
    fn test_transform_flag_convex_bipolar() {
        // convex=2, bipolar=16 → 18
        let s = ConnectionSource::new(ds::NONE, mct::CONVEX, true, false);
        assert_eq!(s.to_transform_flag(), 18);
    }

    // ── from_sf_source: non-CC sources ───────────────────────────────────────

    #[test]
    fn test_from_sf_source_no_controller() {
        let sf = ModulatorSource::new(ms::NO_CONTROLLER, mct::LINEAR, false, false, false);
        let conn = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(conn.source, ds::NONE);
    }

    #[test]
    fn test_from_sf_source_note_on_key_num() {
        let sf = ModulatorSource::new(ms::NOTE_ON_KEY_NUM, mct::LINEAR, false, false, false);
        let conn = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(conn.source, ds::KEY_NUM);
    }

    #[test]
    fn test_from_sf_source_note_on_velocity() {
        let sf = ModulatorSource::new(ms::NOTE_ON_VELOCITY, mct::LINEAR, false, false, false);
        let conn = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(conn.source, ds::VELOCITY);
    }

    #[test]
    fn test_from_sf_source_pitch_wheel() {
        let sf = ModulatorSource::new(ms::PITCH_WHEEL, mct::LINEAR, false, false, false);
        let conn = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(conn.source, ds::PITCH_WHEEL);
    }

    #[test]
    fn test_from_sf_source_pitch_wheel_range() {
        let sf = ModulatorSource::new(ms::PITCH_WHEEL_RANGE, mct::LINEAR, false, false, false);
        let conn = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(conn.source, ds::PITCH_WHEEL_RANGE);
    }

    #[test]
    fn test_from_sf_source_poly_pressure() {
        let sf = ModulatorSource::new(ms::POLY_PRESSURE, mct::LINEAR, false, false, false);
        let conn = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(conn.source, ds::POLY_PRESSURE);
    }

    #[test]
    fn test_from_sf_source_channel_pressure() {
        let sf = ModulatorSource::new(ms::CHANNEL_PRESSURE, mct::LINEAR, false, false, false);
        let conn = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(conn.source, ds::CHANNEL_PRESSURE);
    }

    #[test]
    fn test_from_sf_source_unknown_non_cc_returns_none() {
        // Index 50 is not a recognised modulator source
        let sf = ModulatorSource::new(50, mct::LINEAR, false, false, false);
        assert!(ConnectionSource::from_sf_source(&sf).is_none());
    }

    // ── from_sf_source: CC sources ────────────────────────────────────────────

    #[test]
    fn test_from_sf_source_modulation_wheel_cc() {
        let sf = ModulatorSource::new(cc::MODULATION_WHEEL, mct::LINEAR, true, false, false);
        let conn = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(conn.source, ds::MODULATION_WHEEL);
    }

    #[test]
    fn test_from_sf_source_main_volume_cc() {
        let sf = ModulatorSource::new(cc::MAIN_VOLUME, mct::LINEAR, true, false, false);
        let conn = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(conn.source, ds::VOLUME);
    }

    #[test]
    fn test_from_sf_source_pan_cc() {
        let sf = ModulatorSource::new(cc::PAN, mct::LINEAR, true, false, false);
        let conn = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(conn.source, ds::PAN);
    }

    #[test]
    fn test_from_sf_source_expression_cc() {
        let sf = ModulatorSource::new(cc::EXPRESSION_CONTROLLER, mct::LINEAR, true, false, false);
        let conn = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(conn.source, ds::EXPRESSION);
    }

    #[test]
    fn test_from_sf_source_chorus_cc() {
        let sf = ModulatorSource::new(cc::CHORUS_DEPTH, mct::LINEAR, true, false, false);
        let conn = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(conn.source, ds::CHORUS);
    }

    #[test]
    fn test_from_sf_source_reverb_cc() {
        let sf = ModulatorSource::new(cc::REVERB_DEPTH, mct::LINEAR, true, false, false);
        let conn = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(conn.source, ds::REVERB);
    }

    #[test]
    fn test_from_sf_source_unknown_cc_returns_none() {
        // Sustain pedal (64) is a valid CC but not supported by DLS
        let sf = ModulatorSource::new(64, mct::LINEAR, true, false, false);
        assert!(ConnectionSource::from_sf_source(&sf).is_none());
    }

    // ── from_sf_source: flags are preserved ──────────────────────────────────

    #[test]
    fn test_from_sf_source_preserves_curve_and_flags() {
        let sf = ModulatorSource::new(
            ms::NOTE_ON_VELOCITY,
            mct::CONCAVE,
            false,
            true, // bipolar
            true, // negative → invert
        );
        let conn = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(conn.source, ds::VELOCITY);
        assert_eq!(conn.transform, mct::CONCAVE);
        assert!(conn.bipolar);
        assert!(conn.invert);
    }

    // ── to_sf_source: non-CC sources ─────────────────────────────────────────

    #[test]
    fn test_to_sf_source_none() {
        let conn = ConnectionSource::new(ds::NONE, mct::LINEAR, false, false);
        let sf = conn.to_sf_source().unwrap();
        assert_eq!(sf.index, ms::NO_CONTROLLER);
        assert!(!sf.is_cc);
    }

    #[test]
    fn test_to_sf_source_key_num() {
        let conn = ConnectionSource::new(ds::KEY_NUM, mct::LINEAR, false, false);
        let sf = conn.to_sf_source().unwrap();
        assert_eq!(sf.index, ms::NOTE_ON_KEY_NUM);
        assert!(!sf.is_cc);
    }

    #[test]
    fn test_to_sf_source_velocity() {
        let conn = ConnectionSource::new(ds::VELOCITY, mct::LINEAR, false, false);
        let sf = conn.to_sf_source().unwrap();
        assert_eq!(sf.index, ms::NOTE_ON_VELOCITY);
        assert!(!sf.is_cc);
    }

    #[test]
    fn test_to_sf_source_poly_pressure() {
        let conn = ConnectionSource::new(ds::POLY_PRESSURE, mct::LINEAR, false, false);
        let sf = conn.to_sf_source().unwrap();
        assert_eq!(sf.index, ms::POLY_PRESSURE);
        assert!(!sf.is_cc);
    }

    #[test]
    fn test_to_sf_source_channel_pressure() {
        let conn = ConnectionSource::new(ds::CHANNEL_PRESSURE, mct::LINEAR, false, false);
        let sf = conn.to_sf_source().unwrap();
        assert_eq!(sf.index, ms::CHANNEL_PRESSURE);
        assert!(!sf.is_cc);
    }

    #[test]
    fn test_to_sf_source_pitch_wheel() {
        let conn = ConnectionSource::new(ds::PITCH_WHEEL, mct::LINEAR, false, false);
        let sf = conn.to_sf_source().unwrap();
        assert_eq!(sf.index, ms::PITCH_WHEEL);
        assert!(!sf.is_cc);
    }

    #[test]
    fn test_to_sf_source_pitch_wheel_range() {
        let conn = ConnectionSource::new(ds::PITCH_WHEEL_RANGE, mct::LINEAR, false, false);
        let sf = conn.to_sf_source().unwrap();
        assert_eq!(sf.index, ms::PITCH_WHEEL_RANGE);
        assert!(!sf.is_cc);
    }

    // ── to_sf_source: CC sources ──────────────────────────────────────────────

    #[test]
    fn test_to_sf_source_modulation_wheel() {
        let conn = ConnectionSource::new(ds::MODULATION_WHEEL, mct::LINEAR, false, false);
        let sf = conn.to_sf_source().unwrap();
        assert_eq!(sf.index, cc::MODULATION_WHEEL);
        assert!(sf.is_cc);
    }

    #[test]
    fn test_to_sf_source_volume() {
        let conn = ConnectionSource::new(ds::VOLUME, mct::LINEAR, false, false);
        let sf = conn.to_sf_source().unwrap();
        assert_eq!(sf.index, cc::MAIN_VOLUME);
        assert!(sf.is_cc);
    }

    #[test]
    fn test_to_sf_source_pan() {
        let conn = ConnectionSource::new(ds::PAN, mct::LINEAR, false, false);
        let sf = conn.to_sf_source().unwrap();
        assert_eq!(sf.index, cc::PAN);
        assert!(sf.is_cc);
    }

    #[test]
    fn test_to_sf_source_expression() {
        let conn = ConnectionSource::new(ds::EXPRESSION, mct::LINEAR, false, false);
        let sf = conn.to_sf_source().unwrap();
        assert_eq!(sf.index, cc::EXPRESSION_CONTROLLER);
        assert!(sf.is_cc);
    }

    #[test]
    fn test_to_sf_source_chorus() {
        let conn = ConnectionSource::new(ds::CHORUS, mct::LINEAR, false, false);
        let sf = conn.to_sf_source().unwrap();
        assert_eq!(sf.index, cc::CHORUS_DEPTH);
        assert!(sf.is_cc);
    }

    #[test]
    fn test_to_sf_source_reverb() {
        let conn = ConnectionSource::new(ds::REVERB, mct::LINEAR, false, false);
        let sf = conn.to_sf_source().unwrap();
        assert_eq!(sf.index, cc::REVERB_DEPTH);
        assert!(sf.is_cc);
    }

    // ── to_sf_source: unsupported DLS sources ────────────────────────────────

    #[test]
    fn test_to_sf_source_mod_lfo_returns_none() {
        let conn = ConnectionSource::new(ds::MOD_LFO, mct::LINEAR, false, false);
        assert!(conn.to_sf_source().is_none());
    }

    #[test]
    fn test_to_sf_source_vibrato_lfo_returns_none() {
        let conn = ConnectionSource::new(ds::VIBRATO_LFO, mct::LINEAR, false, false);
        assert!(conn.to_sf_source().is_none());
    }

    #[test]
    fn test_to_sf_source_coarse_tune_returns_none() {
        let conn = ConnectionSource::new(ds::COARSE_TUNE, mct::LINEAR, false, false);
        assert!(conn.to_sf_source().is_none());
    }

    #[test]
    fn test_to_sf_source_fine_tune_returns_none() {
        let conn = ConnectionSource::new(ds::FINE_TUNE, mct::LINEAR, false, false);
        assert!(conn.to_sf_source().is_none());
    }

    #[test]
    fn test_to_sf_source_mod_env_returns_none() {
        let conn = ConnectionSource::new(ds::MOD_ENV, mct::LINEAR, false, false);
        assert!(conn.to_sf_source().is_none());
    }

    #[test]
    fn test_to_sf_source_vol_env_returns_none() {
        // VOL_ENV has no SF2 equivalent (falls to default/wildcard arm)
        let conn = ConnectionSource::new(ds::VOL_ENV, mct::LINEAR, false, false);
        assert!(conn.to_sf_source().is_none());
    }

    #[test]
    fn test_to_sf_source_unknown_source_returns_none() {
        let conn = ConnectionSource::new(0xFFFF, mct::LINEAR, false, false);
        assert!(conn.to_sf_source().is_none());
    }

    // ── to_sf_source: flags are preserved ────────────────────────────────────

    #[test]
    fn test_to_sf_source_preserves_curve_and_flags() {
        let conn = ConnectionSource::new(ds::VELOCITY, mct::CONCAVE, true, true);
        let sf = conn.to_sf_source().unwrap();
        assert_eq!(sf.curve_type, mct::CONCAVE);
        assert!(sf.is_bipolar);
        assert!(sf.is_negative);
    }

    // ── SF2 ↔ DLS round-trip ─────────────────────────────────────────────────

    #[test]
    fn test_roundtrip_non_cc_no_controller() {
        let original = ConnectionSource::new(ds::NONE, mct::LINEAR, false, false);
        let sf = original.to_sf_source().unwrap();
        let recovered = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(recovered, original);
    }

    #[test]
    fn test_roundtrip_cc_volume_bipolar() {
        let original = ConnectionSource::new(ds::VOLUME, mct::CONCAVE, true, false);
        let sf = original.to_sf_source().unwrap();
        let recovered = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(recovered, original);
    }

    #[test]
    fn test_roundtrip_velocity_switch_invert() {
        let original = ConnectionSource::new(ds::VELOCITY, mct::SWITCH, false, true);
        let sf = original.to_sf_source().unwrap();
        let recovered = ConnectionSource::from_sf_source(&sf).unwrap();
        assert_eq!(recovered, original);
    }

    #[test]
    fn test_roundtrip_all_supported_non_cc_sources() {
        let sources = [
            ds::NONE,
            ds::KEY_NUM,
            ds::VELOCITY,
            ds::POLY_PRESSURE,
            ds::CHANNEL_PRESSURE,
            ds::PITCH_WHEEL,
            ds::PITCH_WHEEL_RANGE,
        ];
        for src in sources {
            let conn = ConnectionSource::new(src, mct::LINEAR, false, false);
            let sf = conn.to_sf_source().unwrap();
            let back = ConnectionSource::from_sf_source(&sf).unwrap();
            assert_eq!(back, conn, "round-trip failed for DLS source 0x{:x}", src);
        }
    }

    #[test]
    fn test_roundtrip_all_supported_cc_sources() {
        let sources = [
            ds::MODULATION_WHEEL,
            ds::VOLUME,
            ds::PAN,
            ds::EXPRESSION,
            ds::CHORUS,
            ds::REVERB,
        ];
        for src in sources {
            let conn = ConnectionSource::new(src, mct::LINEAR, false, false);
            let sf = conn.to_sf_source().unwrap();
            let back = ConnectionSource::from_sf_source(&sf).unwrap();
            assert_eq!(
                back, conn,
                "round-trip failed for DLS CC source 0x{:x}",
                src
            );
        }
    }

    // ── Display / to_string ───────────────────────────────────────────────────

    #[test]
    fn test_display_default() {
        let s = ConnectionSource::default();
        let text = s.to_string();
        assert!(text.contains("none"), "got: {}", text);
        assert!(text.contains("linear"), "got: {}", text);
        assert!(text.contains("unipolar"), "got: {}", text);
        assert!(text.contains("positive"), "got: {}", text);
    }

    #[test]
    fn test_display_velocity_concave_bipolar_inverted() {
        let s = ConnectionSource::new(ds::VELOCITY, mct::CONCAVE, true, true);
        let text = s.to_string();
        assert!(text.contains("velocity"), "got: {}", text);
        assert!(text.contains("concave"), "got: {}", text);
        assert!(text.contains("bipolar"), "got: {}", text);
        assert!(text.contains("inverted"), "got: {}", text);
    }

    #[test]
    fn test_display_unknown_source_falls_back_to_number() {
        let s = ConnectionSource::new(0xABCD, mct::LINEAR, false, false);
        let text = s.to_string();
        // Unknown source falls back to numeric representation
        assert!(text.starts_with("43981 "), "got: {}", text); // 0xABCD = 43981
    }

    #[test]
    fn test_display_chorus_switch_unipolar_positive() {
        let s = ConnectionSource::new(ds::CHORUS, mct::SWITCH, false, false);
        let text = s.to_string();
        assert!(text.contains("chorus"), "got: {}", text);
        assert!(text.contains("switch"), "got: {}", text);
        assert!(text.contains("unipolar"), "got: {}", text);
        assert!(text.contains("positive"), "got: {}", text);
    }
}
