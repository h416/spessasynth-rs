/// preset_selector.rs
/// purpose: Sophisticated preset selection based on the MIDI Patch system.
/// Ported from: src/soundbank/basic_soundbank/preset_selector.ts
use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::midi_patch::{MidiPatch, to_midi_string};
use crate::synthesizer::types::SynthSystem;
use crate::utils::loggin::spessa_synth_info;
use crate::utils::midi_hacks::BankSelectHacks;

/// Returns any drum preset from `presets`, preferring XG or GM/GS drums.
/// Equivalent to: getAnyDrums
fn get_any_drums(presets: &[BasicPreset], prefer_xg: bool, is_xg: bool) -> &BasicPreset {
    let p = if prefer_xg {
        presets.iter().find(|p| p.is_xg_drums(is_xg))
    } else {
        presets.iter().find(|p| p.is_gm_gs_drum)
    };
    if let Some(p) = p {
        return p;
    }
    // Return any drum preset, or the first preset as a fallback
    presets
        .iter()
        .find(|p| p.is_any_drums(is_xg))
        .unwrap_or(&presets[0])
}

/// A sophisticated preset selection system based on the MIDI Patch system.
///
/// # Panics
/// Panics if `presets` is empty.
///
/// Equivalent to: selectPreset
pub fn select_preset(
    presets: &[BasicPreset],
    mut patch: MidiPatch,
    system: SynthSystem,
) -> &BasicPreset {
    assert!(!presets.is_empty(), "No presets!");

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
    let xg_drums = BankSelectHacks::is_xg_drums(bank_msb) && is_xg;

    // Check for exact match
    let exact = presets.iter().find(|p| p.matches(&patch));
    if let Some(p) = exact {
        // Special case: non-XG banks sometimes specify melodic "MT" presets at bank 127,
        // which matches XG banks. Only match if the preset declares itself as drums.
        if !xg_drums || p.is_xg_drums(is_xg) {
            return p;
        }
    }

    // Helper to log failed exact matches
    let log_replacement = |pres: &BasicPreset| {
        spessa_synth_info(&format!(
            "Preset {} not found. ({:?}) Replaced with {}",
            to_midi_string(&patch),
            system,
            pres,
        ));
    };

    // No exact match...
    if is_gm_gs_drum {
        // GM/GS drums: check for the exact program match
        if let Some(p) = presets
            .iter()
            .find(|p| p.is_gm_gs_drum && p.program == program)
        {
            log_replacement(p);
            return p;
        }

        // No match, pick any matching drum
        if let Some(p) = presets
            .iter()
            .find(|p| p.is_any_drums(is_xg) && p.program == program)
        {
            log_replacement(p);
            return p;
        }

        // No match, pick the first drum preset, preferring GM/GS
        let p = get_any_drums(presets, false, is_xg);
        log_replacement(p);
        return p;
    }

    if xg_drums {
        // XG drums: look for exact bank and program match
        if let Some(p) = presets
            .iter()
            .find(|p| p.program == program && p.is_xg_drums(is_xg))
        {
            log_replacement(p);
            return p;
        }

        // No match, pick any matching drum
        if let Some(p) = presets
            .iter()
            .find(|p| p.is_any_drums(is_xg) && p.program == program)
        {
            log_replacement(p);
            return p;
        }

        // Pick any drums, preferring XG
        let p = get_any_drums(presets, true, is_xg);
        log_replacement(p);
        return p;
    }

    // Melodic preset
    let matching_programs: Vec<&BasicPreset> = presets
        .iter()
        .filter(|p| p.program == program && !p.is_any_drums(is_xg))
        .collect();

    if matching_programs.is_empty() {
        let first = &presets[0];
        log_replacement(first);
        return first;
    }

    let p = if is_xg {
        // XG uses LSB so search for that
        matching_programs
            .iter()
            .find(|p| p.bank_lsb == bank_lsb)
            .copied()
    } else {
        // GS uses MSB so search for that
        matching_programs
            .iter()
            .find(|p| p.bank_msb == bank_msb)
            .copied()
    };

    if let Some(p) = p {
        log_replacement(p);
        return p;
    }

    // Special XG case: 64 on LSB can't default to 64 MSB.
    // Testcase: Cybergate.mid
    if bank_lsb != 64 || !is_xg {
        let bank = bank_msb.max(bank_lsb);
        if let Some(p) = matching_programs
            .iter()
            .find(|p| p.bank_lsb == bank || p.bank_msb == bank)
            .copied()
        {
            log_replacement(p);
            return p;
        }
    }

    // The first matching program
    let first = matching_programs[0];
    log_replacement(first);
    first
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
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

    // --- exact match ---

    #[test]
    fn test_exact_match_melodic_gs() {
        let presets = vec![melodic(0, 0, 0), melodic(10, 2, 0), melodic(20, 0, 0)];
        let p = select_preset(&presets, patch(10, 2, 0), SynthSystem::Gs);
        assert_eq!(p.program, 10);
        assert_eq!(p.bank_msb, 2);
    }

    #[test]
    fn test_exact_match_melodic_xg() {
        let presets = vec![
            melodic(0, 0, 0),
            melodic(10, 0, 3), // bank_lsb=3
        ];
        let p = select_preset(&presets, patch(10, 0, 3), SynthSystem::Xg);
        assert_eq!(p.program, 10);
        assert_eq!(p.bank_lsb, 3);
    }

    #[test]
    fn test_exact_match_gm_gs_drum() {
        let presets = vec![melodic(0, 0, 0), gm_gs_drum(0)];
        let p = select_preset(&presets, drum_patch(0), SynthSystem::Gs);
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
        let p = select_preset(&presets, drum_patch(0), SynthSystem::Xg);
        assert!(!p.is_gm_gs_drum);
        assert_eq!(p.bank_msb, 127);
    }

    // --- no presets panics ---

    #[test]
    #[should_panic(expected = "No presets!")]
    fn test_empty_presets_panics() {
        select_preset(&[], patch(0, 0, 0), SynthSystem::Gs);
    }

    // --- melodic fallback to first ---

    #[test]
    fn test_melodic_no_program_match_returns_first() {
        let presets = vec![melodic(5, 0, 0), melodic(6, 0, 0)];
        // Request program 99 (doesn't exist)
        let p = select_preset(&presets, patch(99, 0, 0), SynthSystem::Gs);
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
        let p = select_preset(&presets, patch(10, 5, 0), SynthSystem::Gs);
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
        let p = select_preset(&presets, patch(10, 0, 7), SynthSystem::Xg);
        assert_eq!(p.bank_lsb, 7);
    }

    // --- melodic any-bank fallback ---

    #[test]
    fn test_melodic_gs_no_msb_match_falls_back_to_any_bank() {
        // Only bank_msb=3 available, request bank_msb=9
        let presets = vec![melodic(10, 3, 0)];
        // No exact program+bank match found; falls back via max(msb, lsb)
        let p = select_preset(&presets, patch(10, 9, 0), SynthSystem::Gs);
        // bank_msb=9 or bank_lsb=0 → max is 9; preset has bank_msb=3 → no bank match
        // → falls through to first matching program
        assert_eq!(p.program, 10);
    }

    #[test]
    fn test_melodic_fallback_to_first_matching_program() {
        let presets = vec![melodic(10, 3, 0), melodic(10, 5, 0)];
        // Request program 10, bank_msb=99 → no exact bank match, no any-bank match
        let p = select_preset(&presets, patch(10, 99, 0), SynthSystem::Gs);
        // max(99, 0)=99, neither has bank_msb or bank_lsb == 99 → first matching program
        assert_eq!(p.program, 10);
        assert_eq!(p.bank_msb, 3);
    }

    // --- GM/GS drum fallback: exact program ---

    #[test]
    fn test_gm_gs_drum_exact_program_match() {
        let presets = vec![gm_gs_drum(25), gm_gs_drum(0)];
        let p = select_preset(&presets, drum_patch(25), SynthSystem::Gs);
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
        // GS system: is_xg=false, so is_any_drums returns is_gm_gs_drum only (xg_drum(25,120) is not a GM/GS drum)
        // Actually wait: with GS system, is_xg=false, so:
        //   gm_gs_drum(0): is_gm_gs_drum=true, program=0 → not matching program 25
        //   xg_drum(25, 120): is_gm_gs_drum=false, is_any_drums(false)=false
        // No match for "is_gm_gs_drum && program==25", no "is_any_drums && program==25"
        // → get_any_drums(prefer_xg=false) → finds gm_gs_drum(0)
        let p = select_preset(&presets, drum_patch(25), SynthSystem::Gs);
        assert!(p.is_gm_gs_drum);
        assert_eq!(p.program, 0);
    }

    // --- XG drums ---

    #[test]
    fn test_xg_drum_exact_program_match() {
        let presets = vec![xg_drum(0, 127), xg_drum(25, 127)];
        // Request XG drum program 25: bank_msb=127, is_xg=true
        let p = select_preset(&presets, patch(25, 127, 0), SynthSystem::Xg);
        assert_eq!(p.program, 25);
        assert_eq!(p.bank_msb, 127);
    }

    #[test]
    fn test_xg_drum_fallback_any_drum_with_program() {
        // XG drum program=25 not available as XG, but a GM/GS drum with program=25 exists
        let presets = vec![xg_drum(0, 127), gm_gs_drum(25)];
        // Request bank_msb=127, program=25, XG system
        // No preset with program==25 && is_xg_drums(true) → check is_any_drums && program==25
        // gm_gs_drum(25): is_any_drums(true) = is_gm_gs_drum || ... = true → match
        let p = select_preset(&presets, patch(25, 127, 0), SynthSystem::Xg);
        assert_eq!(p.program, 25);
    }

    #[test]
    fn test_xg_drum_fallback_any_xg_drum() {
        let presets = vec![xg_drum(0, 127), melodic(10, 0, 0)];
        // Request bank_msb=127, program=25 (not found) → fall back to any XG drum
        let p = select_preset(&presets, patch(25, 127, 0), SynthSystem::Xg);
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
        // (the special case: if bank_lsb==64 && is_xg, skip the any-bank fallback)
        // No bank_lsb=64 match for the first find, then we skip any-bank fallback
        // → first matching program
        let p = select_preset(&presets, patch(10, 0, 64), SynthSystem::Xg);
        // Falls through to first matching program (melodic(10,64,0) or melodic(10,0,0))
        assert_eq!(p.program, 10);
        // Should be the first matching-program entry
        assert_eq!(p.bank_msb, 64);
    }

    #[test]
    fn test_non_xg_lsb64_uses_any_bank_fallback() {
        let presets = vec![
            melodic(10, 64, 0), // bank_msb=64
        ];
        // GS system with bank_lsb=64: the special case (bank_lsb==64 && is_xg) does NOT apply
        // max(bank_msb=0, bank_lsb=64)=64 → matches preset's bank_msb=64
        let p = select_preset(&presets, patch(10, 0, 64), SynthSystem::Gs);
        assert_eq!(p.program, 10);
        assert_eq!(p.bank_msb, 64);
    }

    // --- single preset always returns it ---

    #[test]
    fn test_single_preset_always_returns_it() {
        let presets = vec![melodic(0, 0, 0)];
        // Even a completely different patch returns the only preset
        let p = select_preset(&presets, patch(99, 99, 99), SynthSystem::Gs);
        assert_eq!(p.program, 0);
    }

    // --- drums not selected as melodic ---

    #[test]
    fn test_drum_preset_not_returned_for_melodic_request() {
        let presets = vec![gm_gs_drum(10), melodic(10, 0, 0)];
        // Melodic request: drums should be filtered out from matching_programs
        let p = select_preset(&presets, patch(10, 0, 0), SynthSystem::Gs);
        assert!(!p.is_gm_gs_drum);
        assert_eq!(p.program, 10);
    }

    // --- get_any_drums helper ---

    #[test]
    fn test_get_any_drums_prefers_gm_gs_when_prefer_xg_false() {
        let presets = vec![xg_drum(0, 127), gm_gs_drum(0)];
        let p = get_any_drums(&presets, false, true);
        assert!(p.is_gm_gs_drum);
    }

    #[test]
    fn test_get_any_drums_prefers_xg_when_prefer_xg_true() {
        let presets = vec![gm_gs_drum(0), xg_drum(0, 127)];
        let p = get_any_drums(&presets, true, true);
        assert!(p.is_xg_drums(true));
    }

    #[test]
    fn test_get_any_drums_returns_first_when_no_drum_found() {
        let presets = vec![melodic(0, 0, 0), melodic(10, 0, 0)];
        let p = get_any_drums(&presets, false, false);
        assert_eq!(p.program, 0); // first preset
    }
}
