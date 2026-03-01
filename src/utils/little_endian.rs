/// little_endian.rs
/// purpose: Little-endian byte read/write utilities.
/// Ported from: src/utils/byte_functions/little_endian.ts
use crate::utils::indexed_array::IndexedByteArray;

/// Reads `bytes_amount` bytes as a little-endian unsigned integer from `data` at `offset`.
/// Equivalent to: readLittleEndian(dataArray, bytesAmount, offset)
/// Assumes bytes_amount <= 4.
pub fn read_little_endian(data: &[u8], bytes_amount: usize, offset: usize) -> u32 {
    let mut out: u32 = 0;
    for i in 0..bytes_amount {
        out |= (data[offset + i] as u32) << (i * 8);
    }
    out
}

/// Reads `bytes_amount` bytes as a little-endian unsigned integer from an IndexedByteArray,
/// advancing current_index by bytes_amount.
/// Equivalent to: readLittleEndianIndexed(dataArray, bytesAmount)
pub fn read_little_endian_indexed(data: &mut IndexedByteArray, bytes_amount: usize) -> u32 {
    let res = read_little_endian(data, bytes_amount, data.current_index);
    data.current_index += bytes_amount;
    res
}

/// Writes `number` as a little-endian byte sequence of `byte_target` bytes into `data`,
/// advancing current_index by byte_target.
/// Equivalent to: writeLittleEndianIndexed(dataArray, number, byteTarget)
pub fn write_little_endian_indexed(data: &mut IndexedByteArray, number: u32, byte_target: usize) {
    for i in 0..byte_target {
        let idx = data.current_index;
        data[idx] = ((number >> (i * 8)) & 0xff) as u8;
        data.current_index += 1;
    }
}

/// Writes a 16-bit value (WORD) as little-endian into `data`.
/// Equivalent to: writeWord(dataArray, word)
pub fn write_word(data: &mut IndexedByteArray, word: u32) {
    let idx = data.current_index;
    data[idx] = (word & 0xff) as u8;
    data.current_index += 1;
    let idx = data.current_index;
    data[idx] = (word >> 8) as u8;
    data.current_index += 1;
}

/// Writes a 32-bit value (DWORD) as little-endian into `data`.
/// Equivalent to: writeDword(dataArray, dword)
pub fn write_dword(data: &mut IndexedByteArray, dword: u32) {
    write_little_endian_indexed(data, dword, 4);
}

/// Interprets two bytes as a signed 16-bit little-endian integer.
/// Equivalent to: signedInt16(byte1, byte2)
pub fn signed_int16(byte1: u8, byte2: u8) -> i16 {
    i16::from_le_bytes([byte1, byte2])
}

/// Interprets a byte as a signed 8-bit integer.
/// Equivalent to: signedInt8(byte)
pub fn signed_int8(byte: u8) -> i8 {
    byte as i8
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- read_little_endian ---

    #[test]
    fn test_read_2bytes() {
        assert_eq!(read_little_endian(&[0x01, 0x02], 2, 0), 0x0201);
    }

    #[test]
    fn test_read_4bytes() {
        assert_eq!(
            read_little_endian(&[0x01, 0x02, 0x03, 0x04], 4, 0),
            0x04030201
        );
    }

    #[test]
    fn test_read_with_offset() {
        assert_eq!(read_little_endian(&[0x00, 0x01, 0x02], 2, 1), 0x0201);
    }

    #[test]
    fn test_read_1byte() {
        assert_eq!(read_little_endian(&[0x80], 1, 0), 0x80);
    }

    // --- read_little_endian_indexed ---

    #[test]
    fn test_read_indexed_advances_cursor() {
        let mut arr = IndexedByteArray::from_vec(vec![0x01, 0x02, 0x03, 0x04]);
        assert_eq!(read_little_endian_indexed(&mut arr, 2), 0x0201);
        assert_eq!(arr.current_index, 2);
    }

    #[test]
    fn test_read_indexed_sequential() {
        let mut arr = IndexedByteArray::from_vec(vec![0x01, 0x02, 0x03, 0x04]);
        assert_eq!(read_little_endian_indexed(&mut arr, 2), 0x0201);
        assert_eq!(read_little_endian_indexed(&mut arr, 2), 0x0403);
        assert_eq!(arr.current_index, 4);
    }

    // --- write_little_endian_indexed ---

    #[test]
    fn test_write_2bytes() {
        let mut arr = IndexedByteArray::new(2);
        write_little_endian_indexed(&mut arr, 0x0102, 2);
        assert_eq!(arr[0], 0x02);
        assert_eq!(arr[1], 0x01);
        assert_eq!(arr.current_index, 2);
    }

    #[test]
    fn test_write_4bytes() {
        let mut arr = IndexedByteArray::new(4);
        write_little_endian_indexed(&mut arr, 0x01020304, 4);
        let s: &[u8] = &arr;
        assert_eq!(s, &[0x04, 0x03, 0x02, 0x01]);
    }

    // --- write_word ---

    #[test]
    fn test_write_word() {
        let mut arr = IndexedByteArray::new(2);
        write_word(&mut arr, 0x0102);
        let s: &[u8] = &arr;
        assert_eq!(s, &[0x02, 0x01]);
        assert_eq!(arr.current_index, 2);
    }

    // --- write_dword ---

    #[test]
    fn test_write_dword() {
        let mut arr = IndexedByteArray::new(4);
        write_dword(&mut arr, 0x01020304);
        let s: &[u8] = &arr;
        assert_eq!(s, &[0x04, 0x03, 0x02, 0x01]);
        assert_eq!(arr.current_index, 4);
    }

    // --- signed_int16 ---

    #[test]
    fn test_signed_int16_positive() {
        assert_eq!(signed_int16(0x01, 0x00), 1);
    }

    #[test]
    fn test_signed_int16_max() {
        assert_eq!(signed_int16(0xFF, 0x7F), 32767);
    }

    #[test]
    fn test_signed_int16_min() {
        assert_eq!(signed_int16(0x00, 0x80), -32768);
    }

    #[test]
    fn test_signed_int16_minus_one() {
        assert_eq!(signed_int16(0xFF, 0xFF), -1);
    }

    // --- signed_int8 ---

    #[test]
    fn test_signed_int8_zero() {
        assert_eq!(signed_int8(0x00), 0);
    }

    #[test]
    fn test_signed_int8_max() {
        assert_eq!(signed_int8(0x7F), 127);
    }

    #[test]
    fn test_signed_int8_min() {
        assert_eq!(signed_int8(0x80), -128);
    }

    #[test]
    fn test_signed_int8_minus_one() {
        assert_eq!(signed_int8(0xFF), -1);
    }
}
