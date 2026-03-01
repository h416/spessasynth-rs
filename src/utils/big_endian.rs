/// big_endian.rs
/// purpose: Big-endian byte read/write utilities.
/// Ported from: src/utils/byte_functions/big_endian.ts
use crate::utils::indexed_array::IndexedByteArray;

/// Reads `bytes_amount` bytes as a big-endian unsigned integer from `data` at `offset`.
/// Equivalent to: readBigEndian(dataArray, bytesAmount, offset)
/// Assumes bytes_amount <= 4.
pub fn read_big_endian(data: &[u8], bytes_amount: usize, offset: usize) -> u32 {
    let mut out: u32 = 0;
    for i in 0..bytes_amount {
        out = (out << 8) | data[offset + i] as u32;
    }
    out
}

/// Reads `bytes_amount` bytes as a big-endian unsigned integer from an IndexedByteArray,
/// advancing current_index by bytes_amount.
/// Equivalent to: readBigEndianIndexed(dataArray, bytesAmount)
pub fn read_big_endian_indexed(data: &mut IndexedByteArray, bytes_amount: usize) -> u32 {
    let res = read_big_endian(data, bytes_amount, data.current_index);
    data.current_index += bytes_amount;
    res
}

/// Writes `number` as a big-endian byte sequence of `bytes_amount` bytes.
/// Equivalent to: writeBigEndian(number, bytesAmount)
pub fn write_big_endian(mut number: u32, bytes_amount: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; bytes_amount];
    for i in (0..bytes_amount).rev() {
        bytes[i] = (number & 0xff) as u8;
        number >>= 8;
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- read_big_endian ---

    #[test]
    fn test_read_2bytes() {
        assert_eq!(read_big_endian(&[0x01, 0x02], 2, 0), 0x0102);
    }

    #[test]
    fn test_read_4bytes() {
        assert_eq!(read_big_endian(&[0x01, 0x02, 0x03, 0x04], 4, 0), 0x01020304);
    }

    #[test]
    fn test_read_with_offset() {
        assert_eq!(read_big_endian(&[0x00, 0x01, 0x02], 2, 1), 0x0102);
    }

    #[test]
    fn test_read_1byte() {
        assert_eq!(read_big_endian(&[0x80], 1, 0), 0x80);
    }

    // --- read_big_endian_indexed ---

    #[test]
    fn test_read_indexed_advances_cursor() {
        let mut arr = IndexedByteArray::from_vec(vec![0x01, 0x02, 0x03, 0x04]);
        assert_eq!(read_big_endian_indexed(&mut arr, 2), 0x0102);
        assert_eq!(arr.current_index, 2);
    }

    #[test]
    fn test_read_indexed_sequential() {
        let mut arr = IndexedByteArray::from_vec(vec![0x01, 0x02, 0x03, 0x04]);
        assert_eq!(read_big_endian_indexed(&mut arr, 2), 0x0102);
        assert_eq!(read_big_endian_indexed(&mut arr, 2), 0x0304);
        assert_eq!(arr.current_index, 4);
    }

    // --- write_big_endian ---

    #[test]
    fn test_write_2bytes() {
        assert_eq!(write_big_endian(0x0102, 2), vec![0x01, 0x02]);
    }

    #[test]
    fn test_write_4bytes() {
        assert_eq!(
            write_big_endian(0x01020304, 4),
            vec![0x01, 0x02, 0x03, 0x04]
        );
    }

    #[test]
    fn test_write_zero_padding() {
        assert_eq!(write_big_endian(0xFF, 2), vec![0x00, 0xFF]);
    }

    #[test]
    fn test_write_all_zeros() {
        assert_eq!(write_big_endian(0x00, 4), vec![0x00, 0x00, 0x00, 0x00]);
    }
}
