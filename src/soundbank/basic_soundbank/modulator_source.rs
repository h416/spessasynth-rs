//! modulator_source.rs
//! purpose: ModulatorSource struct - parses SF2 modulator source enums and computes curve values.
//! Ported from: src/soundbank/basic_soundbank/modulator_source.ts
//!
//! `getValue()` takes an `SF2Channel` (`midiControllers`, `polyPressures`,
//! `midiParameters.pressure` / `pitchWheelRange`). Rust passes [`SF2Channel`] without the
//! poly pressures: those are copied to the voice before the modulators are computed.
//!
//! TS keeps the curve as a `ModulatorCurve` object (`storedCurve`). Rust keeps the curve type and, for a
//! custom curve that was resolved from the bank, the curve itself (`custom_curve`). A custom type without a
//! curve, or a type this version does not know, outputs 0 like TS `UnresolvedCurve`.
use std::fmt;
use std::sync::Arc;

use crate::soundbank::basic_soundbank::custom_curve::CustomCurve;
use crate::soundbank::basic_soundbank::modulator_curve::{
    SOURCE_VALUE_14_BIT, SOURCE_VALUE_7_BIT, built_in_table, curve_type_name,
};
use crate::soundbank::enums::{ModulatorCurveType, modulator_curve_types, modulator_sources};
use crate::soundbank::types::ModulatorSourceIndex;
use crate::utils::byte_functions::bit_mask::{bit_mask_to_bool, to_numeric_bool};

// ---------------------------------------------------------------------------
// SF2Channel / VoiceModInputs
// ---------------------------------------------------------------------------

/// The channel state read by [`ModulatorSource::get_value`].
/// Equivalent to: SF2Channel (soundbank/types.ts; `polyPressures` is read through
/// [`VoiceModInputs::pressure`])
pub struct SF2Channel<'a> {
    /// All MIDI controller values (14-bit).
    /// Equivalent to: midiControllers
    pub midi_controllers: &'a [i16],
    /// Equivalent to: midiParameters.pressure
    pub pressure: i32,
    /// Equivalent to: midiParameters.pitchWheelRange
    pub pitch_wheel_range: f64,
    /// The modulation depth range (RPN 5) in cents.
    /// Equivalent to: midiParameters.modulationDepth
    pub modulation_depth: f64,
}

/// Minimal voice data needed by [`ModulatorSource::get_value`].
///
/// Only the fields that `getValue()` actually reads are included, so the caller can
/// pass them without borrowing the whole `Voice`.
/// Equivalent to the subset of `Voice` used in `getValue()`.
pub struct VoiceModInputs {
    /// MIDI note number (0–127) — the raw pressed key.
    pub midi_note: u8,
    /// Effective key after key shift / keyNum override (`voice.targetKey`).
    /// TS's `noteOnKeyNum` modulator source reads this, not the raw note.
    pub target_key: i16,
    /// Note-on velocity (0–127).
    pub velocity: u8,
    /// Polyphonic pressure (0–127).
    pub pressure: u8,
}

/// `Math.round`: half-up, unlike `f64::round` (half away from zero).
#[inline]
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

// ---------------------------------------------------------------------------
// ModulatorSource
// ---------------------------------------------------------------------------

/// Parses a SF2 modulator source enum and provides curve value lookup.
/// Equivalent to: class ModulatorSource
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ModulatorSource {
    /// Unipolar (false) or Bipolar (true) polarity mapping.
    /// Equivalent to: isBipolar
    pub is_bipolar: bool,
    /// If true, the controller direction is from maximum to minimum.
    /// Equivalent to: isNegative
    pub is_negative: bool,
    /// Index of the source: MIDI CC number or predefined source, depending on `is_cc`.
    /// Equivalent to: index
    pub index: ModulatorSourceIndex,
    /// If true, the MIDI Controller Palette (CC 0–127) is selected.
    /// Equivalent to: isCC
    pub is_cc: bool,
    /// The curve type applied to the source value.
    /// Equivalent to: curveType
    pub curve_type: ModulatorCurveType,
    /// The custom curve (SFe only), when `curve_type` is custom and the curve was found in the bank.
    /// Equivalent to: curve (when it is a CustomCurve)
    pub custom_curve: Option<Arc<CustomCurve>>,
}

impl ModulatorSource {
    /// Creates a new ModulatorSource with explicit parameters.
    /// Equivalent to: constructor(index, curveType, isCC, isBipolar, isNegative)
    pub fn new(
        index: ModulatorSourceIndex,
        curve_type: ModulatorCurveType,
        is_cc: bool,
        is_bipolar: bool,
        is_negative: bool,
    ) -> Self {
        Self {
            is_bipolar,
            is_negative,
            index,
            is_cc,
            curve_type,
            custom_curve: None,
        }
    }

    /// Creates a source that uses a custom curve.
    /// Equivalent to: constructor(index, curve, isCC, isBipolar, isNegative) with a CustomCurve
    pub fn with_custom_curve(
        index: ModulatorSourceIndex,
        curve: Arc<CustomCurve>,
        is_cc: bool,
        is_bipolar: bool,
        is_negative: bool,
    ) -> Self {
        let mut source = Self::new(
            index,
            modulator_curve_types::CUSTOM,
            is_cc,
            is_bipolar,
            is_negative,
        );
        source.custom_curve = Some(curve);
        source
    }

    /// Selects a built-in curve type (a custom type without a curve outputs 0).
    /// Equivalent to: set curveType(type)
    pub fn set_curve_type(&mut self, curve_type: ModulatorCurveType) {
        self.curve_type = curve_type;
        self.custom_curve = None;
    }

    /// Assigns a custom curve.
    /// Equivalent to: set curve(curve) with a CustomCurve
    pub fn set_custom_curve(&mut self, curve: Arc<CustomCurve>) {
        self.curve_type = modulator_curve_types::CUSTOM;
        self.custom_curve = Some(curve);
    }

    /// Checks if the source is a given MIDI Controller.
    /// Equivalent to: isController(controller)
    pub fn is_controller(&self, controller: u8) -> bool {
        self.is_cc && self.index == controller
    }

    /// Decodes a ModulatorSource from a raw SF2 source enum (16-bit).
    ///
    /// Bit layout (SF2 spec 2.10):
    ///   bits [15:10] = curve type, bit 9 = bipolar, bit 8 = negative,
    ///   bit 7 = CC flag, bits [6:0] = index.
    ///
    /// Equivalent to: static fromSourceEnum(sourceEnum)
    pub fn from_source_enum(source_enum: u16) -> Self {
        let is_bipolar = bit_mask_to_bool(source_enum as u32, 9);
        let is_negative = bit_mask_to_bool(source_enum as u32, 8);
        let is_cc = bit_mask_to_bool(source_enum as u32, 7);
        let index = (source_enum & 0x7F) as ModulatorSourceIndex;
        // 6 bits, SF2 curve types are 0 to 3, SFe adds 4 (custom)
        let curve_type = ((source_enum >> 10) & 0x3f) as ModulatorCurveType;
        Self::new(index, curve_type, is_cc, is_bipolar, is_negative)
    }

    /// Copies a ModulatorSource, with the same curve.
    /// Equivalent to: static copyFrom(source)
    pub fn copy_from(source: &ModulatorSource) -> Self {
        source.clone()
    }

    /// Encodes this ModulatorSource back to a raw SF2 source enum (16-bit).
    /// Equivalent to: toSourceEnum()
    pub fn to_source_enum(&self) -> u16 {
        ((self.curve_type as u16) << 10)
            | ((to_numeric_bool(self.is_bipolar) as u16) << 9)
            | ((to_numeric_bool(self.is_negative) as u16) << 8)
            | ((to_numeric_bool(self.is_cc) as u16) << 7)
            | (self.index as u16)
    }

    /// Returns true if the two ModulatorSources are fully identical.
    /// Equivalent to: isIdentical(source)
    pub fn is_identical(&self, other: &ModulatorSource) -> bool {
        self.index == other.index
            && self.is_negative == other.is_negative
            && self.is_cc == other.is_cc
            && self.is_bipolar == other.is_bipolar
            // Custom curves are the same only if their points are
            && self.curve_is_identical(other)
    }

    /// Equivalent to: ModulatorCurve.isIdentical(other) / CustomCurve.isIdentical(other)
    fn curve_is_identical(&self, other: &ModulatorSource) -> bool {
        match (&self.custom_curve, &other.custom_curve) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b) || a.points() == b.points(),
            (None, None) => {
                // A custom curve is only identical to a custom curve with the same points, never to an unresolved one
                self.curve_type == other.curve_type
                    && self.curve_type != modulator_curve_types::CUSTOM
            }
            _ => false,
        }
    }

    /// Computes the current float value from this source.
    ///
    /// * `channel` – the channel controllers and MIDI parameters.
    /// * `pitch_wheel` – current pitch-wheel raw value (0–16 383).
    /// * `voice` – per-note data (note number, velocity, pressure).
    ///
    /// Returns a value in the range `[0.0, 1.0]` (unipolar) or `[-1.0, 1.0]`
    /// (bipolar) according to the configured curve and polarity.
    ///
    /// Equivalent to: getValue(channel, pitchWheel, voice)
    pub fn get_value(
        &self,
        channel: &SF2Channel,
        pitch_wheel: i32,
        voice: &VoiceModInputs,
    ) -> f32 {
        // The source value x, 0 to 16,384 (SFe 4.0, section 7.2): 7-bit and 14-bit values are mapped center-max,
        // So the center and the maximum are exact
        let v7 = &*SOURCE_VALUE_7_BIT;
        let v14 = &*SOURCE_VALUE_14_BIT;
        let x: usize = if self.is_cc {
            // A controller is its 7-bit value alone, the LSB controllers are sources of their own
            v7[(channel.midi_controllers[self.index as usize] >> 7) as usize] as usize
        } else {
            match self.index {
                modulator_sources::NOTE_ON_VELOCITY => v7[voice.velocity as usize] as usize,
                modulator_sources::NOTE_ON_KEY_NUM => v7[voice.target_key as usize] as usize,
                // Use MIDI Note here as key shift is internal
                modulator_sources::POLY_PRESSURE => v7[voice.pressure as usize] as usize,
                modulator_sources::CHANNEL_PRESSURE => v7[channel.pressure as usize] as usize,
                modulator_sources::PITCH_WHEEL => v14[pitch_wheel as usize] as usize,
                // The range in 1/128 semitones, it may be fractional.
                // Ranges of 128 semitones and more would read past the table
                modulator_sources::PITCH_WHEEL_RANGE => {
                    v14[16_383.min((channel.pitch_wheel_range * 128.0).floor() as i32) as usize] as usize
                }
                // The parameter is in cents, RPN 5 value is cents * 1.28 (100/128 cents per step)
                // The source is full at 16 semitones, so it is 8 times that value
                modulator_sources::MODULATION_DEPTH_RANGE => {
                    v14[16_383.min(js_round(channel.modulation_depth * 1.28) as i32 * 8) as usize] as usize
                }
                // No Controller is the end of the range.
                // An unknown source makes the modulator ignored when it is read, this is only a fallback
                _ => 16_384,
            }
        };

        if let Some(curve) = &self.custom_curve {
            return curve.get_value(x, self.is_bipolar, self.is_negative);
        }
        // 2-bit transform as in 0bPD
        let transform = (if self.is_bipolar { 0b10 } else { 0 }) | (if self.is_negative { 1 } else { 0 });
        match built_in_table(self.curve_type, transform) {
            Some(table) => table[x],
            // An unresolved curve outputs 0
            None => 0.0,
        }
    }
}

impl fmt::Display for ModulatorSource {
    /// Equivalent to: toString()
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let source_name = source_index_name(self.index, self.is_cc);
        let curve_name = match &self.custom_curve {
            Some(curve) => curve.to_string(),
            None => curve_type_name(self.curve_type),
        };
        let polarity = if self.is_bipolar {
            "bipolar"
        } else {
            "unipolar"
        };
        let direction = if self.is_negative {
            "negative"
        } else {
            "positive"
        };
        write!(
            f,
            "{} {} {} {}",
            source_name, curve_name, polarity, direction
        )
    }
}

// ---------------------------------------------------------------------------
// Private helpers: name lookups
// Equivalent to: private get sourceName() / private get curveTypeName()
// ---------------------------------------------------------------------------

fn source_index_name(index: ModulatorSourceIndex, is_cc: bool) -> String {
    if is_cc {
        // MIDI Controller Palette – look up controller name.
        let name: Option<&str> = match index {
            0 => Some("bank_select"),
            1 => Some("modulation_wheel"),
            2 => Some("breath_controller"),
            4 => Some("foot_controller"),
            5 => Some("portamento_time"),
            6 => Some("data_entry_msb"),
            7 => Some("main_volume"),
            8 => Some("balance"),
            10 => Some("pan"),
            11 => Some("expression_controller"),
            64 => Some("sustain_pedal"),
            65 => Some("portamento_on_off"),
            66 => Some("sostenuto_pedal"),
            67 => Some("soft_pedal"),
            96 => Some("data_increment"),
            97 => Some("data_decrement"),
            98 => Some("non_registered_parameter_lsb"),
            99 => Some("non_registered_parameter_msb"),
            100 => Some("registered_parameter_lsb"),
            101 => Some("registered_parameter_msb"),
            120 => Some("all_sound_off"),
            121 => Some("reset_all_controllers"),
            123 => Some("all_notes_off"),
            _ => None,
        };
        name.map(|s| s.to_string())
            .unwrap_or_else(|| index.to_string())
    } else {
        // Predefined modulator source palette.
        let name: Option<&str> = match index {
            0 => Some("no_controller"),
            2 => Some("note_on_velocity"),
            3 => Some("note_on_key_num"),
            10 => Some("poly_pressure"),
            13 => Some("channel_pressure"),
            14 => Some("pitch_wheel"),
            16 => Some("pitch_wheel_range"),
            17 => Some("modulation_depth_range"),
            127 => Some("link"),
            _ => None,
        };
        name.map(|s| s.to_string())
            .unwrap_or_else(|| index.to_string())
    }
}


// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthesizer::audio_engine::channel::reset::DEFAULT_MIDI_CONTROLLERS;
    use crate::synthesizer::audio_engine::synth_constants::CONTROLLER_TABLE_SIZE;

    const EPS: f32 = 1e-4;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < EPS
    }

    fn make_controllers() -> Vec<i16> {
        DEFAULT_MIDI_CONTROLLERS.to_vec()
    }

    /// A channel with the default pressure (0) and pitch wheel range (2).
    fn chan(midi_controllers: &[i16]) -> SF2Channel<'_> {
        SF2Channel { midi_controllers, pressure: 0, pitch_wheel_range: 2.0, modulation_depth: 50.0 }
    }

    fn voice(midi_note: u8, velocity: u8, pressure: u8) -> VoiceModInputs {
        VoiceModInputs {
            midi_note,
            // In tests without key shift / keyNum override, the effective key equals the note.
            target_key: midi_note as i16,
            velocity,
            pressure,
        }
    }

    // ── Default ──────────────────────────────────────────────────────────────

    #[test]
    fn test_default_fields() {
        let s = ModulatorSource::default();
        assert_eq!(s.index, modulator_sources::NO_CONTROLLER);
        assert_eq!(s.curve_type, modulator_curve_types::LINEAR);
        assert!(!s.is_cc);
        assert!(!s.is_bipolar);
        assert!(!s.is_negative);
    }

    // ── new() ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_new_sets_all_fields() {
        let s = ModulatorSource::new(7, modulator_curve_types::CONCAVE, true, true, false);
        assert_eq!(s.index, 7);
        assert_eq!(s.curve_type, modulator_curve_types::CONCAVE);
        assert!(s.is_cc);
        assert!(s.is_bipolar);
        assert!(!s.is_negative);
    }

    // ── from_source_enum / to_source_enum ────────────────────────────────────

    #[test]
    fn test_from_source_enum_no_controller() {
        // Source enum 0 = no controller, linear, unipolar, positive, not CC
        let s = ModulatorSource::from_source_enum(0);
        assert_eq!(s.index, 0);
        assert_eq!(s.curve_type, modulator_curve_types::LINEAR);
        assert!(!s.is_cc);
        assert!(!s.is_bipolar);
        assert!(!s.is_negative);
    }

    #[test]
    fn test_to_source_enum_zero() {
        let s = ModulatorSource::default();
        assert_eq!(s.to_source_enum(), 0);
    }

    #[test]
    fn test_from_to_roundtrip_all_flags() {
        // curveType=3, isBipolar=true, isNegative=true, isCC=true, index=7
        // = (3<<10) | (1<<9) | (1<<8) | (1<<7) | 7
        // = 3072  | 512   | 256   | 128  | 7 = 3975
        let encoded: u16 = (3 << 10) | (1 << 9) | (1 << 8) | (1 << 7) | 7;
        let s = ModulatorSource::from_source_enum(encoded);
        assert_eq!(s.curve_type, 3);
        assert!(s.is_bipolar);
        assert!(s.is_negative);
        assert!(s.is_cc);
        assert_eq!(s.index, 7);
        assert_eq!(s.to_source_enum(), encoded);
    }

    #[test]
    fn test_from_to_roundtrip_cc_linear() {
        // CC flag set, index=10 (pan), linear, unipolar positive
        let encoded: u16 = (1 << 7) | 10; // = 0b10001010 = 138
        let s = ModulatorSource::from_source_enum(encoded);
        assert!(s.is_cc);
        assert_eq!(s.index, 10);
        assert_eq!(s.curve_type, modulator_curve_types::LINEAR);
        assert!(!s.is_bipolar);
        assert!(!s.is_negative);
        assert_eq!(s.to_source_enum(), encoded);
    }

    #[test]
    fn test_to_source_enum_bipolar_only() {
        let s = ModulatorSource::new(0, modulator_curve_types::LINEAR, false, true, false);
        // (0<<10) | (1<<9) | (0<<8) | (0<<7) | 0 = 512
        assert_eq!(s.to_source_enum(), 512);
    }

    #[test]
    fn test_to_source_enum_negative_only() {
        let s = ModulatorSource::new(0, modulator_curve_types::LINEAR, false, false, true);
        // (0<<10) | (0<<9) | (1<<8) | (0<<7) | 0 = 256
        assert_eq!(s.to_source_enum(), 256);
    }

    #[test]
    fn test_roundtrip_all_curve_types() {
        for ct in 0u8..4 {
            let s = ModulatorSource::new(0, ct, false, false, false);
            let encoded = s.to_source_enum();
            let decoded = ModulatorSource::from_source_enum(encoded);
            assert_eq!(
                decoded.curve_type, ct,
                "curve_type {} round-trip failed",
                ct
            );
        }
    }

    // ── copy_from ─────────────────────────────────────────────────────────────

    #[test]
    fn test_copy_from_is_equal() {
        let original = ModulatorSource::new(7, modulator_curve_types::CONCAVE, true, false, true);
        let copy = ModulatorSource::copy_from(&original);
        assert_eq!(original, copy);
    }

    #[test]
    fn test_copy_from_is_independent() {
        let original = ModulatorSource::new(7, modulator_curve_types::CONCAVE, true, false, true);
        let mut copy = ModulatorSource::copy_from(&original);
        copy.index = 99;
        assert_eq!(copy.index, 99); // copy was mutated
        assert_eq!(original.index, 7); // original unchanged
    }

    // ── is_identical ──────────────────────────────────────────────────────────

    #[test]
    fn test_is_identical_same() {
        let a = ModulatorSource::new(3, modulator_curve_types::SWITCH, false, true, false);
        let b = a.clone();
        assert!(a.is_identical(&b));
    }

    #[test]
    fn test_is_identical_different_index() {
        let a = ModulatorSource::new(3, modulator_curve_types::LINEAR, false, false, false);
        let b = ModulatorSource::new(4, modulator_curve_types::LINEAR, false, false, false);
        assert!(!a.is_identical(&b));
    }

    #[test]
    fn test_is_identical_different_curve() {
        let a = ModulatorSource::new(0, modulator_curve_types::LINEAR, false, false, false);
        let b = ModulatorSource::new(0, modulator_curve_types::CONCAVE, false, false, false);
        assert!(!a.is_identical(&b));
    }

    #[test]
    fn test_is_identical_different_bipolar() {
        let a = ModulatorSource::new(0, modulator_curve_types::LINEAR, false, true, false);
        let b = ModulatorSource::new(0, modulator_curve_types::LINEAR, false, false, false);
        assert!(!a.is_identical(&b));
    }

    // ── get_value: non-CC sources ─────────────────────────────────────────────

    #[test]
    fn test_get_value_no_controller_linear_unipolar() {
        // rawValue = 16383; linear unipolar: value ≈ 1.0
        let s = ModulatorSource::default(); // no_controller, linear, unipolar positive
        let cc = make_controllers();
        let v = voice(60, 100, 0);
        let val = s.get_value(&chan(&cc), 0, &v);
        assert!(val > 0.999, "expected ≈1.0, got {}", val);
    }

    #[test]
    fn test_get_value_note_on_key_num_middle_c() {
        // key=64 → rawValue = 64<<7 = 8192 → table value = 8192/16384 = 0.5 (linear)
        let s = ModulatorSource::new(
            modulator_sources::NOTE_ON_KEY_NUM,
            modulator_curve_types::LINEAR,
            false,
            false,
            false,
        );
        let cc = make_controllers();
        let v = voice(64, 100, 0);
        let val = s.get_value(&chan(&cc), 0, &v);
        assert!(approx_eq(val, 0.5), "expected 0.5, got {}", val);
    }

    #[test]
    fn test_get_value_note_on_velocity_half() {
        // velocity=64 → rawValue = 64<<7 = 8192 → 0.5 (linear)
        let s = ModulatorSource::new(
            modulator_sources::NOTE_ON_VELOCITY,
            modulator_curve_types::LINEAR,
            false,
            false,
            false,
        );
        let cc = make_controllers();
        let v = voice(60, 64, 0);
        let val = s.get_value(&chan(&cc), 0, &v);
        assert!(approx_eq(val, 0.5), "expected 0.5, got {}", val);
    }

    #[test]
    fn test_get_value_poly_pressure_half() {
        // pressure=64 → rawValue = 64<<7 = 8192 → 0.5 (linear)
        let s = ModulatorSource::new(
            modulator_sources::POLY_PRESSURE,
            modulator_curve_types::LINEAR,
            false,
            false,
            false,
        );
        let cc = make_controllers();
        let v = voice(60, 100, 64);
        let val = s.get_value(&chan(&cc), 0, &v);
        assert!(approx_eq(val, 0.5), "expected 0.5, got {}", val);
    }

    #[test]
    fn test_get_value_pitch_wheel_center() {
        // pitch_wheel=8192 → 0.5 (linear unipolar)
        let s = ModulatorSource::new(
            modulator_sources::PITCH_WHEEL,
            modulator_curve_types::LINEAR,
            false,
            false,
            false,
        );
        let cc = make_controllers();
        let v = voice(60, 100, 0);
        let val = s.get_value(&chan(&cc), 8192, &v);
        assert!(approx_eq(val, 0.5), "expected 0.5, got {}", val);
    }

    // ── get_value: CC source ──────────────────────────────────────────────────

    #[test]
    fn test_get_value_cc_main_volume_default() {
        // main_volume default = 100 (from DEFAULT_MIDI_CONTROLLERS),
        // stored as 100 << 7 = 12800, mapped center-max (linear)
        let s = ModulatorSource::new(7, modulator_curve_types::LINEAR, true, false, false);
        let cc = make_controllers();
        let v = voice(60, 100, 0);
        let val = s.get_value(&chan(&cc), 0, &v);
        // Main volume default is 100, at the source value round(8192 + 8192 * 36 / 63) = 12873
        let expected = 12873.0f32 / 16_384.0;
        assert!(
            approx_eq(val, expected),
            "expected {}, got {}",
            expected,
            val
        );
    }

    #[test]
    fn test_get_value_cc_zero_value() {
        // CC index 0 (bank select), default = 0 → rawValue = 0 → linear unipolar = 0.0
        let s = ModulatorSource::new(0, modulator_curve_types::LINEAR, true, false, false);
        let mut cc = vec![0i16; CONTROLLER_TABLE_SIZE];
        cc[0] = 0;
        let v = voice(60, 100, 0);
        let val = s.get_value(&chan(&cc), 0, &v);
        assert!(approx_eq(val, 0.0), "expected 0.0, got {}", val);
    }

    // ── get_value: polarity / direction ──────────────────────────────────────

    #[test]
    fn test_get_value_bipolar_linear_center_is_zero() {
        // rawValue = 8192, bipolar linear: 0.5 * 2 - 1 = 0.0
        let s = ModulatorSource::new(
            modulator_sources::PITCH_WHEEL,
            modulator_curve_types::LINEAR,
            false,
            true, // bipolar
            false,
        );
        let cc = make_controllers();
        let v = voice(60, 100, 0);
        let val = s.get_value(&chan(&cc), 8192, &v);
        assert!(
            approx_eq(val, 0.0),
            "expected 0.0 (bipolar center), got {}",
            val
        );
    }

    #[test]
    fn test_get_value_negative_linear_inverts() {
        // rawValue = 0, unipolar negative: 1.0 - 0.0 = 1.0
        let s = ModulatorSource::new(
            modulator_sources::PITCH_WHEEL,
            modulator_curve_types::LINEAR,
            false,
            false,
            true, // negative
        );
        let cc = make_controllers();
        let v = voice(60, 100, 0);
        let val = s.get_value(&chan(&cc), 0, &v);
        assert!(
            approx_eq(val, 1.0),
            "expected 1.0 (negative direction), got {}",
            val
        );
    }

    #[test]
    fn test_get_value_bipolar_positive_max() {
        // rawValue = 16383, bipolar positive linear: ~1.0 * 2 - 1 ≈ 1.0
        let s = ModulatorSource::new(
            modulator_sources::NO_CONTROLLER,
            modulator_curve_types::LINEAR,
            false,
            true, // bipolar
            false,
        );
        let cc = make_controllers();
        let v = voice(60, 100, 0);
        let val = s.get_value(&chan(&cc), 0, &v);
        // 16383/16384 * 2 - 1 ≈ 0.9998
        assert!(val > 0.999, "expected ≈1.0 (bipolar max), got {}", val);
    }

    // ── Display / to_string ───────────────────────────────────────────────────

    #[test]
    fn test_display_default() {
        let s = ModulatorSource::default();
        let display = s.to_string();
        assert!(display.contains("no_controller"), "got: {}", display);
        assert!(display.contains("linear"), "got: {}", display);
        assert!(display.contains("unipolar"), "got: {}", display);
        assert!(display.contains("positive"), "got: {}", display);
    }

    #[test]
    fn test_display_cc_bipolar_negative() {
        let s = ModulatorSource::new(7, modulator_curve_types::CONCAVE, true, true, true);
        let display = s.to_string();
        assert!(display.contains("main_volume"), "got: {}", display);
        assert!(display.contains("concave"), "got: {}", display);
        assert!(display.contains("bipolar"), "got: {}", display);
        assert!(display.contains("negative"), "got: {}", display);
    }

    #[test]
    fn test_display_unknown_index() {
        // index 50 is not a named modulator source, should fall back to "50"
        let s = ModulatorSource::new(50, modulator_curve_types::LINEAR, false, false, false);
        let display = s.to_string();
        assert!(display.starts_with("50 "), "got: {}", display);
    }

    #[test]
    fn test_display_pitch_wheel_source() {
        let s = ModulatorSource::new(
            modulator_sources::PITCH_WHEEL,
            modulator_curve_types::LINEAR,
            false,
            true,
            false,
        );
        let display = s.to_string();
        assert!(display.contains("pitch_wheel"), "got: {}", display);
        assert!(display.contains("bipolar"), "got: {}", display);
    }
}
