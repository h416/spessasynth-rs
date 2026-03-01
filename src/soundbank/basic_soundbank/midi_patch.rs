/// midi_patch.rs
/// purpose: MIDI patch (program/bank) data types and conversion utilities.
/// Ported from: src/soundbank/basic_soundbank/midi_patch.ts
use std::cmp::Ordering;

/// A MIDI patch (program + bank selection).
/// Equivalent to: MIDIPatch
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiPatch {
    /// The MIDI program number.
    pub program: u8,
    /// The MIDI bank MSB number.
    pub bank_msb: u8,
    /// The MIDI bank LSB number.
    pub bank_lsb: u8,
    /// If the preset is marked as GM/GS drum preset.
    pub is_gm_gs_drum: bool,
}

/// A MIDI patch with an associated name.
/// Equivalent to: MIDIPatchNamed (extends MIDIPatch)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiPatchNamed {
    pub patch: MidiPatch,
    pub name: String,
}

/// Converts a MidiPatch to its string representation.
/// Equivalent to: MIDIPatchTools.toMIDIString
pub fn to_midi_string(patch: &MidiPatch) -> String {
    if patch.is_gm_gs_drum {
        format!("DRUM:{}", patch.program)
    } else {
        format!("{}:{}:{}", patch.bank_lsb, patch.bank_msb, patch.program)
    }
}

/// Parses a MidiPatch from its string representation.
/// Equivalent to: MIDIPatchTools.fromMIDIString
pub fn from_midi_string(s: &str) -> Result<MidiPatch, String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() > 3 || parts.len() < 2 {
        return Err("Invalid MIDI string:".to_string());
    }
    if s.starts_with("DRUM") {
        let program = parts[1]
            .parse::<u8>()
            .map_err(|e| format!("Invalid MIDI string: {e}"))?;
        Ok(MidiPatch {
            bank_msb: 0,
            bank_lsb: 0,
            program,
            is_gm_gs_drum: true,
        })
    } else {
        let bank_lsb = parts[0]
            .parse::<u8>()
            .map_err(|e| format!("Invalid MIDI string: {e}"))?;
        let bank_msb = parts[1]
            .parse::<u8>()
            .map_err(|e| format!("Invalid MIDI string: {e}"))?;
        let program = parts[2]
            .parse::<u8>()
            .map_err(|e| format!("Invalid MIDI string: {e}"))?;
        Ok(MidiPatch {
            bank_lsb,
            bank_msb,
            program,
            is_gm_gs_drum: false,
        })
    }
}

/// Converts a MidiPatchNamed to its string representation.
/// Equivalent to: MIDIPatchTools.toNamedMIDIString
pub fn to_named_midi_string(patch: &MidiPatchNamed) -> String {
    format!("{} {}", to_midi_string(&patch.patch), patch.name)
}

/// Parses a MidiPatchNamed from its string representation.
/// Equivalent to: MIDIPatchTools.fromNamedMIDIString
pub fn from_named_midi_string(s: &str) -> Result<MidiPatchNamed, String> {
    let first_space = s
        .find(' ')
        .ok_or_else(|| format!("Invalid named MIDI string: {s}"))?;
    let patch = from_midi_string(&s[..first_space])?;
    let name = s[first_space + 1..].to_string();
    Ok(MidiPatchNamed { patch, name })
}

/// Checks if two MidiPatches match.
/// Equivalent to: MIDIPatchTools.matches
pub fn matches(patch1: &MidiPatch, patch2: &MidiPatch) -> bool {
    if patch1.is_gm_gs_drum || patch2.is_gm_gs_drum {
        // For drums only compare program and the drum flag
        return patch1.is_gm_gs_drum == patch2.is_gm_gs_drum && patch1.program == patch2.program;
    }
    patch1.program == patch2.program
        && patch1.bank_lsb == patch2.bank_lsb
        && patch1.bank_msb == patch2.bank_msb
}

/// Sorts two MidiPatches. Drum presets are forced to be last.
/// Equivalent to: MIDIPatchTools.sorter
pub fn sorter(a: &MidiPatch, b: &MidiPatch) -> Ordering {
    if a.program != b.program {
        return a.program.cmp(&b.program);
    }
    match (a.is_gm_gs_drum, b.is_gm_gs_drum) {
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    if a.bank_msb != b.bank_msb {
        return a.bank_msb.cmp(&b.bank_msb);
    }
    a.bank_lsb.cmp(&b.bank_lsb)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal(program: u8, bank_msb: u8, bank_lsb: u8) -> MidiPatch {
        MidiPatch {
            program,
            bank_msb,
            bank_lsb,
            is_gm_gs_drum: false,
        }
    }

    fn drum(program: u8) -> MidiPatch {
        MidiPatch {
            program,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: true,
        }
    }

    // --- to_midi_string ---

    #[test]
    fn test_to_midi_string_normal() {
        assert_eq!(to_midi_string(&normal(10, 2, 3)), "3:2:10");
    }

    #[test]
    fn test_to_midi_string_drum() {
        assert_eq!(to_midi_string(&drum(25)), "DRUM:25");
    }

    #[test]
    fn test_to_midi_string_bank_zero() {
        assert_eq!(to_midi_string(&normal(0, 0, 0)), "0:0:0");
    }

    // --- from_midi_string ---

    #[test]
    fn test_from_midi_string_normal() {
        let p = from_midi_string("3:2:10").unwrap();
        assert_eq!(p.bank_lsb, 3);
        assert_eq!(p.bank_msb, 2);
        assert_eq!(p.program, 10);
        assert!(!p.is_gm_gs_drum);
    }

    #[test]
    fn test_from_midi_string_drum() {
        let p = from_midi_string("DRUM:25").unwrap();
        assert_eq!(p.program, 25);
        assert!(p.is_gm_gs_drum);
        assert_eq!(p.bank_msb, 0);
        assert_eq!(p.bank_lsb, 0);
    }

    #[test]
    fn test_from_midi_string_too_few_parts() {
        assert!(from_midi_string("5").is_err());
    }

    #[test]
    fn test_from_midi_string_too_many_parts() {
        assert!(from_midi_string("1:2:3:4").is_err());
    }

    #[test]
    fn test_from_midi_string_roundtrip_normal() {
        let p = normal(42, 1, 5);
        assert_eq!(from_midi_string(&to_midi_string(&p)).unwrap(), p);
    }

    #[test]
    fn test_from_midi_string_roundtrip_drum() {
        let p = drum(10);
        assert_eq!(from_midi_string(&to_midi_string(&p)).unwrap(), p);
    }

    // --- to_named_midi_string ---

    #[test]
    fn test_to_named_midi_string_normal() {
        let np = MidiPatchNamed {
            patch: normal(10, 2, 3),
            name: "Piano".to_string(),
        };
        assert_eq!(to_named_midi_string(&np), "3:2:10 Piano");
    }

    #[test]
    fn test_to_named_midi_string_drum() {
        let np = MidiPatchNamed {
            patch: drum(0),
            name: "Standard Kit".to_string(),
        };
        assert_eq!(to_named_midi_string(&np), "DRUM:0 Standard Kit");
    }

    // --- from_named_midi_string ---

    #[test]
    fn test_from_named_midi_string_normal() {
        let np = from_named_midi_string("3:2:10 Piano").unwrap();
        assert_eq!(np.patch, normal(10, 2, 3));
        assert_eq!(np.name, "Piano");
    }

    #[test]
    fn test_from_named_midi_string_drum() {
        let np = from_named_midi_string("DRUM:0 Standard Kit").unwrap();
        assert_eq!(np.patch, drum(0));
        assert_eq!(np.name, "Standard Kit");
    }

    #[test]
    fn test_from_named_midi_string_no_space_is_err() {
        assert!(from_named_midi_string("3:2:10").is_err());
    }

    #[test]
    fn test_from_named_midi_string_roundtrip() {
        let np = MidiPatchNamed {
            patch: normal(7, 0, 0),
            name: "Harpsichord".to_string(),
        };
        assert_eq!(
            from_named_midi_string(&to_named_midi_string(&np)).unwrap(),
            np
        );
    }

    // --- matches ---

    #[test]
    fn test_matches_same_normal() {
        assert!(matches(&normal(10, 2, 3), &normal(10, 2, 3)));
    }

    #[test]
    fn test_matches_diff_program() {
        assert!(!matches(&normal(10, 2, 3), &normal(11, 2, 3)));
    }

    #[test]
    fn test_matches_diff_bank_msb() {
        assert!(!matches(&normal(10, 2, 3), &normal(10, 1, 3)));
    }

    #[test]
    fn test_matches_diff_bank_lsb() {
        assert!(!matches(&normal(10, 2, 3), &normal(10, 2, 4)));
    }

    #[test]
    fn test_matches_same_drum() {
        assert!(matches(&drum(25), &drum(25)));
    }

    #[test]
    fn test_matches_diff_drum_program() {
        assert!(!matches(&drum(25), &drum(26)));
    }

    #[test]
    fn test_matches_drum_vs_normal_same_program() {
        // drum flag differs → not a match
        assert!(!matches(&drum(10), &normal(10, 0, 0)));
    }

    // --- sorter ---

    #[test]
    fn test_sorter_by_program() {
        assert_eq!(sorter(&normal(5, 0, 0), &normal(10, 0, 0)), Ordering::Less);
        assert_eq!(
            sorter(&normal(10, 0, 0), &normal(5, 0, 0)),
            Ordering::Greater
        );
    }

    #[test]
    fn test_sorter_equal_program_no_drum() {
        assert_eq!(
            sorter(&normal(10, 0, 0), &normal(10, 0, 0)),
            Ordering::Equal
        );
    }

    #[test]
    fn test_sorter_drum_after_normal_same_program() {
        assert_eq!(sorter(&drum(10), &normal(10, 0, 0)), Ordering::Greater);
        assert_eq!(sorter(&normal(10, 0, 0), &drum(10)), Ordering::Less);
    }

    #[test]
    fn test_sorter_by_bank_msb() {
        assert_eq!(sorter(&normal(10, 1, 0), &normal(10, 2, 0)), Ordering::Less);
    }

    #[test]
    fn test_sorter_by_bank_lsb() {
        assert_eq!(sorter(&normal(10, 0, 1), &normal(10, 0, 2)), Ordering::Less);
    }
}
