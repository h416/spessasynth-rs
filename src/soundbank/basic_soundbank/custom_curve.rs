//! A modulator source curve stored in an SFe bank (curve type 4).
//! Ported from: src/soundbank/basic_soundbank/custom_curve.ts
//!
//! Not ported (API only, no effect on the MIDI-to-WAV output): the links to the elements that use a curve
//! (`linkedTo`, `useCount`, `linkTo`, `unlinkFrom`) and `setPoints`. They keep the curves of an edited bank
//! consistent. A Rust curve is shared by the sources that use it (`Arc`) and never changes after loading.

use std::sync::LazyLock;

use crate::soundbank::basic_soundbank::modulator_curve::SOURCE_VALUE_7_BIT;

/// The source values a 7-bit value reaches, in either direction (x and 16384 - x).
/// Equivalent to: GRID_POSITIONS
static GRID_POSITIONS: LazyLock<Vec<i32>> = LazyLock::new(|| {
    let mut positions: Vec<i32> = Vec::new();
    let forward = SOURCE_VALUE_7_BIT.iter().map(|&x| x as i32);
    let reversed = SOURCE_VALUE_7_BIT.iter().map(|&x| 16_384 - x as i32);
    for x in forward.chain(reversed) {
        if !positions.contains(&x) {
            positions.push(x);
        }
    }
    positions
});

/// Maps a value of [`GRID_POSITIONS`] to its slot in the grids of a curve, -1 for any other value.
/// Equivalent to: GRID_SLOT
static GRID_SLOT: LazyLock<Vec<i16>> = LazyLock::new(|| {
    let mut slots = vec![-1i16; 16_385];
    for (slot, &x) in GRID_POSITIONS.iter().enumerate() {
        slots[x as usize] = slot as i16;
    }
    slots
});

/// A single point of a [`CustomCurve`].
/// Equivalent to: CustomCurvePoint
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CustomCurvePoint {
    /// The source value, 0 to 16,384, after the center-max mapping of SFe 4.0, section 7.2.
    /// A 7-bit value `v` up to 64 is at `v * 128`, above it at `round(8192 + 8192 * (v - 64) / 63)`, so 127 is 16,384.
    /// A 14-bit value `w` up to 8192 is at `w`, above it at `round(8192 + 8192 * (w - 8192) / 8191)`.
    pub x: i32,
    /// The output, 0 to 16,384. A unipolar source outputs `y / 16384`, a bipolar source `(y - 8192) / 8192`.
    pub y: i32,
}

/// A modulator source curve stored in an SFe bank (curve type 4).
/// The value between two points is linearly interpolated.
///
/// The first point should be at `x = 0` and the last at `x = 16384`. `x` never decreases, and two points at the
/// same `x` make a vertical step.
/// Equivalent to: class CustomCurve
#[derive(Clone, Debug)]
pub struct CustomCurve {
    /// The name of the curve, up to 40 characters are written.
    pub name: String,
    stored_points: Vec<CustomCurvePoint>,
    // The points, for the evaluation
    px: Vec<i32>,
    py: Vec<f64>,
    // For each x >> 7 (0 to 128): the last point with x <= (x >> 7) * 128, -1 if there is none
    start: [i32; 129],
    // The unipolar and bipolar outputs at every value a 7-bit source reaches, so they are a single read
    grid_unipolar: Vec<f32>,
    grid_bipolar: Vec<f32>,
}

/// Custom curves are the same only if their points are.
/// Equivalent to: isIdentical(other)
impl PartialEq for CustomCurve {
    fn eq(&self, other: &Self) -> bool {
        self.stored_points == other.stored_points
    }
}

impl CustomCurve {
    /// Creates a new custom curve. `points` are ordered by `x`, at least two are needed.
    /// Returns the error message of the TS constructor if the points are invalid.
    /// Equivalent to: constructor(points, name)
    pub fn new(points: &[CustomCurvePoint], name: &str) -> Result<Self, String> {
        let stored_points = Self::check_points(points)?;
        let mut curve = Self {
            name: name.to_string(),
            stored_points,
            px: Vec::new(),
            py: Vec::new(),
            start: [0; 129],
            grid_unipolar: vec![0.0; GRID_POSITIONS.len()],
            grid_bipolar: vec![0.0; GRID_POSITIONS.len()],
        };
        curve.precompute();
        Ok(curve)
    }

    /// The curve points, ordered by `x`.
    /// Equivalent to: get points
    pub fn points(&self) -> &[CustomCurvePoint] {
        &self.stored_points
    }

    /// Validates the points.
    /// Equivalent to: checkPoints(points) (private static)
    fn check_points(points: &[CustomCurvePoint]) -> Result<Vec<CustomCurvePoint>, String> {
        if points.len() < 2 {
            return Err(format!(
                "A custom curve needs at least 2 points, got {}.",
                points.len()
            ));
        }
        for (i, p) in points.iter().enumerate() {
            if p.x < 0 || p.x > 16_384 || p.y < 0 || p.y > 16_384 {
                return Err(format!(
                    "Custom curve point {} ({}, {}) is out of range.",
                    i, p.x, p.y
                ));
            }
            if i > 0 && p.x < points[i - 1].x {
                return Err(format!(
                    "Custom curve point {} goes back in x ({} to {}).",
                    i,
                    points[i - 1].x,
                    p.x
                ));
            }
        }
        Ok(points.to_vec())
    }

    /// Gets the curve's `y` for a source value (0 to 16,384).
    /// At a vertical step, the last point with that `x` wins.
    /// Equivalent to: getY(x)
    pub fn get_y(&self, x: i32) -> f64 {
        let px = &self.px;
        let last = px.len() as i32 - 1;
        // Jump close to the segment, then step to the last point with px <= x
        let mut i = self.start[(x >> 7) as usize];
        while i < last && px[(i + 1) as usize] <= x {
            i += 1;
        }
        if i < 0 {
            return self.py[0];
        }
        if i == last {
            return self.py[last as usize];
        }
        let i = i as usize;
        let ay = self.py[i];
        ay + ((self.py[i + 1] - ay) * (x - px[i]) as f64) / (px[i + 1] - px[i]) as f64
    }

    /// Gets the output of a source using this curve.
    /// `raw` is the source value, 0 to 16,384.
    /// Returns 0 to 1, or -1 to 1 when bipolar.
    /// Equivalent to: getValue(raw, isBipolar, isNegative)
    pub fn get_value(&self, raw: usize, is_bipolar: bool, is_negative: bool) -> f32 {
        let raw = raw as i32;
        // The reversed value is 16384 - x like the SF2 curves, so 64 stays the center
        let x = if is_negative { 16_384 - raw } else { raw };
        // 7-bit sources always land on the grid
        let slot = GRID_SLOT[x as usize];
        if slot >= 0 {
            return if is_bipolar {
                self.grid_bipolar[slot as usize]
            } else {
                self.grid_unipolar[slot as usize]
            };
        }
        let y = self.get_y(x);
        // Rounded to a float like the built-in curve tables
        (if is_bipolar {
            (y - 8192.0) / 8192.0
        } else {
            y / 16_384.0
        }) as f32
    }

    /// Builds the evaluation arrays from the points.
    /// Equivalent to: precompute() (private)
    fn precompute(&mut self) {
        self.px = self.stored_points.iter().map(|p| p.x).collect();
        self.py = self.stored_points.iter().map(|p| p.y as f64).collect();
        let n = self.stored_points.len() as i32;
        let mut i: i32 = -1;
        for c in 0..=128i32 {
            let x = c * 128;
            while i < n - 1 && self.stored_points[(i + 1) as usize].x <= x {
                i += 1;
            }
            self.start[c as usize] = i;
        }
        for (slot, &x) in GRID_POSITIONS.iter().enumerate() {
            let y = self.get_y(x);
            self.grid_unipolar[slot] = (y / 16_384.0) as f32;
            self.grid_bipolar[slot] = ((y - 8192.0) / 8192.0) as f32;
        }
    }
}

impl std::fmt::Display for CustomCurve {
    /// Equivalent to: toString()
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.name.is_empty() {
            write!(f, "Custom curve ({} points)", self.stored_points.len())
        } else {
            write!(f, "{}", self.name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i32, y: i32) -> CustomCurvePoint {
        CustomCurvePoint { x, y }
    }

    #[test]
    fn interpolates_between_points() {
        let c = CustomCurve::new(&[p(0, 0), p(16_384, 16_384)], "").unwrap();
        assert_eq!(c.get_value(8192, false, false), 0.5);
        assert_eq!(c.get_value(8192, true, false), 0.0);
        assert!((c.get_value(100, false, false) - 100.0 / 16_384.0).abs() < 1e-7);
        assert_eq!(c.get_value(0, false, true), 1.0);
    }

    #[test]
    fn vertical_step_takes_the_last_point() {
        let c = CustomCurve::new(&[p(0, 0), p(8192, 0), p(8192, 16_384), p(16_384, 16_384)], "").unwrap();
        assert_eq!(c.get_value(8192, false, false), 1.0);
        assert_eq!(c.get_value(8191, false, false), 0.0);
    }

    #[test]
    fn rejects_invalid_points() {
        assert!(CustomCurve::new(&[p(0, 0)], "").is_err());
        assert!(CustomCurve::new(&[p(0, 0), p(-1, 0)], "").is_err());
        assert!(CustomCurve::new(&[p(10, 0), p(5, 0)], "").is_err());
    }
}
