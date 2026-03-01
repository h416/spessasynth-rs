/// string.rs
/// purpose: ASCII binary string read/write utilities for byte arrays.
/// Ported from: src/utils/byte_functions/string.ts
use crate::utils::indexed_array::IndexedByteArray;

/// Reads `bytes` bytes starting at `offset` as an ASCII string.
/// Stops early if a null byte (0x00) is encountered.
/// Equivalent to: readBinaryString(dataArray, bytes, offset)
pub fn read_binary_string(data: &[u8], bytes: usize, offset: usize) -> String {
    let mut string = String::new();
    for i in 0..bytes {
        let byte = data[offset + i];
        if byte == 0 {
            return string;
        }
        string.push(byte as char);
    }
    string
}

/// Reads `bytes` bytes from an IndexedByteArray as an ASCII string,
/// advancing current_index by `bytes`.
/// Equivalent to: readBinaryStringIndexed(dataArray, bytes)
pub fn read_binary_string_indexed(data: &mut IndexedByteArray, bytes: usize) -> String {
    let start_index = data.current_index;
    data.current_index += bytes;
    read_binary_string(data, bytes, start_index)
}

/// Converts an ASCII string into an IndexedByteArray.
/// `add_zero`: appends a null terminator byte.
/// `ensure_even`: pads to an even byte count.
/// Equivalent to: getStringBytes(string, addZero, ensureEven)
pub fn get_string_bytes(string: &str, add_zero: bool, ensure_even: bool) -> IndexedByteArray {
    let mut len = string.len();
    if add_zero {
        len += 1;
    }
    #[allow(clippy::manual_is_multiple_of)]
    if ensure_even && len % 2 != 0 {
        len += 1;
    }
    let mut arr = IndexedByteArray::new(len);
    write_binary_string_indexed(&mut arr, string, 0);
    arr
}

/// Writes ASCII bytes of `string` into `out_array` at current_index.
/// If `pad_length > 0`:
/// - truncates `string` to `pad_length` if it is longer.
/// - pads with null bytes if `string` is shorter than `pad_length`.
///
/// Equivalent to: writeBinaryStringIndexed(outArray, string, padLength)
pub fn write_binary_string_indexed(
    out_array: &mut IndexedByteArray,
    string: &str,
    pad_length: usize,
) {
    let effective = if pad_length > 0 && string.len() > pad_length {
        &string[..pad_length]
    } else {
        string
    };

    for &byte in effective.as_bytes() {
        let idx = out_array.current_index;
        out_array[idx] = byte;
        out_array.current_index += 1;
    }

    if pad_length > effective.len() {
        for _ in 0..(pad_length - effective.len()) {
            let idx = out_array.current_index;
            out_array[idx] = 0;
            out_array.current_index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- read_binary_string ---

    #[test]
    fn test_read_basic_ascii() {
        let data = b"Hello";
        assert_eq!(read_binary_string(data, 5, 0), "Hello");
    }

    #[test]
    fn test_read_stops_at_null() {
        let data = b"Hi\x00World";
        assert_eq!(read_binary_string(data, 7, 0), "Hi");
    }

    #[test]
    fn test_read_starts_with_null_returns_empty() {
        let data = b"\x00ABC";
        assert_eq!(read_binary_string(data, 4, 0), "");
    }

    #[test]
    fn test_read_with_offset() {
        let data = b"XXHello";
        assert_eq!(read_binary_string(data, 5, 2), "Hello");
    }

    #[test]
    fn test_read_fewer_bytes_than_array() {
        let data = b"Hello";
        assert_eq!(read_binary_string(data, 3, 0), "Hel");
    }

    #[test]
    fn test_read_no_null_reads_all() {
        let data = b"ABCD";
        assert_eq!(read_binary_string(data, 4, 0), "ABCD");
    }

    #[test]
    fn test_read_null_padded_name() {
        // SoundFont-style: name padded to 20 bytes with nulls
        let mut data = [0u8; 20];
        data[..5].copy_from_slice(b"Piano");
        assert_eq!(read_binary_string(&data, 20, 0), "Piano");
    }

    // --- read_binary_string_indexed ---

    #[test]
    fn test_read_indexed_basic() {
        let data = vec![b'H', b'i', b'\x00', b'!'];
        let mut arr = IndexedByteArray::from_vec(data);
        assert_eq!(read_binary_string_indexed(&mut arr, 4), "Hi");
    }

    #[test]
    fn test_read_indexed_advances_cursor() {
        let data = vec![b'A', b'B', b'C', b'D'];
        let mut arr = IndexedByteArray::from_vec(data);
        read_binary_string_indexed(&mut arr, 2);
        assert_eq!(arr.current_index, 2);
    }

    #[test]
    fn test_read_indexed_sequential() {
        let data = vec![b'H', b'i', b'\x00', b'O', b'k', b'\x00'];
        let mut arr = IndexedByteArray::from_vec(data);
        assert_eq!(read_binary_string_indexed(&mut arr, 3), "Hi");
        assert_eq!(read_binary_string_indexed(&mut arr, 3), "Ok");
        assert_eq!(arr.current_index, 6);
    }

    // --- write_binary_string_indexed ---

    #[test]
    fn test_write_basic() {
        let mut arr = IndexedByteArray::new(5);
        write_binary_string_indexed(&mut arr, "Hello", 0);
        assert_eq!(arr[0], b'H');
        assert_eq!(arr[4], b'o');
        assert_eq!(arr.current_index, 5);
    }

    #[test]
    fn test_write_with_pad_length_shorter_string() {
        // "Hi" into a field of 5 → "Hi\0\0\0"
        let mut arr = IndexedByteArray::new(5);
        write_binary_string_indexed(&mut arr, "Hi", 5);
        let s: &[u8] = &arr;
        assert_eq!(&s[..5], b"Hi\x00\x00\x00");
        assert_eq!(arr.current_index, 5);
    }

    #[test]
    fn test_write_with_pad_length_exact_string() {
        // "Hello" into a field of 5 → no padding
        let mut arr = IndexedByteArray::new(5);
        write_binary_string_indexed(&mut arr, "Hello", 5);
        let s: &[u8] = &arr;
        assert_eq!(&s[..5], b"Hello");
    }

    #[test]
    fn test_write_with_pad_length_truncates() {
        // "Hello" into a field of 3 → "Hel"
        let mut arr = IndexedByteArray::new(3);
        write_binary_string_indexed(&mut arr, "Hello", 3);
        let s: &[u8] = &arr;
        assert_eq!(&s[..3], b"Hel");
        assert_eq!(arr.current_index, 3);
    }

    #[test]
    fn test_write_no_pad_length_no_padding() {
        // pad_length=0: writes string as-is, no zero padding
        let mut arr = IndexedByteArray::new(5);
        write_binary_string_indexed(&mut arr, "Hi", 0);
        assert_eq!(arr.current_index, 2);
        assert_eq!(arr[0], b'H');
        assert_eq!(arr[1], b'i');
    }

    // --- get_string_bytes ---

    #[test]
    fn test_get_string_bytes_basic() {
        let arr = get_string_bytes("AB", false, false);
        assert_eq!(arr[0], b'A');
        assert_eq!(arr[1], b'B');
    }

    #[test]
    fn test_get_string_bytes_length() {
        let arr = get_string_bytes("Hi", false, false);
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn test_get_string_bytes_add_zero() {
        let arr = get_string_bytes("Hi", true, false);
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[2], 0);
    }

    #[test]
    fn test_get_string_bytes_ensure_even_odd_len() {
        // "Hi" = 2 bytes (even) + add_zero → 3 (odd) → padded to 4
        let arr = get_string_bytes("Hi", true, true);
        assert_eq!(arr.len(), 4);
    }

    #[test]
    fn test_get_string_bytes_ensure_even_already_even() {
        // "Hi" = 2 bytes (already even), no change
        let arr = get_string_bytes("Hi", false, true);
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn test_get_string_bytes_roundtrip() {
        let original = "Piano";
        let arr = get_string_bytes(original, false, false);
        let result = read_binary_string(&arr, arr.len(), 0);
        assert_eq!(result, original);
    }
}
