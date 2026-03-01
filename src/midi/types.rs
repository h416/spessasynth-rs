/// types.rs
/// purpose: Common data types for MIDI and RMIDI files.
/// Ported from: src/midi/types.ts
use chrono::NaiveDateTime;

use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;

// ─────────────────────────────────────────────────────────────────────────────
// RMIDInfoData
// ─────────────────────────────────────────────────────────────────────────────

/// Metadata for an RMIDI file.
/// Equivalent to: RMIDInfoData
#[derive(Clone, Debug, Default)]
pub struct RMIDInfoData {
    /// Song name.
    pub name: String,
    /// Name of the engineer responsible for the sound bank.
    pub engineer: String,
    /// Artist name of the MIDI file.
    pub artist: String,
    /// Album name.
    pub album: String,
    /// Genre.
    pub genre: String,
    /// Album art image data (ArrayBuffer → Vec<u8>).
    pub picture: Vec<u8>,
    /// Comment.
    pub comment: String,
    /// File creation date (Date → NaiveDateTime).
    pub creation_date: Option<NaiveDateTime>,
    /// Copyright notice.
    pub copyright: String,
    /// Encoding of the RMIDI info chunk.
    pub info_encoding: String,
    /// Encoding of MIDI file text messages.
    pub midi_encoding: String,
    /// Name of the software that wrote the file.
    pub software: String,
    /// File subject.
    pub subject: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// RMIDInfoDataPartial
// ─────────────────────────────────────────────────────────────────────────────

/// Partial version of RMIDI metadata (all fields optional).
/// Equivalent to TypeScript's `Partial<Omit<RMIDInfoData, "infoEncoding">>`.
/// Used in the `RMIDIWriteOptions.metadata` field.
#[derive(Clone, Debug, Default)]
pub struct RMIDInfoDataPartial {
    pub name: Option<String>,
    pub engineer: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub picture: Option<Vec<u8>>,
    pub comment: Option<String>,
    pub creation_date: Option<NaiveDateTime>,
    pub copyright: Option<String>,
    pub midi_encoding: Option<String>,
    pub software: Option<String>,
    pub subject: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// TempoChange
// ─────────────────────────────────────────────────────────────────────────────

/// A tempo change point within a MIDI file.
/// Equivalent to: TempoChange
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoChange {
    /// Absolute tick position from the start of the MIDI file.
    pub ticks: u32,
    /// New tempo (BPM).
    pub tempo: f64,
}

// ─────────────────────────────────────────────────────────────────────────────
// MidiLoopType / MidiLoop
// ─────────────────────────────────────────────────────────────────────────────

/// Loop type.
/// - `Soft`: Immediately jumps the playback position to the loop start point (Touhou / GameMaker style).
/// - `Hard`: Reprocesses messages up to the loop start to restore the synth state (default).
///
/// Equivalent to: MIDILoopType ("soft" | "hard")
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MidiLoopType {
    Soft,
    #[default]
    Hard,
}

/// Definition of a MIDI loop region.
/// Equivalent to: MIDILoop
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiLoop {
    /// Loop start position (MIDI ticks).
    pub start: u32,
    /// Loop end position (MIDI ticks).
    pub end: u32,
    /// Loop type.
    pub loop_type: MidiLoopType,
}

// ─────────────────────────────────────────────────────────────────────────────
// MidiFormat
// ─────────────────────────────────────────────────────────────────────────────

/// MIDI file format number (0 / 1 / 2).
/// Equivalent to: MIDIFormat (0 | 1 | 2)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum MidiFormat {
    /// Single track.
    #[default]
    SingleTrack = 0,
    /// Multiple tracks (synchronous playback).
    MultiTrack = 1,
    /// Multiple patterns (asynchronous).
    MultiPattern = 2,
}

impl MidiFormat {
    /// Converts from `u8` to `MidiFormat`. Returns `None` for unknown values.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(MidiFormat::SingleTrack),
            1 => Some(MidiFormat::MultiTrack),
            2 => Some(MidiFormat::MultiPattern),
            _ => None,
        }
    }

    /// Converts `MidiFormat` to `u8`.
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// NoteTime
// ─────────────────────────────────────────────────────────────────────────────

/// Note timing information (in seconds).
/// Equivalent to: NoteTime
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteTime {
    /// MIDI note number.
    pub midi_note: u8,
    /// Note start time (seconds).
    pub start: f64,
    /// Note duration (seconds).
    pub length: f64,
    /// MIDI velocity.
    pub velocity: u8,
}

// ─────────────────────────────────────────────────────────────────────────────
// DesiredProgramChange / DesiredControllerChange / DesiredChannelTranspose
// ─────────────────────────────────────────────────────────────────────────────

/// Desired program change for a MIDI channel.
/// In TypeScript this extends `MIDIPatch`; in Rust it is represented by composition.
/// Equivalent to: DesiredProgramChange (extends MIDIPatch)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DesiredProgramChange {
    /// Target channel number.
    pub channel: u8,
    /// Desired patch (program + bank).
    pub patch: MidiPatch,
}

/// Desired controller change for a MIDI channel.
/// Equivalent to: DesiredControllerChange
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DesiredControllerChange {
    /// Target channel number.
    pub channel: u8,
    /// MIDI controller number.
    pub controller_number: u8,
    /// New controller value.
    pub controller_value: u8,
}

/// Desired transpose change for a MIDI channel.
/// Equivalent to: DesiredChannelTranspose
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesiredChannelTranspose {
    /// Target channel number.
    pub channel: u8,
    /// Number of semitones to transpose. The fractional part is used for fine-tuning in cents.
    pub key_shift: f64,
}

// ─────────────────────────────────────────────────────────────────────────────
// RMIDIWriteOptions
// ─────────────────────────────────────────────────────────────────────────────

/// RMIDI file write options.
/// Equivalent to: RMIDIWriteOptions
///
/// Note: The TypeScript `soundBank?: BasicSoundBank` field is
/// omitted because BasicSoundBank has not been ported yet.
#[derive(Clone, Debug, Default)]
pub struct RMIDIWriteOptions {
    /// Bank offset for RMIDI.
    pub bank_offset: u8,
    /// File metadata (optional).
    pub metadata: RMIDInfoDataPartial,
    /// Whether to fix the MIDI file internals to match the bank offset.
    pub correct_bank_offset: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// RMIDInfoFourCC
// ─────────────────────────────────────────────────────────────────────────────

/// RIFF FourCC identifiers used in the RMIDI info chunk.
/// Equivalent to: RMIDInfoFourCC
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RMIDInfoFourCC {
    /// Song title ("INAM")
    Inam,
    /// Album ("IPRD")
    Iprd,
    /// Album (legacy notation) ("IALB")
    Ialb,
    /// Artist ("IART")
    Iart,
    /// Genre ("IGNR")
    Ignr,
    /// Picture ("IPIC")
    Ipic,
    /// Copyright ("ICOP")
    Icop,
    /// Creation date ("ICRD")
    Icrd,
    /// Creation date (legacy spessasynth) ("ICRT")
    Icrt,
    /// Comment ("ICMT")
    Icmt,
    /// Engineer ("IENG")
    Ieng,
    /// Software ("ISFT")
    Isft,
    /// Subject ("ISBJ")
    Isbj,
    /// Info encoding ("IENC")
    Ienc,
    /// MIDI encoding ("MENC")
    Menc,
    /// Bank offset ("DBNK")
    Dbnk,
}

impl RMIDInfoFourCC {
    /// Returns the 4-character ASCII identifier string.
    pub fn as_str(self) -> &'static str {
        match self {
            RMIDInfoFourCC::Inam => "INAM",
            RMIDInfoFourCC::Iprd => "IPRD",
            RMIDInfoFourCC::Ialb => "IALB",
            RMIDInfoFourCC::Iart => "IART",
            RMIDInfoFourCC::Ignr => "IGNR",
            RMIDInfoFourCC::Ipic => "IPIC",
            RMIDInfoFourCC::Icop => "ICOP",
            RMIDInfoFourCC::Icrd => "ICRD",
            RMIDInfoFourCC::Icrt => "ICRT",
            RMIDInfoFourCC::Icmt => "ICMT",
            RMIDInfoFourCC::Ieng => "IENG",
            RMIDInfoFourCC::Isft => "ISFT",
            RMIDInfoFourCC::Isbj => "ISBJ",
            RMIDInfoFourCC::Ienc => "IENC",
            RMIDInfoFourCC::Menc => "MENC",
            RMIDInfoFourCC::Dbnk => "DBNK",
        }
    }

    /// Converts a 4-character string to `RMIDInfoFourCC`. Returns `None` for unknown values.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "INAM" => Some(RMIDInfoFourCC::Inam),
            "IPRD" => Some(RMIDInfoFourCC::Iprd),
            "IALB" => Some(RMIDInfoFourCC::Ialb),
            "IART" => Some(RMIDInfoFourCC::Iart),
            "IGNR" => Some(RMIDInfoFourCC::Ignr),
            "IPIC" => Some(RMIDInfoFourCC::Ipic),
            "ICOP" => Some(RMIDInfoFourCC::Icop),
            "ICRD" => Some(RMIDInfoFourCC::Icrd),
            "ICRT" => Some(RMIDInfoFourCC::Icrt),
            "ICMT" => Some(RMIDInfoFourCC::Icmt),
            "IENG" => Some(RMIDInfoFourCC::Ieng),
            "ISFT" => Some(RMIDInfoFourCC::Isft),
            "ISBJ" => Some(RMIDInfoFourCC::Isbj),
            "IENC" => Some(RMIDInfoFourCC::Ienc),
            "MENC" => Some(RMIDInfoFourCC::Menc),
            "DBNK" => Some(RMIDInfoFourCC::Dbnk),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;

    // ── RMIDInfoData ──────────────────────────────────────────────────────────

    #[test]
    fn test_rmid_info_data_default() {
        let d = RMIDInfoData::default();
        assert!(d.name.is_empty());
        assert!(d.picture.is_empty());
        assert!(d.creation_date.is_none());
    }

    #[test]
    fn test_rmid_info_data_fields() {
        let d = RMIDInfoData {
            name: "Test Song".to_string(),
            artist: "Artist".to_string(),
            picture: vec![0xFF, 0xD8],
            ..Default::default()
        };
        assert_eq!(d.name, "Test Song");
        assert_eq!(d.artist, "Artist");
        assert_eq!(d.picture, vec![0xFF, 0xD8]);
    }

    // ── RMIDInfoDataPartial ───────────────────────────────────────────────────

    #[test]
    fn test_rmid_info_data_partial_default_all_none() {
        let p = RMIDInfoDataPartial::default();
        assert!(p.name.is_none());
        assert!(p.artist.is_none());
        assert!(p.picture.is_none());
        assert!(p.creation_date.is_none());
    }

    #[test]
    fn test_rmid_info_data_partial_set_some() {
        let p = RMIDInfoDataPartial {
            name: Some("My Song".to_string()),
            ..Default::default()
        };
        assert_eq!(p.name.as_deref(), Some("My Song"));
        assert!(p.artist.is_none());
    }

    // ── TempoChange ───────────────────────────────────────────────────────────

    #[test]
    fn test_tempo_change_fields() {
        let tc = TempoChange {
            ticks: 480,
            tempo: 120.0,
        };
        assert_eq!(tc.ticks, 480);
        assert_eq!(tc.tempo, 120.0);
    }

    #[test]
    fn test_tempo_change_equality() {
        let a = TempoChange {
            ticks: 0,
            tempo: 100.0,
        };
        let b = TempoChange {
            ticks: 0,
            tempo: 100.0,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_tempo_change_copy() {
        let a = TempoChange {
            ticks: 100,
            tempo: 90.0,
        };
        let b = a; // Copy
        assert_eq!(a.ticks, b.ticks);
    }

    // ── MidiLoopType ─────────────────────────────────────────────────────────

    #[test]
    fn test_midi_loop_type_default_is_hard() {
        assert_eq!(MidiLoopType::default(), MidiLoopType::Hard);
    }

    #[test]
    fn test_midi_loop_type_soft_ne_hard() {
        assert_ne!(MidiLoopType::Soft, MidiLoopType::Hard);
    }

    // ── MidiLoop ─────────────────────────────────────────────────────────────

    #[test]
    fn test_midi_loop_fields() {
        let lp = MidiLoop {
            start: 0,
            end: 4800,
            loop_type: MidiLoopType::Soft,
        };
        assert_eq!(lp.start, 0);
        assert_eq!(lp.end, 4800);
        assert_eq!(lp.loop_type, MidiLoopType::Soft);
    }

    #[test]
    fn test_midi_loop_hard_default_type() {
        let lp = MidiLoop {
            start: 10,
            end: 200,
            loop_type: MidiLoopType::default(),
        };
        assert_eq!(lp.loop_type, MidiLoopType::Hard);
    }

    // ── MidiFormat ────────────────────────────────────────────────────────────

    #[test]
    fn test_midi_format_default_is_single_track() {
        assert_eq!(MidiFormat::default(), MidiFormat::SingleTrack);
    }

    #[test]
    fn test_midi_format_as_u8() {
        assert_eq!(MidiFormat::SingleTrack.as_u8(), 0);
        assert_eq!(MidiFormat::MultiTrack.as_u8(), 1);
        assert_eq!(MidiFormat::MultiPattern.as_u8(), 2);
    }

    #[test]
    fn test_midi_format_from_u8_valid() {
        assert_eq!(MidiFormat::from_u8(0), Some(MidiFormat::SingleTrack));
        assert_eq!(MidiFormat::from_u8(1), Some(MidiFormat::MultiTrack));
        assert_eq!(MidiFormat::from_u8(2), Some(MidiFormat::MultiPattern));
    }

    #[test]
    fn test_midi_format_from_u8_invalid() {
        assert_eq!(MidiFormat::from_u8(3), None);
        assert_eq!(MidiFormat::from_u8(255), None);
    }

    #[test]
    fn test_midi_format_roundtrip() {
        for fmt in [
            MidiFormat::SingleTrack,
            MidiFormat::MultiTrack,
            MidiFormat::MultiPattern,
        ] {
            assert_eq!(MidiFormat::from_u8(fmt.as_u8()), Some(fmt));
        }
    }

    // ── NoteTime ─────────────────────────────────────────────────────────────

    #[test]
    fn test_note_time_fields() {
        let nt = NoteTime {
            midi_note: 60,
            start: 0.5,
            length: 1.0,
            velocity: 100,
        };
        assert_eq!(nt.midi_note, 60);
        assert_eq!(nt.start, 0.5);
        assert_eq!(nt.length, 1.0);
        assert_eq!(nt.velocity, 100);
    }

    #[test]
    fn test_note_time_copy() {
        let a = NoteTime {
            midi_note: 69,
            start: 0.0,
            length: 0.5,
            velocity: 80,
        };
        let b = a;
        assert_eq!(a.midi_note, b.midi_note);
    }

    // ── DesiredProgramChange ─────────────────────────────────────────────────

    #[test]
    fn test_desired_program_change_fields() {
        let patch = MidiPatch {
            program: 10,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        };
        let dpc = DesiredProgramChange { channel: 3, patch };
        assert_eq!(dpc.channel, 3);
        assert_eq!(dpc.patch.program, 10);
    }

    #[test]
    fn test_desired_program_change_drum() {
        let patch = MidiPatch {
            program: 0,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: true,
        };
        let dpc = DesiredProgramChange { channel: 9, patch };
        assert!(dpc.patch.is_gm_gs_drum);
        assert_eq!(dpc.channel, 9);
    }

    // ── DesiredControllerChange ──────────────────────────────────────────────

    #[test]
    fn test_desired_controller_change_fields() {
        let dcc = DesiredControllerChange {
            channel: 0,
            controller_number: 7, // MAIN_VOLUME
            controller_value: 100,
        };
        assert_eq!(dcc.channel, 0);
        assert_eq!(dcc.controller_number, 7);
        assert_eq!(dcc.controller_value, 100);
    }

    #[test]
    fn test_desired_controller_change_equality() {
        let a = DesiredControllerChange {
            channel: 1,
            controller_number: 10,
            controller_value: 64,
        };
        let b = DesiredControllerChange {
            channel: 1,
            controller_number: 10,
            controller_value: 64,
        };
        assert_eq!(a, b);
    }

    // ── DesiredChannelTranspose ──────────────────────────────────────────────

    #[test]
    fn test_desired_channel_transpose_fields() {
        let dct = DesiredChannelTranspose {
            channel: 2,
            key_shift: 2.5,
        };
        assert_eq!(dct.channel, 2);
        assert_eq!(dct.key_shift, 2.5);
    }

    #[test]
    fn test_desired_channel_transpose_negative_shift() {
        let dct = DesiredChannelTranspose {
            channel: 0,
            key_shift: -1.0,
        };
        assert_eq!(dct.key_shift, -1.0);
    }

    // ── RMIDIWriteOptions ─────────────────────────────────────────────────────

    #[test]
    fn test_rmidi_write_options_default() {
        let opts = RMIDIWriteOptions::default();
        assert_eq!(opts.bank_offset, 0);
        assert!(!opts.correct_bank_offset);
        assert!(opts.metadata.name.is_none());
    }

    #[test]
    fn test_rmidi_write_options_custom() {
        let opts = RMIDIWriteOptions {
            bank_offset: 5,
            correct_bank_offset: true,
            metadata: RMIDInfoDataPartial {
                name: Some("Demo".to_string()),
                ..Default::default()
            },
        };
        assert_eq!(opts.bank_offset, 5);
        assert!(opts.correct_bank_offset);
        assert_eq!(opts.metadata.name.as_deref(), Some("Demo"));
    }

    // ── RMIDInfoFourCC ────────────────────────────────────────────────────────

    #[test]
    fn test_rmid_info_fourcc_as_str_all() {
        let cases = [
            (RMIDInfoFourCC::Inam, "INAM"),
            (RMIDInfoFourCC::Iprd, "IPRD"),
            (RMIDInfoFourCC::Ialb, "IALB"),
            (RMIDInfoFourCC::Iart, "IART"),
            (RMIDInfoFourCC::Ignr, "IGNR"),
            (RMIDInfoFourCC::Ipic, "IPIC"),
            (RMIDInfoFourCC::Icop, "ICOP"),
            (RMIDInfoFourCC::Icrd, "ICRD"),
            (RMIDInfoFourCC::Icrt, "ICRT"),
            (RMIDInfoFourCC::Icmt, "ICMT"),
            (RMIDInfoFourCC::Ieng, "IENG"),
            (RMIDInfoFourCC::Isft, "ISFT"),
            (RMIDInfoFourCC::Isbj, "ISBJ"),
            (RMIDInfoFourCC::Ienc, "IENC"),
            (RMIDInfoFourCC::Menc, "MENC"),
            (RMIDInfoFourCC::Dbnk, "DBNK"),
        ];
        for (variant, expected) in cases {
            assert_eq!(variant.as_str(), expected);
        }
    }

    #[test]
    fn test_rmid_info_fourcc_from_str_valid() {
        assert_eq!(RMIDInfoFourCC::from_str("INAM"), Some(RMIDInfoFourCC::Inam));
        assert_eq!(RMIDInfoFourCC::from_str("MENC"), Some(RMIDInfoFourCC::Menc));
        assert_eq!(RMIDInfoFourCC::from_str("DBNK"), Some(RMIDInfoFourCC::Dbnk));
    }

    #[test]
    fn test_rmid_info_fourcc_from_str_unknown() {
        assert_eq!(RMIDInfoFourCC::from_str("XXXX"), None);
        assert_eq!(RMIDInfoFourCC::from_str(""), None);
        assert_eq!(RMIDInfoFourCC::from_str("inam"), None); // case-sensitive
    }

    #[test]
    fn test_rmid_info_fourcc_roundtrip_all() {
        let all = [
            RMIDInfoFourCC::Inam,
            RMIDInfoFourCC::Iprd,
            RMIDInfoFourCC::Ialb,
            RMIDInfoFourCC::Iart,
            RMIDInfoFourCC::Ignr,
            RMIDInfoFourCC::Ipic,
            RMIDInfoFourCC::Icop,
            RMIDInfoFourCC::Icrd,
            RMIDInfoFourCC::Icrt,
            RMIDInfoFourCC::Icmt,
            RMIDInfoFourCC::Ieng,
            RMIDInfoFourCC::Isft,
            RMIDInfoFourCC::Isbj,
            RMIDInfoFourCC::Ienc,
            RMIDInfoFourCC::Menc,
            RMIDInfoFourCC::Dbnk,
        ];
        for v in all {
            assert_eq!(RMIDInfoFourCC::from_str(v.as_str()), Some(v));
        }
    }

    #[test]
    fn test_rmid_info_fourcc_as_str_is_4_chars() {
        let all = [
            RMIDInfoFourCC::Inam,
            RMIDInfoFourCC::Iprd,
            RMIDInfoFourCC::Ialb,
            RMIDInfoFourCC::Iart,
            RMIDInfoFourCC::Ignr,
            RMIDInfoFourCC::Ipic,
            RMIDInfoFourCC::Icop,
            RMIDInfoFourCC::Icrd,
            RMIDInfoFourCC::Icrt,
            RMIDInfoFourCC::Icmt,
            RMIDInfoFourCC::Ieng,
            RMIDInfoFourCC::Isft,
            RMIDInfoFourCC::Isbj,
            RMIDInfoFourCC::Ienc,
            RMIDInfoFourCC::Menc,
            RMIDInfoFourCC::Dbnk,
        ];
        for v in all {
            assert_eq!(v.as_str().len(), 4, "{:?}.as_str() should be 4 chars", v);
        }
    }
}
