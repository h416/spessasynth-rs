/// other.rs
/// purpose: Miscellaneous utility functions.
/// Ported from: src/utils/other.ts (spessasynth_core 4.3.0)
///
/// Note: TS 4.3.0 renamed `consoleColors` to `ConsoleColors` (naming-only, values unchanged).
/// Rust already uses the idiomatic `console_colors` module with SCREAMING_SNAKE_CASE constants,
/// so no change was needed for that rename.
/// Seedable, deterministic random generator (new in 4.3.16).
/// Source - https://stackoverflow.com/a/47593316
///
/// The TypeScript version is a closure over a single `number` held in module scope
/// (`export const randomGenerator = splitmix32(81_572)`), shared by every processor in the
/// module instance. Rust stores one instance per `SynthesizerCore` instead: for the
/// one-synthesizer-per-process rendering flow both are equivalent, and a per-instance state
/// keeps renders reproducible when several synthesizers live in the same process.
///
/// JS uses `| 0` (int32) and `>>> ` (logical shift on the 32-bit pattern) plus `Math.imul`
/// (low 32 bits of the product). The bit patterns are identical to `u32` wrapping
/// arithmetic, so this produces the exact same sequence.
///
/// Equivalent to: splitmix32(a)
#[derive(Clone, Debug)]
pub struct SplitMix32 {
    state: u32,
}

/// Seed used by the upstream `randomGenerator` export.
/// Equivalent to: splitmix32(81_572)
pub const RANDOM_GENERATOR_SEED: u32 = 81_572;

impl SplitMix32 {
    /// Creates a generator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// Returns the next value in `[0, 1)`.
    /// Equivalent to: randomGenerator()
    pub fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x9E37_79B9);
        let mut t = self.state ^ (self.state >> 16);
        t = t.wrapping_mul(0x21F0_AAAD);
        t ^= t >> 15;
        t = t.wrapping_mul(0x735A_2D97);
        f64::from(t ^ (t >> 15)) / 4_294_967_296.0
    }
}

impl Default for SplitMix32 {
    fn default() -> Self {
        Self::new(RANDOM_GENERATOR_SEED)
    }
}

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
/// Note: 4.3.0 changed this to only insert a space *between* elements (no trailing space
/// after the last byte); 4.2.0 always appended a trailing space.
/// Equivalent to: arrayToHexString
pub fn array_to_hex_string(arr: &[u8]) -> String {
    let mut hex_string = String::new();
    for (i, &byte) in arr.iter().enumerate() {
        hex_string.push_str(&format!("{:02X}", byte));
        if i < arr.len() - 1 {
            hex_string.push(' ');
        }
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
        assert_eq!(array_to_hex_string(&[0x00]), "00");
    }

    #[test]
    fn test_hex_string_single_byte_ff() {
        assert_eq!(array_to_hex_string(&[0xFF]), "FF");
    }

    #[test]
    fn test_hex_string_multiple_bytes() {
        assert_eq!(array_to_hex_string(&[0x00, 0xFF, 0xAB]), "00 FF AB");
    }

    #[test]
    fn test_hex_string_uppercase() {
        // Letters must be uppercase
        assert_eq!(array_to_hex_string(&[0xde, 0xad]), "DE AD");
    }

    #[test]
    fn test_hex_string_pads_single_nibble() {
        // 0x0F → "0F"
        assert_eq!(array_to_hex_string(&[0x0F]), "0F");
    }

    #[test]
    fn test_hex_string_no_trailing_space() {
        // 4.3.0: space is only inserted *between* elements, not after the last one
        let s = array_to_hex_string(&[0x01, 0x02]);
        assert!(!s.ends_with(' '));
        assert_eq!(s, "01 02");
    }

    // --- SplitMix32 ---

    #[test]
    fn test_split_mix32_matches_typescript_sequence() {
        // Reference values produced by the upstream 4.3.16 `randomGenerator` export
        // (splitmix32(81_572)) under Node.
        let mut g = SplitMix32::default();
        let expected = [
            0.416_167_726_507_410_41_f64,
            0.887_790_377_018_973_23,
            0.102_066_878_462_210_30,
            0.657_098_128_693_178_30,
            0.305_592_403_514_310_72,
            0.907_971_704_611_554_74,
        ];
        for (i, &e) in expected.iter().enumerate() {
            assert_eq!(g.next_f64(), e, "value {} differs", i);
        }
    }

    #[test]
    fn test_split_mix32_random_pan_values() {
        // The note_on random pan formula: Math.round(randomGenerator() * 1000 - 500).
        // `Math.round` is floor(x + 0.5), which differs from Rust's `f64::round`
        // (half away from zero) on exact .5 ties at negative values.
        let mut g = SplitMix32::default();
        let pans: Vec<i32> = (0..6)
            .map(|_| (g.next_f64() * 1000.0 - 500.0 + 0.5).floor() as i32)
            .collect();
        assert_eq!(pans, vec![-84, 388, -398, 157, -194, 408]);
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
