/// midi_hacks.rs
/// purpose: Bank select hacks for GS, XG, GM2 patch selection compatibility.
/// Ported from: src/utils/midi_hacks.ts
use crate::synthesizer::types::SynthSystem;

/// XG SFX Voice bank MSB.
/// Equivalent to: XG_SFX_VOICE
pub const XG_SFX_VOICE: u8 = 64;

/// GM2 default bank MSB.
/// Equivalent to: GM2_DEFAULT_BANK (module-level const in TS)
const GM2_DEFAULT_BANK: u8 = 121;

/// Bank-select compatibility helpers for GS, XG and GM2 MIDI systems.
/// All methods are pure functions with no state.
/// Equivalent to: BankSelectHacks (static class)
pub struct BankSelectHacks;

impl BankSelectHacks {
    /// Returns the default bank MSB for the given MIDI system.
    /// GM2 uses bank 121; all other systems use 0.
    /// Equivalent to: BankSelectHacks.getDefaultBank
    pub fn get_default_bank(sys: SynthSystem) -> u8 {
        if sys == SynthSystem::Gm2 {
            GM2_DEFAULT_BANK
        } else {
            0
        }
    }

    /// Returns the drum bank MSB for the given system.
    /// Returns `Some(120)` for GM2, `Some(127)` for XG.
    /// Returns `None` for systems that have no dedicated drum bank (GM, GS).
    /// Equivalent to: BankSelectHacks.getDrumBank (throws for GM/GS)
    pub fn get_drum_bank(sys: SynthSystem) -> Option<u8> {
        match sys {
            SynthSystem::Gm2 => Some(120),
            SynthSystem::Xg => Some(127),
            _ => None,
        }
    }

    /// Returns `true` if `bank_msb` corresponds to an XG drum bank (120 or 127).
    /// Equivalent to: BankSelectHacks.isXGDrums
    pub fn is_xg_drums(bank_msb: u8) -> bool {
        bank_msb == 120 || bank_msb == 127
    }

    /// Returns `true` if `bank_msb` is a valid XG MSB
    /// (XG drums, XG SFX voice, or GM2 default bank).
    /// Equivalent to: BankSelectHacks.isValidXGMSB
    pub fn is_valid_xg_msb(bank_msb: u8) -> bool {
        Self::is_xg_drums(bank_msb) || bank_msb == XG_SFX_VOICE || bank_msb == GM2_DEFAULT_BANK
    }

    /// Returns `true` if the system belongs to the XG family (GM2 or XG).
    /// Equivalent to: BankSelectHacks.isSystemXG
    pub fn is_system_xg(system: SynthSystem) -> bool {
        system == SynthSystem::Gm2 || system == SynthSystem::Xg
    }

    /// Adds `bank_offset` to `bank_msb`, clamped to 127.
    /// When `xg_drums` is `true`, XG drum banks (120, 127) are returned unchanged.
    /// Equivalent to: BankSelectHacks.addBankOffset
    pub fn add_bank_offset(bank_msb: u8, bank_offset: u8, xg_drums: bool) -> u8 {
        if xg_drums && Self::is_xg_drums(bank_msb) {
            return bank_msb;
        }
        (bank_msb as u16 + bank_offset as u16).min(127) as u8
    }

    /// Subtracts `bank_offset` from `bank_msb`, clamped to 0.
    /// When `xg_drums` is `true`, XG drum banks (120, 127) are returned unchanged.
    /// Equivalent to: BankSelectHacks.subtrackBankOffset (note: TS typo preserved)
    pub fn subtrak_bank_offset(bank_msb: u8, bank_offset: u8, xg_drums: bool) -> u8 {
        if xg_drums && Self::is_xg_drums(bank_msb) {
            return bank_msb;
        }
        bank_msb.saturating_sub(bank_offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- XG_SFX_VOICE ---

    #[test]
    fn test_xg_sfx_voice_value() {
        assert_eq!(XG_SFX_VOICE, 64);
    }

    // --- get_default_bank ---

    #[test]
    fn test_get_default_bank_gm2_returns_121() {
        assert_eq!(BankSelectHacks::get_default_bank(SynthSystem::Gm2), 121);
    }

    #[test]
    fn test_get_default_bank_gm_returns_0() {
        assert_eq!(BankSelectHacks::get_default_bank(SynthSystem::Gm), 0);
    }

    #[test]
    fn test_get_default_bank_gs_returns_0() {
        assert_eq!(BankSelectHacks::get_default_bank(SynthSystem::Gs), 0);
    }

    #[test]
    fn test_get_default_bank_xg_returns_0() {
        assert_eq!(BankSelectHacks::get_default_bank(SynthSystem::Xg), 0);
    }

    // --- get_drum_bank ---

    #[test]
    fn test_get_drum_bank_gm2_returns_120() {
        assert_eq!(BankSelectHacks::get_drum_bank(SynthSystem::Gm2), Some(120));
    }

    #[test]
    fn test_get_drum_bank_xg_returns_127() {
        assert_eq!(BankSelectHacks::get_drum_bank(SynthSystem::Xg), Some(127));
    }

    #[test]
    fn test_get_drum_bank_gm_returns_none() {
        assert_eq!(BankSelectHacks::get_drum_bank(SynthSystem::Gm), None);
    }

    #[test]
    fn test_get_drum_bank_gs_returns_none() {
        assert_eq!(BankSelectHacks::get_drum_bank(SynthSystem::Gs), None);
    }

    // --- is_xg_drums ---

    #[test]
    fn test_is_xg_drums_120_true() {
        assert!(BankSelectHacks::is_xg_drums(120));
    }

    #[test]
    fn test_is_xg_drums_127_true() {
        assert!(BankSelectHacks::is_xg_drums(127));
    }

    #[test]
    fn test_is_xg_drums_0_false() {
        assert!(!BankSelectHacks::is_xg_drums(0));
    }

    #[test]
    fn test_is_xg_drums_64_false() {
        assert!(!BankSelectHacks::is_xg_drums(64));
    }

    #[test]
    fn test_is_xg_drums_121_false() {
        assert!(!BankSelectHacks::is_xg_drums(121));
    }

    // --- is_valid_xg_msb ---

    #[test]
    fn test_is_valid_xg_msb_120_true() {
        assert!(BankSelectHacks::is_valid_xg_msb(120));
    }

    #[test]
    fn test_is_valid_xg_msb_127_true() {
        assert!(BankSelectHacks::is_valid_xg_msb(127));
    }

    #[test]
    fn test_is_valid_xg_msb_sfx_voice_true() {
        assert!(BankSelectHacks::is_valid_xg_msb(XG_SFX_VOICE));
    }

    #[test]
    fn test_is_valid_xg_msb_gm2_default_bank_true() {
        // GM2_DEFAULT_BANK = 121
        assert!(BankSelectHacks::is_valid_xg_msb(121));
    }

    #[test]
    fn test_is_valid_xg_msb_0_false() {
        assert!(!BankSelectHacks::is_valid_xg_msb(0));
    }

    #[test]
    fn test_is_valid_xg_msb_1_false() {
        assert!(!BankSelectHacks::is_valid_xg_msb(1));
    }

    // --- is_system_xg ---

    #[test]
    fn test_is_system_xg_gm2_true() {
        assert!(BankSelectHacks::is_system_xg(SynthSystem::Gm2));
    }

    #[test]
    fn test_is_system_xg_xg_true() {
        assert!(BankSelectHacks::is_system_xg(SynthSystem::Xg));
    }

    #[test]
    fn test_is_system_xg_gm_false() {
        assert!(!BankSelectHacks::is_system_xg(SynthSystem::Gm));
    }

    #[test]
    fn test_is_system_xg_gs_false() {
        assert!(!BankSelectHacks::is_system_xg(SynthSystem::Gs));
    }

    // --- add_bank_offset ---

    #[test]
    fn test_add_bank_offset_normal() {
        assert_eq!(BankSelectHacks::add_bank_offset(10, 5, true), 15);
    }

    #[test]
    fn test_add_bank_offset_clamps_to_127() {
        assert_eq!(BankSelectHacks::add_bank_offset(120, 20, false), 127);
    }

    #[test]
    fn test_add_bank_offset_xg_drums_preserved_when_flag_true() {
        // XG drum bank 120: returned unchanged
        assert_eq!(BankSelectHacks::add_bank_offset(120, 5, true), 120);
        // XG drum bank 127: returned unchanged
        assert_eq!(BankSelectHacks::add_bank_offset(127, 5, true), 127);
    }

    #[test]
    fn test_add_bank_offset_xg_drums_modified_when_flag_false() {
        // xg_drums=false: XG drum banks are treated like any other bank
        assert_eq!(BankSelectHacks::add_bank_offset(120, 5, false), 125);
    }

    #[test]
    fn test_add_bank_offset_zero_offset() {
        assert_eq!(BankSelectHacks::add_bank_offset(50, 0, true), 50);
    }

    #[test]
    fn test_add_bank_offset_result_exactly_127() {
        assert_eq!(BankSelectHacks::add_bank_offset(100, 27, true), 127);
    }

    // --- subtrak_bank_offset ---

    #[test]
    fn test_subtrak_bank_offset_normal() {
        assert_eq!(BankSelectHacks::subtrak_bank_offset(20, 5, true), 15);
    }

    #[test]
    fn test_subtrak_bank_offset_clamps_to_0() {
        assert_eq!(BankSelectHacks::subtrak_bank_offset(3, 10, false), 0);
    }

    #[test]
    fn test_subtrak_bank_offset_xg_drums_preserved_when_flag_true() {
        assert_eq!(BankSelectHacks::subtrak_bank_offset(120, 5, true), 120);
        assert_eq!(BankSelectHacks::subtrak_bank_offset(127, 5, true), 127);
    }

    #[test]
    fn test_subtrak_bank_offset_xg_drums_modified_when_flag_false() {
        assert_eq!(BankSelectHacks::subtrak_bank_offset(120, 5, false), 115);
    }

    #[test]
    fn test_subtrak_bank_offset_zero_offset() {
        assert_eq!(BankSelectHacks::subtrak_bank_offset(50, 0, true), 50);
    }

    #[test]
    fn test_subtrak_bank_offset_result_exactly_0() {
        assert_eq!(BankSelectHacks::subtrak_bank_offset(10, 10, true), 0);
    }
}
