/// helpers.rs
/// purpose: Helper functions for SysEx log output.
/// Ported from: src/synthesizer/audio_engine/engine_methods/system_exclusive/helpers.ts
use crate::utils::loggin::spessa_synth_info;
use crate::utils::other::array_to_hex_string;

/// Outputs an informational log for a SysEx operation.
/// TypeScript's `SysExAcceptedArray` is a union of various typed arrays,
/// but in Rust it is unified as raw bytes `&[u8]`.
/// Equivalent to: sysExLogging
pub fn sys_ex_logging(
    syx: &[u8],
    channel: u8,
    value: &dyn std::fmt::Display,
    what: &str,
    units: &str,
) {
    spessa_synth_info(&format!(
        "Channel {} {}. {} {}, with {}",
        channel,
        what,
        value,
        units,
        array_to_hex_string(syx),
    ));
}

/// Outputs an informational log for an unrecognized SysEx message.
/// Equivalent to: sysExNotRecognized
pub fn sys_ex_not_recognized(syx: &[u8], what: &str) {
    spessa_synth_info(&format!(
        "Unrecognized {} SysEx: {}",
        what,
        array_to_hex_string(syx),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::loggin::spessa_synth_logging;

    // --- sys_ex_logging: verify no panic ---

    #[test]
    fn test_logging_empty_syx_does_not_panic() {
        spessa_synth_logging(true, true, false);
        sys_ex_logging(&[], 0, &"0", "master volume", "");
    }

    #[test]
    fn test_logging_with_bytes_does_not_panic() {
        spessa_synth_logging(true, true, false);
        sys_ex_logging(&[0x41, 0x10, 0x42, 0x12], 1, &100u8, "reverb level", "");
    }

    #[test]
    fn test_logging_numeric_value_does_not_panic() {
        spessa_synth_logging(true, true, false);
        sys_ex_logging(&[0xF0, 0x7E, 0xF7], 9, &3.14f64, "pitch bend", "semitones");
    }

    #[test]
    fn test_logging_string_value_does_not_panic() {
        spessa_synth_logging(true, true, false);
        sys_ex_logging(&[0x00, 0xFF], 15, &"on", "reverb", "");
    }

    #[test]
    fn test_logging_channel_max_does_not_panic() {
        spessa_synth_logging(true, true, false);
        sys_ex_logging(&[0x7F], 255, &127i32, "pan", "");
    }

    #[test]
    fn test_logging_disabled_does_not_panic() {
        // Should not panic even when info logging is disabled
        spessa_synth_logging(false, false, false);
        sys_ex_logging(&[0x01, 0x02, 0x03], 5, &42i32, "chorus", "level");
    }

    // --- sys_ex_not_recognized: verify no panic ---

    #[test]
    fn test_not_recognized_empty_syx_does_not_panic() {
        spessa_synth_logging(true, true, false);
        sys_ex_not_recognized(&[], "GM");
    }

    #[test]
    fn test_not_recognized_gs_does_not_panic() {
        spessa_synth_logging(true, true, false);
        sys_ex_not_recognized(&[0x41, 0x10, 0x42], "GS");
    }

    #[test]
    fn test_not_recognized_xg_does_not_panic() {
        spessa_synth_logging(true, true, false);
        sys_ex_not_recognized(&[0x43, 0x10, 0x4C], "XG");
    }

    #[test]
    fn test_not_recognized_disabled_does_not_panic() {
        spessa_synth_logging(false, false, false);
        sys_ex_not_recognized(&[0xFF], "unknown");
    }

    // --- Format content verification ---
    // Log output goes to stderr and cannot be captured directly, but
    // we verify the format indirectly by testing array_to_hex_string output.

    #[test]
    fn test_hex_included_in_format() {
        use crate::utils::other::array_to_hex_string;
        // Verify the hex string that sys_ex_logging should produce internally
        let syx = &[0x41u8, 0x10, 0x42];
        let hex = array_to_hex_string(syx);
        assert!(hex.contains("41"), "hex={hex}");
        assert!(hex.contains("10"), "hex={hex}");
        assert!(hex.contains("42"), "hex={hex}");
    }

    #[test]
    fn test_hex_empty_syx() {
        use crate::utils::other::array_to_hex_string;
        let hex = array_to_hex_string(&[]);
        assert_eq!(hex, "");
    }

    // --- All 16 channels should not panic ---

    #[test]
    fn test_all_channels_no_panic() {
        spessa_synth_logging(true, true, false);
        for ch in 0u8..=15 {
            sys_ex_logging(&[0x7F], ch, &ch, "test", "");
        }
    }
}
