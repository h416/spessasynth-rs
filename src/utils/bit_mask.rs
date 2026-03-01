/// bit_mask.rs
/// purpose: Bit manipulation utilities.
/// Ported from: src/utils/byte_functions/bit_mask.ts
/// Converts a given bit position to bool.
/// Equivalent to: bitMaskToBool(num, bit)
pub fn bit_mask_to_bool(num: u32, bit: u32) -> bool {
    ((num >> bit) & 1) > 0
}

/// Converts a bool to a numeric value (1 or 0).
/// Equivalent to: toNumericBool(bool)
pub fn to_numeric_bool(b: bool) -> u8 {
    b as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bit0_is_set() {
        assert!(bit_mask_to_bool(0b0000_0001, 0));
    }

    #[test]
    fn test_bit0_is_not_set() {
        assert!(!bit_mask_to_bool(0b0000_0010, 0));
    }

    #[test]
    fn test_bit1_is_set() {
        assert!(bit_mask_to_bool(0b0000_0010, 1));
    }

    #[test]
    fn test_all_zeros() {
        assert!(!bit_mask_to_bool(0x00, 0));
    }

    #[test]
    fn test_highest_bit() {
        assert!(bit_mask_to_bool(0xFF, 7));
    }

    #[test]
    fn test_to_numeric_bool_true() {
        assert_eq!(to_numeric_bool(true), 1);
    }

    #[test]
    fn test_to_numeric_bool_false() {
        assert_eq!(to_numeric_bool(false), 0);
    }
}
