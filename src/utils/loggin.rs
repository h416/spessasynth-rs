/// loggin.rs
/// purpose: Configurable logging output (info, warn, group) plus GM/GS/XG log helpers.
/// Ported from: src/utils/loggin.ts (spessasynth_core 4.3.0)
///
/// TS 4.3.0 turned the free functions `SpessaSynthLogging` / `SpessaSynthInfo` /
/// `SpessaSynthWarn` / `SpessaSynthGroup` / `SpessaSynthGroupCollapsed` /
/// `SpessaSynthGroupEnd` into static members of a `SpessaLog` class, and added several new
/// helper methods (`unsupported`, `gmInfo`/`gmFail`, `gsInfo`/`gsFail`, `xgInfo`/`xgFail`,
/// `coolInfo`) used to report GM/GS/XG-related SysEx handling. `SpessaLog` is ported below as
/// a zero-sized struct with associated functions (matching the existing `BankSelectHacks`
/// pattern used elsewhere in this crate for TS static classes).
///
/// Note: TS's `%c`-prefixed `ConsoleColors` styling is a browser DevTools feature with no
/// terminal equivalent; as with the pre-4.3.0 logging functions, colors are dropped and only
/// the plain message text is printed to stderr.
///
/// The original free functions (`spessa_synth_logging`, `spessa_synth_info`,
/// `spessa_synth_warn`, `spessa_synth_group`, `spessa_synth_group_collapsed`,
/// `spessa_synth_group_end`) are kept as thin backward-compatible wrappers because ~40 other
/// files across the crate (out of scope for this task) still call them; those call sites will
/// be migrated to `SpessaLog::...` as each file is ported in later phase-2 tasks.
use std::sync::atomic::{AtomicBool, Ordering};

use crate::utils::other::array_to_hex_string;

static INFO_ENABLED: AtomicBool = AtomicBool::new(false);
static WARN_ENABLED: AtomicBool = AtomicBool::new(true);
static GROUP_ENABLED: AtomicBool = AtomicBool::new(false);

/// Manage the log level of `spessasynth_core`.
/// Equivalent to: `class SpessaLog` in TypeScript.
pub struct SpessaLog;

impl SpessaLog {
    /// The most verbose log level, prints out a lot of small details.
    /// Equivalent to: `SpessaLog.infoEnabled`
    pub fn info_enabled() -> bool {
        INFO_ENABLED.load(Ordering::Relaxed)
    }

    /// The default log level, prints out warnings for unexpected and erroneous behavior.
    /// Equivalent to: `SpessaLog.warnEnabled`
    pub fn warn_enabled() -> bool {
        WARN_ENABLED.load(Ordering::Relaxed)
    }

    /// If grouping of the log messages is allowed. Recommended for the `info` verbosity level.
    /// Equivalent to: `SpessaLog.groupEnabled`
    pub fn group_enabled() -> bool {
        GROUP_ENABLED.load(Ordering::Relaxed)
    }

    /// Enables or disables logging.
    /// Equivalent to: `SpessaLog.setLogLevel(enableInfo, enableWarn, enableGroup)`
    pub fn set_log_level(enable_info: bool, enable_warn: bool, enable_group: bool) {
        INFO_ENABLED.store(enable_info, Ordering::Relaxed);
        WARN_ENABLED.store(enable_warn, Ordering::Relaxed);
        GROUP_ENABLED.store(enable_group, Ordering::Relaxed);
    }

    /// Equivalent to: `SpessaLog.info(...message)`
    pub fn info(message: &str) {
        if Self::info_enabled() {
            eprintln!("[SpessaSynth INFO] {message}");
        }
    }

    /// Equivalent to: `SpessaLog.warn(...message)`
    pub fn warn(message: &str) {
        if Self::warn_enabled() {
            eprintln!("[SpessaSynth WARN] {message}");
        }
    }

    /// Equivalent to: `SpessaLog.group(...message)`
    /// (console.group indentation has no terminal equivalent; prints as a regular message.)
    pub fn group(message: &str) {
        if Self::group_enabled() {
            eprintln!("[SpessaSynth GROUP ▶] {message}");
        }
    }

    /// Equivalent to: `SpessaLog.groupCollapsed(...message)`
    pub fn group_collapsed(message: &str) {
        if Self::group_enabled() {
            eprintln!("[SpessaSynth GROUP ▶] {message}");
        }
    }

    /// Equivalent to: `SpessaLog.groupEnd()`
    /// (console.groupEnd() has no terminal equivalent; this is a no-op.)
    pub fn group_end() {}

    /// Logs an "unsupported message" notice, including the hex dump of the SysEx data.
    /// Equivalent to: `SpessaLog.unsupported(what, syx, reason = "")`
    pub fn unsupported(what: &str, syx: &[u8], reason: &str) {
        if Self::info_enabled() {
            Self::info(&format!(
                "Unsupported {what} message: {}. {reason}",
                array_to_hex_string(syx)
            ));
        }
    }

    /// Equivalent to: `SpessaLog.gmInfo(what, value, unit = "")`
    pub fn gm_info(what: &str, value: impl std::fmt::Display, unit: &str) {
        if Self::info_enabled() {
            Self::cool_info(&format!("General MIDI {what}"), value, unit);
        }
    }

    /// Equivalent to: `SpessaLog.gmFail(what, syx)`
    pub fn gm_fail(what: &str, syx: &[u8]) {
        if Self::info_enabled() {
            Self::unsupported(&format!("General MIDI {what}"), syx, "");
        }
    }

    /// Equivalent to: `SpessaLog.gsInfo(what, value, unit = "")`
    pub fn gs_info(what: &str, value: impl std::fmt::Display, unit: &str) {
        if Self::info_enabled() {
            Self::cool_info(&format!("Roland GS {what}"), value, unit);
        }
    }

    /// Equivalent to: `SpessaLog.gsFail(what, syx, reason = "")`
    pub fn gs_fail(what: &str, syx: &[u8], reason: &str) {
        if Self::info_enabled() {
            Self::unsupported(&format!("Roland GS {what}"), syx, reason);
        }
    }

    /// Equivalent to: `SpessaLog.xgInfo(what, value, unit = "")`
    pub fn xg_info(what: &str, value: impl std::fmt::Display, unit: &str) {
        if Self::info_enabled() {
            Self::cool_info(&format!("Yamaha XG {what}"), value, unit);
        }
    }

    /// Equivalent to: `SpessaLog.xgFail(what, syx, reason = "")`
    pub fn xg_fail(what: &str, syx: &[u8], reason: &str) {
        if Self::info_enabled() {
            Self::unsupported(&format!("Yamaha XG {what}"), syx, reason);
        }
    }

    /// Equivalent to: `SpessaLog.coolInfo(what, value, unit = "")`
    pub fn cool_info(what: &str, value: impl std::fmt::Display, unit: &str) {
        if !Self::info_enabled() {
            return;
        }
        if !unit.is_empty() {
            Self::info(&format!("{what} is now set to {value} {unit}."));
        } else {
            Self::info(&format!("{what} is now set to {value}."));
        }
    }
}

// -----------------------------------------------------------------------------------------
// Backward-compatible free-function wrappers (pre-4.3.0 API shape).
// See the module doc comment above for why these are kept.
// -----------------------------------------------------------------------------------------

/// Enables or disables each logging category globally.
/// Equivalent to: `SpessaLog.setLogLevel`
pub fn spessa_synth_logging(enable_info: bool, enable_warn: bool, enable_group: bool) {
    SpessaLog::set_log_level(enable_info, enable_warn, enable_group);
}

/// Logs an info message to stderr if info logging is enabled.
/// Equivalent to: `SpessaLog.info`
pub fn spessa_synth_info(message: &str) {
    SpessaLog::info(message);
}

/// Logs a warning message to stderr if warn logging is enabled.
/// Equivalent to: `SpessaLog.warn`
pub fn spessa_synth_warn(message: &str) {
    SpessaLog::warn(message);
}

/// Opens a log group to stderr if group logging is enabled.
/// Equivalent to: `SpessaLog.group`
pub fn spessa_synth_group(message: &str) {
    SpessaLog::group(message);
}

/// Opens a collapsed log group to stderr if group logging is enabled.
/// Equivalent to: `SpessaLog.groupCollapsed`
pub fn spessa_synth_group_collapsed(message: &str) {
    SpessaLog::group_collapsed(message);
}

/// Closes a log group.
/// Equivalent to: `SpessaLog.groupEnd`
pub fn spessa_synth_group_end() {
    SpessaLog::group_end();
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- flag setter ---

    #[test]
    fn test_logging_sets_info_on() {
        spessa_synth_logging(true, false, false);
        assert!(SpessaLog::info_enabled());
        assert!(!SpessaLog::warn_enabled());
        assert!(!SpessaLog::group_enabled());
    }

    #[test]
    fn test_logging_sets_warn_on() {
        spessa_synth_logging(false, true, false);
        assert!(!SpessaLog::info_enabled());
        assert!(SpessaLog::warn_enabled());
        assert!(!SpessaLog::group_enabled());
    }

    #[test]
    fn test_logging_sets_group_on() {
        spessa_synth_logging(false, false, true);
        assert!(!SpessaLog::info_enabled());
        assert!(!SpessaLog::warn_enabled());
        assert!(SpessaLog::group_enabled());
    }

    #[test]
    fn test_logging_sets_all_on() {
        spessa_synth_logging(true, true, true);
        assert!(SpessaLog::info_enabled());
        assert!(SpessaLog::warn_enabled());
        assert!(SpessaLog::group_enabled());
    }

    #[test]
    fn test_logging_sets_all_off() {
        spessa_synth_logging(false, false, false);
        assert!(!SpessaLog::info_enabled());
        assert!(!SpessaLog::warn_enabled());
        assert!(!SpessaLog::group_enabled());
    }

    // --- set_log_level (new 4.3.0 name) ---

    #[test]
    fn test_set_log_level_matches_legacy_setter() {
        SpessaLog::set_log_level(true, false, true);
        assert!(SpessaLog::info_enabled());
        assert!(!SpessaLog::warn_enabled());
        assert!(SpessaLog::group_enabled());
    }

    // --- no-panic when all disabled ---

    #[test]
    fn test_all_functions_no_panic_when_disabled() {
        spessa_synth_logging(false, false, false);
        spessa_synth_info("info msg");
        spessa_synth_warn("warn msg");
        spessa_synth_group("group msg");
        spessa_synth_group_collapsed("collapsed msg");
        spessa_synth_group_end();
    }

    // --- no-panic when all enabled ---

    #[test]
    fn test_all_functions_no_panic_when_enabled() {
        spessa_synth_logging(true, true, true);
        spessa_synth_info("info msg");
        spessa_synth_warn("warn msg");
        spessa_synth_group("group msg");
        spessa_synth_group_collapsed("collapsed msg");
        spessa_synth_group_end();
    }

    // --- group_end is always a no-op ---

    #[test]
    fn test_group_end_is_noop() {
        // Should not panic regardless of flag state.
        spessa_synth_logging(true, true, true);
        spessa_synth_group_end();
        spessa_synth_logging(false, false, false);
        spessa_synth_group_end();
    }

    // --- new 4.3.0 GM/GS/XG helper methods: no-panic sanity checks ---

    #[test]
    fn test_unsupported_no_panic_enabled() {
        SpessaLog::set_log_level(true, true, false);
        SpessaLog::unsupported("Test", &[0x41, 0x10, 0x42], "reason");
    }

    #[test]
    fn test_unsupported_no_panic_disabled() {
        SpessaLog::set_log_level(false, false, false);
        SpessaLog::unsupported("Test", &[0x41, 0x10, 0x42], "reason");
    }

    #[test]
    fn test_gm_info_no_panic() {
        SpessaLog::set_log_level(true, true, false);
        SpessaLog::gm_info("System", "on", "");
        SpessaLog::gm_info("Master Volume", 100, "%");
    }

    #[test]
    fn test_gm_fail_no_panic() {
        SpessaLog::set_log_level(true, true, false);
        SpessaLog::gm_fail("SysEx", &[0x7e, 0x00, 0x09, 0x01]);
    }

    #[test]
    fn test_gs_info_no_panic() {
        SpessaLog::set_log_level(true, true, false);
        SpessaLog::gs_info("Reverb", "Hall", "");
    }

    #[test]
    fn test_gs_fail_no_panic() {
        SpessaLog::set_log_level(true, true, false);
        SpessaLog::gs_fail("SysEx", &[0x41, 0x10, 0x42], "unsupported param");
    }

    #[test]
    fn test_xg_info_no_panic() {
        SpessaLog::set_log_level(true, true, false);
        SpessaLog::xg_info("Effect", "Chorus", "");
    }

    #[test]
    fn test_xg_fail_no_panic() {
        SpessaLog::set_log_level(true, true, false);
        SpessaLog::xg_fail("SysEx", &[0x43, 0x10, 0x4c], "unsupported param");
    }

    #[test]
    fn test_cool_info_with_unit() {
        SpessaLog::set_log_level(true, true, false);
        SpessaLog::cool_info("Master Volume", 100, "%");
    }

    #[test]
    fn test_cool_info_without_unit() {
        SpessaLog::set_log_level(true, true, false);
        SpessaLog::cool_info("System", "GS", "");
    }

    #[test]
    fn test_cool_info_disabled_does_not_panic() {
        SpessaLog::set_log_level(false, true, false);
        SpessaLog::cool_info("System", "GS", "");
    }
}
