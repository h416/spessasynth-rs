/// sysex_detector.rs
/// purpose: Detects specific SysEx messages (XG, GS, GM) by inspecting data bytes.
/// Ported from: src/utils/sysex_detector.ts
use crate::midi::midi_message::MidiMessage;

/// Checks if this is a XG ON system exclusive.
/// Equivalent to: isXGOn
pub fn is_xg_on(msg: &MidiMessage) -> bool {
    let d = &msg.data;
    d.len() >= 7
        && d[0] == 0x43 // Yamaha
        && d[2] == 0x4c // XG ON
        && d[5] == 0x7e
        && d[6] == 0x00
}

/// Checks if this is a GS Drum part system exclusive.
/// Equivalent to: isGSDrumsOn
pub fn is_gs_drums_on(msg: &MidiMessage) -> bool {
    let d = &msg.data;
    d.len() >= 7
        && d[0] == 0x41         // Roland
        && d[2] == 0x42         // GS
        && d[3] == 0x12         // GS
        && d[4] == 0x40         // System parameter
        && (d[5] & 0x10) != 0   // Part parameter
        && d[6] == 0x15 // Drum parts
}

/// Checks if this is a GS ON system exclusive.
/// Equivalent to: isGSOn
pub fn is_gs_on(msg: &MidiMessage) -> bool {
    let d = &msg.data;
    d.len() >= 7
        && d[0] == 0x41 // Roland
        && d[2] == 0x42 // GS
        && d[6] == 0x7f // Mode set
}

/// Checks if this is a GM ON system exclusive.
/// Equivalent to: isGMOn
pub fn is_gm_on(msg: &MidiMessage) -> bool {
    let d = &msg.data;
    d.len() >= 4
        && d[0] == 0x7e // Non realtime
        && d[2] == 0x09 // GM system
        && d[3] == 0x01 // GM1
}

/// Checks if this is a GM2 ON system exclusive.
/// Equivalent to: isGM2On
pub fn is_gm2_on(msg: &MidiMessage) -> bool {
    let d = &msg.data;
    d.len() >= 4
        && d[0] == 0x7e // Non realtime
        && d[2] == 0x09 // GM system
        && d[3] == 0x03 // GM2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sysex(data: Vec<u8>) -> MidiMessage {
        MidiMessage::new(0, 0xF0, data)
    }

    // --- is_xg_on ---

    #[test]
    fn test_xg_on_valid() {
        // [Yamaha, ?, XG_ON, ?, ?, 0x7e, 0x00]
        assert!(is_xg_on(&sysex(vec![
            0x43, 0x00, 0x4c, 0x00, 0x00, 0x7e, 0x00
        ])));
    }

    #[test]
    fn test_xg_on_wrong_manufacturer() {
        assert!(!is_xg_on(&sysex(vec![
            0x41, 0x00, 0x4c, 0x00, 0x00, 0x7e, 0x00
        ])));
    }

    #[test]
    fn test_xg_on_wrong_byte2() {
        assert!(!is_xg_on(&sysex(vec![
            0x43, 0x00, 0x10, 0x00, 0x00, 0x7e, 0x00
        ])));
    }

    #[test]
    fn test_xg_on_wrong_byte5() {
        assert!(!is_xg_on(&sysex(vec![
            0x43, 0x00, 0x4c, 0x00, 0x00, 0x00, 0x00
        ])));
    }

    #[test]
    fn test_xg_on_wrong_byte6() {
        assert!(!is_xg_on(&sysex(vec![
            0x43, 0x00, 0x4c, 0x00, 0x00, 0x7e, 0x01
        ])));
    }

    #[test]
    fn test_xg_on_too_short() {
        assert!(!is_xg_on(&sysex(vec![0x43, 0x00, 0x4c])));
    }

    // --- is_gs_drums_on ---

    #[test]
    fn test_gs_drums_on_valid() {
        // byte5 = 0x10: bit 4 is set
        assert!(is_gs_drums_on(&sysex(vec![
            0x41, 0x00, 0x42, 0x12, 0x40, 0x10, 0x15
        ])));
    }

    #[test]
    fn test_gs_drums_on_byte5_other_bit_set() {
        // byte5 = 0x30 (bits 4 and 5 set) → still valid
        assert!(is_gs_drums_on(&sysex(vec![
            0x41, 0x00, 0x42, 0x12, 0x40, 0x30, 0x15
        ])));
    }

    #[test]
    fn test_gs_drums_on_byte5_bit_not_set() {
        // byte5 = 0x05 (bit 4 not set) → false
        assert!(!is_gs_drums_on(&sysex(vec![
            0x41, 0x00, 0x42, 0x12, 0x40, 0x05, 0x15
        ])));
    }

    #[test]
    fn test_gs_drums_on_wrong_manufacturer() {
        assert!(!is_gs_drums_on(&sysex(vec![
            0x43, 0x00, 0x42, 0x12, 0x40, 0x10, 0x15
        ])));
    }

    #[test]
    fn test_gs_drums_on_wrong_byte6() {
        assert!(!is_gs_drums_on(&sysex(vec![
            0x41, 0x00, 0x42, 0x12, 0x40, 0x10, 0x00
        ])));
    }

    #[test]
    fn test_gs_drums_on_too_short() {
        assert!(!is_gs_drums_on(&sysex(vec![0x41, 0x00, 0x42])));
    }

    // --- is_gs_on ---

    #[test]
    fn test_gs_on_valid() {
        assert!(is_gs_on(&sysex(vec![
            0x41, 0x00, 0x42, 0x00, 0x00, 0x00, 0x7f
        ])));
    }

    #[test]
    fn test_gs_on_wrong_manufacturer() {
        assert!(!is_gs_on(&sysex(vec![
            0x43, 0x00, 0x42, 0x00, 0x00, 0x00, 0x7f
        ])));
    }

    #[test]
    fn test_gs_on_wrong_byte2() {
        assert!(!is_gs_on(&sysex(vec![
            0x41, 0x00, 0x10, 0x00, 0x00, 0x00, 0x7f
        ])));
    }

    #[test]
    fn test_gs_on_wrong_byte6() {
        assert!(!is_gs_on(&sysex(vec![
            0x41, 0x00, 0x42, 0x00, 0x00, 0x00, 0x00
        ])));
    }

    #[test]
    fn test_gs_on_too_short() {
        assert!(!is_gs_on(&sysex(vec![0x41, 0x00, 0x42])));
    }

    // --- is_gm_on ---

    #[test]
    fn test_gm_on_valid() {
        assert!(is_gm_on(&sysex(vec![0x7e, 0x00, 0x09, 0x01])));
    }

    #[test]
    fn test_gm_on_wrong_byte0() {
        assert!(!is_gm_on(&sysex(vec![0x7f, 0x00, 0x09, 0x01])));
    }

    #[test]
    fn test_gm_on_wrong_byte2() {
        assert!(!is_gm_on(&sysex(vec![0x7e, 0x00, 0x08, 0x01])));
    }

    #[test]
    fn test_gm_on_gm2_byte_is_false() {
        // GM2 has 0x03, not 0x01
        assert!(!is_gm_on(&sysex(vec![0x7e, 0x00, 0x09, 0x03])));
    }

    #[test]
    fn test_gm_on_too_short() {
        assert!(!is_gm_on(&sysex(vec![0x7e, 0x00, 0x09])));
    }

    // --- is_gm2_on ---

    #[test]
    fn test_gm2_on_valid() {
        assert!(is_gm2_on(&sysex(vec![0x7e, 0x00, 0x09, 0x03])));
    }

    #[test]
    fn test_gm2_on_wrong_byte3() {
        // GM1 byte → false for GM2 check
        assert!(!is_gm2_on(&sysex(vec![0x7e, 0x00, 0x09, 0x01])));
    }

    #[test]
    fn test_gm2_on_wrong_byte0() {
        assert!(!is_gm2_on(&sysex(vec![0x41, 0x00, 0x09, 0x03])));
    }

    #[test]
    fn test_gm2_on_too_short() {
        assert!(!is_gm2_on(&sysex(vec![0x7e, 0x00])));
    }

    // --- cross checks: each detector rejects others' messages ---

    #[test]
    fn test_gm_on_does_not_match_gm2() {
        let gm2 = sysex(vec![0x7e, 0x00, 0x09, 0x03]);
        assert!(!is_gm_on(&gm2));
    }

    #[test]
    fn test_gm2_on_does_not_match_gm() {
        let gm = sysex(vec![0x7e, 0x00, 0x09, 0x01]);
        assert!(!is_gm2_on(&gm));
    }
}
