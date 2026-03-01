/// basic_zone.rs
/// purpose: Base zone struct shared by preset zones and instrument zones.
/// Ported from: src/soundbank/basic_soundbank/basic_zone.ts
use crate::soundbank::basic_soundbank::generator::Generator;
use crate::soundbank::basic_soundbank::generator_types::{
    GENERATOR_LIMITS, GeneratorType, generator_types as gt,
};
use crate::soundbank::basic_soundbank::modulator::Modulator;
use crate::soundbank::types::GenericRange;

/// Byte size of one PBAG/IBAG record in SF2.
/// Equivalent to: export const BAG_BYTE_SIZE = 4
pub const BAG_BYTE_SIZE: usize = 4;

/// Base zone: generators, modulators, key range, and velocity range.
/// Equivalent to: class BasicZone
#[derive(Clone, Debug)]
pub struct BasicZone {
    /// Velocity range. `min == -1.0` means the zone has no explicit velocity range.
    /// Equivalent to: velRange: GenericRange = { min: -1, max: 127 }
    pub vel_range: GenericRange,

    /// Key range. `min == -1.0` means the zone has no explicit key range.
    /// Equivalent to: keyRange: GenericRange = { min: -1, max: 127 }
    pub key_range: GenericRange,

    /// Zone generators (keyRange/velRange/sampleID/instrument handled separately).
    /// Equivalent to: generators: Generator[] = []
    pub generators: Vec<Generator>,

    /// Zone modulators.
    /// Equivalent to: modulators: Modulator[] = []
    pub modulators: Vec<Modulator>,
}

impl Default for BasicZone {
    fn default() -> Self {
        Self {
            vel_range: GenericRange {
                min: -1.0,
                max: 127.0,
            },
            key_range: GenericRange {
                min: -1.0,
                max: 127.0,
            },
            generators: Vec::new(),
            modulators: Vec::new(),
        }
    }
}

impl BasicZone {
    /// Creates a new BasicZone with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if an explicit key range has been set.
    /// Equivalent to: get hasKeyRange(): boolean
    #[inline]
    pub fn has_key_range(&self) -> bool {
        self.key_range.min != -1.0
    }

    /// Returns true if an explicit velocity range has been set.
    /// Equivalent to: get hasVelRange(): boolean
    #[inline]
    pub fn has_vel_range(&self) -> bool {
        self.vel_range.min != -1.0
    }

    /// Returns the current tuning in cents (coarse semitones × 100 + fine cents).
    /// Equivalent to: get fineTuning(): number
    pub fn fine_tuning(&self) -> i32 {
        let current_coarse = self.get_generator(gt::COARSE_TUNE, 0);
        let current_fine = self.get_generator(gt::FINE_TUNE, 0);
        current_coarse * 100 + current_fine
    }

    /// Sets tuning in cents, splitting into coarse semitones and fine cents.
    /// Equivalent to: set fineTuning(tuningCents: number)
    pub fn set_fine_tuning(&mut self, tuning_cents: i32) {
        // i32 division truncates toward zero, matching Math.trunc(tuningCents / 100)
        let coarse = tuning_cents / 100;
        let fine = tuning_cents % 100;
        self.set_generator(gt::COARSE_TUNE, Some(coarse as f64), true);
        self.set_generator(gt::FINE_TUNE, Some(fine as f64), true);
    }

    /// Adds `value` to the current generator value (or its default if absent).
    /// Equivalent to: addToGenerator(type: GeneratorType, value: number, validate = true)
    pub fn add_to_generator(&mut self, gen_type: GeneratorType, value: i32, validate: bool) {
        let default_val = if gen_type >= 0 {
            GENERATOR_LIMITS
                .get(gen_type as usize)
                .and_then(|l| *l)
                .map(|l| l.def)
                .unwrap_or(0)
        } else {
            0
        };
        let gen_value = self.get_generator(gen_type, default_val);
        self.set_generator(gen_type, Some((value + gen_value) as f64), validate);
    }

    /// Sets a generator to `value`, or removes it if `value` is `None`.
    /// Panics for sampleID / instrument / velRange / keyRange (use dedicated methods).
    /// Equivalent to: setGenerator(type, value: number | null, validate = true)
    pub fn set_generator(&mut self, gen_type: GeneratorType, value: Option<f64>, validate: bool) {
        match gen_type {
            gt::SAMPLE_ID => panic!("Use setSample()"),
            gt::INSTRUMENT => panic!("Use setInstrument()"),
            gt::VEL_RANGE | gt::KEY_RANGE => panic!("Set the range manually"),
            _ => {}
        }
        match value {
            None => {
                self.generators.retain(|g| g.generator_type != gen_type);
            }
            Some(v) => {
                let new_gen = if validate {
                    Generator::new(gen_type, v)
                } else {
                    Generator::new_unvalidated(gen_type, v)
                };
                if let Some(pos) = self
                    .generators
                    .iter()
                    .position(|g| g.generator_type == gen_type)
                {
                    self.generators[pos] = new_gen;
                } else {
                    self.add_generators(&[new_gen]);
                }
            }
        }
    }

    /// Appends generators, routing range/special types to dedicated fields.
    /// Equivalent to: addGenerators(...generators: Generator[])
    pub fn add_generators(&mut self, generators: &[Generator]) {
        for g in generators {
            match g.generator_type {
                gt::SAMPLE_ID | gt::INSTRUMENT => {
                    // Don't add these; they have their own properties on subclasses
                }
                gt::VEL_RANGE => {
                    self.vel_range.min = (g.generator_value & 0x7f) as f64;
                    self.vel_range.max = ((g.generator_value >> 8) & 0x7f) as f64;
                }
                gt::KEY_RANGE => {
                    self.key_range.min = (g.generator_value & 0x7f) as f64;
                    self.key_range.max = ((g.generator_value >> 8) & 0x7f) as f64;
                }
                _ => {
                    self.generators.push(g.clone());
                }
            }
        }
    }

    /// Appends modulators to the zone.
    /// Equivalent to: addModulators(...modulators: Modulator[])
    pub fn add_modulators(&mut self, modulators: &[Modulator]) {
        self.modulators.extend_from_slice(modulators);
    }

    /// Returns the value of `generator_type`, or `not_found_value` if absent.
    /// Equivalent to: getGenerator<K>(generatorType, notFoundValue: number | K): number | K
    pub fn get_generator(&self, generator_type: GeneratorType, not_found_value: i32) -> i32 {
        self.generators
            .iter()
            .find(|g| g.generator_type == generator_type)
            .map(|g| g.generator_value as i32)
            .unwrap_or(not_found_value)
    }

    /// Deep-copies generators, modulators, and ranges from another zone.
    /// Equivalent to: copyFrom(zone: BasicZone)
    pub fn copy_from(&mut self, zone: &BasicZone) {
        self.generators = zone
            .generators
            .iter()
            .map(|g| Generator::new_unvalidated(g.generator_type, g.generator_value as f64))
            .collect();
        self.modulators = zone.modulators.clone();
        self.vel_range = zone.vel_range.clone();
        self.key_range = zone.key_range.clone();
    }

    /// Returns write-ready generators for this zone, with range generators prepended.
    ///
    /// The `_bank` parameter is unused in `BasicZone` but required for subclass
    /// consistency (`BasicPresetZone` / `BasicInstrumentZone` use it to resolve indexes).
    /// In TypeScript, the bank is validated as non-null; Rust references are always
    /// valid so no runtime check is required.
    ///
    /// Equivalent to: getWriteGenerators(bank: BasicSoundBank): Generator[]
    pub fn get_write_generators<B>(&self, _bank: &B) -> Vec<Generator> {
        let mut generators: Vec<Generator> = self
            .generators
            .iter()
            .filter(|g| {
                g.generator_type != gt::SAMPLE_ID
                    && g.generator_type != gt::INSTRUMENT
                    && g.generator_type != gt::KEY_RANGE
                    && g.generator_type != gt::VEL_RANGE
            })
            .cloned()
            .collect();

        // Unshift vel then key (to make key first in the output list).
        // Matches the TypeScript `generators.unshift(velRange); generators.unshift(keyRange);`
        if self.has_vel_range() {
            let packed = ((self.vel_range.max as i32) << 8) | (self.vel_range.min as i32).max(0);
            generators.insert(0, Generator::new_unvalidated(gt::VEL_RANGE, packed as f64));
        }
        if self.has_key_range() {
            let packed = ((self.key_range.max as i32) << 8) | (self.key_range.min as i32).max(0);
            generators.insert(0, Generator::new_unvalidated(gt::KEY_RANGE, packed as f64));
        }

        generators
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;

    // --- BAG_BYTE_SIZE ---

    #[test]
    fn test_bag_byte_size() {
        assert_eq!(BAG_BYTE_SIZE, 4);
    }

    // --- Default / new() ---

    #[test]
    fn test_default_vel_range_min_is_minus_one() {
        let z = BasicZone::new();
        assert_eq!(z.vel_range.min, -1.0);
    }

    #[test]
    fn test_default_vel_range_max_is_127() {
        let z = BasicZone::new();
        assert_eq!(z.vel_range.max, 127.0);
    }

    #[test]
    fn test_default_key_range_min_is_minus_one() {
        let z = BasicZone::new();
        assert_eq!(z.key_range.min, -1.0);
    }

    #[test]
    fn test_default_key_range_max_is_127() {
        let z = BasicZone::new();
        assert_eq!(z.key_range.max, 127.0);
    }

    #[test]
    fn test_default_generators_empty() {
        let z = BasicZone::new();
        assert!(z.generators.is_empty());
    }

    #[test]
    fn test_default_modulators_empty() {
        let z = BasicZone::new();
        assert!(z.modulators.is_empty());
    }

    // --- has_key_range / has_vel_range ---

    #[test]
    fn test_has_key_range_false_by_default() {
        let z = BasicZone::new();
        assert!(!z.has_key_range());
    }

    #[test]
    fn test_has_vel_range_false_by_default() {
        let z = BasicZone::new();
        assert!(!z.has_vel_range());
    }

    #[test]
    fn test_has_key_range_true_after_add() {
        let mut z = BasicZone::new();
        // packed: min=0, max=60  →  (60 << 8) | 0 = 15360
        z.add_generators(&[Generator::new_unvalidated(gt::KEY_RANGE, 15360.0)]);
        assert!(z.has_key_range());
    }

    #[test]
    fn test_has_vel_range_true_after_add() {
        let mut z = BasicZone::new();
        // packed: min=10, max=100  →  (100 << 8) | 10 = 25610
        z.add_generators(&[Generator::new_unvalidated(gt::VEL_RANGE, 25610.0)]);
        assert!(z.has_vel_range());
    }

    // --- fine_tuning getter ---

    #[test]
    fn test_fine_tuning_default_is_zero() {
        let z = BasicZone::new();
        assert_eq!(z.fine_tuning(), 0);
    }

    #[test]
    fn test_fine_tuning_coarse_only() {
        let mut z = BasicZone::new();
        z.set_generator(gt::COARSE_TUNE, Some(3.0), true);
        assert_eq!(z.fine_tuning(), 300);
    }

    #[test]
    fn test_fine_tuning_fine_only() {
        let mut z = BasicZone::new();
        z.set_generator(gt::FINE_TUNE, Some(50.0), true);
        assert_eq!(z.fine_tuning(), 50);
    }

    #[test]
    fn test_fine_tuning_combined() {
        let mut z = BasicZone::new();
        z.set_generator(gt::COARSE_TUNE, Some(2.0), true);
        z.set_generator(gt::FINE_TUNE, Some(25.0), true);
        assert_eq!(z.fine_tuning(), 225);
    }

    // --- set_fine_tuning setter ---

    #[test]
    fn test_set_fine_tuning_zero() {
        let mut z = BasicZone::new();
        z.set_fine_tuning(0);
        assert_eq!(z.get_generator(gt::COARSE_TUNE, -999), 0);
        assert_eq!(z.get_generator(gt::FINE_TUNE, -999), 0);
    }

    #[test]
    fn test_set_fine_tuning_exact_semitone() {
        let mut z = BasicZone::new();
        z.set_fine_tuning(200); // 2 semitones
        assert_eq!(z.get_generator(gt::COARSE_TUNE, -999), 2);
        assert_eq!(z.get_generator(gt::FINE_TUNE, -999), 0);
    }

    #[test]
    fn test_set_fine_tuning_with_fine_part() {
        let mut z = BasicZone::new();
        z.set_fine_tuning(350); // 3 semitones + 50 cents
        assert_eq!(z.get_generator(gt::COARSE_TUNE, -999), 3);
        assert_eq!(z.get_generator(gt::FINE_TUNE, -999), 50);
    }

    #[test]
    fn test_set_fine_tuning_negative() {
        let mut z = BasicZone::new();
        z.set_fine_tuning(-150); // -1 semitone + -50 cents
        assert_eq!(z.get_generator(gt::COARSE_TUNE, -999), -1);
        assert_eq!(z.get_generator(gt::FINE_TUNE, -999), -50);
    }

    #[test]
    fn test_set_then_get_fine_tuning_roundtrip() {
        let mut z = BasicZone::new();
        z.set_fine_tuning(425);
        assert_eq!(z.fine_tuning(), 425);
    }

    // --- add_to_generator ---

    #[test]
    fn test_add_to_generator_from_default() {
        let mut z = BasicZone::new();
        // pan default = 0; add 100
        z.add_to_generator(gt::PAN, 100, true);
        assert_eq!(z.get_generator(gt::PAN, -999), 100);
    }

    #[test]
    fn test_add_to_generator_from_existing() {
        let mut z = BasicZone::new();
        z.set_generator(gt::PAN, Some(50.0), true);
        z.add_to_generator(gt::PAN, 30, true);
        assert_eq!(z.get_generator(gt::PAN, -999), 80);
    }

    #[test]
    fn test_add_to_generator_clamps_when_validated() {
        let mut z = BasicZone::new();
        // pan max = 500; add 600 to default 0 → clamped to 500
        z.add_to_generator(gt::PAN, 600, true);
        assert_eq!(z.get_generator(gt::PAN, -999), 500);
    }

    // --- set_generator ---

    #[test]
    fn test_set_generator_adds_new() {
        let mut z = BasicZone::new();
        z.set_generator(gt::PAN, Some(100.0), true);
        assert_eq!(z.get_generator(gt::PAN, -999), 100);
    }

    #[test]
    fn test_set_generator_replaces_existing() {
        let mut z = BasicZone::new();
        z.set_generator(gt::PAN, Some(100.0), true);
        z.set_generator(gt::PAN, Some(200.0), true);
        assert_eq!(z.get_generator(gt::PAN, -999), 200);
        assert_eq!(
            z.generators.len(),
            1,
            "should have exactly one pan generator"
        );
    }

    #[test]
    fn test_set_generator_none_removes() {
        let mut z = BasicZone::new();
        z.set_generator(gt::PAN, Some(100.0), true);
        z.set_generator(gt::PAN, None, true);
        assert_eq!(z.get_generator(gt::PAN, -999), -999);
    }

    #[test]
    fn test_set_generator_none_on_absent_is_noop() {
        let mut z = BasicZone::new();
        z.set_generator(gt::PAN, None, true);
        assert!(z.generators.is_empty());
    }

    #[test]
    #[should_panic(expected = "Use setSample()")]
    fn test_set_generator_panics_for_sample_id() {
        let mut z = BasicZone::new();
        z.set_generator(gt::SAMPLE_ID, Some(0.0), true);
    }

    #[test]
    #[should_panic(expected = "Use setInstrument()")]
    fn test_set_generator_panics_for_instrument() {
        let mut z = BasicZone::new();
        z.set_generator(gt::INSTRUMENT, Some(0.0), true);
    }

    #[test]
    #[should_panic(expected = "Set the range manually")]
    fn test_set_generator_panics_for_vel_range() {
        let mut z = BasicZone::new();
        z.set_generator(gt::VEL_RANGE, Some(0.0), true);
    }

    #[test]
    #[should_panic(expected = "Set the range manually")]
    fn test_set_generator_panics_for_key_range() {
        let mut z = BasicZone::new();
        z.set_generator(gt::KEY_RANGE, Some(0.0), true);
    }

    // --- add_generators ---

    #[test]
    fn test_add_generators_normal() {
        let mut z = BasicZone::new();
        z.add_generators(&[Generator::new(gt::PAN, 50.0)]);
        assert_eq!(z.generators.len(), 1);
        assert_eq!(z.generators[0].generator_type, gt::PAN);
    }

    #[test]
    fn test_add_generators_key_range_sets_fields() {
        let mut z = BasicZone::new();
        // min=10, max=100: packed = (100 << 8) | 10 = 25610
        z.add_generators(&[Generator::new_unvalidated(gt::KEY_RANGE, 25610.0)]);
        assert_eq!(z.key_range.min, 10.0);
        assert_eq!(z.key_range.max, 100.0);
        assert!(
            z.generators.is_empty(),
            "keyRange should not be in generators"
        );
    }

    #[test]
    fn test_add_generators_vel_range_sets_fields() {
        let mut z = BasicZone::new();
        // min=20, max=80: packed = (80 << 8) | 20 = 20500
        z.add_generators(&[Generator::new_unvalidated(gt::VEL_RANGE, 20500.0)]);
        assert_eq!(z.vel_range.min, 20.0);
        assert_eq!(z.vel_range.max, 80.0);
        assert!(
            z.generators.is_empty(),
            "velRange should not be in generators"
        );
    }

    #[test]
    fn test_add_generators_sample_id_ignored() {
        let mut z = BasicZone::new();
        z.add_generators(&[Generator::new_unvalidated(gt::SAMPLE_ID, 5.0)]);
        assert!(z.generators.is_empty());
    }

    #[test]
    fn test_add_generators_instrument_ignored() {
        let mut z = BasicZone::new();
        z.add_generators(&[Generator::new_unvalidated(gt::INSTRUMENT, 3.0)]);
        assert!(z.generators.is_empty());
    }

    #[test]
    fn test_add_generators_multiple() {
        let mut z = BasicZone::new();
        z.add_generators(&[
            Generator::new(gt::PAN, 10.0),
            Generator::new(gt::INITIAL_ATTENUATION, 20.0),
        ]);
        assert_eq!(z.generators.len(), 2);
    }

    // --- add_modulators ---

    #[test]
    fn test_add_modulators_empty_slice() {
        let mut z = BasicZone::new();
        z.add_modulators(&[]);
        assert!(z.modulators.is_empty());
    }

    #[test]
    fn test_add_modulators_appends() {
        let mut z = BasicZone::new();
        z.add_modulators(&[Modulator::default()]);
        assert_eq!(z.modulators.len(), 1);
    }

    #[test]
    fn test_add_modulators_twice_appends() {
        let mut z = BasicZone::new();
        z.add_modulators(&[Modulator::default()]);
        z.add_modulators(&[Modulator::default()]);
        assert_eq!(z.modulators.len(), 2);
    }

    // --- get_generator ---

    #[test]
    fn test_get_generator_found() {
        let mut z = BasicZone::new();
        z.set_generator(gt::PAN, Some(42.0), true);
        assert_eq!(z.get_generator(gt::PAN, -999), 42);
    }

    #[test]
    fn test_get_generator_not_found_returns_default() {
        let z = BasicZone::new();
        assert_eq!(z.get_generator(gt::PAN, -999), -999);
    }

    #[test]
    fn test_get_generator_returns_first_match() {
        let mut z = BasicZone::new();
        z.set_generator(gt::PAN, Some(10.0), true);
        z.set_generator(gt::PAN, Some(20.0), true);
        // set_generator replaces in-place, so only one exists
        assert_eq!(z.get_generator(gt::PAN, -999), 20);
    }

    // --- copy_from ---

    #[test]
    fn test_copy_from_copies_generators() {
        let mut src = BasicZone::new();
        src.set_generator(gt::PAN, Some(77.0), true);
        let mut dst = BasicZone::new();
        dst.copy_from(&src);
        assert_eq!(dst.get_generator(gt::PAN, -999), 77);
    }

    #[test]
    fn test_copy_from_copies_ranges() {
        let mut src = BasicZone::new();
        src.add_generators(&[Generator::new_unvalidated(gt::KEY_RANGE, 25610.0)]); // min=10, max=100
        let mut dst = BasicZone::new();
        dst.copy_from(&src);
        assert_eq!(dst.key_range.min, 10.0);
        assert_eq!(dst.key_range.max, 100.0);
    }

    #[test]
    fn test_copy_from_is_deep_copy() {
        let mut src = BasicZone::new();
        src.set_generator(gt::PAN, Some(10.0), true);
        let mut dst = BasicZone::new();
        dst.copy_from(&src);
        // Mutating dst should not affect src
        dst.set_generator(gt::PAN, Some(99.0), true);
        assert_eq!(src.get_generator(gt::PAN, -999), 10);
    }

    #[test]
    fn test_copy_from_copies_vel_range() {
        let mut src = BasicZone::new();
        // min=5, max=110: (110 << 8) | 5 = 28165
        src.add_generators(&[Generator::new_unvalidated(gt::VEL_RANGE, 28165.0)]);
        let mut dst = BasicZone::new();
        dst.copy_from(&src);
        assert_eq!(dst.vel_range.min, 5.0);
        assert_eq!(dst.vel_range.max, 110.0);
    }

    // --- get_write_generators ---

    #[test]
    fn test_get_write_generators_empty_zone() {
        let z = BasicZone::new();
        let gens = z.get_write_generators(&());
        assert!(gens.is_empty());
    }

    #[test]
    fn test_get_write_generators_excludes_sample_id() {
        let mut z = BasicZone::new();
        // Directly push a sampleID generator (bypassing set_generator check)
        z.generators
            .push(Generator::new_unvalidated(gt::SAMPLE_ID, 1.0));
        let gens = z.get_write_generators(&());
        assert!(!gens.iter().any(|g| g.generator_type == gt::SAMPLE_ID));
    }

    #[test]
    fn test_get_write_generators_excludes_instrument() {
        let mut z = BasicZone::new();
        z.generators
            .push(Generator::new_unvalidated(gt::INSTRUMENT, 2.0));
        let gens = z.get_write_generators(&());
        assert!(!gens.iter().any(|g| g.generator_type == gt::INSTRUMENT));
    }

    #[test]
    fn test_get_write_generators_includes_normal_generators() {
        let mut z = BasicZone::new();
        z.set_generator(gt::PAN, Some(50.0), true);
        let gens = z.get_write_generators(&());
        assert!(gens.iter().any(|g| g.generator_type == gt::PAN));
    }

    #[test]
    fn test_get_write_generators_no_key_range_when_not_set() {
        let z = BasicZone::new();
        let gens = z.get_write_generators(&());
        assert!(!gens.iter().any(|g| g.generator_type == gt::KEY_RANGE));
    }

    #[test]
    fn test_get_write_generators_prepends_key_range() {
        let mut z = BasicZone::new();
        // min=0, max=60: (60 << 8) | 0 = 15360
        z.add_generators(&[Generator::new_unvalidated(gt::KEY_RANGE, 15360.0)]);
        z.set_generator(gt::PAN, Some(10.0), true);
        let gens = z.get_write_generators(&());
        assert_eq!(
            gens[0].generator_type,
            gt::KEY_RANGE,
            "key range should be first"
        );
    }

    #[test]
    fn test_get_write_generators_key_range_packed_correctly() {
        let mut z = BasicZone::new();
        // min=10, max=100: (100 << 8) | 10 = 25610
        z.add_generators(&[Generator::new_unvalidated(gt::KEY_RANGE, 25610.0)]);
        let gens = z.get_write_generators(&());
        let kr = &gens[0];
        assert_eq!(kr.generator_type, gt::KEY_RANGE);
        // packed = (100 << 8) | 10 = 25610
        assert_eq!(kr.generator_value, 25610);
    }

    #[test]
    fn test_get_write_generators_order_key_then_vel() {
        let mut z = BasicZone::new();
        // key: min=0, max=60  → (60 << 8) | 0 = 15360
        z.add_generators(&[Generator::new_unvalidated(gt::KEY_RANGE, 15360.0)]);
        // vel: min=20, max=80  → (80 << 8) | 20 = 20500
        z.add_generators(&[Generator::new_unvalidated(gt::VEL_RANGE, 20500.0)]);
        let gens = z.get_write_generators(&());
        assert_eq!(
            gens[0].generator_type,
            gt::KEY_RANGE,
            "key range should come first"
        );
        assert_eq!(
            gens[1].generator_type,
            gt::VEL_RANGE,
            "vel range should come second"
        );
    }

    #[test]
    fn test_get_write_generators_vel_range_packed_correctly() {
        let mut z = BasicZone::new();
        // min=20, max=80: (80 << 8) | 20 = 20500
        z.add_generators(&[Generator::new_unvalidated(gt::VEL_RANGE, 20500.0)]);
        let gens = z.get_write_generators(&());
        let vr = &gens[0];
        assert_eq!(vr.generator_type, gt::VEL_RANGE);
        assert_eq!(vr.generator_value, 20500);
    }

    #[test]
    fn test_get_write_generators_does_not_modify_original() {
        let mut z = BasicZone::new();
        z.set_generator(gt::PAN, Some(5.0), true);
        let _ = z.get_write_generators(&());
        // Original generators list should be unchanged
        assert_eq!(z.generators.len(), 1);
        assert_eq!(z.generators[0].generator_type, gt::PAN);
    }
}
