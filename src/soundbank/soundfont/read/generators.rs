/// generators.rs
/// purpose: reads SF2 generator records from a pgen/igen RIFF chunk.
/// Ported from: src/soundbank/soundfont/read/generators.ts
///
/// Note: TypeScript has a `ReadGenerator` class that extends `Generator`.
/// Rust does not have class inheritance, so the constructor logic is implemented
/// as a private `read_generator()` function that returns a `Generator` directly.
use crate::soundbank::basic_soundbank::generator::Generator;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::read_little_endian_indexed;
use crate::utils::riff_chunk::RIFFChunk;

/// Reads one 4-byte generator record from `data_array`, advancing the cursor by 4.
/// Layout: [type_lo, type_hi, value_lo, value_hi] (all little-endian).
/// Validation is intentionally skipped — some SF2 files use out-of-range values
/// that become correct only after the modulator stage.
/// Equivalent to: `new ReadGenerator(dataArray)`
fn read_generator(data_array: &mut IndexedByteArray) -> Generator {
    // 2-byte LE generator type (cast to i16 = GeneratorType)
    let generator_type = read_little_endian_indexed(data_array, 2) as i16;
    // 2-byte LE generator value, interpreted as signed (equivalent to signedInt16)
    let generator_value = read_little_endian_indexed(data_array, 2) as i16;
    // false = no validation, matching: super(generatorType, generatorValue, false)
    Generator::new_unvalidated(generator_type, generator_value as f64)
}

/// Reads all SF2 generator records from a pgen or igen RIFF chunk.
/// The terminal record (all zeros) is automatically stripped.
/// Equivalent to: `readGenerators(generatorChunk)`
pub fn read_generators(generator_chunk: &mut RIFFChunk) -> Vec<Generator> {
    let mut gens: Vec<Generator> = Vec::new();

    while generator_chunk.data.len() > generator_chunk.data.current_index {
        gens.push(read_generator(&mut generator_chunk.data));
    }

    // Remove terminal record
    gens.pop();
    gens
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
    use crate::utils::indexed_array::IndexedByteArray;
    use crate::utils::riff_chunk::RIFFChunk;

    /// Builds a pgen/igen chunk from (type, value) pairs plus an automatic terminal record.
    fn make_generators_chunk(entries: &[(i16, i16)]) -> RIFFChunk {
        let mut bytes: Vec<u8> = Vec::new();
        for &(gen_type, gen_value) in entries {
            bytes.extend_from_slice(&(gen_type as u16).to_le_bytes());
            bytes.extend_from_slice(&(gen_value as u16).to_le_bytes());
        }
        // Terminal record: 4 zero bytes
        bytes.extend_from_slice(&[0u8; 4]);
        RIFFChunk::new(
            "pgen".to_string(),
            bytes.len() as u32,
            IndexedByteArray::from_vec(bytes),
        )
    }

    #[test]
    fn test_only_terminal_returns_empty() {
        let mut chunk = RIFFChunk::new(
            "pgen".to_string(),
            4,
            IndexedByteArray::from_vec(vec![0u8; 4]),
        );
        let result = read_generators(&mut chunk);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_generator() {
        // pan (17) = 0x0011, value = 100 = 0x0064
        let mut chunk = make_generators_chunk(&[(gt::PAN, 100)]);
        let result = read_generators(&mut chunk);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].generator_type, gt::PAN);
        assert_eq!(result[0].generator_value, 100);
    }

    #[test]
    fn test_multiple_generators() {
        let entries = [
            (gt::PAN, 50),
            (gt::INITIAL_FILTER_FC, 8000),
            (gt::REVERB_EFFECTS_SEND, 200),
        ];
        let mut chunk = make_generators_chunk(&entries);
        let result = read_generators(&mut chunk);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].generator_type, gt::PAN);
        assert_eq!(result[0].generator_value, 50);
        assert_eq!(result[1].generator_type, gt::INITIAL_FILTER_FC);
        assert_eq!(result[1].generator_value, 8000);
        assert_eq!(result[2].generator_type, gt::REVERB_EFFECTS_SEND);
        assert_eq!(result[2].generator_value, 200);
    }

    #[test]
    fn test_terminal_record_stripped() {
        // 1 real entry + terminal → only 1 returned
        let mut chunk = make_generators_chunk(&[(gt::PAN, 0)]);
        let result = read_generators(&mut chunk);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_negative_value_parsed_correctly() {
        // Negative attenuation — the real-world case mentioned in the comment
        let mut chunk = make_generators_chunk(&[(gt::INITIAL_ATTENUATION, -100)]);
        let result = read_generators(&mut chunk);
        assert_eq!(result[0].generator_value, -100);
    }

    #[test]
    fn test_validation_skipped_out_of_range_value_preserved() {
        // initialFilterFc normal max = 13500, but out-of-range values must be preserved
        // (validation is intentionally skipped in ReadGenerator / read_generator)
        let out_of_range: i16 = 20000_i16.min(i16::MAX);
        let mut chunk = make_generators_chunk(&[(gt::INITIAL_FILTER_FC, out_of_range)]);
        let result = read_generators(&mut chunk);
        assert_eq!(result[0].generator_value, out_of_range);
    }

    #[test]
    fn test_generator_type_little_endian() {
        // Type bytes: [0x11, 0x00] → 0x0011 = 17 = pan
        let bytes: Vec<u8> = vec![
            0x11, 0x00, 0x00, 0x00, // pan, value=0
            0x00, 0x00, 0x00, 0x00, // terminal
        ];
        let mut chunk = RIFFChunk::new("pgen".to_string(), 8, IndexedByteArray::from_vec(bytes));
        let result = read_generators(&mut chunk);
        assert_eq!(result[0].generator_type, gt::PAN);
    }

    #[test]
    fn test_generator_value_little_endian_signed() {
        // value bytes: [0x9C, 0xFF] = 0xFF9C as u16 = -100 as i16
        let bytes: Vec<u8> = vec![
            0x11, 0x00, 0x9C, 0xFF, // pan, value=-100
            0x00, 0x00, 0x00, 0x00, // terminal
        ];
        let mut chunk = RIFFChunk::new("pgen".to_string(), 8, IndexedByteArray::from_vec(bytes));
        let result = read_generators(&mut chunk);
        assert_eq!(result[0].generator_value, -100);
    }

    #[test]
    fn test_cursor_exhausted_after_read() {
        // 2 real entries + terminal = 3 records × 4 bytes = 12 bytes
        let mut chunk = make_generators_chunk(&[(gt::PAN, 0), (gt::REVERB_EFFECTS_SEND, 0)]);
        read_generators(&mut chunk);
        assert_eq!(chunk.data.current_index, 12);
        assert_eq!(chunk.data.current_index, chunk.data.len());
    }

    #[test]
    fn test_i16_min_max_values() {
        let mut chunk = make_generators_chunk(&[(gt::PAN, i16::MAX), (gt::PAN, i16::MIN)]);
        let result = read_generators(&mut chunk);
        assert_eq!(result[0].generator_value, i16::MAX);
        assert_eq!(result[1].generator_value, i16::MIN);
    }
}
