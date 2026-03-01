/// other.rs
/// purpose: Miscellaneous utility functions.
/// Ported from: src/utils/other.ts
/// Return value of format_time().
/// Equivalent to: { minutes, seconds, time }
pub struct FormattedTime {
    pub minutes: u32,
    pub seconds: u32,
    pub time: String,
}

/// Formats the given seconds into a readable MM:SS string.
/// Equivalent to: formatTime
pub fn format_time(total_seconds: f64) -> FormattedTime {
    let total_seconds = total_seconds.floor() as u32;
    let minutes = total_seconds / 60;
    let seconds = total_seconds - minutes * 60;
    FormattedTime {
        minutes,
        seconds,
        time: format!("{:02}:{:02}", minutes, seconds),
    }
}

/// Converts a byte slice to a space-separated uppercase hex string.
/// Equivalent to: arrayToHexString
pub fn array_to_hex_string(arr: &[u8]) -> String {
    let mut hex_string = String::new();
    for &byte in arr {
        hex_string.push_str(&format!("{:02X} ", byte));
    }
    hex_string
}

/// CSS color strings for console output (browser-specific, kept for completeness).
/// Equivalent to: consoleColors
pub mod console_colors {
    pub const WARN: &str = "color: orange;";
    pub const UNRECOGNIZED: &str = "color: red;";
    pub const INFO: &str = "color: aqua;";
    pub const RECOGNIZED: &str = "color: lime";
    pub const VALUE: &str = "color: yellow; background-color: black;";
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- format_time ---

    #[test]
    fn test_format_time_zero() {
        let t = format_time(0.0);
        assert_eq!(t.minutes, 0);
        assert_eq!(t.seconds, 0);
        assert_eq!(t.time, "00:00");
    }

    #[test]
    fn test_format_time_one_minute_thirty() {
        let t = format_time(90.0);
        assert_eq!(t.minutes, 1);
        assert_eq!(t.seconds, 30);
        assert_eq!(t.time, "01:30");
    }

    #[test]
    fn test_format_time_exactly_one_minute() {
        let t = format_time(60.0);
        assert_eq!(t.minutes, 1);
        assert_eq!(t.seconds, 0);
        assert_eq!(t.time, "01:00");
    }

    #[test]
    fn test_format_time_59_seconds() {
        let t = format_time(59.0);
        assert_eq!(t.minutes, 0);
        assert_eq!(t.seconds, 59);
        assert_eq!(t.time, "00:59");
    }

    #[test]
    fn test_format_time_floors_fractional() {
        // 90.9 → floor → 90 → 1m 30s
        let t = format_time(90.9);
        assert_eq!(t.minutes, 1);
        assert_eq!(t.seconds, 30);
        assert_eq!(t.time, "01:30");
    }

    #[test]
    fn test_format_time_large_value() {
        // 3661 s = 61m 1s
        let t = format_time(3661.0);
        assert_eq!(t.minutes, 61);
        assert_eq!(t.seconds, 1);
        assert_eq!(t.time, "61:01");
    }

    #[test]
    fn test_format_time_pads_single_digit_minutes() {
        let t = format_time(65.0); // 1m 5s
        assert_eq!(t.time, "01:05");
    }

    #[test]
    fn test_format_time_pads_single_digit_seconds() {
        let t = format_time(601.0); // 10m 1s
        assert_eq!(t.time, "10:01");
    }

    // --- array_to_hex_string ---

    #[test]
    fn test_hex_string_empty() {
        assert_eq!(array_to_hex_string(&[]), "");
    }

    #[test]
    fn test_hex_string_single_byte_zero() {
        assert_eq!(array_to_hex_string(&[0x00]), "00 ");
    }

    #[test]
    fn test_hex_string_single_byte_ff() {
        assert_eq!(array_to_hex_string(&[0xFF]), "FF ");
    }

    #[test]
    fn test_hex_string_multiple_bytes() {
        assert_eq!(array_to_hex_string(&[0x00, 0xFF, 0xAB]), "00 FF AB ");
    }

    #[test]
    fn test_hex_string_uppercase() {
        // Letters must be uppercase
        assert_eq!(array_to_hex_string(&[0xde, 0xad]), "DE AD ");
    }

    #[test]
    fn test_hex_string_pads_single_nibble() {
        // 0x0F → "0F"
        assert_eq!(array_to_hex_string(&[0x0F]), "0F ");
    }

    #[test]
    fn test_hex_string_trailing_space() {
        // Each byte is followed by a space, including the last
        let s = array_to_hex_string(&[0x01, 0x02]);
        assert!(s.ends_with(' '));
    }

    // --- console_colors ---

    #[test]
    fn test_console_colors_warn() {
        assert_eq!(console_colors::WARN, "color: orange;");
    }

    #[test]
    fn test_console_colors_info() {
        assert_eq!(console_colors::INFO, "color: aqua;");
    }
}
