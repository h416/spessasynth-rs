//! Reads SF2 modulator records from a pmod, imod or DMOD chunk, with the SFe extended records.
//! Ported from: src/soundbank/soundfont/read/modulators.ts
use std::sync::Arc;

use crate::soundbank::basic_soundbank::custom_curve::CustomCurve;
use crate::soundbank::basic_soundbank::generator_types::{GENERATOR_LIMITS, MAX_GENERATOR, generator_types as gt};
use crate::soundbank::basic_soundbank::modulator::{
    DecodedModulator, MODULATOR_EXTENDED_FLAG, Modulator, XMOD_BYTE_SIZE,
};
use crate::soundbank::basic_soundbank::modulator_source::ModulatorSource;
use crate::soundbank::enums::{modulator_curve_types, modulator_sources, modulator_transform_types};
use crate::utils::byte_functions::little_endian::{read_little_endian_indexed, signed_int16};
use crate::utils::riff_chunk::RIFFChunk;

/// The destinations a modulator may have: every generator the bank model knows. A reserved number is unknown,
/// and so is endOper, which only ends a generator list.
/// Equivalent to: KNOWN_DESTINATIONS.has(destination)
fn is_known_destination(destination: i16) -> bool {
    (0..=MAX_GENERATOR).contains(&destination)
        && GENERATOR_LIMITS[destination as usize].is_some()
        && destination != gt::END_OPER
}

/// Equivalent to: isDepthRange(source)
fn is_depth_range(source: &ModulatorSource) -> bool {
    !source.is_cc && source.index == modulator_sources::MODULATION_DEPTH_RANGE
}

/// The curve a source gets.
/// Equivalent to: resolveCurve(source, index)
enum ResolvedCurve {
    /// Its own built-in curve.
    BuiltIn,
    /// A custom curve from the CURV chunk.
    Custom(Arc<CustomCurve>),
    /// Nothing: the modulator is ignored.
    Unresolved,
}

/// Reads the modulators.
/// * `modulator_chunk` - pmod, imod or DMOD
/// * `allow_sfe` - if destinations above endOper and custom curves are allowed (SFe banks)
/// * `curves` - the SFe custom curves (an invalid curve is `None`)
/// * `extended_chunk` - the SFe extended records (xdta pmod, imod or XMOD)
///
/// Equivalent to: readModulators(modulatorChunk, allowSFe, curves = [], extendedChunk?)
pub fn read_modulators(
    modulator_chunk: &mut RIFFChunk,
    allow_sfe: bool,
    curves: &[Option<Arc<CustomCurve>>],
    extended_chunk: Option<&mut RIFFChunk>,
) -> Vec<Modulator> {
    let mut mods: Vec<Modulator> = Vec::new();
    // Which modulators have an extended record, in an SFe file
    let mut flags: Vec<bool> = Vec::new();
    while modulator_chunk.data.len() > modulator_chunk.data.current_index {
        let data_array = &mut modulator_chunk.data;
        let source_enum = read_little_endian_indexed(data_array, 2) as u16;
        let destination = read_little_endian_indexed(data_array, 2) as u16;
        let byte1 = data_array[data_array.current_index];
        data_array.current_index += 1;
        let byte2 = data_array[data_array.current_index];
        data_array.current_index += 1;
        let amount = signed_int16(byte1, byte2);
        let secondary_source_enum = read_little_endian_indexed(data_array, 2) as u16;
        let transform_enum = read_little_endian_indexed(data_array, 2) as u16;
        // Bit 15 is the flag in an SFe file, the transform is the lower 15 bits.
        // Elsewhere the whole word is the transform, and bit 15 makes it unknown
        let flagged = allow_sfe && (transform_enum & MODULATOR_EXTENDED_FLAG) != 0;
        let transform_type = if allow_sfe {
            transform_enum & !MODULATOR_EXTENDED_FLAG
        } else {
            transform_enum
        };
        // A destination above i16 can only be invalid
        let destination = if destination > MAX_GENERATOR as u16 {
            gt::INVALID
        } else {
            destination as i16
        };
        let mut m = DecodedModulator::new(
            source_enum,
            secondary_source_enum,
            destination,
            amount as f64,
            transform_type,
        );
        if (!allow_sfe
            && (m.destination > gt::END_OPER
                // The modulation depth range is an SFe source
                || is_depth_range(&m.primary_source)
                || is_depth_range(&m.secondary_source)))
            // An unknown destination (a reserved number, or one a later version defines)
            || (m.destination != gt::INVALID && !is_known_destination(m.destination))
            // An unknown transform
            || (transform_type != modulator_transform_types::LINEAR as u16
                && transform_type != modulator_transform_types::ABSOLUTE as u16)
        {
            m.destination = gt::INVALID;
        }
        mods.push(m);
        flags.push(flagged);
    }

    // Remove terminal
    mods.pop();
    flags.pop();

    // One extended record for each flagged modulator, in order, then the terminal record.
    // A different count means the records can't be matched, so every flagged modulator is ignored.
    // The count includes modulators ignored for another reason, every reader must agree on it.
    let flagged_count = flags.iter().filter(|&&f| f).count();
    let mut ext = match extended_chunk {
        Some(chunk)
            if allow_sfe
                // A size that is not a whole number of records is as good as missing
                && chunk.data.len() % XMOD_BYTE_SIZE == 0
                && (chunk.data.len() / XMOD_BYTE_SIZE) as i64 - 1 == flagged_count as i64 =>
        {
            Some(&mut chunk.data)
        }
        _ => None,
    };
    let mut record = 0usize;

    // The curve a source gets: its custom curve, its own built-in one, or nothing if the modulator is ignored
    let resolve_curve = |source: &ModulatorSource, index: u32| -> ResolvedCurve {
        if source.curve_type == modulator_curve_types::CUSTOM && allow_sfe {
            return match curves.get(index as usize) {
                Some(Some(curve)) => ResolvedCurve::Custom(Arc::clone(curve)),
                _ => ResolvedCurve::Unresolved,
            };
        }
        // SF2 2.04 section 8.2: unknown curve types make the modulator ignored
        if source.curve_type <= modulator_curve_types::SWITCH {
            ResolvedCurve::BuiltIn
        } else {
            ResolvedCurve::Unresolved
        }
    };

    for (m, md) in mods.iter_mut().enumerate() {
        let mut source_curve = 0u32;
        let mut amount_curve = 0u32;
        // A reader that does not understand why a modulator has a record ignores it and still consumes the record
        let mut understood = true;
        if flags[m] {
            if let Some(ext) = ext.as_deref_mut() {
                ext.current_index = record * XMOD_BYTE_SIZE;
                record += 1;
                source_curve = read_little_endian_indexed(ext, 4);
                amount_curve = read_little_endian_indexed(ext, 4);
                // A later version may give the reserved bytes a meaning
                understood = (read_little_endian_indexed(ext, 4) | read_little_endian_indexed(ext, 4)) == 0;
            } else {
                understood = false;
            }
            // The only reason 4.0 gives a record is a custom curve
            if !md.uses_custom_curve() {
                understood = false;
            }
        } else if md.uses_custom_curve() {
            // A custom curve needs the flag
            understood = false;
        }
        let primary = resolve_curve(&md.primary_source, source_curve);
        let secondary = resolve_curve(&md.secondary_source, amount_curve);
        // Assigned only when both resolve, so an ignored modulator never links a curve
        match (understood, primary, secondary) {
            (true, ResolvedCurve::Unresolved, _) | (true, _, ResolvedCurve::Unresolved) | (false, _, _) => {
                md.destination = gt::INVALID;
            }
            (true, p, s) => {
                if let ResolvedCurve::Custom(c) = p {
                    md.primary_source.set_custom_curve(c);
                }
                if let ResolvedCurve::Custom(c) = s {
                    md.secondary_source.set_custom_curve(c);
                }
            }
        }
    }
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
            bytes.len(),
            IndexedByteArray::from_vec(bytes),
        )
    }

    #[test]
    fn test_known_destinations_match_typescript() {
        // Object.values(GeneratorTypes) without endOper, from TS
        let unknown = [14, 18, 19, 20, 42, 49, 55, 59, 60, 90];
        for d in 0..=90i16 {
            assert_eq!(is_known_destination(d), !unknown.contains(&d), "destination {d}");
        }
    }

    #[test]
    fn test_only_terminal_returns_empty() {
        // A chunk with only the terminal record yields no modulators
        let mut chunk = RIFFChunk::new(
            "pmod".to_string(),
            10,
            IndexedByteArray::from_vec(vec![0u8; 10]),
        );
        let result = read_modulators(&mut chunk, true, &[], None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_modulator() {
        let mut chunk = make_modulators_chunk(&[(0x0502, 0x000A, 960, 0x0000, 0x0000)]);
        let result = read_modulators(&mut chunk, true, &[], None);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].primary_source.to_source_enum(), 0x0502);
        assert_eq!(result[0].destination, 0x000A);
        assert_eq!(result[0].transform_amount, 960.0);
        assert_eq!(result[0].secondary_source.to_source_enum(), 0x0000);
        assert_eq!(result[0].transform_type, 0x0000);
    }

    #[test]
    fn test_multiple_modulators() {
        let entries = [
            (0x0502, 0x0005, 200, 0x0000, 0x0000),
            (0x0102, 0x0011, -960, 0x0000, 0x0000),
        ];
        let mut chunk = make_modulators_chunk(&entries);
        let result = read_modulators(&mut chunk, true, &[], None);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].primary_source.to_source_enum(), 0x0502);
        assert_eq!(result[1].primary_source.to_source_enum(), 0x0102);
        assert_eq!(result[1].transform_amount, -960.0);
    }

    #[test]
    fn test_terminal_record_is_stripped() {
        // One real modulator + terminal → only 1 returned
        let mut chunk = make_modulators_chunk(&[(1, 2, 100, 3, 4)]);
        let result = read_modulators(&mut chunk, true, &[], None);
        assert_eq!(result.len(), 1);
        // Confirm the terminal (all-zero) was not included
        assert_ne!(result[0].primary_source.to_source_enum(), 0);
    }

    #[test]
    fn test_negative_amount_parsed_correctly() {
        let mut chunk = make_modulators_chunk(&[(0x0000, 0x0000, -1, 0x0000, 0x0000)]);
        let result = read_modulators(&mut chunk, true, &[], None);
        assert_eq!(result[0].transform_amount, -1.0);
    }

    #[test]
    fn test_min_max_amount() {
        let mut chunk = make_modulators_chunk(&[
            (0x0000, 0x0000, i16::MAX, 0x0000, 0x0000),
            (0x0000, 0x0000, i16::MIN, 0x0000, 0x0000),
        ]);
        let result = read_modulators(&mut chunk, true, &[], None);
        assert_eq!(result[0].transform_amount, i16::MAX as f64);
        assert_eq!(result[1].transform_amount, i16::MIN as f64);
    }

    #[test]
    fn test_all_fields_read_in_correct_order() {
        // Verify field order: src(2) dst(2) amt(2) sec(2) tfm(2)
        let mut chunk = make_modulators_chunk(&[(0x0502, 0x000B, 500, 0x0081, 0x0002)]);
        let result = read_modulators(&mut chunk, true, &[], None);
        assert_eq!(result[0].primary_source.to_source_enum(), 0x0502);
        assert_eq!(result[0].destination, 0x000B);
        assert_eq!(result[0].transform_amount, 500.0);
        assert_eq!(result[0].secondary_source.to_source_enum(), 0x0081);
        assert_eq!(result[0].transform_type, 0x0002);
    }

    #[test]
    fn test_cursor_exhausted_after_read() {
        let mut chunk = make_modulators_chunk(&[(1, 2, 3, 4, 5)]);
        read_modulators(&mut chunk, true, &[], None);
        // 1 real + 1 terminal = 20 bytes total
        assert_eq!(chunk.data.current_index, 20);
        assert_eq!(chunk.data.current_index, chunk.data.len());
    }
}
