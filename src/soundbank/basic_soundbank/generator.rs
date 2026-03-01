/// generator.rs
/// purpose: SoundFont2 Generator struct (type + 16-bit value).
/// Ported from: src/soundbank/basic_soundbank/generator.ts
use std::fmt;

use crate::soundbank::basic_soundbank::generator_types::{GENERATOR_LIMITS, GeneratorType};
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::write_word;

/// Byte size of one generator record in SF2 (type WORD + value WORD).
/// Equivalent to: GEN_BYTE_SIZE = 4
pub const GEN_BYTE_SIZE: usize = 4;

/// Represents a single SoundFont2 generator.
/// Equivalent to: Generator
#[derive(Clone, Debug, PartialEq)]
pub struct Generator {
    /// The generator's SF2 type.
    /// Equivalent to: generatorType
    pub generator_type: GeneratorType,

    /// The generator's 16-bit value.
    /// Equivalent to: generatorValue
    pub generator_value: i16,
}

impl Generator {
    /// Constructs a new Generator with limit validation (clamps value to defined limits).
    /// Equivalent to: new Generator(type, value) or new Generator(type, value, true)
    pub fn new(generator_type: GeneratorType, value: f64) -> Self {
        Self::new_inner(generator_type, value, true)
    }

    /// Constructs a new Generator without limit validation.
    /// Equivalent to: new Generator(type, value, false)
    pub fn new_unvalidated(generator_type: GeneratorType, value: f64) -> Self {
        Self::new_inner(generator_type, value, false)
    }

    fn new_inner(generator_type: GeneratorType, value: f64, validate: bool) -> Self {
        // Note: Math.round() in JS rounds half-up; Rust f64::round() rounds half away from zero.
        // For integer inputs (the common case) the result is identical.
        let mut generator_value = value.round() as i32;

        if validate {
            // GENERATOR_LIMITS is indexed 0..=62.
            // For INVALID (-1) or any out-of-range type, .get() returns None → no clamping.
            if let Some(Some(lim)) = GENERATOR_LIMITS.get(generator_type as usize) {
                generator_value = generator_value.max(lim.min).min(lim.max);
            }
        }

        Self {
            generator_type,
            generator_value: generator_value as i16,
        }
    }

    /// Writes the generator as 4 bytes to `gen_data` (type WORD, value WORD, little-endian).
    /// The comment "name is deceptive, it works on negatives" from the original source refers
    /// to writeWord writing the raw bit pattern, correctly handling signed values.
    /// Equivalent to: write(genData)
    pub fn write(&self, gen_data: &mut IndexedByteArray) {
        // Name is deceptive, it works on negatives
        write_word(gen_data, self.generator_type as u16 as u32);
        write_word(gen_data, self.generator_value as u16 as u32);
    }
}

/// Equivalent to: toString()
impl fmt::Display for Generator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.generator_type, self.generator_value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;

    // --- GEN_BYTE_SIZE ---

    #[test]
    fn test_gen_byte_size() {
        assert_eq!(GEN_BYTE_SIZE, 4);
    }

    // --- new() with validation ---

    #[test]
    fn test_new_value_within_limits() {
        // initialFilterFc (8): min=1500, max=13500
        let g = Generator::new(gt::INITIAL_FILTER_FC, 8000.0);
        assert_eq!(g.generator_type, gt::INITIAL_FILTER_FC);
        assert_eq!(g.generator_value, 8000);
    }

    #[test]
    fn test_new_clamps_above_max() {
        // initialFilterFc (8): max=13500
        let g = Generator::new(gt::INITIAL_FILTER_FC, 20000.0);
        assert_eq!(g.generator_value, 13500);
    }

    #[test]
    fn test_new_clamps_below_min() {
        // initialFilterFc (8): min=1500
        let g = Generator::new(gt::INITIAL_FILTER_FC, 0.0);
        assert_eq!(g.generator_value, 1500);
    }

    #[test]
    fn test_new_clamps_negative_pan() {
        // pan (17): min=-500, max=500
        let g = Generator::new(gt::PAN, -600.0);
        assert_eq!(g.generator_value, -500);
    }

    #[test]
    fn test_new_clamps_positive_pan() {
        // pan (17): min=-500, max=500
        let g = Generator::new(gt::PAN, 600.0);
        assert_eq!(g.generator_value, 500);
    }

    #[test]
    fn test_new_no_limit_instrument() {
        // instrument (41): no limit defined (None) → no clamping
        let g = Generator::new(gt::INSTRUMENT, 200.0);
        assert_eq!(g.generator_value, 200);
    }

    #[test]
    fn test_new_invalid_type_no_clamp() {
        // INVALID (-1): out of GENERATOR_LIMITS range → no clamping
        let g = Generator::new(gt::INVALID, 5000.0);
        assert_eq!(g.generator_value, 5000);
    }

    #[test]
    fn test_new_rounds_up() {
        // 3.6 rounds to 4 (within pan limits)
        let g = Generator::new(gt::PAN, 3.6);
        assert_eq!(g.generator_value, 4);
    }

    #[test]
    fn test_new_rounds_down() {
        // 3.2 rounds to 3 (within pan limits)
        let g = Generator::new(gt::PAN, 3.2);
        assert_eq!(g.generator_value, 3);
    }

    #[test]
    fn test_new_negative_rounds() {
        // -3.7 rounds to -4 (within pan limits)
        let g = Generator::new(gt::PAN, -3.7);
        assert_eq!(g.generator_value, -4);
    }

    #[test]
    fn test_new_at_exact_max() {
        // initialFilterFc at exactly max (13500) → stays 13500
        let g = Generator::new(gt::INITIAL_FILTER_FC, 13500.0);
        assert_eq!(g.generator_value, 13500);
    }

    #[test]
    fn test_new_at_exact_min() {
        // initialFilterFc at exactly min (1500) → stays 1500
        let g = Generator::new(gt::INITIAL_FILTER_FC, 1500.0);
        assert_eq!(g.generator_value, 1500);
    }

    #[test]
    fn test_new_zero_value() {
        // pan value 0 is within limits
        let g = Generator::new(gt::PAN, 0.0);
        assert_eq!(g.generator_value, 0);
    }

    // --- new_unvalidated() ---

    #[test]
    fn test_new_unvalidated_outside_limits() {
        // initialFilterFc max=13500, pass 14000 → not clamped
        let g = Generator::new_unvalidated(gt::INITIAL_FILTER_FC, 14000.0);
        assert_eq!(g.generator_value, 14000);
    }

    #[test]
    fn test_new_unvalidated_below_min() {
        // initialFilterFc min=1500, pass 500 → not clamped
        let g = Generator::new_unvalidated(gt::INITIAL_FILTER_FC, 500.0);
        assert_eq!(g.generator_value, 500);
    }

    #[test]
    fn test_new_unvalidated_rounds() {
        // Rounding still applies even without validation
        let g = Generator::new_unvalidated(gt::PAN, 3.7);
        assert_eq!(g.generator_value, 4);
    }

    // --- write() ---

    #[test]
    fn test_write_positive_value() {
        // type=5 (MOD_LFO_TO_PITCH), value=100
        // bytes: [0x05, 0x00, 0x64, 0x00]
        let g = Generator::new(gt::MOD_LFO_TO_PITCH, 100.0);
        let mut buf = IndexedByteArray::new(4);
        g.write(&mut buf);
        let s: &[u8] = &buf;
        assert_eq!(s, &[0x05, 0x00, 0x64, 0x00]);
    }

    #[test]
    fn test_write_negative_value() {
        // type=17 (PAN), value=-100
        // type bytes: [0x11, 0x00]
        // value=-100 as u16 = 0xFF9C → [0x9C, 0xFF]
        let g = Generator::new(gt::PAN, -100.0);
        let mut buf = IndexedByteArray::new(4);
        g.write(&mut buf);
        let s: &[u8] = &buf;
        assert_eq!(s, &[0x11, 0x00, 0x9C, 0xFF]);
    }

    #[test]
    fn test_write_zero_value() {
        // type=0 (START_ADDRS_OFFSET), value=0 → all zeros
        let g = Generator::new(gt::START_ADDRS_OFFSET, 0.0);
        let mut buf = IndexedByteArray::new(4);
        g.write(&mut buf);
        let s: &[u8] = &buf;
        assert_eq!(s, &[0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_write_advances_index_by_four() {
        let g = Generator::new(gt::PAN, 0.0);
        let mut buf = IndexedByteArray::new(4);
        g.write(&mut buf);
        assert_eq!(buf.current_index, 4);
    }

    #[test]
    fn test_write_sequential() {
        // Write two generators back to back
        let g1 = Generator::new(gt::START_ADDRS_OFFSET, 1.0);
        let g2 = Generator::new(gt::PAN, 2.0);
        let mut buf = IndexedByteArray::new(8);
        g1.write(&mut buf);
        g2.write(&mut buf);
        let s: &[u8] = &buf;
        // g1: type=0 [0x00,0x00], value=1 [0x01,0x00]
        // g2: type=17 [0x11,0x00], value=2 [0x02,0x00]
        assert_eq!(s, &[0x00, 0x00, 0x01, 0x00, 0x11, 0x00, 0x02, 0x00]);
    }

    // --- Display ---

    #[test]
    fn test_display_contains_type_and_value() {
        // pan type=17, value=42
        let g = Generator::new(gt::PAN, 42.0);
        let s = format!("{}", g);
        assert!(s.contains("17"), "display should contain type number");
        assert!(s.contains("42"), "display should contain value");
    }

    #[test]
    fn test_display_negative_value() {
        let g = Generator::new(gt::PAN, -100.0);
        let s = format!("{}", g);
        assert!(s.contains("-100"), "display should contain negative value");
    }
}
