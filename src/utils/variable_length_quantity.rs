/// variable_length_quantity.rs
/// purpose: Variable-length quantity (VLQ) encoding used in MIDI files.
/// Ported from: src/utils/byte_functions/variable_length_quantity.ts
use crate::utils::indexed_array::IndexedByteArray;

/// Reads a VLQ-encoded integer from an IndexedByteArray, advancing current_index.
/// Equivalent to: readVariableLengthQuantity(MIDIbyteArray)
pub fn read_variable_length_quantity(data: &mut IndexedByteArray) -> u32 {
    let mut out: u32 = 0;
    loop {
        let idx = data.current_index;
        data.current_index += 1;
        let byte = data[idx];
        // Extract the lower 7 bits and accumulate
        out = (out << 7) | (byte & 127) as u32;
        // If MSB is 0, this is the last byte
        if byte >> 7 != 1 {
            break;
        }
    }
    out
}

/// Encodes an integer as a VLQ byte sequence.
/// Equivalent to: writeVariableLengthQuantity(number)
pub fn write_variable_length_quantity(mut number: u32) -> Vec<u8> {
    // The first (least significant) group of 7 bits, MSB=0 (last byte)
    let mut bytes = vec![(number & 127) as u8];
    number >>= 7;

    // Remaining groups: prepend with MSB=1 (more bytes follow)
    // Using push + reverse instead of unshift for efficiency
    while number > 0 {
        bytes.push(((number & 127) | 128) as u8);
        number >>= 7;
    }
    bytes.reverse();
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- read_variable_length_quantity ---

    #[test]
    fn test_read_zero() {
        let mut arr = IndexedByteArray::from_vec(vec![0x00]);
        assert_eq!(read_variable_length_quantity(&mut arr), 0);
    }

    #[test]
    fn test_read_1byte_max() {
        let mut arr = IndexedByteArray::from_vec(vec![0x7F]);
        assert_eq!(read_variable_length_quantity(&mut arr), 127);
    }

    #[test]
    fn test_read_2bytes_min() {
        let mut arr = IndexedByteArray::from_vec(vec![0x81, 0x00]);
        assert_eq!(read_variable_length_quantity(&mut arr), 128);
    }

    #[test]
    fn test_read_255() {
        let mut arr = IndexedByteArray::from_vec(vec![0x81, 0x7F]);
        assert_eq!(read_variable_length_quantity(&mut arr), 255);
    }

    #[test]
    fn test_read_2bytes_max() {
        let mut arr = IndexedByteArray::from_vec(vec![0xFF, 0x7F]);
        assert_eq!(read_variable_length_quantity(&mut arr), 16383);
    }

    #[test]
    fn test_read_3bytes_min() {
        let mut arr = IndexedByteArray::from_vec(vec![0x81, 0x80, 0x00]);
        assert_eq!(read_variable_length_quantity(&mut arr), 16384);
    }

    #[test]
    fn test_read_3bytes_max() {
        let mut arr = IndexedByteArray::from_vec(vec![0xFF, 0xFF, 0x7F]);
        assert_eq!(read_variable_length_quantity(&mut arr), 2097151);
    }

    #[test]
    fn test_read_4bytes_max() {
        let mut arr = IndexedByteArray::from_vec(vec![0xFF, 0xFF, 0xFF, 0x7F]);
        assert_eq!(read_variable_length_quantity(&mut arr), 268435455);
    }

    #[test]
    fn test_read_advances_cursor_sequential() {
        let mut arr = IndexedByteArray::from_vec(vec![0x40, 0x7F]);
        assert_eq!(read_variable_length_quantity(&mut arr), 0x40);
        assert_eq!(arr.current_index, 1);
        assert_eq!(read_variable_length_quantity(&mut arr), 0x7F);
        assert_eq!(arr.current_index, 2);
    }

    // --- write_variable_length_quantity ---

    #[test]
    fn test_write_zero() {
        assert_eq!(write_variable_length_quantity(0), vec![0x00]);
    }

    #[test]
    fn test_write_1byte_max() {
        assert_eq!(write_variable_length_quantity(127), vec![0x7F]);
    }

    #[test]
    fn test_write_2bytes_min() {
        assert_eq!(write_variable_length_quantity(128), vec![0x81, 0x00]);
    }

    #[test]
    fn test_write_255() {
        assert_eq!(write_variable_length_quantity(255), vec![0x81, 0x7F]);
    }

    #[test]
    fn test_write_2bytes_max() {
        assert_eq!(write_variable_length_quantity(16383), vec![0xFF, 0x7F]);
    }

    #[test]
    fn test_write_3bytes_min() {
        assert_eq!(
            write_variable_length_quantity(16384),
            vec![0x81, 0x80, 0x00]
        );
    }

    #[test]
    fn test_write_3bytes_max() {
        assert_eq!(
            write_variable_length_quantity(2097151),
            vec![0xFF, 0xFF, 0x7F]
        );
    }

    // --- Roundtrip tests (write → read) ---

    fn roundtrip(value: u32) {
        let encoded = write_variable_length_quantity(value);
        let mut arr = IndexedByteArray::from_vec(encoded);
        assert_eq!(read_variable_length_quantity(&mut arr), value);
    }

    #[test]
    fn test_roundtrip_zero() {
        roundtrip(0);
    }

    #[test]
    fn test_roundtrip_127() {
        roundtrip(127);
    }

    #[test]
    fn test_roundtrip_128() {
        roundtrip(128);
    }

    #[test]
    fn test_roundtrip_16383() {
        roundtrip(16383);
    }

    #[test]
    fn test_roundtrip_16384() {
        roundtrip(16384);
    }

    #[test]
    fn test_roundtrip_2097151() {
        roundtrip(2097151);
    }
}
