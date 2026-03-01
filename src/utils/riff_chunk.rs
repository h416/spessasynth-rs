/// riff_chunk.rs
/// purpose: RIFF chunk read/write utilities.
/// Ported from: src/utils/riff_chunk.ts
///
/// Note: TypeScript's `WAVFourCC`, `GenericRIFFFourCC`, and `FourCC` are string literal union
/// types used only for documentation/type checking. In Rust these are represented as `String`.
/// Per CLAUDE.md, these type aliases are defined here (not in soundbank/types.rs) to avoid
/// the circular dependency that exists in the TypeScript version.
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::{read_little_endian_indexed, write_dword};
use crate::utils::string::{
    read_binary_string, read_binary_string_indexed, write_binary_string_indexed,
};

/// Equivalent to TypeScript's `GenericRIFFFourCC = "RIFF" | "LIST" | "INFO"`.
pub type GenericRIFFFourCC = String;

/// Equivalent to TypeScript's `WAVFourCC = "wave" | "cue " | "fmt "`.
pub type WAVFourCC = String;

/// Equivalent to TypeScript's `FourCC` (union of all known RIFF FourCC strings).
pub type FourCC = String;

/// Represents a RIFF chunk.
/// Equivalent to: `class RIFFChunk` in TypeScript.
#[derive(Debug)]
pub struct RIFFChunk {
    /// The chunk's FourCC code.
    pub header: FourCC,
    /// The chunk's size in bytes.
    pub size: u32,
    /// The chunk's binary data.
    /// Note: `data.length` will be 0 if `read_data` was set to `false` in `read_riff_chunk`.
    pub data: IndexedByteArray,
}

impl RIFFChunk {
    /// Creates a new RIFF chunk.
    /// Equivalent to: `new RIFFChunk(header, size, data)`
    pub fn new(header: FourCC, size: u32, data: IndexedByteArray) -> Self {
        Self { header, size, data }
    }
}

/// Reads a RIFF chunk from an `IndexedByteArray`, advancing the cursor.
/// Equivalent to: `readRIFFChunk(dataArray, readData = true, forceShift = false)`
pub fn read_riff_chunk(
    data_array: &mut IndexedByteArray,
    read_data: bool,
    force_shift: bool,
) -> RIFFChunk {
    let header = read_binary_string_indexed(data_array, 4);
    let mut size = read_little_endian_indexed(data_array, 4);
    // Safeguard against evil DLS files (e.g. CrysDLS v1.23.dls)
    // https://github.com/spessasus/spessasynth_core/issues/5
    if header.is_empty() {
        size = 0;
    }

    let chunk_data = if read_data {
        let start = data_array.current_index;
        let end = start + size as usize;
        data_array.slice(start, end)
    } else {
        IndexedByteArray::new(0)
    };

    if read_data || force_shift {
        data_array.current_index += size as usize;
        #[allow(clippy::manual_is_multiple_of)]
        if size % 2 != 0 {
            data_array.current_index += 1;
        }
    }

    RIFFChunk::new(header, size, chunk_data)
}

/// Writes a RIFF chunk given a raw byte slice.
/// Equivalent to: `writeRIFFChunkRaw(header, data, addZeroByte = false, isList = false)`
pub fn write_riff_chunk_raw(
    header: &str,
    data: &[u8],
    add_zero_byte: bool,
    is_list: bool,
) -> IndexedByteArray {
    assert_eq!(header.len(), 4, "Invalid header length: {}", header);

    let data_start_offset: usize = if is_list { 12 } else { 8 };
    let header_written: &str = if is_list { "LIST" } else { header };

    let mut data_length = data.len();
    if add_zero_byte {
        data_length += 1;
    }
    let mut written_size = data_length;
    if is_list {
        written_size += 4;
    }

    let mut final_size = data_start_offset + data_length;
    #[allow(clippy::manual_is_multiple_of)]
    if final_size % 2 != 0 {
        // Pad byte does not get included in the size
        final_size += 1;
    }

    let mut out_array = IndexedByteArray::new(final_size);
    // FourCC ("RIFF", "LIST", "pdta" etc.)
    write_binary_string_indexed(&mut out_array, header_written, 0);
    // Chunk size
    write_dword(&mut out_array, written_size as u32);
    if is_list {
        // List type (e.g. "INFO")
        write_binary_string_indexed(&mut out_array, header, 0);
    }
    // Write data at data_start_offset (out_array.current_index is now at data_start_offset)
    for (i, &byte) in data.iter().enumerate() {
        out_array[data_start_offset + i] = byte;
    }

    out_array
}

/// Writes a RIFF chunk from multiple byte slices, combining them in order.
/// Equivalent to: `writeRIFFChunkParts(header, chunks, isList = false)`
pub fn write_riff_chunk_parts(header: &str, chunks: &[&[u8]], is_list: bool) -> IndexedByteArray {
    assert_eq!(header.len(), 4, "Invalid header length: {}", header);

    let data_start_offset: usize = if is_list { 12 } else { 8 };
    let header_written: &str = if is_list { "LIST" } else { header };

    let data_length: usize = chunks.iter().map(|c| c.len()).sum();
    let mut written_size = data_length;
    if is_list {
        written_size += 4;
    }

    let mut final_size = data_start_offset + data_length;
    #[allow(clippy::manual_is_multiple_of)]
    if final_size % 2 != 0 {
        // Pad byte does not get included in the size
        final_size += 1;
    }

    let mut out_array = IndexedByteArray::new(final_size);
    // FourCC
    write_binary_string_indexed(&mut out_array, header_written, 0);
    // Chunk size
    write_dword(&mut out_array, written_size as u32);
    if is_list {
        // List type
        write_binary_string_indexed(&mut out_array, header, 0);
    }

    let mut data_offset = data_start_offset;
    for chunk in chunks {
        for (i, &byte) in chunk.iter().enumerate() {
            out_array[data_offset + i] = byte;
        }
        data_offset += chunk.len();
    }

    out_array
}

/// Finds a chunk with a given LIST type in a collection.
/// Also sets `data.current_index` to 4 (past the LIST type FourCC) on the found chunk,
/// so the caller can immediately start reading sub-chunks.
/// Equivalent to: `findRIFFListType(collection, type)`
pub fn find_riff_list_type<'a>(
    collection: &'a mut [RIFFChunk],
    type_: &str,
) -> Option<&'a mut RIFFChunk> {
    // Use position() with immutable iter to find the index, then mutably borrow once.
    let pos = collection.iter().position(|c| {
        if c.header != "LIST" {
            return false;
        }
        // The list type is at offset 0 of chunk.data (readBinaryString default offset = 0)
        read_binary_string(&c.data, 4, 0) == type_
    })?;
    let chunk = &mut collection[pos];
    // Side effect: skip cursor past the list FourCC so caller can read sub-chunks
    chunk.data.current_index = 4;
    Some(chunk)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // read_riff_chunk
    // -----------------------------------------------------------------------

    fn make_chunk_bytes(header: &str, data: &[u8]) -> IndexedByteArray {
        // [header 4B][size 4B LE][data]
        let mut bytes = Vec::new();
        bytes.extend_from_slice(header.as_bytes());
        let size = data.len() as u32;
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.extend_from_slice(data);
        IndexedByteArray::from_vec(bytes)
    }

    #[test]
    fn test_read_basic_chunk() {
        let mut buf = make_chunk_bytes("fmt ", &[1, 2, 3, 4]);
        let chunk = read_riff_chunk(&mut buf, true, false);
        assert_eq!(chunk.header, "fmt ");
        assert_eq!(chunk.size, 4);
        assert_eq!(chunk.data.len(), 4);
        assert_eq!(chunk.data[0], 1);
        assert_eq!(chunk.data[3], 4);
        // Cursor advanced past header(4) + size(4) + data(4) = 12
        assert_eq!(buf.current_index, 12);
    }

    #[test]
    fn test_read_chunk_odd_size_advances_pad_byte() {
        // Odd data size → cursor must skip one extra pad byte
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&3u32.to_le_bytes()); // size = 3
        bytes.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0x00]); // 3 bytes + 1 pad
        let mut buf = IndexedByteArray::from_vec(bytes);
        let chunk = read_riff_chunk(&mut buf, true, false);
        assert_eq!(chunk.size, 3);
        assert_eq!(chunk.data.len(), 3);
        // 4 (header) + 4 (size) + 3 (data) + 1 (pad) = 12
        assert_eq!(buf.current_index, 12);
    }

    #[test]
    fn test_read_chunk_read_data_false_no_advance() {
        let mut buf = make_chunk_bytes("fmt ", &[1, 2, 3, 4]);
        let chunk = read_riff_chunk(&mut buf, false, false);
        assert_eq!(chunk.header, "fmt ");
        assert_eq!(chunk.size, 4);
        assert_eq!(chunk.data.len(), 0); // data not read
        // Cursor stays at 8 (after header + size only)
        assert_eq!(buf.current_index, 8);
    }

    #[test]
    fn test_read_chunk_force_shift_advances_without_reading() {
        let mut buf = make_chunk_bytes("fmt ", &[1, 2, 3, 4]);
        let chunk = read_riff_chunk(&mut buf, false, true);
        assert_eq!(chunk.data.len(), 0); // data not read
        // Cursor advanced past all bytes: 4 + 4 + 4 = 12
        assert_eq!(buf.current_index, 12);
    }

    #[test]
    fn test_read_chunk_empty_header_evil_dls() {
        // First byte is 0x00 → header = "" → size must be set to 0
        let mut bytes = vec![0x00, 0x00, 0x00, 0x00]; // header = ""
        bytes.extend_from_slice(&100u32.to_le_bytes()); // file says size = 100
        let mut buf = IndexedByteArray::from_vec(bytes);
        let chunk = read_riff_chunk(&mut buf, true, false);
        assert_eq!(chunk.header, "");
        assert_eq!(chunk.size, 0); // overridden to 0
        assert_eq!(chunk.data.len(), 0);
    }

    #[test]
    fn test_read_multiple_chunks_sequential() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&[0x01, 0x02]);
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&[0x0A, 0x0B]);
        let mut buf = IndexedByteArray::from_vec(bytes);

        let c1 = read_riff_chunk(&mut buf, true, false);
        assert_eq!(c1.header, "fmt ");
        assert_eq!(c1.data[0], 0x01);

        let c2 = read_riff_chunk(&mut buf, true, false);
        assert_eq!(c2.header, "data");
        assert_eq!(c2.data[0], 0x0A);
    }

    // -----------------------------------------------------------------------
    // write_riff_chunk_raw
    // -----------------------------------------------------------------------

    #[test]
    fn test_write_basic_chunk() {
        let out = write_riff_chunk_raw("fmt ", &[1, 2, 3, 4], false, false);
        // [fmt ][04 00 00 00][01 02 03 04]
        assert_eq!(out.len(), 12);
        let s: &[u8] = &out;
        assert_eq!(&s[0..4], b"fmt ");
        assert_eq!(&s[4..8], &4u32.to_le_bytes());
        assert_eq!(&s[8..12], &[1u8, 2, 3, 4]);
    }

    #[test]
    fn test_write_chunk_is_list() {
        let out = write_riff_chunk_raw("INFO", &[0xAA, 0xBB], false, true);
        // [LIST][06 00 00 00][INFO][AA BB]
        // written_size = 2 + 4 = 6
        // final_size = 12 + 2 = 14 (even, no extra pad)
        assert_eq!(out.len(), 14);
        let s: &[u8] = &out;
        assert_eq!(&s[0..4], b"LIST");
        assert_eq!(&s[4..8], &6u32.to_le_bytes());
        assert_eq!(&s[8..12], b"INFO");
        assert_eq!(s[12], 0xAA);
        assert_eq!(s[13], 0xBB);
    }

    #[test]
    fn test_write_chunk_add_zero_byte() {
        let out = write_riff_chunk_raw("TEST", &[1, 2], true, false);
        // data_length = 2 + 1 = 3, written_size = 3
        // final_size = 8 + 3 = 11 → round up to 12
        assert_eq!(out.len(), 12);
        let s: &[u8] = &out;
        assert_eq!(&s[4..8], &3u32.to_le_bytes()); // written_size = 3
        assert_eq!(s[8], 1);
        assert_eq!(s[9], 2);
        assert_eq!(s[10], 0); // zero byte
    }

    #[test]
    fn test_write_chunk_odd_data_gets_pad_byte() {
        let out = write_riff_chunk_raw("TEST", &[1, 2, 3], false, false);
        // final_size = 8 + 3 = 11 → padded to 12
        // written_size stays 3 (pad not counted)
        assert_eq!(out.len(), 12);
        let s: &[u8] = &out;
        assert_eq!(&s[4..8], &3u32.to_le_bytes());
        assert_eq!(s[11], 0); // pad byte
    }

    #[test]
    fn test_write_chunk_even_data_no_pad() {
        let out = write_riff_chunk_raw("TEST", &[1, 2], false, false);
        // final_size = 8 + 2 = 10 (even, no pad)
        assert_eq!(out.len(), 10);
    }

    // -----------------------------------------------------------------------
    // write_riff_chunk_parts
    // -----------------------------------------------------------------------

    #[test]
    fn test_write_parts_basic() {
        let c1: &[u8] = &[1, 2];
        let c2: &[u8] = &[3, 4];
        let out = write_riff_chunk_parts("TEST", &[c1, c2], false);
        // total data = 4, even
        assert_eq!(out.len(), 12);
        let s: &[u8] = &out;
        assert_eq!(&s[0..4], b"TEST");
        assert_eq!(&s[4..8], &4u32.to_le_bytes());
        assert_eq!(&s[8..12], &[1u8, 2, 3, 4]);
    }

    #[test]
    fn test_write_parts_is_list() {
        let c1: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF];
        let out = write_riff_chunk_parts("pdta", &[c1], true);
        // written_size = 4 + 4 = 8
        let s: &[u8] = &out;
        assert_eq!(&s[0..4], b"LIST");
        assert_eq!(&s[4..8], &8u32.to_le_bytes());
        assert_eq!(&s[8..12], b"pdta");
        assert_eq!(&s[12..16], &[0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn test_write_parts_empty_chunks() {
        let out = write_riff_chunk_parts("TEST", &[], false);
        // data_length = 0, written_size = 0, final_size = 8 (even)
        assert_eq!(out.len(), 8);
        let s: &[u8] = &out;
        assert_eq!(&s[0..4], b"TEST");
        assert_eq!(&s[4..8], &0u32.to_le_bytes());
    }

    #[test]
    fn test_write_parts_odd_total_gets_pad() {
        let c1: &[u8] = &[1, 2, 3];
        let out = write_riff_chunk_parts("TEST", &[c1], false);
        // final_size = 8 + 3 = 11 → 12
        assert_eq!(out.len(), 12);
        assert_eq!(out[11], 0);
    }

    // -----------------------------------------------------------------------
    // find_riff_list_type
    // -----------------------------------------------------------------------

    fn make_list_chunk(list_type: &str, content: &[u8]) -> RIFFChunk {
        // data = [list_type 4B][content]
        let mut data_bytes = Vec::new();
        data_bytes.extend_from_slice(list_type.as_bytes());
        data_bytes.extend_from_slice(content);
        RIFFChunk::new(
            "LIST".to_string(),
            data_bytes.len() as u32,
            IndexedByteArray::from_vec(data_bytes),
        )
    }

    #[test]
    fn test_find_list_type_found() {
        let mut collection = vec![make_list_chunk("INFO", &[1, 2, 3, 4])];
        let found = find_riff_list_type(&mut collection, "INFO");
        assert!(found.is_some());
        let chunk = found.unwrap();
        assert_eq!(chunk.header, "LIST");
        // cursor set to 4 (past list type FourCC)
        assert_eq!(chunk.data.current_index, 4);
    }

    #[test]
    fn test_find_list_type_not_found() {
        let mut collection = vec![make_list_chunk("INFO", &[1, 2])];
        let found = find_riff_list_type(&mut collection, "pdta");
        assert!(found.is_none());
    }

    #[test]
    fn test_find_list_type_skips_non_list_chunks() {
        let non_list = RIFFChunk::new("RIFF".to_string(), 4, IndexedByteArray::from_slice(b"sfbk"));
        let list_chunk = make_list_chunk("INFO", &[]);
        let mut collection = vec![non_list, list_chunk];
        let found = find_riff_list_type(&mut collection, "INFO");
        assert!(found.is_some());
        assert_eq!(found.unwrap().header, "LIST");
    }

    #[test]
    fn test_find_list_type_multiple_returns_first() {
        let mut collection = vec![
            make_list_chunk("pdta", &[0xAA]),
            make_list_chunk("INFO", &[0xBB]),
            make_list_chunk("INFO", &[0xCC]),
        ];
        let found = find_riff_list_type(&mut collection, "INFO").unwrap();
        // First matching chunk has content [0xBB]
        assert_eq!(found.data[4], 0xBB);
    }

    #[test]
    fn test_find_list_type_cursor_set_to_4() {
        let mut collection = vec![make_list_chunk("sdta", &[9, 8, 7])];
        let found = find_riff_list_type(&mut collection, "sdta").unwrap();
        assert_eq!(found.data.current_index, 4);
        // Verify sub-chunk content starts at index 4
        assert_eq!(found.data[4], 9);
    }

    // -----------------------------------------------------------------------
    // Round-trip: write → read
    // -----------------------------------------------------------------------

    #[test]
    fn test_roundtrip_basic_chunk() {
        let original_data: &[u8] = &[0x10, 0x20, 0x30, 0x40];
        let written = write_riff_chunk_raw("fmt ", original_data, false, false);
        let mut buf = IndexedByteArray::from_vec(written.to_vec());
        let chunk = read_riff_chunk(&mut buf, true, false);
        assert_eq!(chunk.header, "fmt ");
        assert_eq!(chunk.size, 4);
        let s: &[u8] = &chunk.data;
        assert_eq!(&s[0..4], original_data);
    }

    #[test]
    fn test_roundtrip_list_chunk_then_find() {
        let content: &[u8] = &[0xDE, 0xAD];
        let written = write_riff_chunk_raw("INFO", content, false, true);
        let mut buf = IndexedByteArray::from_vec(written.to_vec());
        let chunk = read_riff_chunk(&mut buf, true, false);
        assert_eq!(chunk.header, "LIST");

        let mut collection = vec![chunk];
        let found = find_riff_list_type(&mut collection, "INFO").unwrap();
        // After find, cursor is at 4 (past "INFO")
        assert_eq!(found.data.current_index, 4);
        assert_eq!(found.data[4], 0xDE);
        assert_eq!(found.data[5], 0xAD);
    }
}
