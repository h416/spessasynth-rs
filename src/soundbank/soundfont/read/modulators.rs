/// modulators.rs
/// purpose: reads SF2 modulator records from a pmod/imod RIFF chunk.
/// Ported from: src/soundbank/soundfont/read/modulators.ts
use crate::soundbank::basic_soundbank::modulator::DecodedModulator;
use crate::utils::little_endian::{read_little_endian_indexed, signed_int16};
use crate::utils::riff_chunk::RIFFChunk;

/// Reads SF2 modulator records from a pmod or imod RIFF chunk.
/// Each record is 10 bytes: sourceEnum(2) + destination(2) + amount(2) + secSourceEnum(2) + transformType(2).
/// The terminal record (all zeros) is automatically stripped.
/// Equivalent to: `readModulators(modulatorChunk)`
pub fn read_modulators(modulator_chunk: &mut RIFFChunk) -> Vec<DecodedModulator> {
    let mut mods: Vec<DecodedModulator> = Vec::new();

    while modulator_chunk.data.len() > modulator_chunk.data.current_index {
        let source_enum = read_little_endian_indexed(&mut modulator_chunk.data, 2) as u16;
        let destination = read_little_endian_indexed(&mut modulator_chunk.data, 2) as i16;
        let byte1 = modulator_chunk.data[modulator_chunk.data.current_index];
        modulator_chunk.data.current_index += 1;
        let byte2 = modulator_chunk.data[modulator_chunk.data.current_index];
        modulator_chunk.data.current_index += 1;
        let amount = signed_int16(byte1, byte2);
        let secondary_source_enum = read_little_endian_indexed(&mut modulator_chunk.data, 2) as u16;
        let transform_type = read_little_endian_indexed(&mut modulator_chunk.data, 2) as u16;

        mods.push(DecodedModulator::new(
            source_enum,
            secondary_source_enum,
            destination,
            amount,
            transform_type,
        ));
    }

    // Remove terminal record
    mods.pop();
    mods
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::indexed_array::IndexedByteArray;
    use crate::utils::riff_chunk::RIFFChunk;

    /// Builds a pmod/imod chunk from a list of raw modulator field tuples.
    /// Each tuple: (source_enum, destination, amount, secondary_source_enum, transform_type)
    fn make_modulators_chunk(entries: &[(u16, u16, i16, u16, u16)]) -> RIFFChunk {
        let mut bytes = Vec::new();
        for &(src, dst, amt, sec, tfm) in entries {
            bytes.extend_from_slice(&src.to_le_bytes());
            bytes.extend_from_slice(&dst.to_le_bytes());
            bytes.extend_from_slice(&(amt as u16).to_le_bytes());
            bytes.extend_from_slice(&sec.to_le_bytes());
            bytes.extend_from_slice(&tfm.to_le_bytes());
        }
        // Append terminal record (10 zero bytes)
        bytes.extend_from_slice(&[0u8; 10]);
        RIFFChunk::new(
            "pmod".to_string(),
            bytes.len() as u32,
            IndexedByteArray::from_vec(bytes),
        )
    }

    #[test]
    fn test_only_terminal_returns_empty() {
        // A chunk with only the terminal record yields no modulators
        let mut chunk = RIFFChunk::new(
            "pmod".to_string(),
            10,
            IndexedByteArray::from_vec(vec![0u8; 10]),
        );
        let result = read_modulators(&mut chunk);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_modulator() {
        let mut chunk = make_modulators_chunk(&[(0x0502, 0x000A, 960, 0x0000, 0x0000)]);
        let result = read_modulators(&mut chunk);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].source_enum, 0x0502);
        assert_eq!(result[0].destination, 0x000A);
        assert_eq!(result[0].transform_amount, 960.0);
        assert_eq!(result[0].secondary_source_enum, 0x0000);
        assert_eq!(result[0].transform_type, 0x0000);
    }

    #[test]
    fn test_multiple_modulators() {
        let entries = [
            (0x0502, 0x0005, 200, 0x0000, 0x0000),
            (0x0102, 0x0011, -960, 0x0000, 0x0000),
        ];
        let mut chunk = make_modulators_chunk(&entries);
        let result = read_modulators(&mut chunk);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].source_enum, 0x0502);
        assert_eq!(result[1].source_enum, 0x0102);
        assert_eq!(result[1].transform_amount, -960.0);
    }

    #[test]
    fn test_terminal_record_is_stripped() {
        // One real modulator + terminal → only 1 returned
        let mut chunk = make_modulators_chunk(&[(1, 2, 100, 3, 4)]);
        let result = read_modulators(&mut chunk);
        assert_eq!(result.len(), 1);
        // Confirm the terminal (all-zero) was not included
        assert_ne!(result[0].source_enum, 0);
    }

    #[test]
    fn test_negative_amount_parsed_correctly() {
        let mut chunk = make_modulators_chunk(&[(0x0000, 0x0000, -1, 0x0000, 0x0000)]);
        let result = read_modulators(&mut chunk);
        assert_eq!(result[0].transform_amount, -1.0);
    }

    #[test]
    fn test_min_max_amount() {
        let mut chunk = make_modulators_chunk(&[
            (0x0000, 0x0000, i16::MAX, 0x0000, 0x0000),
            (0x0000, 0x0000, i16::MIN, 0x0000, 0x0000),
        ]);
        let result = read_modulators(&mut chunk);
        assert_eq!(result[0].transform_amount, i16::MAX as f64);
        assert_eq!(result[1].transform_amount, i16::MIN as f64);
    }

    #[test]
    fn test_all_fields_read_in_correct_order() {
        // Verify field order: src(2) dst(2) amt(2) sec(2) tfm(2)
        let mut chunk = make_modulators_chunk(&[(0x00AA, 0x000B, 500, 0x00CC, 0x0001)]);
        let result = read_modulators(&mut chunk);
        assert_eq!(result[0].source_enum, 0x00AA);
        assert_eq!(result[0].destination, 0x000B);
        assert_eq!(result[0].transform_amount, 500.0);
        assert_eq!(result[0].secondary_source_enum, 0x00CC);
        assert_eq!(result[0].transform_type, 0x0001);
    }

    #[test]
    fn test_cursor_exhausted_after_read() {
        let mut chunk = make_modulators_chunk(&[(1, 2, 3, 4, 5)]);
        read_modulators(&mut chunk);
        // 1 real + 1 terminal = 20 bytes total
        assert_eq!(chunk.data.current_index, 20);
        assert_eq!(chunk.data.current_index, chunk.data.len());
    }
}
