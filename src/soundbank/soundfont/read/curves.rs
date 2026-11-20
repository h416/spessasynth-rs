//! Reads the SFe custom curves (CURV chunk).
//! Ported from: src/soundbank/soundfont/read/curves.ts
use std::sync::Arc;

use crate::soundbank::basic_soundbank::custom_curve::{CustomCurve, CustomCurvePoint};
use crate::utils::byte_functions::little_endian::read_little_endian_indexed;
use crate::utils::byte_functions::string::read_binary_string_indexed;
use crate::utils::loggin::SpessaLog;
use crate::utils::riff_chunk::RIFFChunk;

/// The name and the point count.
/// Equivalent to: CURVE_HEADER_SIZE
const CURVE_HEADER_SIZE: usize = 42;

/// Reads the SFe custom curves.
/// Returns the curves, an invalid curve is `None`.
/// Panics (TS throws) if the chunk's size does not match its counts (spec 5.4), such as the drafts with a WORD count.
/// Equivalent to: readCurves(curveChunk)
pub fn read_curves(curve_chunk: &mut RIFFChunk) -> Vec<Option<Arc<CustomCurve>>> {
    let data = &mut curve_chunk.data;
    let mut curves: Vec<Option<Arc<CustomCurve>>> = Vec::new();
    if data.len() < 4 {
        panic!("The CURV chunk is too short.");
    }
    let count = read_little_endian_indexed(data, 4);
    for c in 0..count {
        if data.current_index + CURVE_HEADER_SIZE > data.len() {
            panic!("The CURV chunk is truncated at curve {c}.");
        }
        let name = read_binary_string_indexed(data, 40);
        let point_count = read_little_endian_indexed(data, 2) as usize;
        if data.current_index + point_count * 4 > data.len() {
            panic!("The CURV chunk is truncated at curve {c}.");
        }
        let mut points = Vec::with_capacity(point_count);
        for _ in 0..point_count {
            let x = read_little_endian_indexed(data, 2) as i32;
            let y = read_little_endian_indexed(data, 2) as i32;
            points.push(CustomCurvePoint { x, y });
        }
        // The constructor validates the points
        match CustomCurve::new(&points, &name) {
            Ok(curve) => curves.push(Some(Arc::new(curve))),
            Err(error) => {
                SpessaLog::warn(&format!(
                    "Custom curve {c} \"{name}\" is invalid, ignoring: {error}"
                ));
                curves.push(None);
            }
        }
    }
    if data.current_index != data.len() {
        panic!(
            "The CURV chunk has {} bytes after its last curve.",
            data.len() - data.current_index
        );
    }
    curves
}
