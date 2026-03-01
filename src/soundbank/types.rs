use crate::soundbank::basic_soundbank::generator_types::GENERATORS_AMOUNT;
/// types.rs
/// purpose: Core types and interfaces for the SoundBank module.
/// Ported from: src/soundbank/types.ts
///
/// Skipped (out of MIDI→WAV scope or unported dependencies):
///   - SoundBankManagerListEntry  (BasicSoundBank not yet ported)
///   - SampleEncodingFunction     (write-only async fn)
///   - ProgressFunction           (write-only async fn)
///   - SoundFont2WriteOptions     (write-only)
///   - DLSWriteOptions            (write-only)
use crate::soundbank::basic_soundbank::modulator::Modulator;
use crate::soundbank::enums::DLSLoopType;

// ---------------------------------------------------------------------------
// FourCC type aliases
// TypeScript string literal union types are represented as String type aliases in Rust.
// Unified with the FourCC / WAVFourCC pattern in riff_chunk.rs.
// ---------------------------------------------------------------------------

/// Generic FourCC for RIFF INFO chunks.
/// Equivalent to: GenericBankInfoFourCC
pub type GenericBankInfoFourCC = String;

/// FourCC for SF2 INFO chunks.
/// Equivalent to: SF2InfoFourCC
pub type SF2InfoFourCC = String;

/// FourCC for SF2 data chunks (pdta/sdta, etc.).
/// Equivalent to: SF2ChunkFourCC
pub type SF2ChunkFourCC = String;

/// FourCC for DLS INFO chunks.
/// Equivalent to: DLSInfoFourCC
pub type DLSInfoFourCC = String;

/// FourCC for DLS data chunks (including WAVFourCC).
/// Equivalent to: DLSChunkFourCC (= WAVFourCC | "dls " | "dlid" | ...)
pub type DLSChunkFourCC = String;

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// SF2 version tag (ifil / iver chunk).
/// Equivalent to: SF2VersionTag
#[derive(Debug, Clone, PartialEq)]
pub struct SF2VersionTag {
    pub major: u16,
    pub minor: u16,
}

/// Metadata for a sound bank.
/// Equivalent to: SoundBankInfoData
/// The TypeScript Date type is stored as a string in the SF2 spec (ICRD chunk), so it is represented as String.
#[derive(Debug, Clone)]
pub struct SoundBankInfoData {
    pub name: String,
    pub version: SF2VersionTag,
    /// Creation date (string value of the SF2 ICRD chunk).
    pub creation_date: String,
    pub sound_engine: String,
    pub engineer: Option<String>,
    pub product: Option<String>,
    pub copyright: Option<String>,
    pub comment: Option<String>,
    pub subject: Option<String>,
    pub rom_info: Option<String>,
    pub software: Option<String>,
    pub rom_version: Option<SF2VersionTag>,
}

/// A string representing a field name of SoundBankInfoData.
/// Equivalent to TypeScript's `keyof SoundBankInfoData`.
/// Equivalent to: SoundBankInfoFourCC
pub type SoundBankInfoFourCC = String;

/// Generic numeric range (min/max pair).
/// Equivalent to: GenericRange
#[derive(Debug, Clone, PartialEq)]
pub struct GenericRange {
    pub min: f64,
    pub max: f64,
}

/// Voice synthesis parameters returned by BasicPreset::get_voice_parameters.
/// Equivalent to: VoiceParameters
///
/// The TypeScript `sample: BasicSample` field is replaced with an index `sample_idx: usize`.
/// The caller accesses the sample via `BasicSoundBank::samples[sample_idx]`.
pub struct VoiceParameters {
    /// Generator value array for synthesis (64 elements).
    /// Equivalent to: generators: Int16Array
    pub generators: [i16; GENERATORS_AMOUNT],

    /// List of modulators to apply.
    /// Equivalent to: modulators: Modulator[]
    pub modulators: Vec<Modulator>,

    /// Index into `BasicSoundBank::samples` for the sample to use.
    /// Equivalent to: sample: BasicSample
    pub sample_idx: usize,
}

/// DLS loop information.
/// Equivalent to: DLSLoop
#[derive(Debug, Clone, PartialEq)]
pub struct DLSLoop {
    pub loop_type: DLSLoopType,
    /// Loop start position (absolute offset in samples).
    pub loop_start: u32,
    /// Loop length (number of samples).
    pub loop_length: u32,
}

/// Index of a modulator source.
/// In TypeScript this is `ModulatorSourceEnum | MIDIController` (both are integers 0–127).
/// Equivalent to: ModulatorSourceIndex
pub type ModulatorSourceIndex = u8;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::enums::dls_loop_types;

    // --- SF2VersionTag ---

    #[test]
    fn test_sf2_version_tag_fields() {
        let v = SF2VersionTag { major: 2, minor: 1 };
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 1);
    }

    #[test]
    fn test_sf2_version_tag_clone() {
        let v = SF2VersionTag { major: 2, minor: 4 };
        let c = v.clone();
        assert_eq!(c.major, v.major);
        assert_eq!(c.minor, v.minor);
    }

    #[test]
    fn test_sf2_version_tag_eq() {
        let a = SF2VersionTag { major: 2, minor: 1 };
        let b = SF2VersionTag { major: 2, minor: 1 };
        assert_eq!(a, b);
    }

    #[test]
    fn test_sf2_version_tag_neq() {
        let a = SF2VersionTag { major: 2, minor: 0 };
        let b = SF2VersionTag { major: 2, minor: 4 };
        assert_ne!(a, b);
    }

    // --- SoundBankInfoData ---

    #[test]
    fn test_sound_bank_info_data_required_fields() {
        let info = SoundBankInfoData {
            name: "GeneralUser GS".to_string(),
            version: SF2VersionTag { major: 2, minor: 1 },
            creation_date: "2024-01-01".to_string(),
            sound_engine: "EMU8000".to_string(),
            engineer: None,
            product: None,
            copyright: None,
            comment: None,
            subject: None,
            rom_info: None,
            software: None,
            rom_version: None,
        };
        assert_eq!(info.name, "GeneralUser GS");
        assert_eq!(info.version.major, 2);
        assert_eq!(info.sound_engine, "EMU8000");
    }

    #[test]
    fn test_sound_bank_info_data_optional_engineer() {
        let info = SoundBankInfoData {
            name: "Test".to_string(),
            version: SF2VersionTag { major: 2, minor: 1 },
            creation_date: String::new(),
            sound_engine: String::new(),
            engineer: Some("Alice".to_string()),
            product: None,
            copyright: None,
            comment: None,
            subject: None,
            rom_info: None,
            software: None,
            rom_version: None,
        };
        assert_eq!(info.engineer.as_deref(), Some("Alice"));
    }

    #[test]
    fn test_sound_bank_info_data_optional_none_by_default() {
        let info = SoundBankInfoData {
            name: "Test".to_string(),
            version: SF2VersionTag { major: 2, minor: 0 },
            creation_date: String::new(),
            sound_engine: String::new(),
            engineer: None,
            product: None,
            copyright: None,
            comment: None,
            subject: None,
            rom_info: None,
            software: None,
            rom_version: None,
        };
        assert!(info.engineer.is_none());
        assert!(info.product.is_none());
        assert!(info.copyright.is_none());
        assert!(info.rom_version.is_none());
    }

    #[test]
    fn test_sound_bank_info_data_rom_version() {
        let info = SoundBankInfoData {
            name: "Test".to_string(),
            version: SF2VersionTag { major: 2, minor: 0 },
            creation_date: String::new(),
            sound_engine: String::new(),
            engineer: None,
            product: None,
            copyright: None,
            comment: None,
            subject: None,
            rom_info: None,
            software: None,
            rom_version: Some(SF2VersionTag { major: 1, minor: 5 }),
        };
        let rv = info.rom_version.unwrap();
        assert_eq!(rv.major, 1);
        assert_eq!(rv.minor, 5);
    }

    // --- GenericRange ---

    #[test]
    fn test_generic_range_fields() {
        let r = GenericRange {
            min: 0.0,
            max: 127.0,
        };
        assert_eq!(r.min, 0.0);
        assert_eq!(r.max, 127.0);
    }

    #[test]
    fn test_generic_range_eq() {
        let a = GenericRange {
            min: -1.0,
            max: 1.0,
        };
        let b = GenericRange {
            min: -1.0,
            max: 1.0,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_generic_range_clone() {
        let r = GenericRange {
            min: 10.0,
            max: 20.0,
        };
        let c = r.clone();
        assert_eq!(c.min, r.min);
        assert_eq!(c.max, r.max);
    }

    #[test]
    fn test_generic_range_negative_min() {
        let r = GenericRange {
            min: -64.0,
            max: 63.0,
        };
        assert!(r.min < 0.0);
        assert!(r.max > 0.0);
    }

    // --- DLSLoop ---

    #[test]
    fn test_dls_loop_fields() {
        let l = DLSLoop {
            loop_type: dls_loop_types::FORWARD,
            loop_start: 1000,
            loop_length: 500,
        };
        assert_eq!(l.loop_type, dls_loop_types::FORWARD);
        assert_eq!(l.loop_start, 1000);
        assert_eq!(l.loop_length, 500);
    }

    #[test]
    fn test_dls_loop_loop_and_release() {
        let l = DLSLoop {
            loop_type: dls_loop_types::LOOP_AND_RELEASE,
            loop_start: 0,
            loop_length: 2048,
        };
        assert_eq!(l.loop_type, dls_loop_types::LOOP_AND_RELEASE);
    }

    #[test]
    fn test_dls_loop_clone() {
        let l = DLSLoop {
            loop_type: dls_loop_types::FORWARD,
            loop_start: 100,
            loop_length: 200,
        };
        let c = l.clone();
        assert_eq!(c.loop_start, l.loop_start);
        assert_eq!(c.loop_length, l.loop_length);
    }

    #[test]
    fn test_dls_loop_eq() {
        let a = DLSLoop {
            loop_type: dls_loop_types::FORWARD,
            loop_start: 10,
            loop_length: 20,
        };
        let b = DLSLoop {
            loop_type: dls_loop_types::FORWARD,
            loop_start: 10,
            loop_length: 20,
        };
        assert_eq!(a, b);
    }

    // --- type aliases (compile-time check) ---

    #[test]
    fn test_type_alias_generic_bank_info_fourcc() {
        let _v: GenericBankInfoFourCC = "INAM".to_string();
    }

    #[test]
    fn test_type_alias_sf2_info_fourcc() {
        let _v: SF2InfoFourCC = "ifil".to_string();
    }

    #[test]
    fn test_type_alias_sf2_chunk_fourcc() {
        let _v: SF2ChunkFourCC = "pdta".to_string();
    }

    #[test]
    fn test_type_alias_dls_info_fourcc() {
        let _v: DLSInfoFourCC = "ISBJ".to_string();
    }

    #[test]
    fn test_type_alias_dls_chunk_fourcc() {
        let _v: DLSChunkFourCC = "dls ".to_string();
    }

    #[test]
    fn test_type_alias_sound_bank_info_fourcc() {
        let _v: SoundBankInfoFourCC = "name".to_string();
    }

    #[test]
    fn test_type_alias_modulator_source_index_is_u8() {
        let _v: ModulatorSourceIndex = 127u8;
    }

    #[test]
    fn test_modulator_source_index_zero() {
        let _v: ModulatorSourceIndex = 0u8;
    }
}
