/// zones.rs
/// purpose: reads pbag/ibag zone index tables from a SoundFont RIFF chunk.
/// Ported from: src/soundbank/soundfont/read/zones.ts
use crate::utils::little_endian::read_little_endian_indexed;
use crate::utils::riff_chunk::RIFFChunk;

/// Holds the generator and modulator start indexes read from a pbag/ibag chunk.
/// Equivalent to: `{ mod: number[]; gen: number[] }` return type of `readZoneIndexes`.
/// Note: TypeScript uses `gen`/`mod` as field names; in Rust (edition 2024) `gen` is a
/// reserved keyword, so the fields are named `gen_ndx` and `mod_ndx`.
pub struct ZoneIndexes {
    /// Generator start indexes (genNdx column from pbag/ibag).
    pub gen_ndx: Vec<u32>,
    /// Modulator start indexes (modNdx column from pbag/ibag).
    pub mod_ndx: Vec<u32>,
}

/// Reads zone indexes from a pbag or ibag RIFF chunk.
/// Each entry is 4 bytes: 2 bytes genNdx (LE) then 2 bytes modNdx (LE).
/// Equivalent to: `readZoneIndexes(zonesChunk)`
pub fn read_zone_indexes(zones_chunk: &mut RIFFChunk) -> ZoneIndexes {
    let mut mod_start_indexes: Vec<u32> = Vec::new();
    let mut gen_start_indexes: Vec<u32> = Vec::new();

    while zones_chunk.data.len() > zones_chunk.data.current_index {
        gen_start_indexes.push(read_little_endian_indexed(&mut zones_chunk.data, 2));
        mod_start_indexes.push(read_little_endian_indexed(&mut zones_chunk.data, 2));
    }

    ZoneIndexes {
        mod_ndx: mod_start_indexes,
        gen_ndx: gen_start_indexes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::indexed_array::IndexedByteArray;
    use crate::utils::riff_chunk::RIFFChunk;

    fn make_zones_chunk(entries: &[(u16, u16)]) -> RIFFChunk {
        // Each entry: genNdx (2B LE) then modNdx (2B LE)
        let mut bytes = Vec::new();
        for &(gen_ndx, mod_ndx) in entries {
            bytes.extend_from_slice(&gen_ndx.to_le_bytes());
            bytes.extend_from_slice(&mod_ndx.to_le_bytes());
        }
        RIFFChunk::new(
            "pbag".to_string(),
            bytes.len() as u32,
            IndexedByteArray::from_vec(bytes),
        )
    }

    #[test]
    fn test_empty_chunk_returns_empty_vecs() {
        let mut chunk = make_zones_chunk(&[]);
        let result = read_zone_indexes(&mut chunk);
        assert!(result.gen_ndx.is_empty());
        assert!(result.mod_ndx.is_empty());
    }

    #[test]
    fn test_single_entry() {
        let mut chunk = make_zones_chunk(&[(5, 3)]);
        let result = read_zone_indexes(&mut chunk);
        assert_eq!(result.gen_ndx, vec![5]);
        assert_eq!(result.mod_ndx, vec![3]);
    }

    #[test]
    fn test_multiple_entries() {
        let mut chunk = make_zones_chunk(&[(0, 0), (4, 2), (7, 5)]);
        let result = read_zone_indexes(&mut chunk);
        assert_eq!(result.gen_ndx, vec![0, 4, 7]);
        assert_eq!(result.mod_ndx, vec![0, 2, 5]);
    }

    #[test]
    fn test_gen_and_mod_interleaved_correctly() {
        // Bytes: [gen0_lo, gen0_hi, mod0_lo, mod0_hi, gen1_lo, gen1_hi, mod1_lo, mod1_hi]
        let mut chunk = make_zones_chunk(&[(0x0100, 0x0200), (0x0300, 0x0400)]);
        let result = read_zone_indexes(&mut chunk);
        assert_eq!(result.gen_ndx[0], 0x0100);
        assert_eq!(result.mod_ndx[0], 0x0200);
        assert_eq!(result.gen_ndx[1], 0x0300);
        assert_eq!(result.mod_ndx[1], 0x0400);
    }

    #[test]
    fn test_ibag_chunk_same_format_as_pbag() {
        // ibag and pbag have the same binary format; only the header name differs
        let entries = [(1, 0), (3, 1), (6, 3), (6, 4)];
        let mut chunk = make_zones_chunk(&entries);
        chunk.header = "ibag".to_string();
        let result = read_zone_indexes(&mut chunk);
        assert_eq!(result.gen_ndx.len(), 4);
        assert_eq!(result.mod_ndx.len(), 4);
        assert_eq!(result.gen_ndx, vec![1, 3, 6, 6]);
        assert_eq!(result.mod_ndx, vec![0, 1, 3, 4]);
    }

    #[test]
    fn test_cursor_exhausted_after_read() {
        let mut chunk = make_zones_chunk(&[(10, 20), (30, 40)]);
        read_zone_indexes(&mut chunk);
        // All 8 bytes consumed (2 entries × 4 bytes each)
        assert_eq!(chunk.data.current_index, 8);
        assert_eq!(chunk.data.len(), chunk.data.current_index);
    }
}
