/// types.rs
/// purpose: Helper type definitions used during SF2 writing.
/// Ported from: src/soundbank/soundfont/write/types.ts
use crate::utils::indexed_array::IndexedByteArray;

/// Struct holding extended SF2 chunks.
/// Equivalent to: `interface ExtendedSF2Chunks`
pub struct ExtendedSF2Chunks {
    /// PDTA (preset / instrument / sample header) chunk portion.
    pub pdta: IndexedByteArray,

    /// XDTA (extended limits proposal: https://github.com/spessasus/soundfont-proposals/blob/main/extended_limits.md) chunk portion.
    pub xdta: IndexedByteArray,
}

/// Index tracking struct for SoundFont file writing.
/// Equivalent to: `interface SoundFontWriteIndexes`
///
/// Field names `gen` and `mod` are Rust reserved words, so they are accessed as `r#gen` / `r#mod`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SoundFontWriteIndexes {
    /// Generator start index. (TS `gen`)
    pub r#gen: usize,
    /// Modulator start index. (TS `mod`)
    pub r#mod: usize,
    /// Zone (bag) start index.
    pub bag: usize,
    /// Preset / instrument header start index.
    pub hdr: usize,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // SoundFontWriteIndexes
    // -----------------------------------------------------------------------

    #[test]
    fn test_write_indexes_default_is_all_zeros() {
        let idx = SoundFontWriteIndexes::default();
        assert_eq!(idx.r#gen, 0);
        assert_eq!(idx.r#mod, 0);
        assert_eq!(idx.bag, 0);
        assert_eq!(idx.hdr, 0);
    }

    #[test]
    fn test_write_indexes_fields_are_assignable() {
        let idx = SoundFontWriteIndexes {
            r#gen: 10,
            r#mod: 20,
            bag: 30,
            hdr: 40,
        };
        assert_eq!(idx.r#gen, 10);
        assert_eq!(idx.r#mod, 20);
        assert_eq!(idx.bag, 30);
        assert_eq!(idx.hdr, 40);
    }

    #[test]
    fn test_write_indexes_copy() {
        let original = SoundFontWriteIndexes {
            r#gen: 1,
            r#mod: 2,
            bag: 3,
            hdr: 4,
        };
        let copy = original;
        assert_eq!(copy, original);
    }

    #[test]
    fn test_write_indexes_clone() {
        let original = SoundFontWriteIndexes {
            r#gen: 5,
            r#mod: 6,
            bag: 7,
            hdr: 8,
        };
        let cloned = original.clone();
        assert_eq!(cloned.r#gen, 5);
        assert_eq!(cloned.r#mod, 6);
        assert_eq!(cloned.bag, 7);
        assert_eq!(cloned.hdr, 8);
    }

    // -----------------------------------------------------------------------
    // ExtendedSF2Chunks
    // -----------------------------------------------------------------------

    #[test]
    fn test_extended_sf2_chunks_pdta_accessible() {
        let chunks = ExtendedSF2Chunks {
            pdta: IndexedByteArray::from_vec(vec![0x01u8, 0x02, 0x03]),
            xdta: IndexedByteArray::from_vec(vec![0xAAu8, 0xBB]),
        };
        assert_eq!(chunks.pdta.len(), 3);
        assert_eq!(chunks.pdta[0], 0x01);
        assert_eq!(chunks.pdta[2], 0x03);
    }

    #[test]
    fn test_extended_sf2_chunks_xdta_accessible() {
        let chunks = ExtendedSF2Chunks {
            pdta: IndexedByteArray::new(0),
            xdta: IndexedByteArray::from_vec(vec![0xDEu8, 0xAD]),
        };
        assert_eq!(chunks.xdta.len(), 2);
        assert_eq!(chunks.xdta[0], 0xDE);
        assert_eq!(chunks.xdta[1], 0xAD);
    }

    #[test]
    fn test_extended_sf2_chunks_empty() {
        let chunks = ExtendedSF2Chunks {
            pdta: IndexedByteArray::new(0),
            xdta: IndexedByteArray::new(0),
        };
        assert!(chunks.pdta.is_empty());
        assert!(chunks.xdta.is_empty());
    }

    #[test]
    fn test_extended_sf2_chunks_independent_data() {
        // Verify that pdta and xdta are independent
        let chunks = ExtendedSF2Chunks {
            pdta: IndexedByteArray::from_vec(vec![1u8, 2, 3]),
            xdta: IndexedByteArray::from_vec(vec![4u8, 5, 6]),
        };
        assert_eq!(chunks.pdta[0], 1);
        assert_eq!(chunks.xdta[0], 4);
    }
}
