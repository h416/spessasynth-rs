/// midi_patch.rs
/// purpose: MIDI patch (program/bank) data types, conversion utilities and the
///          patch selection algorithm (MIDIPatchTools).
/// Ported from: src/soundbank/basic_soundbank/midi_patch.ts
///
/// TS 4.3.0 merged preset_selector.ts into this file as `MIDIPatchTools.selectPatch`
/// (static) and reworked it around the `MIDIPatchFull` interface (`isDrum` flag).
/// In Rust, `MIDIPatchTools`' static methods are free functions, and `selectPatch`
/// operates on `&[BasicPreset]` with a parallel `is_drum: &[bool]` slice, because
/// Rust's `BasicPreset` does not store `parentSoundBank` (whose `isXGBank` flag
/// defines `isDrum` in TypeScript).
use std::cmp::Ordering;

use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::types::MIDISystem;
use crate::utils::loggin::SpessaLog;
use crate::utils::midi_hacks::BankSelectHacks;

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
    /// If the preset is marked as GM/GS drum preset. Note that XG drums do not have this flag.
    pub is_gm_gs_drum: bool,
}

/// A MIDI patch with an associated name and drum flag.
/// Equivalent to: MIDIPatchFull (extends MIDIPatch; TS 4.2.0 name: MIDIPatchNamed)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiPatchFull {
    pub patch: MidiPatch,
    /// The name of the patch.
    pub name: String,
    /// Indicates if this patch is a drum patch.
    /// If `is_gm_gs_drum` is true, then this is a GM/GS drum preset.
    /// If `is_gm_gs_drum` is false, then this is a GM2/XG drum preset.
    pub is_drum: bool,
}

/// Converts a MidiPatch to its string representation.
/// The format is:
/// - `DRUM:program` for `isGMGSDrum` set to `true`.
/// - `bankLSB:bankMSB:program` for `isGMGSDrum` set to `false`.
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
        return Err(format!("Invalid MIDI string: {s}"));
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

/// Converts a MidiPatchFull to its string representation.
/// The format is:
/// - `<MIDIPatch string> D <name>` for `is_drum` set to `true`.
/// - `<MIDIPatch string> M <name>` for `is_drum` set to `false`.
/// Equivalent to: MIDIPatchTools.toFullMIDIString (TS 4.2.0 name: toNamedMIDIString)
pub fn to_full_midi_string(patch: &MidiPatchFull) -> String {
    format!(
        "{} {} {}",
        to_midi_string(&patch.patch),
        if patch.is_drum { "D" } else { "M" },
        patch.name
    )
}

/// Parses a MidiPatchFull from its string representation.
/// Equivalent to: MIDIPatchTools.fromFullMIDIString (TS 4.2.0 name: fromNamedMIDIString)
pub fn from_full_midi_string(s: &str) -> Result<MidiPatchFull, String> {
    let first_space = s
        .find(' ')
        .ok_or_else(|| format!("Invalid named MIDI string: {s}"))?;
    let second_space = s[first_space + 1..]
        .find(' ')
        .map(|i| i + first_space + 1)
        .ok_or_else(|| format!("Invalid named MIDI string: {s}"))?;

    let midi_part = &s[..first_space];
    let drum_mode = &s[first_space + 1..second_space];
    let name = s[second_space + 1..].to_string();
    let patch = from_midi_string(midi_part)?;

    Ok(MidiPatchFull {
        patch,
        is_drum: drum_mode == "D",
        name,
    })
}

/// Checks if two MIDI patches represent the same one.
/// Equivalent to: MIDIPatchTools.matches
pub fn matches(patch1: &MidiPatch, patch2: &MidiPatch) -> bool {
    if patch1.is_gm_gs_drum || patch2.is_gm_gs_drum {
        // For drums only compare programs
        return patch1.is_gm_gs_drum == patch2.is_gm_gs_drum && patch1.program == patch2.program;
    }
    patch1.program == patch2.program
        && patch1.bank_lsb == patch2.bank_lsb
        && patch1.bank_msb == patch2.bank_msb
}

/// A comparison function for sorting, ordering the patches in ascending order.
/// Equivalent to: MIDIPatchTools.compare (TS 4.2.0 name: sorter; 4.3.0 moved the
/// drum check before the program comparison)
pub fn compare(a: &MidiPatch, b: &MidiPatch) -> Ordering {
    // Force drum presets to be last
    match (a.is_gm_gs_drum, b.is_gm_gs_drum) {
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    // First, sort by program
    if a.program != b.program {
        return a.program.cmp(&b.program);
    }
    // Next, sort by bankMSB
    if a.bank_msb != b.bank_msb {
        return a.bank_msb.cmp(&b.bank_msb);
    }
    // Finally, sort by bankLSB
    a.bank_lsb.cmp(&b.bank_lsb)
}

/// Checks if the given patch is an XG/GM2 drum patch.
/// `is_drum` is the preset's `isDrum` flag (see [`BasicPreset::is_drum`]).
/// Equivalent to: MIDIPatchTools.isXGDrum(p) = p.isDrum && !p.isGMGSDrum
#[inline]
pub fn is_xg_drum(is_drum: bool, is_gm_gs_drum: bool) -> bool {
    is_drum && !is_gm_gs_drum
}

/// Returns the index of any drum preset, preferring XG or GM/GS drums.
/// Equivalent to: private static getAnyDrums(presets, preferXG)
fn get_any_drums(presets: &[BasicPreset], is_drum: &[bool], prefer_xg: bool) -> usize {
    let p = if prefer_xg {
        // Get any XG drums
        (0..presets.len()).find(|&i| is_xg_drum(is_drum[i], presets[i].is_gm_gs_drum))
    } else {
        // Get any GM/GS drums
        (0..presets.len()).find(|&i| presets[i].is_gm_gs_drum)
    };
    if let Some(i) = p {
        // Return the found preset
        return i;
    }
    // Return any drum preset ... no? Then just return any preset
    (0..presets.len()).find(|&i| is_drum[i]).unwrap_or(0)
}

/// A sophisticated patch selection system based on the MIDI Patch system.
/// This is the algorithm that the synthesizer uses for selecting presets.
///
/// `is_drum` is a slice parallel to `patches` holding each preset's `isDrum` flag
/// (in TypeScript this is the `MIDIPatchFull.isDrum` property, computed through
/// `parentSoundBank.isXGBank`).
///
/// # Panics
/// Panics if `patches` is empty.
///
/// Equivalent to: MIDIPatchTools.selectPatch(patches, patch, system)
pub fn select_patch<'a>(
    patches: &'a [BasicPreset],
    is_drum: &[bool],
    mut patch: MidiPatch,
    system: MIDISystem,
) -> &'a BasicPreset {
    assert!(!patches.is_empty(), "No presets!");
    assert_eq!(patches.len(), is_drum.len(), "is_drum slice mismatch");

    if patch.is_gm_gs_drum && BankSelectHacks::is_system_xg(system) {
        // GM/GS drums with XG. This shouldn't happen. Force XG drums.
        patch = MidiPatch {
            is_gm_gs_drum: false,
            bank_lsb: 0,
            bank_msb: BankSelectHacks::get_drum_bank(system).unwrap_or(127),
            ..patch
        };
    }

    let is_gm_gs_drum = patch.is_gm_gs_drum;
    let bank_lsb = patch.bank_lsb;
    let bank_msb = patch.bank_msb;
    let program = patch.program;
    let is_xg = BankSelectHacks::is_system_xg(system);
    let xg_drums = BankSelectHacks::is_xg_drum(bank_msb) && is_xg;

    // Check for exact match
    let exact = (0..patches.len()).find(|&i| patches[i].matches(&patch));
    if let Some(i) = exact {
        // Special case:
        // Non XG banks sometimes specify melodic "MT" presets at bank 127,
        // Which matches XG banks.
        // Testcase: 4gmgsmt-sf2_04-compat.sf2
        // Only match if the preset declares itself as drums
        if !xg_drums || is_xg_drum(is_drum[i], patches[i].is_gm_gs_drum) {
            return &patches[i];
        }
    }

    // Helper to log failed exact matches
    let return_replacement = |i: usize| {
        SpessaLog::info(&format!(
            "Preset {} not found. ({:?}) Replaced with {}",
            to_midi_string(&patch),
            system,
            to_full_midi_string(&MidiPatchFull {
                patch: MidiPatch {
                    program: patches[i].program,
                    bank_msb: patches[i].bank_msb,
                    bank_lsb: patches[i].bank_lsb,
                    is_gm_gs_drum: patches[i].is_gm_gs_drum,
                },
                name: patches[i].name.clone(),
                is_drum: is_drum[i],
            }),
        ));
    };

    // No exact match...
    if is_gm_gs_drum {
        // GM/GS drums: check for the exact program match
        if let Some(i) =
            (0..patches.len()).find(|&i| patches[i].is_gm_gs_drum && patches[i].program == program)
        {
            return_replacement(i);
            return &patches[i];
        }

        // No match, pick any matching drum
        if let Some(i) = (0..patches.len()).find(|&i| is_drum[i] && patches[i].program == program) {
            return_replacement(i);
            return &patches[i];
        }

        // No match, pick the first drum preset, preferring GM/GS
        let i = get_any_drums(patches, is_drum, false);
        return_replacement(i);
        return &patches[i];
    }
    if xg_drums {
        // XG drums: Look for exact bank and program match
        if let Some(i) = (0..patches.len())
            .find(|&i| patches[i].program == program && is_drum[i] && !patches[i].is_gm_gs_drum)
        {
            return_replacement(i);
            return &patches[i];
        }

        // No match, pick any matching drum
        let p = (0..patches.len()).find(|&i| is_drum[i] && patches[i].program == program);

        // Program 49 and above start to diverge between GS and XG.
        // For example,
        // XG MU2000 and similar have regular drums on program 56, while GS has the SFX kit.
        // So avoid selecting it and pick any XG drums.
        if let Some(i) = p
            && patches[i].program < 49
        {
            return_replacement(i);
            return &patches[i];
        }

        // Pick any drums, preferring XG
        let i = get_any_drums(patches, is_drum, true);
        return_replacement(i);
        return &patches[i];
    }
    // Melodic preset
    let matching_programs: Vec<usize> = (0..patches.len())
        .filter(|&i| patches[i].program == program && !is_drum[i])
        .collect();
    if matching_programs.is_empty() {
        // The first preset
        return_replacement(0);
        return &patches[0];
    }
    let p = if is_xg {
        // XG uses LSB so search for that.
        matching_programs
            .iter()
            .find(|&&i| patches[i].bank_lsb == bank_lsb)
            .copied()
    } else {
        // GS uses MSB so search for that.
        matching_programs
            .iter()
            .find(|&&i| patches[i].bank_msb == bank_msb)
            .copied()
    };
    if let Some(i) = p {
        return_replacement(i);
        return &patches[i];
    }
    // Special XG case: 64 on LSB can't default to 64 MSB.
    // Testcase: Cybergate.mid
    // Selects 64 LSB on warm pad, on DLSbyXG.dls it gets replaced with Bird 2 SFX
    if bank_lsb != 64 || !is_xg {
        let bank = bank_msb.max(bank_lsb);
        // Any matching bank.
        if let Some(&i) = matching_programs
            .iter()
            .find(|&&i| patches[i].bank_lsb == bank || patches[i].bank_msb == bank)
        {
            return_replacement(i);
            return &patches[i];
        }
    }
    // The first matching program
    return_replacement(matching_programs[0]);
    &patches[matching_programs[0]]
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

    // --- to_full_midi_string ---

    #[test]
    fn test_to_full_midi_string_melodic() {
        let np = MidiPatchFull {
            patch: normal(10, 2, 3),
            name: "Piano".to_string(),
            is_drum: false,
        };
        assert_eq!(to_full_midi_string(&np), "3:2:10 M Piano");
    }

    #[test]
    fn test_to_full_midi_string_drum() {
        let np = MidiPatchFull {
            patch: drum(0),
            name: "Standard Kit".to_string(),
            is_drum: true,
        };
        assert_eq!(to_full_midi_string(&np), "DRUM:0 D Standard Kit");
    }

    // --- from_full_midi_string ---

    #[test]
    fn test_from_full_midi_string_melodic() {
        let np = from_full_midi_string("3:2:10 M Piano").unwrap();
        assert_eq!(np.patch, normal(10, 2, 3));
        assert_eq!(np.name, "Piano");
        assert!(!np.is_drum);
    }

    #[test]
    fn test_from_full_midi_string_drum() {
        let np = from_full_midi_string("DRUM:0 D Standard Kit").unwrap();
        assert_eq!(np.patch, drum(0));
        assert_eq!(np.name, "Standard Kit");
        assert!(np.is_drum);
    }

    #[test]
    fn test_from_full_midi_string_one_space_is_err() {
        assert!(from_full_midi_string("3:2:10 M").is_err());
    }

    #[test]
    fn test_from_full_midi_string_no_space_is_err() {
        assert!(from_full_midi_string("3:2:10").is_err());
    }

    #[test]
    fn test_from_full_midi_string_roundtrip() {
        let np = MidiPatchFull {
            patch: normal(7, 0, 0),
            name: "Harpsichord".to_string(),
            is_drum: false,
        };
        assert_eq!(
            from_full_midi_string(&to_full_midi_string(&np)).unwrap(),
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

    // --- compare ---

    #[test]
    fn test_compare_by_program() {
        assert_eq!(compare(&normal(5, 0, 0), &normal(10, 0, 0)), Ordering::Less);
        assert_eq!(
            compare(&normal(10, 0, 0), &normal(5, 0, 0)),
            Ordering::Greater
        );
    }

    #[test]
    fn test_compare_equal_program_no_drum() {
        assert_eq!(
            compare(&normal(10, 0, 0), &normal(10, 0, 0)),
            Ordering::Equal
        );
    }

    #[test]
    fn test_compare_drum_after_normal_any_program() {
        // TS 4.3.0: the drum check comes before the program comparison,
        // so drums sort after melodic presets regardless of program.
        assert_eq!(compare(&drum(0), &normal(127, 0, 0)), Ordering::Greater);
        assert_eq!(compare(&normal(127, 0, 0), &drum(0)), Ordering::Less);
    }

    #[test]
    fn test_compare_by_bank_msb() {
        assert_eq!(
            compare(&normal(10, 1, 0), &normal(10, 2, 0)),
            Ordering::Less
        );
    }

    #[test]
    fn test_compare_by_bank_lsb() {
        assert_eq!(
            compare(&normal(10, 0, 1), &normal(10, 0, 2)),
            Ordering::Less
        );
    }

    // --- is_xg_drum ---

    #[test]
    fn test_is_xg_drum_true_for_non_gmgs_drum() {
        assert!(is_xg_drum(true, false));
    }

    #[test]
    fn test_is_xg_drum_false_for_gmgs_drum() {
        assert!(!is_xg_drum(true, true));
    }

    #[test]
    fn test_is_xg_drum_false_for_melodic() {
        assert!(!is_xg_drum(false, false));
    }
}

// ---------------------------------------------------------------------------
// Tests for select_patch (ported from the former preset_selector tests)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod select_patch_tests {
    use super::*;

    // --- helpers ---

    fn melodic(program: u8, bank_msb: u8, bank_lsb: u8) -> BasicPreset {
        BasicPreset {
            program,
            bank_msb,
            bank_lsb,
            is_gm_gs_drum: false,
            ..BasicPreset::default()
        }
    }

    fn gm_gs_drum(program: u8) -> BasicPreset {
        BasicPreset {
            program,
            is_gm_gs_drum: true,
            ..BasicPreset::default()
        }
    }

    fn xg_drum(program: u8, bank_msb: u8) -> BasicPreset {
        BasicPreset {
            program,
            bank_msb,
            is_gm_gs_drum: false,
            ..BasicPreset::default()
        }
    }

    fn patch(program: u8, bank_msb: u8, bank_lsb: u8) -> MidiPatch {
        MidiPatch {
            program,
            bank_msb,
            bank_lsb,
            is_gm_gs_drum: false,
        }
    }

    fn drum_patch(program: u8) -> MidiPatch {
        MidiPatch {
            program,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: true,
        }
    }

    /// Builds the parallel isDrum slice for the given presets, mirroring
    /// `BasicPreset::is_drum(is_xg_bank)`.
    fn drum_flags(presets: &[BasicPreset], is_xg_bank: bool) -> Vec<bool> {
        presets.iter().map(|p| p.is_drum(is_xg_bank)).collect()
    }

    fn select<'a>(
        presets: &'a [BasicPreset],
        p: MidiPatch,
        system: MIDISystem,
        is_xg_bank: bool,
    ) -> &'a BasicPreset {
        let flags = drum_flags(presets, is_xg_bank);
        select_patch(presets, &flags, p, system)
    }

    // --- exact match ---

    #[test]
    fn test_exact_match_melodic_gs() {
        let presets = vec![melodic(0, 0, 0), melodic(10, 2, 0), melodic(20, 0, 0)];
        let p = select(&presets, patch(10, 2, 0), MIDISystem::Gs, false);
        assert_eq!(p.program, 10);
        assert_eq!(p.bank_msb, 2);
    }

    #[test]
    fn test_exact_match_melodic_xg() {
        let presets = vec![
            melodic(0, 0, 0),
            melodic(10, 0, 3), // bank_lsb=3
        ];
        let p = select(&presets, patch(10, 0, 3), MIDISystem::Xg, false);
        assert_eq!(p.program, 10);
        assert_eq!(p.bank_lsb, 3);
    }

    #[test]
    fn test_exact_match_gm_gs_drum() {
        let presets = vec![melodic(0, 0, 0), gm_gs_drum(0)];
        let p = select(&presets, drum_patch(0), MIDISystem::Gs, false);
        assert!(p.is_gm_gs_drum);
        assert_eq!(p.program, 0);
    }

    // --- GM/GS drum with XG system forces XG drums ---

    #[test]
    fn test_gm_gs_drum_with_xg_forces_xg_drum() {
        // GM/GS drum patch + XG system should search for XG drum bank
        let presets = vec![
            xg_drum(0, 127), // XG drum bank 127
            gm_gs_drum(0),
        ];
        // Under XG, is_gm_gs_drum is forced to false, bank_msb becomes 127
        // So exact match becomes patch(0, 127, 0) against xg_drum(0, 127)
        let p = select(&presets, drum_patch(0), MIDISystem::Xg, true);
        assert!(!p.is_gm_gs_drum);
        assert_eq!(p.bank_msb, 127);
    }

    // --- no presets panics ---

    #[test]
    #[should_panic(expected = "No presets!")]
    fn test_empty_presets_panics() {
        select_patch(&[], &[], patch(0, 0, 0), MIDISystem::Gs);
    }

    // --- melodic fallback to first ---

    #[test]
    fn test_melodic_no_program_match_returns_first() {
        let presets = vec![melodic(5, 0, 0), melodic(6, 0, 0)];
        // Request program 99 (doesn't exist)
        let p = select(&presets, patch(99, 0, 0), MIDISystem::Gs, false);
        assert_eq!(p.program, 5); // first preset
    }

    // --- GS melodic: uses MSB ---

    #[test]
    fn test_melodic_gs_prefers_bank_msb_match() {
        let presets = vec![
            melodic(10, 0, 0), // bank_msb=0
            melodic(10, 2, 0), // bank_msb=2
            melodic(10, 5, 0), // bank_msb=5
        ];
        let p = select(&presets, patch(10, 5, 0), MIDISystem::Gs, false);
        assert_eq!(p.bank_msb, 5);
    }

    // --- XG melodic: uses LSB ---

    #[test]
    fn test_melodic_xg_prefers_bank_lsb_match() {
        let presets = vec![
            melodic(10, 0, 0), // bank_lsb=0
            melodic(10, 0, 3), // bank_lsb=3
            melodic(10, 0, 7), // bank_lsb=7
        ];
        let p = select(&presets, patch(10, 0, 7), MIDISystem::Xg, false);
        assert_eq!(p.bank_lsb, 7);
    }

    // --- melodic any-bank fallback ---

    #[test]
    fn test_melodic_gs_no_msb_match_falls_back_to_any_bank() {
        // Only bank_msb=3 available, request bank_msb=9
        let presets = vec![melodic(10, 3, 0)];
        // No exact program+bank match found; falls back via max(msb, lsb)
        let p = select(&presets, patch(10, 9, 0), MIDISystem::Gs, false);
        assert_eq!(p.program, 10);
    }

    #[test]
    fn test_melodic_fallback_to_first_matching_program() {
        let presets = vec![melodic(10, 3, 0), melodic(10, 5, 0)];
        // Request program 10, bank_msb=99 → no exact bank match, no any-bank match
        let p = select(&presets, patch(10, 99, 0), MIDISystem::Gs, false);
        // max(99, 0)=99, neither has bank_msb or bank_lsb == 99 → first matching program
        assert_eq!(p.program, 10);
        assert_eq!(p.bank_msb, 3);
    }

    // --- GM/GS drum fallback: exact program ---

    #[test]
    fn test_gm_gs_drum_exact_program_match() {
        let presets = vec![gm_gs_drum(25), gm_gs_drum(0)];
        let p = select(&presets, drum_patch(25), MIDISystem::Gs, false);
        assert!(p.is_gm_gs_drum);
        assert_eq!(p.program, 25);
    }

    #[test]
    fn test_gm_gs_drum_any_drum_program_fallback() {
        // No GM/GS drum with program 25 → falls back to any drum with that program
        let presets = vec![
            xg_drum(25, 120), // XG drum, program=25
            gm_gs_drum(0),    // GM/GS drum, program=0
        ];
        // Non-XG bank: xg_drum(25,120).is_drum == false
        // No match for "is_gm_gs_drum && program==25", no "is_drum && program==25"
        // → get_any_drums(prefer_xg=false) → finds gm_gs_drum(0)
        let p = select(&presets, drum_patch(25), MIDISystem::Gs, false);
        assert!(p.is_gm_gs_drum);
        assert_eq!(p.program, 0);
    }

    // --- XG drums ---

    #[test]
    fn test_xg_drum_exact_program_match() {
        let presets = vec![xg_drum(0, 127), xg_drum(25, 127)];
        // Request XG drum program 25: bank_msb=127, is_xg=true
        let p = select(&presets, patch(25, 127, 0), MIDISystem::Xg, true);
        assert_eq!(p.program, 25);
        assert_eq!(p.bank_msb, 127);
    }

    #[test]
    fn test_xg_drum_fallback_any_drum_with_program_below_49() {
        // XG drum program=25 not available as XG, but a GM/GS drum with program=25 exists
        let presets = vec![xg_drum(0, 127), gm_gs_drum(25)];
        // program 25 < 49 → the any-drum fallback is allowed
        let p = select(&presets, patch(25, 127, 0), MIDISystem::Xg, true);
        assert_eq!(p.program, 25);
    }

    #[test]
    fn test_xg_drum_program_49_and_above_avoids_gs_kit() {
        // TS 4.3.0: for program >= 49 the any-drum (GM/GS) fallback is skipped
        // because GS and XG kits diverge there; any XG drums are preferred.
        let presets = vec![xg_drum(0, 127), gm_gs_drum(56)];
        let p = select(&presets, patch(56, 127, 0), MIDISystem::Xg, true);
        // Skips gm_gs_drum(56); picks any XG drums (program 0, bank 127).
        assert_eq!(p.bank_msb, 127);
        assert_eq!(p.program, 0);
        assert!(!p.is_gm_gs_drum);
    }

    #[test]
    fn test_xg_drum_fallback_any_xg_drum() {
        let presets = vec![xg_drum(0, 127), melodic(10, 0, 0)];
        // Request bank_msb=127, program=25 (not found) → fall back to any XG drum
        let p = select(&presets, patch(25, 127, 0), MIDISystem::Xg, true);
        assert_eq!(p.bank_msb, 127);
    }

    // --- special XG LSB=64 case ---

    #[test]
    fn test_xg_special_lsb64_does_not_fall_back_to_bank64() {
        let presets = vec![
            melodic(10, 64, 0), // bank_msb=64 (SFX voice bank)
            melodic(10, 0, 0),
        ];
        // With XG, bank_lsb=64 must not fall back to bank_msb=64 or bank_lsb=64 match
        let p = select(&presets, patch(10, 0, 64), MIDISystem::Xg, false);
        // Falls through to first matching program
        assert_eq!(p.program, 10);
        assert_eq!(p.bank_msb, 64);
    }

    #[test]
    fn test_non_xg_lsb64_uses_any_bank_fallback() {
        let presets = vec![
            melodic(10, 64, 0), // bank_msb=64
        ];
        // GS system with bank_lsb=64: the special case (bank_lsb==64 && is_xg) does NOT apply
        // max(bank_msb=0, bank_lsb=64)=64 → matches preset's bank_msb=64
        let p = select(&presets, patch(10, 0, 64), MIDISystem::Gs, false);
        assert_eq!(p.program, 10);
        assert_eq!(p.bank_msb, 64);
    }

    // --- single preset always returns it ---

    #[test]
    fn test_single_preset_always_returns_it() {
        let presets = vec![melodic(0, 0, 0)];
        // Even a completely different patch returns the only preset
        let p = select(&presets, patch(99, 99, 99), MIDISystem::Gs, false);
        assert_eq!(p.program, 0);
    }

    // --- drums not selected as melodic ---

    #[test]
    fn test_drum_preset_not_returned_for_melodic_request() {
        let presets = vec![gm_gs_drum(10), melodic(10, 0, 0)];
        // Melodic request: drums should be filtered out from matching_programs
        let p = select(&presets, patch(10, 0, 0), MIDISystem::Gs, false);
        assert!(!p.is_gm_gs_drum);
        assert_eq!(p.program, 10);
    }

    // --- get_any_drums helper ---

    #[test]
    fn test_get_any_drums_prefers_gm_gs_when_prefer_xg_false() {
        let presets = vec![xg_drum(0, 127), gm_gs_drum(0)];
        let flags = drum_flags(&presets, true);
        let i = get_any_drums(&presets, &flags, false);
        assert!(presets[i].is_gm_gs_drum);
    }

    #[test]
    fn test_get_any_drums_prefers_xg_when_prefer_xg_true() {
        let presets = vec![gm_gs_drum(0), xg_drum(0, 127)];
        let flags = drum_flags(&presets, true);
        let i = get_any_drums(&presets, &flags, true);
        assert!(is_xg_drum(flags[i], presets[i].is_gm_gs_drum));
    }

    #[test]
    fn test_get_any_drums_returns_first_when_no_drum_found() {
        let presets = vec![melodic(0, 0, 0), melodic(10, 0, 0)];
        let flags = drum_flags(&presets, false);
        let i = get_any_drums(&presets, &flags, false);
        assert_eq!(presets[i].program, 0); // first preset
    }
}
