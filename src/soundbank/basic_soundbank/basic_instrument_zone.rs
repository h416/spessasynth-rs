/// basic_instrument_zone.rs
/// purpose: Zone within an instrument, referencing a specific sample.
/// Ported from: src/soundbank/basic_soundbank/basic_instrument_zone.ts
///
/// # TypeScript vs Rust design differences
///
/// In TypeScript, direct object references to `BasicSample` and `BasicInstrument` are held,
/// and cross-references are managed via `sample.linkTo(instrument)` / `sample.unlinkFrom(instrument)`.
///
/// In Rust, to avoid circular ownership:
/// - `sample_idx`            : index into `BasicSoundBank::samples`
/// - `parent_instrument_idx` : index into `BasicSoundBank::instruments`
///
/// Back-reference tracking via `linkTo` / `unlinkFrom` is implemented during `BasicSample` / `BasicInstrument` porting.
/// `getWriteGenerators` uses `sample_idx` directly instead of `bank.samples.indexOf(this.sample)`.
use crate::soundbank::basic_soundbank::basic_zone::BasicZone;
use crate::soundbank::basic_soundbank::generator::Generator;
use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;

/// A single zone within an instrument. Holds a reference to a sample and zone-specific generators / modulators.
/// Equivalent to: class BasicInstrumentZone extends BasicZone
#[derive(Clone, Debug)]
pub struct BasicInstrumentZone {
    /// Base zone (generators / modulators / key-vel range).
    /// Equivalent to: extends BasicZone
    pub zone: BasicZone,

    /// Index of the parent instrument in `BasicSoundBank::instruments`.
    /// Equivalent to: public readonly parentInstrument: BasicInstrument
    pub parent_instrument_idx: usize,

    /// Per-zone use count.
    /// Tracked in case multiple presets reference the same instrument.
    /// Equivalent to: public useCount: number
    pub use_count: u32,

    /// Index of the sample in `BasicSoundBank::samples`.
    /// Equivalent to: private _sample: BasicSample
    pub sample_idx: usize,
}

impl BasicInstrumentZone {
    /// Creates a new instrument zone.
    ///
    /// The TypeScript constructor calls `sample.linkTo(parentInstrument)`, which
    /// is implemented after `BasicSample` porting (currently TODO).
    ///
    /// `use_count` is initialized from the instrument's `useCount` at creation time
    /// (i.e., the number of presets referencing that instrument). This corresponds
    /// to TypeScript's `this.useCount = instrument.useCount;`.
    ///
    /// Equivalent to:
    /// ```ts
    /// constructor(instrument: BasicInstrument, sample: BasicSample)
    /// ```
    pub fn new(instrument_idx: usize, instrument_use_count: u32, sample_idx: usize) -> Self {
        // TODO: Call sample.linkTo(instrument) after BasicSample porting.
        Self {
            zone: BasicZone::new(),
            parent_instrument_idx: instrument_idx,
            use_count: instrument_use_count,
            sample_idx,
        }
    }

    /// Returns the sample index for this zone.
    ///
    /// TypeScript's `get sample()` returns the sample object itself, but
    /// the Rust version returns an index (the caller looks it up from the bank).
    ///
    /// Equivalent to: `public get sample(): BasicSample`
    #[inline]
    pub fn sample_idx(&self) -> usize {
        self.sample_idx
    }

    /// Replaces this zone's sample with a different sample (by index).
    ///
    /// TypeScript's setter calls `unlinkFrom` on the old sample then `linkTo` on the new one,
    /// but both operations are TODO until `BasicSample` porting is complete.
    ///
    /// Equivalent to: `public set sample(sample: BasicSample)`
    pub fn set_sample(&mut self, new_sample_idx: usize) {
        // TODO: After BasicSample porting, implement unlinkFrom on the old sample and linkTo on the new one.
        self.sample_idx = new_sample_idx;
    }

    /// Returns the generators list for SF2 writing.
    ///
    /// Appends a `sampleID` generator to the end of the base class (`BasicZone::get_write_generators`) result.
    /// TypeScript uses `bank.samples.indexOf(this.sample)` to find the index, but
    /// the Rust version uses `self.sample_idx` directly, so no bank reference is needed.
    ///
    /// The `_bank` type parameter is a dummy to maintain consistency with `BasicZone::get_write_generators`.
    ///
    /// Equivalent to:
    /// ```ts
    /// public getWriteGenerators(bank: BasicSoundBank): Generator[]
    /// ```
    pub fn get_write_generators<B>(&self, bank: &B) -> Vec<Generator> {
        let mut gens = self.zone.get_write_generators(bank);
        gens.push(Generator::new_unvalidated(
            gt::SAMPLE_ID,
            self.sample_idx as f64,
        ));
        gens
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
    use crate::soundbank::basic_soundbank::modulator::Modulator;

    // --- new() ---

    #[test]
    fn test_new_stores_instrument_idx() {
        let z = BasicInstrumentZone::new(3, 2, 7);
        assert_eq!(z.parent_instrument_idx, 3);
    }

    #[test]
    fn test_new_stores_use_count_from_instrument() {
        let z = BasicInstrumentZone::new(0, 5, 0);
        assert_eq!(z.use_count, 5);
    }

    #[test]
    fn test_new_stores_sample_idx() {
        let z = BasicInstrumentZone::new(0, 0, 9);
        assert_eq!(z.sample_idx, 9);
    }

    #[test]
    fn test_new_zone_has_default_basic_zone() {
        let z = BasicInstrumentZone::new(0, 0, 0);
        assert!(z.zone.generators.is_empty());
        assert!(z.zone.modulators.is_empty());
        assert!(!z.zone.has_key_range());
        assert!(!z.zone.has_vel_range());
    }

    // --- sample_idx getter ---

    #[test]
    fn test_sample_idx_getter_returns_stored_value() {
        let z = BasicInstrumentZone::new(0, 0, 42);
        assert_eq!(z.sample_idx(), 42);
    }

    // --- set_sample setter ---

    #[test]
    fn test_set_sample_updates_sample_idx() {
        let mut z = BasicInstrumentZone::new(0, 0, 1);
        z.set_sample(10);
        assert_eq!(z.sample_idx, 10);
    }

    #[test]
    fn test_set_sample_does_not_change_instrument_idx() {
        let mut z = BasicInstrumentZone::new(5, 0, 1);
        z.set_sample(99);
        assert_eq!(z.parent_instrument_idx, 5);
    }

    // --- get_write_generators ---

    #[test]
    fn test_get_write_generators_appends_sample_id() {
        let z = BasicInstrumentZone::new(0, 0, 7);
        let gens = z.get_write_generators(&());
        assert!(
            gens.iter()
                .any(|g| g.generator_type == gt::SAMPLE_ID && g.generator_value == 7),
            "sampleID generator with value 7 should be present"
        );
    }

    #[test]
    fn test_get_write_generators_sample_id_is_last() {
        let z = BasicInstrumentZone::new(0, 0, 3);
        let gens = z.get_write_generators(&());
        assert_eq!(
            gens.last().map(|g| g.generator_type),
            Some(gt::SAMPLE_ID),
            "sampleID should be the last generator"
        );
    }

    #[test]
    fn test_get_write_generators_includes_base_zone_generators() {
        let mut z = BasicInstrumentZone::new(0, 0, 0);
        z.zone.set_generator(gt::PAN, Some(50.0), true);
        let gens = z.get_write_generators(&());
        assert!(
            gens.iter().any(|g| g.generator_type == gt::PAN),
            "PAN generator from base zone should be present"
        );
    }

    #[test]
    fn test_get_write_generators_key_range_before_sample_id() {
        let mut z = BasicInstrumentZone::new(0, 0, 2);
        // min=0, max=60: (60 << 8) | 0 = 15360
        z.zone
            .add_generators(&[Generator::new_unvalidated(gt::KEY_RANGE, 15360.0)]);
        let gens = z.get_write_generators(&());
        let key_pos = gens
            .iter()
            .position(|g| g.generator_type == gt::KEY_RANGE)
            .unwrap();
        let sample_pos = gens
            .iter()
            .position(|g| g.generator_type == gt::SAMPLE_ID)
            .unwrap();
        assert!(
            key_pos < sample_pos,
            "keyRange should appear before sampleID"
        );
    }

    #[test]
    fn test_get_write_generators_does_not_mutate_zone() {
        let mut z = BasicInstrumentZone::new(0, 0, 0);
        z.zone.set_generator(gt::PAN, Some(10.0), true);
        let _ = z.get_write_generators(&());
        assert_eq!(
            z.zone.generators.len(),
            1,
            "zone.generators should be unchanged"
        );
    }

    // --- zone field passthrough: generators ---

    #[test]
    fn test_add_generators_to_zone() {
        let mut z = BasicInstrumentZone::new(0, 0, 0);
        z.zone.add_generators(&[Generator::new(gt::PAN, 100.0)]);
        assert_eq!(z.zone.generators.len(), 1);
    }

    // --- zone field passthrough: modulators ---

    #[test]
    fn test_add_modulators_to_zone() {
        let mut z = BasicInstrumentZone::new(0, 0, 0);
        z.zone.add_modulators(&[Modulator::default()]);
        assert_eq!(z.zone.modulators.len(), 1);
    }

    // --- use_count manipulation ---

    #[test]
    fn test_use_count_can_be_incremented() {
        let mut z = BasicInstrumentZone::new(0, 1, 0);
        z.use_count += 1;
        assert_eq!(z.use_count, 2);
    }

    #[test]
    fn test_use_count_can_be_decremented() {
        let mut z = BasicInstrumentZone::new(0, 2, 0);
        z.use_count -= 1;
        assert_eq!(z.use_count, 1);
    }

    // --- clone ---

    #[test]
    fn test_clone_produces_independent_copy() {
        let mut z = BasicInstrumentZone::new(1, 3, 5);
        z.zone.set_generator(gt::PAN, Some(20.0), true);
        let mut cloned = z.clone();
        cloned.zone.set_generator(gt::PAN, Some(99.0), true);
        assert_eq!(
            z.zone.get_generator(gt::PAN, -1),
            20,
            "original should be unaffected"
        );
        assert_eq!(cloned.zone.get_generator(gt::PAN, -1), 99);
    }

    #[test]
    fn test_clone_copies_indices() {
        let z = BasicInstrumentZone::new(7, 4, 11);
        let cloned = z.clone();
        assert_eq!(cloned.parent_instrument_idx, 7);
        assert_eq!(cloned.use_count, 4);
        assert_eq!(cloned.sample_idx, 11);
    }
}
