/// modulator.rs
/// purpose: SF2 Modulator and DecodedModulator structs, default modulators list.
/// Ported from: src/soundbank/basic_soundbank/modulator.ts
use std::fmt;
use std::sync::LazyLock;

use crate::soundbank::basic_soundbank::generator_types::{
    GeneratorType, MAX_GENERATOR, generator_types,
};
use crate::soundbank::basic_soundbank::modulator_source::ModulatorSource;
use crate::soundbank::enums::modulator_curve_types;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::write_word;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Size in bytes of a single modulator record in SF2 format (5 × 16-bit words).
/// Equivalent to: MOD_BYTE_SIZE = 10
pub const MOD_BYTE_SIZE: usize = 10;

/// Default transform amount used for velocity/volume/expression → attenuation modulators.
/// Equivalent to: DEFAULT_ATTENUATION_MOD_AMOUNT = 960
pub const DEFAULT_ATTENUATION_MOD_AMOUNT: i32 = 960;

/// Default curve type for attenuation modulators: concave (= 1).
/// Equivalent to: DEFAULT_ATTENUATION_MOD_CURVE_TYPE = modulatorCurveTypes.concave
pub const DEFAULT_ATTENUATION_MOD_CURVE_TYPE: u8 = modulator_curve_types::CONCAVE;

/// Source enum for the default resonant modulator.
/// ModulatorSource::new(filterResonance=71, LINEAR, isCC=true, isBipolar=true, isNegative=false)
///   .to_source_enum()
/// = (0<<10) | (1<<9) | (0<<8) | (1<<7) | 71 = 512 + 128 + 71 = 711
/// Equivalent to: defaultResonantModSource (module-private in modulator.ts)
const DEFAULT_RESONANT_MOD_SOURCE: u16 = 711;

// ---------------------------------------------------------------------------
// SoundFontWriteIndexes (minimal stub)
//
// Equivalent to: SoundFontWriteIndexes (from src/soundbank/soundfont/write/types.ts)
// TODO: Move to soundbank/soundfont/write/types.rs when that file is ported.
// ---------------------------------------------------------------------------

/// Minimal stub for SoundFontWriteIndexes.
/// The only field used by `Modulator::write()` is the modulator counter.
/// Note: the TypeScript field is named `mod`, which is a Rust keyword;
///       it is renamed `mod_count` here.
pub struct SoundFontWriteIndexes {
    /// Running count of modulators written.
    /// Equivalent to: indexes.mod
    pub mod_count: u32,
}

// ---------------------------------------------------------------------------
// get_mod_source_enum
// ---------------------------------------------------------------------------

/// Builds a raw SF2 source enum from curve/polarity/CC parameters and an index.
/// Equivalent to: getModSourceEnum(curveType, isBipolar, isNegative, isCC, index)
pub fn get_mod_source_enum(
    curve_type: u8,
    is_bipolar: bool,
    is_negative: bool,
    is_cc: bool,
    index: u8,
) -> u16 {
    ModulatorSource::new(index, curve_type, is_cc, is_bipolar, is_negative).to_source_enum()
}

// ---------------------------------------------------------------------------
// Modulator
// ---------------------------------------------------------------------------

/// An SF2 Modulator with fully-parsed source objects.
/// Equivalent to: class Modulator
#[derive(Clone, Debug, PartialEq)]
pub struct Modulator {
    /// Generator destination of this modulator.
    /// Equivalent to: destination
    pub destination: GeneratorType,

    /// Transform amount (signed; written as 16-bit LE in SF2 files).
    /// f64 to match TypeScript's number type (allows fractional SysEx modulator amounts).
    /// Equivalent to: transformAmount
    pub transform_amount: f64,

    /// Transform type (0 = linear, 2 = absolute value).
    /// Equivalent to: transformType: ModulatorTransformType
    pub transform_type: u16,

    /// True if this is a reverb/chorus effect modulator (BASSMIDI compatibility).
    /// Equivalent to: isEffectModulator
    pub is_effect_modulator: bool,

    /// True if this is the default resonant modulator (CC 71 → initialFilterQ).
    /// The default resonant modulator does not affect filter gain.
    /// Equivalent to: isDefaultResonantModulator
    pub is_default_resonant_modulator: bool,

    /// Primary modulator source.
    /// Equivalent to: primarySource
    pub primary_source: ModulatorSource,

    /// Secondary (secondary-source) modulator.
    /// Equivalent to: secondarySource
    pub secondary_source: ModulatorSource,
}

impl Default for Modulator {
    /// Default modulator: INVALID destination, zero amounts, default (no-op) sources.
    /// Equivalent to: new Modulator() with all defaults
    fn default() -> Self {
        Self {
            destination: generator_types::INVALID,
            transform_amount: 0.0,
            transform_type: 0,
            is_effect_modulator: false,
            is_default_resonant_modulator: false,
            primary_source: ModulatorSource::default(),
            secondary_source: ModulatorSource::default(),
        }
    }
}

impl Modulator {
    /// Creates a new Modulator with explicit parameters.
    /// Equivalent to: constructor(primarySource, secondarySource, destination, amount,
    ///                            transformType, isEffectModulator, isDefaultResonantModulator)
    pub fn new(
        primary_source: ModulatorSource,
        secondary_source: ModulatorSource,
        destination: GeneratorType,
        transform_amount: f64,
        transform_type: u16,
        is_effect_modulator: bool,
        is_default_resonant_modulator: bool,
    ) -> Self {
        Self {
            destination,
            transform_amount,
            transform_type,
            is_effect_modulator,
            is_default_resonant_modulator,
            primary_source,
            secondary_source,
        }
    }

    /// Checks if two modulators are identical (in SF2 terms).
    /// When `check_amount` is false (default), the transform amount is not compared.
    /// Equivalent to: static isIdentical(mod1, mod2, checkAmount = false)
    pub fn is_identical(mod1: &Modulator, mod2: &Modulator, check_amount: bool) -> bool {
        mod1.primary_source.is_identical(&mod2.primary_source)
            && mod1.secondary_source.is_identical(&mod2.secondary_source)
            && mod1.destination == mod2.destination
            && mod1.transform_type == mod2.transform_type
            && (!check_amount || mod1.transform_amount == mod2.transform_amount)
    }

    /// Copies a modulator.
    /// Equivalent to: static copyFrom(mod)
    pub fn copy_from(mod_: &Modulator) -> Self {
        Self {
            primary_source: ModulatorSource::copy_from(&mod_.primary_source),
            secondary_source: ModulatorSource::copy_from(&mod_.secondary_source),
            destination: mod_.destination,
            transform_amount: mod_.transform_amount,
            transform_type: mod_.transform_type,
            is_effect_modulator: mod_.is_effect_modulator,
            is_default_resonant_modulator: mod_.is_default_resonant_modulator,
        }
    }

    /// Writes this modulator to an IndexedByteArray (10 bytes = 5 × 16-bit LE words).
    /// Optionally increments the modulator counter in `indexes`.
    /// Equivalent to: write(modData, indexes?)
    pub fn write(
        &self,
        mod_data: &mut IndexedByteArray,
        indexes: Option<&mut SoundFontWriteIndexes>,
    ) {
        write_word(mod_data, self.primary_source.to_source_enum() as u32);
        write_word(mod_data, self.destination as u16 as u32);
        write_word(mod_data, self.transform_amount as i16 as u16 as u32); // lower 16 bits (two's complement)
        write_word(mod_data, self.secondary_source.to_source_enum() as u32);
        write_word(mod_data, self.transform_type as u32);
        if let Some(idx) = indexes {
            idx.mod_count += 1;
        }
    }

    /// Sums transform amounts and returns a NEW modulator.
    /// Equivalent to: sumTransform(modulator)
    pub fn sum_transform(&self, modulator: &Modulator) -> Modulator {
        let mut m = Modulator::copy_from(self);
        m.transform_amount += modulator.transform_amount;
        m
    }

    /// Converts this Modulator to a DecodedModulator using raw source enum values.
    /// Used when assigning cached modulators to a voice at note-on time.
    /// Preserves f64 precision for SysEx modulators with fractional amounts.
    /// Equivalent to: no direct TS equivalent; bridges Modulator -> DecodedModulator
    pub fn to_decoded(&self) -> DecodedModulator {
        DecodedModulator::new_f64(
            self.primary_source.to_source_enum(),
            self.secondary_source.to_source_enum(),
            self.destination,
            self.transform_amount,
            self.transform_type,
        )
    }
}

impl fmt::Display for Modulator {
    /// Equivalent to: toString()
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dest_name = generator_type_name(self.destination);
        write!(
            f,
            "Source: {}\nSecondary source: {}\nto: {}\namount: {}{}",
            self.primary_source,
            self.secondary_source,
            dest_name,
            self.transform_amount,
            if self.transform_type == 2 {
                "absolute value"
            } else {
                ""
            }
        )
    }
}

// ---------------------------------------------------------------------------
// DecodedModulator  (backward-compatible, const-fn-capable)
// ---------------------------------------------------------------------------

/// Decoded SF2 Modulator parsed from raw source-enum fields.
/// Stores raw 16-bit source enums in addition to derived flags.
/// Use `primary_source()` / `secondary_source()` to obtain parsed ModulatorSource objects.
/// Equivalent to: class DecodedModulator extends Modulator
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedModulator {
    /// Raw SF2 primary source enum (16-bit).
    pub source_enum: u16,
    /// Raw SF2 secondary source enum (16-bit).
    pub secondary_source_enum: u16,
    /// Destination generator type.
    pub destination: GeneratorType,
    /// Transform amount. Read from SF2 as signed 16-bit, but stored as f64
    /// to match TypeScript's number type (allows fractional SysEx modulator amounts).
    pub transform_amount: f64,
    /// Transform type.
    pub transform_type: u16,
    /// True if this is a reverb/chorus effect modulator (BASSMIDI compatibility).
    /// Equivalent to: isEffectModulator
    pub is_effect_modulator: bool,
    /// True if this is the default resonant modulator.
    /// Equivalent to: isDefaultResonantModulator
    pub is_default_resonant_modulator: bool,
}

impl DecodedModulator {
    /// Creates a new `DecodedModulator` from raw SF2 fields.
    /// This is a `const fn` so it can be used in static/const contexts.
    /// Equivalent to: new DecodedModulator(sourceEnum, secondarySourceEnum, destination, amount, transformType)
    pub const fn new(
        source_enum: u16,
        secondary_source_enum: u16,
        destination: GeneratorType,
        transform_amount: i16,
        transform_type: u16,
    ) -> Self {
        let is_effect_modulator = (source_enum == 0x00db || source_enum == 0x00dd)
            && secondary_source_enum == 0x0
            && (destination == generator_types::REVERB_EFFECTS_SEND
                || destination == generator_types::CHORUS_EFFECTS_SEND);

        let is_default_resonant_modulator = source_enum == DEFAULT_RESONANT_MOD_SOURCE
            && secondary_source_enum == 0x0
            && destination == generator_types::INITIAL_FILTER_Q;

        // Clamp invalid destinations (must happen after the flag checks above).
        let destination = if destination > MAX_GENERATOR {
            generator_types::INVALID
        } else {
            destination
        };

        Self {
            source_enum,
            secondary_source_enum,
            destination,
            transform_amount: transform_amount as f64,
            transform_type,
            is_effect_modulator,
            is_default_resonant_modulator,
        }
    }

    /// Creates a new `DecodedModulator` with an f64 transform_amount.
    /// Used for SysEx modulators that can have fractional amounts.
    pub fn new_f64(
        source_enum: u16,
        secondary_source_enum: u16,
        destination: GeneratorType,
        transform_amount: f64,
        transform_type: u16,
    ) -> Self {
        let is_effect_modulator = (source_enum == 0x00db || source_enum == 0x00dd)
            && secondary_source_enum == 0x0
            && (destination == generator_types::REVERB_EFFECTS_SEND
                || destination == generator_types::CHORUS_EFFECTS_SEND);

        let is_default_resonant_modulator = source_enum == DEFAULT_RESONANT_MOD_SOURCE
            && secondary_source_enum == 0x0
            && destination == generator_types::INITIAL_FILTER_Q;

        let destination = if destination > MAX_GENERATOR {
            generator_types::INVALID
        } else {
            destination
        };

        Self {
            source_enum,
            secondary_source_enum,
            destination,
            transform_amount,
            transform_type,
            is_effect_modulator,
            is_default_resonant_modulator,
        }
    }

    /// Returns the primary ModulatorSource decoded from `source_enum`.
    /// Equivalent to: this.primarySource (decoded from source enum at read time in TS)
    pub fn primary_source(&self) -> ModulatorSource {
        ModulatorSource::from_source_enum(self.source_enum)
    }

    /// Returns the secondary ModulatorSource decoded from `secondary_source_enum`.
    /// Equivalent to: this.secondarySource (decoded from secondary source enum)
    pub fn secondary_source(&self) -> ModulatorSource {
        ModulatorSource::from_source_enum(self.secondary_source_enum)
    }
}

// ---------------------------------------------------------------------------
// Generator type name helper (for Display)
// Equivalent to: private get destinationName() { return Object.keys(generatorTypes).find(...) }
// ---------------------------------------------------------------------------

fn generator_type_name(gt: GeneratorType) -> String {
    match gt {
        generator_types::INVALID => "INVALID".to_string(),
        generator_types::START_ADDRS_OFFSET => "startAddrsOffset".to_string(),
        generator_types::END_ADDR_OFFSET => "endAddrOffset".to_string(),
        generator_types::STARTLOOP_ADDRS_OFFSET => "startloopAddrsOffset".to_string(),
        generator_types::ENDLOOP_ADDRS_OFFSET => "endloopAddrsOffset".to_string(),
        generator_types::START_ADDRS_COARSE_OFFSET => "startAddrsCoarseOffset".to_string(),
        generator_types::MOD_LFO_TO_PITCH => "modLfoToPitch".to_string(),
        generator_types::VIB_LFO_TO_PITCH => "vibLfoToPitch".to_string(),
        generator_types::MOD_ENV_TO_PITCH => "modEnvToPitch".to_string(),
        generator_types::INITIAL_FILTER_FC => "initialFilterFc".to_string(),
        generator_types::INITIAL_FILTER_Q => "initialFilterQ".to_string(),
        generator_types::MOD_LFO_TO_FILTER_FC => "modLfoToFilterFc".to_string(),
        generator_types::MOD_ENV_TO_FILTER_FC => "modEnvToFilterFc".to_string(),
        generator_types::END_ADDRS_COARSE_OFFSET => "endAddrsCoarseOffset".to_string(),
        generator_types::MOD_LFO_TO_VOLUME => "modLfoToVolume".to_string(),
        generator_types::CHORUS_EFFECTS_SEND => "chorusEffectsSend".to_string(),
        generator_types::REVERB_EFFECTS_SEND => "reverbEffectsSend".to_string(),
        generator_types::PAN => "pan".to_string(),
        generator_types::DELAY_MOD_LFO => "delayModLFO".to_string(),
        generator_types::FREQ_MOD_LFO => "freqModLFO".to_string(),
        generator_types::DELAY_VIB_LFO => "delayVibLFO".to_string(),
        generator_types::FREQ_VIB_LFO => "freqVibLFO".to_string(),
        generator_types::DELAY_MOD_ENV => "delayModEnv".to_string(),
        generator_types::ATTACK_MOD_ENV => "attackModEnv".to_string(),
        generator_types::HOLD_MOD_ENV => "holdModEnv".to_string(),
        generator_types::DECAY_MOD_ENV => "decayModEnv".to_string(),
        generator_types::SUSTAIN_MOD_ENV => "sustainModEnv".to_string(),
        generator_types::RELEASE_MOD_ENV => "releaseModEnv".to_string(),
        generator_types::KEY_NUM_TO_MOD_ENV_HOLD => "keyNumToModEnvHold".to_string(),
        generator_types::KEY_NUM_TO_MOD_ENV_DECAY => "keyNumToModEnvDecay".to_string(),
        generator_types::DELAY_VOL_ENV => "delayVolEnv".to_string(),
        generator_types::ATTACK_VOL_ENV => "attackVolEnv".to_string(),
        generator_types::HOLD_VOL_ENV => "holdVolEnv".to_string(),
        generator_types::DECAY_VOL_ENV => "decayVolEnv".to_string(),
        generator_types::SUSTAIN_VOL_ENV => "sustainVolEnv".to_string(),
        generator_types::RELEASE_VOL_ENV => "releaseVolEnv".to_string(),
        generator_types::KEY_NUM_TO_VOL_ENV_HOLD => "keyNumToVolEnvHold".to_string(),
        generator_types::KEY_NUM_TO_VOL_ENV_DECAY => "keyNumToVolEnvDecay".to_string(),
        generator_types::INSTRUMENT => "instrument".to_string(),
        generator_types::KEY_RANGE => "keyRange".to_string(),
        generator_types::VEL_RANGE => "velRange".to_string(),
        generator_types::STARTLOOP_ADDRS_COARSE_OFFSET => "startloopAddrsCoarseOffset".to_string(),
        generator_types::KEY_NUM => "keyNum".to_string(),
        generator_types::VELOCITY => "velocity".to_string(),
        generator_types::INITIAL_ATTENUATION => "initialAttenuation".to_string(),
        generator_types::ENDLOOP_ADDRS_COARSE_OFFSET => "endloopAddrsCoarseOffset".to_string(),
        generator_types::COARSE_TUNE => "coarseTune".to_string(),
        generator_types::FINE_TUNE => "fineTune".to_string(),
        generator_types::SAMPLE_ID => "sampleID".to_string(),
        generator_types::SAMPLE_MODES => "sampleModes".to_string(),
        generator_types::SCALE_TUNING => "scaleTuning".to_string(),
        generator_types::EXCLUSIVE_CLASS => "exclusiveClass".to_string(),
        generator_types::OVERRIDING_ROOT_KEY => "overridingRootKey".to_string(),
        generator_types::VIB_LFO_TO_VOLUME => "vibLfoToVolume".to_string(),
        generator_types::VIB_LFO_TO_FILTER_FC => "vibLfoToFilterFc".to_string(),
        v => v.to_string(),
    }
}

// ---------------------------------------------------------------------------
// SPESSASYNTH_DEFAULT_MODULATORS
// ---------------------------------------------------------------------------

/// Creates a `Modulator` from raw source enums, applying the same logic as
/// `DecodedModulator::new()` (effect/resonant detection, destination clamping).
/// Used internally when building `SPESSASYNTH_DEFAULT_MODULATORS`.
fn decoded_mod(
    src_enum: u16,
    sec_enum: u16,
    dest: GeneratorType,
    amount: i32,
    transform_type: u16,
) -> Modulator {
    let primary_source = ModulatorSource::from_source_enum(src_enum);
    let secondary_source = ModulatorSource::from_source_enum(sec_enum);

    let is_effect_modulator = (src_enum == 0x00db || src_enum == 0x00dd)
        && sec_enum == 0x0
        && (dest == generator_types::REVERB_EFFECTS_SEND
            || dest == generator_types::CHORUS_EFFECTS_SEND);

    let is_default_resonant_modulator = src_enum == DEFAULT_RESONANT_MOD_SOURCE
        && sec_enum == 0x0
        && dest == generator_types::INITIAL_FILTER_Q;

    let dest = if dest > MAX_GENERATOR {
        generator_types::INVALID
    } else {
        dest
    };

    Modulator::new(
        primary_source,
        secondary_source,
        dest,
        amount as f64,
        transform_type,
        is_effect_modulator,
        is_default_resonant_modulator,
    )
}

/// The full set of default SF2 + SpessaSynth modulators applied to every voice.
///
/// Layout: 9 SF2 standard modulators followed by 9 SpessaSynth custom modulators.
/// Total: 18 entries.
///
/// Equivalent to: SPESSASYNTH_DEFAULT_MODULATORS (= [...defaultSoundFont2Modulators, ...defaultSpessaSynthModulators])
pub static SPESSASYNTH_DEFAULT_MODULATORS: LazyLock<Vec<Modulator>> = LazyLock::new(|| {
    // Shorthand constants
    let concave = modulator_curve_types::CONCAVE; // 1
    let linear = modulator_curve_types::LINEAR; // 0
    let convex = modulator_curve_types::CONVEX; // 2
    let switch = modulator_curve_types::SWITCH; // 3

    // Precomputed MIDI CC numbers
    const NOTE_ON_VELOCITY: u8 = 2; // modulatorSources.noteOnVelocity
    const MAIN_VOLUME: u8 = 7; // midiControllers.mainVolume
    const EXPRESSION: u8 = 11; // midiControllers.expressionController
    const TREMOLO_DEPTH: u8 = 92; // midiControllers.tremoloDepth
    const ATTACK_TIME: u8 = 73; // midiControllers.attackTime
    const RELEASE_TIME: u8 = 72; // midiControllers.releaseTime
    const DECAY_TIME: u8 = 75; // midiControllers.decayTime
    const BRIGHTNESS: u8 = 74; // midiControllers.brightness
    const FILTER_RESONANCE: u8 = 71; // midiControllers.filterResonance
    const SOFT_PEDAL: u8 = 67; // midiControllers.softPedal
    const BALANCE: u8 = 8; // midiControllers.balance

    // --- SF2 standard default modulators (Section 8.4 of the SF2 spec) ---

    vec![
        // 1. Velocity → initial attenuation (concave, unipolar, negative, non-CC)
        //    source_enum = (1<<10)|(0<<9)|(1<<8)|(0<<7)|2 = 1024+256+2 = 1282 = 0x0502
        decoded_mod(
            get_mod_source_enum(concave, false, true, false, NOTE_ON_VELOCITY),
            0x0,
            generator_types::INITIAL_ATTENUATION,
            DEFAULT_ATTENUATION_MOD_AMOUNT,
            0,
        ),
        // 2. Mod wheel → vibLFO pitch
        //    source_enum = 0x0081 (CC, index=1=modulation_wheel, linear, unipolar, positive)
        decoded_mod(0x0081, 0x0, generator_types::VIB_LFO_TO_PITCH, 50, 0),
        // 3. Main volume (CC 7) → initial attenuation (concave, unipolar, negative, CC)
        //    source_enum = (1<<10)|(0<<9)|(1<<8)|(1<<7)|7 = 1024+256+128+7 = 1415 = 0x0587
        decoded_mod(
            get_mod_source_enum(concave, false, true, true, MAIN_VOLUME),
            0x0,
            generator_types::INITIAL_ATTENUATION,
            DEFAULT_ATTENUATION_MOD_AMOUNT,
            0,
        ),
        // 4. Channel pressure → vibLFO pitch
        //    source_enum = 0x000d (non-CC, index=13=channel_pressure, linear, unipolar, positive)
        decoded_mod(0x000d, 0x0, generator_types::VIB_LFO_TO_PITCH, 50, 0),
        // 5. Pitch wheel → fine tune
        //    primary = 0x020e (non-CC, bipolar, index=14=pitch_wheel)
        //    secondary = 0x0010 (non-CC, unipolar, index=16=pitch_wheel_range)
        decoded_mod(0x020e, 0x0010, generator_types::FINE_TUNE, 12_700, 0),
        // 6. Pan (CC 10) → pan
        //    source_enum = 0x028a (CC, bipolar, index=10=pan)
        //    Amount: 500 instead of 1000 (see spessasynth issue #59)
        decoded_mod(0x028a, 0x0, generator_types::PAN, 500, 0),
        // 7. Expression (CC 11) → initial attenuation (concave, unipolar, negative, CC)
        //    source_enum = (1<<10)|(0<<9)|(1<<8)|(1<<7)|11 = 1024+256+128+11 = 1419 = 0x058b
        decoded_mod(
            get_mod_source_enum(concave, false, true, true, EXPRESSION),
            0x0,
            generator_types::INITIAL_ATTENUATION,
            DEFAULT_ATTENUATION_MOD_AMOUNT,
            0,
        ),
        // 8. Reverb send (effect mod)
        //    source_enum = 0x00db (non-CC, index=0xdb)
        decoded_mod(0x00db, 0x0, generator_types::REVERB_EFFECTS_SEND, 200, 0),
        // 9. Chorus send (effect mod)
        //    source_enum = 0x00dd (non-CC, index=0xdd)
        decoded_mod(0x00dd, 0x0, generator_types::CHORUS_EFFECTS_SEND, 200, 0),
        // --- SpessaSynth custom default modulators ---

        // 10. CC 92 (tremolo depth) → modLFO volume (linear, unipolar, positive, CC)
        //     source_enum = 0|(0<<9)|(0<<8)|(1<<7)|92 = 128+92 = 220 = 0x00dc
        decoded_mod(
            get_mod_source_enum(linear, false, false, true, TREMOLO_DEPTH),
            0x0,
            generator_types::MOD_LFO_TO_VOLUME,
            24,
            0,
        ),
        // 11. CC 73 (attack time) → volEnv attack (convex, bipolar, positive, CC)
        //     source_enum = (2<<10)|(1<<9)|(0<<8)|(1<<7)|73 = 2048+512+128+73 = 2761 = 0x0ac9
        decoded_mod(
            get_mod_source_enum(convex, true, false, true, ATTACK_TIME),
            0x0,
            generator_types::ATTACK_VOL_ENV,
            6000,
            0,
        ),
        // 12. CC 72 (release time) → volEnv release (linear, bipolar, positive, CC)
        //     source_enum = 0|(1<<9)|(0<<8)|(1<<7)|72 = 512+128+72 = 712 = 0x02c8
        decoded_mod(
            get_mod_source_enum(linear, true, false, true, RELEASE_TIME),
            0x0,
            generator_types::RELEASE_VOL_ENV,
            3600,
            0,
        ),
        // 13. CC 75 (decay time) → volEnv decay (linear, bipolar, positive, CC)
        //     source_enum = 0|(1<<9)|(0<<8)|(1<<7)|75 = 512+128+75 = 715 = 0x02cb
        decoded_mod(
            get_mod_source_enum(linear, true, false, true, DECAY_TIME),
            0x0,
            generator_types::DECAY_VOL_ENV,
            3600,
            0,
        ),
        // 14. CC 74 (brightness) → initialFilterFc (linear, bipolar, positive, CC)
        //     source_enum = 0|(1<<9)|(0<<8)|(1<<7)|74 = 512+128+74 = 714 = 0x02ca
        decoded_mod(
            get_mod_source_enum(linear, true, false, true, BRIGHTNESS),
            0x0,
            generator_types::INITIAL_FILTER_FC,
            9600,
            0,
        ),
        // 15. CC 71 (filter Q) → initialFilterQ (linear, bipolar, positive, CC)
        //     source_enum = DEFAULT_RESONANT_MOD_SOURCE = 711 = 0x02c7
        //     This is the default resonant modulator.
        decoded_mod(
            DEFAULT_RESONANT_MOD_SOURCE,
            0x0,
            generator_types::INITIAL_FILTER_Q,
            200,
            0,
        ),
        // 16. CC 67 (soft pedal) → initial attenuation (switch, unipolar, positive, CC)
        //     source_enum = (3<<10)|(0<<9)|(0<<8)|(1<<7)|67 = 3072+128+67 = 3267 = 0x0cc3
        decoded_mod(
            get_mod_source_enum(switch, false, false, true, SOFT_PEDAL),
            0x0,
            generator_types::INITIAL_ATTENUATION,
            50,
            0,
        ),
        // 17. CC 67 (soft pedal) → initialFilterFc (switch, unipolar, positive, CC)
        //     source_enum = 3267 = 0x0cc3 (same source as above)
        decoded_mod(
            get_mod_source_enum(switch, false, false, true, SOFT_PEDAL),
            0x0,
            generator_types::INITIAL_FILTER_FC,
            -2400,
            0,
        ),
        // 18. CC 8 (balance) → pan (linear, bipolar, positive, CC)
        //     source_enum = 0|(1<<9)|(0<<8)|(1<<7)|8 = 512+128+8 = 648 = 0x0288
        decoded_mod(
            get_mod_source_enum(linear, true, false, true, BALANCE),
            0x0,
            generator_types::PAN,
            500,
            0,
        ),
    ]
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::enums::{modulator_curve_types, modulator_sources};
    use crate::utils::indexed_array::IndexedByteArray;

    // ── MOD_BYTE_SIZE ────────────────────────────────────────────────────────

    #[test]
    fn test_mod_byte_size() {
        assert_eq!(MOD_BYTE_SIZE, 10);
    }

    // ── get_mod_source_enum ──────────────────────────────────────────────────

    #[test]
    fn test_get_mod_source_enum_linear_unipolar_positive_cc() {
        // CC, linear, unipolar, positive, index=7 (main volume)
        // = (0<<10)|(0<<9)|(0<<8)|(1<<7)|7 = 128+7 = 135
        let e = get_mod_source_enum(modulator_curve_types::LINEAR, false, false, true, 7);
        assert_eq!(e, 135);
    }

    #[test]
    fn test_get_mod_source_enum_concave_negative_non_cc() {
        // non-CC, concave, unipolar, negative, index=2 (note_on_velocity)
        // = (1<<10)|(0<<9)|(1<<8)|(0<<7)|2 = 1024+256+2 = 1282
        let e = get_mod_source_enum(modulator_curve_types::CONCAVE, false, true, false, 2);
        assert_eq!(e, 1282);
    }

    #[test]
    fn test_get_mod_source_enum_default_resonant() {
        // The default resonant mod source must equal the module constant
        // linear, bipolar, positive, CC, index=71 (filterResonance)
        // = (0<<10)|(1<<9)|(0<<8)|(1<<7)|71 = 512+128+71 = 711
        let e = get_mod_source_enum(modulator_curve_types::LINEAR, true, false, true, 71);
        assert_eq!(e, DEFAULT_RESONANT_MOD_SOURCE);
        assert_eq!(e, 711);
    }

    // ── Modulator::new / default ─────────────────────────────────────────────

    #[test]
    fn test_modulator_default() {
        let m = Modulator::default();
        assert_eq!(m.destination, generator_types::INVALID);
        assert_eq!(m.transform_amount, 0.0);
        assert_eq!(m.transform_type, 0);
        assert!(!m.is_effect_modulator);
        assert!(!m.is_default_resonant_modulator);
    }

    #[test]
    fn test_modulator_new_sets_all_fields() {
        let ps = ModulatorSource::new(7, modulator_curve_types::CONCAVE, true, false, true);
        let ss = ModulatorSource::default();
        let m = Modulator::new(ps.clone(), ss.clone(), 48, 960.0, 0, false, false);
        assert_eq!(m.destination, 48);
        assert_eq!(m.transform_amount, 960.0);
        assert_eq!(m.transform_type, 0);
        assert!(!m.is_effect_modulator);
        assert!(!m.is_default_resonant_modulator);
        assert_eq!(m.primary_source, ps);
        assert_eq!(m.secondary_source, ss);
    }

    // ── Modulator::is_identical ──────────────────────────────────────────────

    #[test]
    fn test_is_identical_equal_modulators() {
        let m1 = Modulator::default();
        let m2 = Modulator::default();
        assert!(Modulator::is_identical(&m1, &m2, false));
    }

    #[test]
    fn test_is_identical_different_destination() {
        let mut m1 = Modulator::default();
        let m2 = Modulator::default();
        m1.destination = 5;
        assert!(!Modulator::is_identical(&m1, &m2, false));
    }

    #[test]
    fn test_is_identical_different_transform_type() {
        let mut m1 = Modulator::default();
        let m2 = Modulator::default();
        m1.transform_type = 2;
        assert!(!Modulator::is_identical(&m1, &m2, false));
    }

    #[test]
    fn test_is_identical_ignores_amount_when_check_false() {
        let mut m1 = Modulator::default();
        let m2 = Modulator::default();
        m1.transform_amount = 999.0;
        assert!(Modulator::is_identical(&m1, &m2, false));
    }

    #[test]
    fn test_is_identical_checks_amount_when_check_true() {
        let mut m1 = Modulator::default();
        let m2 = Modulator::default();
        m1.transform_amount = 999.0;
        assert!(!Modulator::is_identical(&m1, &m2, true));
    }

    #[test]
    fn test_is_identical_same_amount_check_true() {
        let mut m1 = Modulator::default();
        let mut m2 = Modulator::default();
        m1.transform_amount = 500.0;
        m2.transform_amount = 500.0;
        assert!(Modulator::is_identical(&m1, &m2, true));
    }

    // ── Modulator::copy_from ────────────────────────────────────────────────

    #[test]
    fn test_copy_from_equal() {
        let ps = ModulatorSource::new(7, modulator_curve_types::CONCAVE, true, false, true);
        let m = Modulator::new(ps, ModulatorSource::default(), 48, 960.0, 0, true, false);
        let copy = Modulator::copy_from(&m);
        assert_eq!(m, copy);
    }

    #[test]
    fn test_copy_from_independent() {
        let mut m = Modulator::default();
        m.transform_amount = 100.0;
        let mut copy = Modulator::copy_from(&m);
        copy.transform_amount = 999.0;
        assert_eq!(m.transform_amount, 100.0); // original unchanged
        assert_eq!(copy.transform_amount, 999.0);
    }

    // ── Modulator::sum_transform ─────────────────────────────────────────────

    #[test]
    fn test_sum_transform_adds_amounts() {
        let mut m1 = Modulator::default();
        m1.transform_amount = 300.0;
        let mut m2 = Modulator::default();
        m2.transform_amount = 200.0;
        let result = m1.sum_transform(&m2);
        assert_eq!(result.transform_amount, 500.0);
    }

    #[test]
    fn test_sum_transform_returns_new_modulator() {
        let mut m1 = Modulator::default();
        m1.transform_amount = 300.0;
        let m2 = Modulator::default();
        let result = m1.sum_transform(&m2);
        assert_eq!(m1.transform_amount, 300.0); // original unchanged
        assert_eq!(result.transform_amount, 300.0);
    }

    #[test]
    fn test_sum_transform_negative() {
        let mut m1 = Modulator::default();
        m1.transform_amount = 100.0;
        let mut m2 = Modulator::default();
        m2.transform_amount = -200.0;
        let result = m1.sum_transform(&m2);
        assert_eq!(result.transform_amount, -100.0);
    }

    // ── Modulator::write ─────────────────────────────────────────────────────

    fn make_mod_buffer() -> IndexedByteArray {
        IndexedByteArray::new(MOD_BYTE_SIZE)
    }

    #[test]
    fn test_write_advances_index_by_10() {
        let m = Modulator::default();
        let mut buf = make_mod_buffer();
        m.write(&mut buf, None);
        assert_eq!(buf.current_index, 10);
    }

    #[test]
    fn test_write_does_not_increment_index_without_indexes() {
        // When no SoundFontWriteIndexes is provided, no counter is changed
        let m = Modulator::default();
        let mut buf = make_mod_buffer();
        m.write(&mut buf, None);
        // Just check no panic and correct buffer advancement
        assert_eq!(buf.current_index, 10);
    }

    #[test]
    fn test_write_increments_mod_count() {
        let m = Modulator::default();
        let mut buf = make_mod_buffer();
        let mut idx = SoundFontWriteIndexes { mod_count: 0 };
        m.write(&mut buf, Some(&mut idx));
        assert_eq!(idx.mod_count, 1);
    }

    #[test]
    fn test_write_encodes_correct_bytes() {
        // Modulator: primary_source=(CC, linear, unipolar, positive, index=7) = 0x0087
        //            destination=48 (INITIAL_ATTENUATION)
        //            transform_amount=960
        //            secondary_source=default=0x0000
        //            transform_type=0
        let ps = ModulatorSource::new(7, modulator_curve_types::LINEAR, true, false, false);
        let ss = ModulatorSource::default();
        let m = Modulator::new(ps, ss, 48, 960.0, 0, false, false);
        let mut buf = IndexedByteArray::new(10);
        m.write(&mut buf, None);

        // primary source enum: (0<<10)|(0<<9)|(0<<8)|(1<<7)|7 = 135 = 0x0087
        let ps_enum: u16 = 135;
        // destination: 48 = 0x0030
        let dest: u16 = 48;
        // transform_amount: 960 = 0x03C0
        let amt: u16 = 960;
        // secondary source enum: 0
        let ss_enum: u16 = 0;
        // transform_type: 0
        let tt: u16 = 0;

        let expected: Vec<u8> = [ps_enum, dest, amt, ss_enum, tt]
            .iter()
            .flat_map(|&w| w.to_le_bytes())
            .collect();
        assert_eq!(&(*buf)[..10], expected.as_slice());
    }

    #[test]
    fn test_write_negative_transform_amount() {
        // transform_amount = -2400, written as i16 two's complement: 0xF6A0
        let m = Modulator {
            transform_amount: -2400.0,
            ..Modulator::default()
        };
        let mut buf = IndexedByteArray::new(10);
        m.write(&mut buf, None);
        // bytes 4-5 = transform_amount as LE i16
        let amt_bytes = [buf[4], buf[5]];
        let read_back = i16::from_le_bytes(amt_bytes);
        assert_eq!(read_back, -2400i16);
    }

    // ── DecodedModulator::new ────────────────────────────────────────────────

    #[test]
    fn test_decoded_modulator_new_basic() {
        let dm = DecodedModulator::new(0x0502, 0x0, 48, 960, 0);
        assert_eq!(dm.source_enum, 0x0502);
        assert_eq!(dm.secondary_source_enum, 0x0);
        assert_eq!(dm.destination, 48);
        assert_eq!(dm.transform_amount, 960.0);
        assert_eq!(dm.transform_type, 0);
        assert!(!dm.is_effect_modulator);
        assert!(!dm.is_default_resonant_modulator);
    }

    #[test]
    fn test_decoded_modulator_effect_reverb() {
        let dm = DecodedModulator::new(0x00db, 0x0, generator_types::REVERB_EFFECTS_SEND, 200, 0);
        assert!(dm.is_effect_modulator);
        assert!(!dm.is_default_resonant_modulator);
    }

    #[test]
    fn test_decoded_modulator_effect_chorus() {
        let dm = DecodedModulator::new(0x00dd, 0x0, generator_types::CHORUS_EFFECTS_SEND, 200, 0);
        assert!(dm.is_effect_modulator);
    }

    #[test]
    fn test_decoded_modulator_effect_wrong_destination() {
        // Reverb source_enum but wrong destination → not effect mod
        let dm = DecodedModulator::new(0x00db, 0x0, generator_types::INITIAL_ATTENUATION, 200, 0);
        assert!(!dm.is_effect_modulator);
    }

    #[test]
    fn test_decoded_modulator_effect_wrong_secondary() {
        // Reverb source_enum but non-zero secondary → not effect mod
        let dm =
            DecodedModulator::new(0x00db, 0x0001, generator_types::REVERB_EFFECTS_SEND, 200, 0);
        assert!(!dm.is_effect_modulator);
    }

    #[test]
    fn test_decoded_modulator_default_resonant() {
        // source_enum = 711 (DEFAULT_RESONANT_MOD_SOURCE), secondary=0, dest=initialFilterQ(9)
        let dm = DecodedModulator::new(
            DEFAULT_RESONANT_MOD_SOURCE,
            0x0,
            generator_types::INITIAL_FILTER_Q,
            200,
            0,
        );
        assert!(dm.is_default_resonant_modulator);
        assert!(!dm.is_effect_modulator);
    }

    #[test]
    fn test_decoded_modulator_resonant_wrong_source() {
        let dm = DecodedModulator::new(0x0001, 0x0, generator_types::INITIAL_FILTER_Q, 200, 0);
        assert!(!dm.is_default_resonant_modulator);
    }

    #[test]
    fn test_decoded_modulator_clamps_invalid_destination() {
        let dm = DecodedModulator::new(0x0, 0x0, 100, 0, 0); // 100 > MAX_GENERATOR(62)
        assert_eq!(dm.destination, generator_types::INVALID);
    }

    #[test]
    fn test_decoded_modulator_valid_destination_not_clamped() {
        let dm = DecodedModulator::new(0x0, 0x0, 62, 0, 0); // 62 == MAX_GENERATOR
        assert_eq!(dm.destination, 62);
    }

    // ── DecodedModulator primary_source / secondary_source ───────────────────

    #[test]
    fn test_decoded_modulator_primary_source_roundtrip() {
        let ps = ModulatorSource::new(7, modulator_curve_types::CONCAVE, true, true, false);
        let enum_val = ps.to_source_enum();
        let dm = DecodedModulator::new(enum_val, 0, 0, 0, 0);
        let decoded = dm.primary_source();
        assert_eq!(decoded, ps);
    }

    #[test]
    fn test_decoded_modulator_secondary_source_default() {
        let dm = DecodedModulator::new(0, 0, 0, 0, 0);
        let sec = dm.secondary_source();
        assert_eq!(sec, ModulatorSource::default());
    }

    // ── DEFAULT_RESONANT_MOD_SOURCE value ────────────────────────────────────

    #[test]
    fn test_default_resonant_mod_source_value() {
        // Verify the computed constant matches the expected formula
        // = (0<<10)|(1<<9)|(0<<8)|(1<<7)|71 = 512+128+71 = 711
        assert_eq!(DEFAULT_RESONANT_MOD_SOURCE, 711);
    }

    #[test]
    fn test_default_resonant_mod_source_matches_get_fn() {
        let computed = get_mod_source_enum(
            modulator_curve_types::LINEAR,
            true,  // isBipolar
            false, // isNegative
            true,  // isCC
            71,    // filterResonance
        );
        assert_eq!(computed, DEFAULT_RESONANT_MOD_SOURCE);
    }

    // ── DEFAULT_ATTENUATION_MOD_AMOUNT / CURVE_TYPE ──────────────────────────

    #[test]
    fn test_default_attenuation_mod_amount() {
        assert_eq!(DEFAULT_ATTENUATION_MOD_AMOUNT, 960);
    }

    #[test]
    fn test_default_attenuation_mod_curve_type() {
        assert_eq!(
            DEFAULT_ATTENUATION_MOD_CURVE_TYPE,
            modulator_curve_types::CONCAVE
        );
    }

    // ── SPESSASYNTH_DEFAULT_MODULATORS ───────────────────────────────────────

    #[test]
    fn test_default_modulators_count() {
        let mods = &*SPESSASYNTH_DEFAULT_MODULATORS;
        assert_eq!(
            mods.len(),
            18,
            "Expected 9 SF2 + 9 SpessaSynth = 18 modulators"
        );
    }

    #[test]
    fn test_default_mod_0_vel_to_attenuation() {
        let mods = &*SPESSASYNTH_DEFAULT_MODULATORS;
        let m = &mods[0];
        assert_eq!(m.destination, generator_types::INITIAL_ATTENUATION);
        assert_eq!(m.transform_amount, 960.0);
        // primary: non-CC, concave, unipolar, negative, note_on_velocity=2
        assert_eq!(m.primary_source.index, modulator_sources::NOTE_ON_VELOCITY);
        assert!(!m.primary_source.is_cc);
        assert!(m.primary_source.is_negative);
        assert!(!m.primary_source.is_bipolar);
    }

    #[test]
    fn test_default_mod_1_mod_wheel_to_vibrato() {
        let mods = &*SPESSASYNTH_DEFAULT_MODULATORS;
        let m = &mods[1];
        assert_eq!(m.destination, generator_types::VIB_LFO_TO_PITCH);
        assert_eq!(m.transform_amount, 50.0);
        // primary source_enum = 0x0081 → CC, index=1 (mod wheel), linear, unipolar, positive
        assert_eq!(m.primary_source.to_source_enum(), 0x0081);
        assert!(m.primary_source.is_cc);
        assert_eq!(m.primary_source.index, 1);
    }

    #[test]
    fn test_default_mod_4_pitch_wheel() {
        // Pitch wheel to tuning: source=0x020e, secondary=0x0010, dest=FINE_TUNE, amount=12700
        let mods = &*SPESSASYNTH_DEFAULT_MODULATORS;
        let m = &mods[4];
        assert_eq!(m.destination, generator_types::FINE_TUNE);
        assert_eq!(m.transform_amount, 12_700.0);
        assert_eq!(m.primary_source.to_source_enum(), 0x020e);
        assert_eq!(m.secondary_source.to_source_enum(), 0x0010);
    }

    #[test]
    fn test_default_mod_7_reverb_is_effect() {
        let mods = &*SPESSASYNTH_DEFAULT_MODULATORS;
        let m = &mods[7];
        assert_eq!(m.destination, generator_types::REVERB_EFFECTS_SEND);
        assert_eq!(m.transform_amount, 200.0);
        assert!(m.is_effect_modulator);
    }

    #[test]
    fn test_default_mod_8_chorus_is_effect() {
        let mods = &*SPESSASYNTH_DEFAULT_MODULATORS;
        let m = &mods[8];
        assert_eq!(m.destination, generator_types::CHORUS_EFFECTS_SEND);
        assert_eq!(m.transform_amount, 200.0);
        assert!(m.is_effect_modulator);
    }

    #[test]
    fn test_default_mod_14_resonant_is_default_resonant() {
        // Entry 14 (index 14, 0-based) = CC 71 filter Q → initialFilterQ (default resonant)
        let mods = &*SPESSASYNTH_DEFAULT_MODULATORS;
        let m = &mods[14];
        assert_eq!(m.destination, generator_types::INITIAL_FILTER_Q);
        assert_eq!(m.transform_amount, 200.0);
        assert!(m.is_default_resonant_modulator);
        assert!(!m.is_effect_modulator);
    }

    #[test]
    fn test_default_mod_16_soft_pedal_filter_fc_negative_amount() {
        // Entry 16 (index 16) = CC 67 soft pedal → initialFilterFc, amount=-2400
        let mods = &*SPESSASYNTH_DEFAULT_MODULATORS;
        let m = &mods[16];
        assert_eq!(m.destination, generator_types::INITIAL_FILTER_FC);
        assert_eq!(m.transform_amount, -2400.0);
    }

    #[test]
    fn test_default_mod_17_balance_to_pan() {
        // Entry 17 (index 17) = CC 8 balance → pan, amount=500
        let mods = &*SPESSASYNTH_DEFAULT_MODULATORS;
        let m = &mods[17];
        assert_eq!(m.destination, generator_types::PAN);
        assert_eq!(m.transform_amount, 500.0);
        assert!(m.primary_source.is_cc);
        assert_eq!(m.primary_source.index, 8); // balance = CC 8
        assert!(m.primary_source.is_bipolar);
    }

    #[test]
    fn test_default_modulators_no_invalid_destination() {
        // All default modulators should have valid destinations
        let mods = &*SPESSASYNTH_DEFAULT_MODULATORS;
        for (i, m) in mods.iter().enumerate() {
            assert_ne!(
                m.destination,
                generator_types::INVALID,
                "modulator[{}] has INVALID destination",
                i
            );
        }
    }

    // ── Display ──────────────────────────────────────────────────────────────

    #[test]
    fn test_modulator_display_contains_source_and_destination() {
        let ps = ModulatorSource::new(
            modulator_sources::NOTE_ON_VELOCITY,
            modulator_curve_types::CONCAVE,
            false,
            false,
            true,
        );
        let m = Modulator::new(
            ps,
            ModulatorSource::default(),
            generator_types::INITIAL_ATTENUATION,
            960.0,
            0,
            false,
            false,
        );
        let s = m.to_string();
        assert!(s.contains("Source:"), "got: {}", s);
        assert!(s.contains("Secondary source:"), "got: {}", s);
        assert!(s.contains("initialAttenuation"), "got: {}", s);
        assert!(s.contains("960"), "got: {}", s);
    }

    #[test]
    fn test_modulator_display_absolute_value_suffix() {
        let m = Modulator {
            transform_type: 2,
            transform_amount: 100.0,
            ..Modulator::default()
        };
        let s = m.to_string();
        assert!(s.contains("absolute value"), "got: {}", s);
    }

    #[test]
    fn test_modulator_display_no_suffix_for_linear() {
        let m = Modulator {
            transform_type: 0,
            transform_amount: 100.0,
            ..Modulator::default()
        };
        let s = m.to_string();
        assert!(!s.contains("absolute value"), "got: {}", s);
    }

    // ── generator_type_name ──────────────────────────────────────────────────

    #[test]
    fn test_generator_type_name_known() {
        assert_eq!(
            generator_type_name(generator_types::INITIAL_ATTENUATION),
            "initialAttenuation"
        );
        assert_eq!(
            generator_type_name(generator_types::VIB_LFO_TO_PITCH),
            "vibLfoToPitch"
        );
        assert_eq!(generator_type_name(generator_types::INVALID), "INVALID");
        assert_eq!(generator_type_name(generator_types::FINE_TUNE), "fineTune");
        assert_eq!(generator_type_name(generator_types::PAN), "pan");
    }

    #[test]
    fn test_generator_type_name_unknown() {
        assert_eq!(generator_type_name(100), "100");
    }

    // ── SoundFontWriteIndexes ─────────────────────────────────────────────────

    #[test]
    fn test_sound_font_write_indexes_initial() {
        let idx = SoundFontWriteIndexes { mod_count: 0 };
        assert_eq!(idx.mod_count, 0);
    }

    #[test]
    fn test_sound_font_write_indexes_increment_via_write() {
        let m = Modulator::default();
        let mut buf = IndexedByteArray::new(10);
        let mut idx = SoundFontWriteIndexes { mod_count: 5 };
        m.write(&mut buf, Some(&mut idx));
        assert_eq!(idx.mod_count, 6);
    }
}
