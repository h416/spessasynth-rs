//! SF2 modulators, the modulators decoded from a sound bank, and the default modulators.
//! Ported from: src/soundbank/basic_soundbank/modulator.ts
//!
//! TS `DecodedModulator` is a subclass of `Modulator` that only adds a constructor. Rust keeps it as a unit
//! struct whose `new()` returns a [`Modulator`].
//!
//! Not ported (API only, no effect on the MIDI-to-WAV output): `checkAddable`, `linkCurves` and `unlinkCurves`
//! (the links of an edited bank's custom curves) and `writeExtended` (the SFe XMOD writer).
use std::fmt;
use std::sync::{Arc, LazyLock};

use crate::midi::enums::midi_controllers;
use crate::soundbank::basic_soundbank::custom_curve::{CustomCurve, CustomCurvePoint};
use crate::soundbank::basic_soundbank::generator_types::{
    GeneratorType, MAX_GENERATOR, generator_types,
};
use crate::soundbank::basic_soundbank::modulator_source::ModulatorSource;
use crate::soundbank::enums::{modulator_curve_types, modulator_sources};
use crate::utils::byte_functions::little_endian::write_word;
use crate::utils::indexed_array::IndexedByteArray;

/// Size in bytes of a single modulator record in SF2 format (5 x 16-bit words).
/// Equivalent to: MOD_BYTE_SIZE
pub const MOD_BYTE_SIZE: usize = 10;

/// Size in bytes of a single record of the SFe XMOD chunk.
/// Equivalent to: XMOD_BYTE_SIZE
pub const XMOD_BYTE_SIZE: usize = 16;

/// Bit 15 of the transform: the modulator has an XMOD record (SFe only).
/// Equivalent to: MODULATOR_EXTENDED_FLAG
pub const MODULATOR_EXTENDED_FLAG: u16 = 0x80_00;

/// Minimal stub for SoundFontWriteIndexes.
/// The only field used by `Modulator::write()` is the modulator counter.
/// The TS field is named `mod`, a Rust keyword, so it is `mod_count` here.
/// Equivalent to: SoundFontWriteIndexes (soundfont/write/types.ts)
pub struct SoundFontWriteIndexes {
    /// Equivalent to: indexes.mod
    pub mod_count: u32,
}

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

/// Linear forwards bipolar cc 71.
/// = (0<<10) | (1<<9) | (0<<8) | (1<<7) | 71 = 711
/// Equivalent to: DEFAULT_RESONANT_MOD_SOURCE
pub const DEFAULT_RESONANT_MOD_SOURCE: u16 = 711;

/// Switch unipolar positive cc 71: on from 64.
/// = (3<<10) | (0<<9) | (0<<8) | (1<<7) | 71 = 3271
/// Equivalent to: RESONANT_COMPENSATOR_AMOUNT_SOURCE
pub const RESONANT_COMPENSATOR_AMOUNT_SOURCE: u16 = 3271;

/// An SF2 Modulator with fully-parsed source objects.
/// Equivalent to: class Modulator
#[derive(Clone, Debug, PartialEq)]
pub struct Modulator {
    /// Equivalent to: destination
    pub destination: GeneratorType,
    /// f64 to match TypeScript's number type (allows fractional SysEx modulator amounts).
    /// Equivalent to: transformAmount
    pub transform_amount: f64,
    /// Transform type (0 = linear, 2 = absolute value).
    /// Equivalent to: transformType: ModulatorTransformType
    pub transform_type: u16,
    /// Equivalent to: primarySource
    pub primary_source: ModulatorSource,
    /// Equivalent to: secondarySource
    pub secondary_source: ModulatorSource,
}

impl Default for Modulator {
    /// Equivalent to: new Modulator() with all defaults
    fn default() -> Self {
        Self::new(
            ModulatorSource::default(),
            ModulatorSource::default(),
            generator_types::INVALID,
            0.0,
            0,
        )
    }
}

impl Modulator {
    /// Equivalent to: constructor(primarySource, secondarySource, destination, amount, transformType)
    pub fn new(
        primary_source: ModulatorSource,
        secondary_source: ModulatorSource,
        destination: GeneratorType,
        transform_amount: f64,
        transform_type: u16,
    ) -> Self {
        Self {
            destination,
            transform_amount,
            transform_type,
            primary_source,
            secondary_source,
        }
    }

    /// If any source uses a custom curve (the modulator needs an XMOD record).
    /// Equivalent to: get usesCustomCurve
    pub fn uses_custom_curve(&self) -> bool {
        self.primary_source.curve_type == modulator_curve_types::CUSTOM
            || self.secondary_source.curve_type == modulator_curve_types::CUSTOM
    }

    /// If a source has the custom curve type but no curve.
    /// Equivalent to: get isMissingCurve
    pub fn is_missing_curve(&self) -> bool {
        (self.primary_source.curve_type == modulator_curve_types::CUSTOM
            && self.primary_source.custom_curve.is_none())
            || (self.secondary_source.curve_type == modulator_curve_types::CUSTOM
                && self.secondary_source.custom_curve.is_none())
    }

    /// Checks if two modulators are identical (in SF2 terms).
    /// Equivalent to: static isIdentical(mod1, mod2, checkAmount = false)
    pub fn is_identical(mod1: &Modulator, mod2: &Modulator, check_amount: bool) -> bool {
        mod1.primary_source.is_identical(&mod2.primary_source)
            && mod1.secondary_source.is_identical(&mod2.secondary_source)
            && mod1.destination == mod2.destination
            && mod1.transform_type == mod2.transform_type
            && (!check_amount || mod1.transform_amount == mod2.transform_amount)
    }

    /// Copies a modulator, with the same curves.
    /// Equivalent to: static copyFrom(mod)
    pub fn copy_from(mod_: &Modulator) -> Self {
        Self::new(
            ModulatorSource::copy_from(&mod_.primary_source),
            ModulatorSource::copy_from(&mod_.secondary_source),
            mod_.destination,
            mod_.transform_amount,
            mod_.transform_type,
        )
    }

    /// Writes this modulator (10 bytes = 5 x 16-bit LE words).
    /// Not ported (writer only): the `sfe` argument that sets the extended flag.
    /// Equivalent to: write(modData, indexes?)
    pub fn write(
        &self,
        mod_data: &mut IndexedByteArray,
        indexes: Option<&mut SoundFontWriteIndexes>,
    ) {
        write_word(mod_data, self.primary_source.to_source_enum() as u32);
        write_word(mod_data, self.destination as u16 as u32);
        write_word(mod_data, self.transform_amount as i16 as u16 as u32);
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

/// Equivalent to: GENERAL_SOURCES
const GENERAL_SOURCES: [u8; 8] = [
    modulator_sources::NO_CONTROLLER,
    modulator_sources::NOTE_ON_VELOCITY,
    modulator_sources::NOTE_ON_KEY_NUM,
    modulator_sources::POLY_PRESSURE,
    modulator_sources::CHANNEL_PRESSURE,
    modulator_sources::PITCH_WHEEL,
    modulator_sources::PITCH_WHEEL_RANGE,
    modulator_sources::MODULATION_DEPTH_RANGE,
];

/// A source SF2 does not allow: a reserved or non-continuous controller, or an undefined general source
/// (including the link source 127).
/// Equivalent to: isIllegalSource(source)
pub fn is_illegal_source(source: &ModulatorSource) -> bool {
    if source.is_cc {
        source.index == 0
            || source.index == 6
            || source.index == 32
            || source.index == 38
            || (98..=101).contains(&source.index)
            || source.index >= 120
    } else {
        !GENERAL_SOURCES.contains(&source.index)
    }
}

/// A modulator decoded from the raw SF2 fields.
/// Equivalent to: class DecodedModulator extends Modulator
pub struct DecodedModulator;

impl DecodedModulator {
    /// Equivalent to: new DecodedModulator(sourceEnum, secondarySourceEnum, destination, amount, transformType)
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        source_enum: u16,
        secondary_source_enum: u16,
        destination: GeneratorType,
        amount: f64,
        transform_type: u16,
    ) -> Modulator {
        let mut m = Modulator::new(
            ModulatorSource::from_source_enum(source_enum),
            ModulatorSource::from_source_enum(secondary_source_enum),
            destination,
            amount,
            transform_type,
        );
        // Linked modulators are reserved: a link source, or a destination with bit 15 set, is ignored.
        // So is a modulator with an illegal or reserved controller as a source
        if m.destination > MAX_GENERATOR
            || is_illegal_source(&m.primary_source)
            || is_illegal_source(&m.secondary_source)
        {
            m.destination = generator_types::INVALID;
        }
        m
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
        generator_types::FILTER_TYPE => "filterType".to_string(),
        generator_types::MAX_FILTER_FC => "maxFilterFc".to_string(),
        generator_types::FILTER_Q_OFFSET => "filterQOffset".to_string(),
        generator_types::VIB_LFO_TO_FILTER_FC => "vibLfoToFilterFc".to_string(),
        generator_types::VIB_LFO_TO_VOLUME => "vibLfoToVolume".to_string(),
        generator_types::MOD_LFO_TO_AMPLITUDE => "modLfoToAmplitude".to_string(),
        generator_types::VIB_LFO_TO_AMPLITUDE => "vibLfoToAmplitude".to_string(),
        generator_types::FADE_MOD_LFO => "fadeModLFO".to_string(),
        generator_types::FADE_VIB_LFO => "fadeVibLFO".to_string(),
        generator_types::MOD_LFO_WAVEFORM => "modLfoWaveform".to_string(),
        generator_types::VIB_LFO_WAVEFORM => "vibLfoWaveform".to_string(),
        generator_types::MOD_LFO_START_PHASE => "modLfoStartPhase".to_string(),
        generator_types::VIB_LFO_START_PHASE => "vibLfoStartPhase".to_string(),
        generator_types::MOD_ENV2_TO_PITCH => "modEnv2ToPitch".to_string(),
        generator_types::MOD_ENV2_TO_FILTER_FC => "modEnv2ToFilterFc".to_string(),
        generator_types::DELAY_MOD_ENV2 => "delayModEnv2".to_string(),
        generator_types::ATTACK_MOD_ENV2 => "attackModEnv2".to_string(),
        generator_types::HOLD_MOD_ENV2 => "holdModEnv2".to_string(),
        generator_types::DECAY_MOD_ENV2 => "decayModEnv2".to_string(),
        generator_types::SUSTAIN_MOD_ENV2 => "sustainModEnv2".to_string(),
        generator_types::RELEASE_MOD_ENV2 => "releaseModEnv2".to_string(),
        generator_types::KEY_NUM_TO_MOD_ENV2_HOLD => "keyNumToModEnv2Hold".to_string(),
        generator_types::KEY_NUM_TO_MOD_ENV2_DECAY => "keyNumToModEnv2Decay".to_string(),
        generator_types::DECAY2_VOL_ENV => "decay2VolEnv".to_string(),
        generator_types::SUSTAIN2_VOL_ENV => "sustain2VolEnv".to_string(),
        generator_types::GAIN => "gain".to_string(),
        generator_types::DELAY_VOICE => "delayVoice".to_string(),
        generator_types::IGNORE_NOTE_OFF => "ignoreNoteOff".to_string(),
        generator_types::VARIATION_EFFECTS_SEND => "variationEffectsSend".to_string(),
        generator_types::END_OPER => "endOper".to_string(),
        v => v.to_string(),
    }
}

// ---------------------------------------------------------------------------
// SPESSASYNTH_DEFAULT_MODULATORS
// ---------------------------------------------------------------------------

/// Equivalent to: getCurve(name, points)
fn get_curve(name: &str, points: &[(i32, i32)]) -> Arc<CustomCurve> {
    let points: Vec<CustomCurvePoint> = points
        .iter()
        .map(|&(cc, y)| CustomCurvePoint { x: cc << 7, y })
        .collect();
    Arc::new(CustomCurve::new(&points, name).expect("valid default curve"))
}

/// Equivalent to: ATTACK_TIME_CURVE
pub static ATTACK_TIME_CURVE: LazyLock<Arc<CustomCurve>> = LazyLock::new(|| {
    get_curve(
        "Attack time",
        &[
            (0, 0),
            (64, 8192),
            (69, 8192),
            (70, 8260),
            (72, 8453),
            (74, 8661),
            (77, 8988),
            (81, 9444),
            (87, 10_149),
            (95, 11_112),
            (105, 12_337),
            (119, 14_075),
            (127, 15_074),
        ],
    )
});

/// Equivalent to: DECAY_RELEASE_TIME_CURVE
pub static DECAY_RELEASE_TIME_CURVE: LazyLock<Arc<CustomCurve>> = LazyLock::new(|| {
    get_curve(
        "Decay and release time",
        &[
            (0, 0),
            (64, 8192),
            (65, 8263),
            (66, 8290),
            (67, 8297),
            (110, 8297),
            (111, 8309),
            (113, 8387),
            (114, 8434),
            (115, 8488),
            (116, 8548),
            (117, 8618),
            (118, 8697),
            (119, 8789),
            (120, 8896),
            (121, 9023),
            (122, 9176),
            (123, 9363),
            (124, 9601),
            (125, 9919),
            (126, 10_384),
            (127, 11_205),
        ],
    )
});

/// Equivalent to: envelopeTime(cc, curve, destination)
fn envelope_time(cc: u8, curve: &Arc<CustomCurve>, destination: GeneratorType) -> Modulator {
    Modulator::new(
        ModulatorSource::with_custom_curve(cc, Arc::clone(curve), true, true, false),
        ModulatorSource::default(),
        destination,
        9600.0,
        0,
    )
}

/// The default SF2 modulators followed by the spessasynth ones.
/// Equivalent to: SPESSASYNTH_DEFAULT_MODULATORS (= [...defaultSoundFont2Modulators, ...defaultSpessaSynthModulators])
pub static SPESSASYNTH_DEFAULT_MODULATORS: LazyLock<Vec<Modulator>> = LazyLock::new(|| {
    use modulator_curve_types::{CONCAVE, LINEAR, SWITCH};
    vec![
        // Vel to attenuation
        DecodedModulator::new(
            get_mod_source_enum(CONCAVE, false, true, false, modulator_sources::NOTE_ON_VELOCITY),
            0x0,
            generator_types::INITIAL_ATTENUATION,
            960.0,
            0,
        ),
        // Mod wheel to vibrato, scaled by the modulation depth range (RPN 5)
        // 1600 is 50 cents at the default range, the source is full at 16 semitones
        DecodedModulator::new(
            0x00_81,
            modulator_sources::MODULATION_DEPTH_RANGE as u16,
            generator_types::VIB_LFO_TO_PITCH,
            1600.0,
            0,
        ),
        // Vol to attenuation
        DecodedModulator::new(
            get_mod_source_enum(CONCAVE, false, true, true, midi_controllers::MAIN_VOLUME),
            0x0,
            generator_types::INITIAL_ATTENUATION,
            960.0,
            0,
        ),
        // Pitch wheel to tuning
        DecodedModulator::new(0x02_0e, 0x00_10, generator_types::FINE_TUNE, 12_700.0, 0),
        // Pan to uhh, pan
        // Amount is 500 instead of 1000, see #59
        DecodedModulator::new(0x02_8a, 0x0, generator_types::PAN, 500.0, 0),
        // Expression to attenuation
        DecodedModulator::new(
            get_mod_source_enum(CONCAVE, false, true, true, midi_controllers::EXPRESSION_CONTROLLER),
            0x0,
            generator_types::INITIAL_ATTENUATION,
            960.0,
            0,
        ),
        // Reverb effects to send
        DecodedModulator::new(0x00_db, 0x0, generator_types::REVERB_EFFECTS_SEND, 1000.0, 0),
        // Chorus effects to send
        DecodedModulator::new(0x00_dd, 0x0, generator_types::CHORUS_EFFECTS_SEND, 1000.0, 0),
        // Custom modulators heck yeah
        // Attack time to attackVolEnv
        envelope_time(
            midi_controllers::ATTACK_TIME,
            &ATTACK_TIME_CURVE,
            generator_types::ATTACK_VOL_ENV,
        ),
        // Decay time to decayVolEnv
        envelope_time(
            midi_controllers::DECAY_TIME,
            &DECAY_RELEASE_TIME_CURVE,
            generator_types::DECAY_VOL_ENV,
        ),
        // Release time to releaseVolEnv
        envelope_time(
            midi_controllers::RELEASE_TIME,
            &DECAY_RELEASE_TIME_CURVE,
            generator_types::RELEASE_VOL_ENV,
        ),
        // Cc 74 (brightness) to filterFc
        DecodedModulator::new(
            // Linear forwards bipolar cc 74
            get_mod_source_enum(LINEAR, true, false, true, midi_controllers::BRIGHTNESS),
            0x0, // No controller
            generator_types::INITIAL_FILTER_FC,
            4800.0,
            0,
        ),
        // Cc 71 (filter Q) to filter Q (default resonant modulator)
        DecodedModulator::new(
            DEFAULT_RESONANT_MOD_SOURCE,
            0x0, // No controller
            generator_types::INITIAL_FILTER_Q,
            250.0,
            0,
        ),
        // Cc 71 to gain: cancels the filter's gain reduction for the resonance cc 71 adds, as GS and XG don't get quieter
        // Half of the amount above, only above 64 (where the resonance goes up)
        DecodedModulator::new(
            DEFAULT_RESONANT_MOD_SOURCE,
            RESONANT_COMPENSATOR_AMOUNT_SOURCE,
            generator_types::GAIN,
            125.0,
            0,
        ),
        // Cc 67 (soft pedal) to filter fc
        DecodedModulator::new(
            // Switch unipolar positive 67
            get_mod_source_enum(SWITCH, false, false, true, midi_controllers::SOFT_PEDAL),
            0x0, // No controller
            generator_types::INITIAL_FILTER_FC,
            -1200.0,
            0,
        ),
        // CC 94 (variation depth) to the variation send
        DecodedModulator::new(0x00_de, 0x0, generator_types::VARIATION_EFFECTS_SEND, 1000.0, 0),
    ]
});

/// Copies modulators. A Rust custom curve never changes after it is created, so the copies share the curves.
/// Equivalent to: copyWithOwnCurves(modulators).modulators
pub fn copy_with_own_curves(modulators: &[Modulator]) -> Vec<Modulator> {
    modulators.iter().map(Modulator::copy_from).collect()
}

/// Equivalent to: copyDefaultModulators().modulators
pub fn copy_default_modulators() -> Vec<Modulator> {
    copy_with_own_curves(&SPESSASYNTH_DEFAULT_MODULATORS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::enums::modulator_curve_types;

    /// The default modulator table, as printed by TS. Re-capture it from TS when upstream
    /// changes the table (do not edit by hand):
    /// `for (const m of SPESSASYNTH_DEFAULT_MODULATORS)` print
    /// `index,isCC,isBipolar,isNegative,curveType` of both sources, then
    /// `destination|transformAmount|transformType`, joined with `|`.
    #[test]
    fn test_default_modulators_match_typescript() {
        let expected = [
            "2,0,0,1,1|0,0,0,0,0|48|960|0",
            "1,1,0,0,0|17,0,0,0,0|6|1600|0",
            "7,1,0,1,1|0,0,0,0,0|48|960|0",
            "14,0,1,0,0|16,0,0,0,0|52|12700|0",
            "10,1,1,0,0|0,0,0,0,0|17|500|0",
            "11,1,0,1,1|0,0,0,0,0|48|960|0",
            "91,1,0,0,0|0,0,0,0,0|16|1000|0",
            "93,1,0,0,0|0,0,0,0,0|15|1000|0",
            "73,1,1,0,4|0,0,0,0,0|34|9600|0",
            "75,1,1,0,4|0,0,0,0,0|36|9600|0",
            "72,1,1,0,4|0,0,0,0,0|38|9600|0",
            "74,1,1,0,0|0,0,0,0,0|8|4800|0",
            "71,1,1,0,0|0,0,0,0,0|9|250|0",
            "71,1,1,0,0|71,1,0,0,3|86|125|0",
            "67,1,0,0,3|0,0,0,0,0|8|-1200|0",
            "94,1,0,0,0|0,0,0,0,0|89|1000|0",
        ];
        let src = |s: &ModulatorSource| {
            format!(
                "{},{},{},{},{}",
                s.index, s.is_cc as u8, s.is_bipolar as u8, s.is_negative as u8, s.curve_type
            )
        };
        let actual: Vec<String> = SPESSASYNTH_DEFAULT_MODULATORS
            .iter()
            .map(|m| {
                format!(
                    "{}|{}|{}|{}|{}",
                    src(&m.primary_source),
                    src(&m.secondary_source),
                    m.destination,
                    m.transform_amount,
                    m.transform_type
                )
            })
            .collect();
        assert_eq!(actual, expected);
        for m in SPESSASYNTH_DEFAULT_MODULATORS.iter() {
            assert!(!m.is_missing_curve());
        }
    }

    #[test]
    fn test_source_enum_constants() {
        assert_eq!(
            get_mod_source_enum(modulator_curve_types::LINEAR, true, false, true, 71),
            DEFAULT_RESONANT_MOD_SOURCE
        );
        assert_eq!(
            get_mod_source_enum(modulator_curve_types::SWITCH, false, false, true, 71),
            RESONANT_COMPENSATOR_AMOUNT_SOURCE
        );
    }

    #[test]
    fn test_decoded_modulator_illegal_sources() {
        // Link source (non-CC 127)
        assert_eq!(DecodedModulator::new(0x007f, 0, 48, 1.0, 0).destination, generator_types::INVALID);
        // CC 6 (data entry)
        assert_eq!(DecodedModulator::new(0x0086, 0, 48, 1.0, 0).destination, generator_types::INVALID);
        // Destination above the last generator
        assert_eq!(DecodedModulator::new(0x0081, 0, 90, 1.0, 0).destination, generator_types::INVALID);
        // Modulation depth range is a general source
        assert_eq!(DecodedModulator::new(0x0011, 0, 48, 1.0, 0).destination, 48);
    }

    #[test]
    fn test_write_roundtrip() {
        let m = DecodedModulator::new(0x0502, 0x0081, 48, -960.0, 2);
        let mut data = IndexedByteArray::new(MOD_BYTE_SIZE);
        let mut idx = SoundFontWriteIndexes { mod_count: 0 };
        m.write(&mut data, Some(&mut idx));
        assert_eq!(idx.mod_count, 1);
        assert_eq!(data.to_vec(), vec![0x02u8, 0x05, 48, 0, 0x40, 0xfc, 0x81, 0x00, 2, 0]);
    }
}
