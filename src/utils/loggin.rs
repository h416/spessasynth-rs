/// loggin.rs
/// purpose: Configurable logging output (info, warn, group).
/// Ported from: src/utils/loggin.ts
use std::sync::atomic::{AtomicBool, Ordering};

static ENABLE_INFO: AtomicBool = AtomicBool::new(false);
static ENABLE_WARN: AtomicBool = AtomicBool::new(true);
static ENABLE_GROUP: AtomicBool = AtomicBool::new(false);

/// Enables or disables each logging category globally.
/// Equivalent to: SpessaSynthLogging
pub fn spessa_synth_logging(enable_info: bool, enable_warn: bool, enable_group: bool) {
    ENABLE_INFO.store(enable_info, Ordering::Relaxed);
    ENABLE_WARN.store(enable_warn, Ordering::Relaxed);
    ENABLE_GROUP.store(enable_group, Ordering::Relaxed);
}

/// Logs an info message to stderr if info logging is enabled.
/// Equivalent to: SpessaSynthInfo
pub fn spessa_synth_info(message: &str) {
    if ENABLE_INFO.load(Ordering::Relaxed) {
        eprintln!("[SpessaSynth INFO] {message}");
    }
}

/// Logs a warning message to stderr if warn logging is enabled.
/// Equivalent to: SpessaSynthWarn
pub fn spessa_synth_warn(message: &str) {
    if ENABLE_WARN.load(Ordering::Relaxed) {
        eprintln!("[SpessaSynth WARN] {message}");
    }
}

/// Opens a log group to stderr if group logging is enabled.
/// Equivalent to: SpessaSynthGroup
/// (console.group indentation has no terminal equivalent; prints as a regular message.)
pub fn spessa_synth_group(message: &str) {
    if ENABLE_GROUP.load(Ordering::Relaxed) {
        eprintln!("[SpessaSynth GROUP ▶] {message}");
    }
}

/// Opens a collapsed log group to stderr if group logging is enabled.
/// Equivalent to: SpessaSynthGroupCollapsed
pub fn spessa_synth_group_collapsed(message: &str) {
    if ENABLE_GROUP.load(Ordering::Relaxed) {
        eprintln!("[SpessaSynth GROUP ▶] {message}");
    }
}

/// Closes a log group.
/// Equivalent to: SpessaSynthGroupEnd
/// (console.groupEnd() has no terminal equivalent; this is a no-op.)
pub fn spessa_synth_group_end() {}

#[cfg(test)]
mod tests {
    use super::*;

    // --- flag setter ---

    #[test]
    fn test_logging_sets_info_on() {
        spessa_synth_logging(true, false, false);
        assert!(ENABLE_INFO.load(Ordering::Relaxed));
        assert!(!ENABLE_WARN.load(Ordering::Relaxed));
        assert!(!ENABLE_GROUP.load(Ordering::Relaxed));
    }

    #[test]
    fn test_logging_sets_warn_on() {
        spessa_synth_logging(false, true, false);
        assert!(!ENABLE_INFO.load(Ordering::Relaxed));
        assert!(ENABLE_WARN.load(Ordering::Relaxed));
        assert!(!ENABLE_GROUP.load(Ordering::Relaxed));
    }

    #[test]
    fn test_logging_sets_group_on() {
        spessa_synth_logging(false, false, true);
        assert!(!ENABLE_INFO.load(Ordering::Relaxed));
        assert!(!ENABLE_WARN.load(Ordering::Relaxed));
        assert!(ENABLE_GROUP.load(Ordering::Relaxed));
    }

    #[test]
    fn test_logging_sets_all_on() {
        spessa_synth_logging(true, true, true);
        assert!(ENABLE_INFO.load(Ordering::Relaxed));
        assert!(ENABLE_WARN.load(Ordering::Relaxed));
        assert!(ENABLE_GROUP.load(Ordering::Relaxed));
    }

    #[test]
    fn test_logging_sets_all_off() {
        spessa_synth_logging(false, false, false);
        assert!(!ENABLE_INFO.load(Ordering::Relaxed));
        assert!(!ENABLE_WARN.load(Ordering::Relaxed));
        assert!(!ENABLE_GROUP.load(Ordering::Relaxed));
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
}
