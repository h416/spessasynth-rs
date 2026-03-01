/// basic_preset_zone.rs
/// purpose: Zone within a preset, referencing a specific instrument.
/// Ported from: src/soundbank/basic_soundbank/basic_preset_zone.ts
///
/// # TypeScript vs Rust design differences
///
/// The TypeScript version holds direct object references to `BasicPreset` and `BasicInstrument`,
/// and manages cross-references via `instrument.linkTo(preset)` / `instrument.unlinkFrom(preset)`.
///
/// The Rust version uses indices to avoid circular ownership:
/// - `parent_preset_idx`  : index into `BasicSoundBank::presets`
/// - `instrument_idx`     : index into `BasicSoundBank::instruments`
///
/// Reverse-reference tracking via `linkTo` / `unlinkFrom` will be implemented when porting `BasicPreset` / `BasicInstrument`.
/// `getWriteGenerators` uses `instrument_idx` directly instead of
/// `bank.instruments.indexOf(this.instrument)`.
use crate::soundbank::basic_soundbank::basic_zone::BasicZone;
use crate::soundbank::basic_soundbank::generator::Generator;
use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;

/// A single zone within a preset, holding a reference to an instrument and zone-specific generators / modulators.
/// Equivalent to: class BasicPresetZone extends BasicZone
#[derive(Clone, Debug)]
pub struct BasicPresetZone {
    /// Base zone (generators / modulators / key-vel range).
    /// Equivalent to: extends BasicZone
    pub zone: BasicZone,

    /// Index of the parent preset in `BasicSoundBank::presets`.
    /// Equivalent to: public readonly parentPreset: BasicPreset
    pub parent_preset_idx: usize,

    /// Index of the instrument in `BasicSoundBank::instruments`.
    /// Equivalent to: private _instrument: BasicInstrument
    pub instrument_idx: usize,
}

impl BasicPresetZone {
    /// Creates a new preset zone.
    ///
    /// The TypeScript constructor calls `instrument.linkTo(preset)`,
    /// which will be implemented after `BasicInstrument` is ported (currently TODO).
    ///
    /// Equivalent to:
    /// ```ts
    /// constructor(preset: BasicPreset, instrument: BasicInstrument)
    /// ```
    pub fn new(preset_idx: usize, instrument_idx: usize) -> Self {
        // TODO: Call instrument.linkTo(preset) after BasicInstrument is ported.
        Self {
            zone: BasicZone::new(),
            parent_preset_idx: preset_idx,
            instrument_idx,
        }
    }

    /// Returns the instrument index of this zone.
    ///
    /// TypeScript's `get instrument()` returns the instrument object itself, but
    /// the Rust version returns an index (the caller can look it up in the bank).
    ///
    /// Equivalent to: `public get instrument(): BasicInstrument`
    #[inline]
    pub fn instrument_idx(&self) -> usize {
        self.instrument_idx
    }

    /// Replaces the instrument of this zone with another instrument (by index).
    ///
    /// The TypeScript setter calls `unlinkFrom` on the old instrument and `linkTo` on the new instrument,
    /// but both operations are TODO until `BasicInstrument` is ported.
    ///
    /// Equivalent to: `public set instrument(instrument: BasicInstrument)`
    pub fn set_instrument(&mut self, new_instrument_idx: usize) {
        // TODO: After BasicInstrument is ported, implement unlinkFrom on the old instrument and
        //       linkTo on the new instrument.
        self.instrument_idx = new_instrument_idx;
    }

    /// Returns the list of generators for SF2 writing.
    ///
    /// Appends an `instrument` generator to the end of the result from the base class (`BasicZone::get_write_generators`).
    /// The TypeScript version uses `bank.instruments.indexOf(this.instrument)` to find the index, but
    /// the Rust version uses `self.instrument_idx` directly, so no bank reference is needed.
    ///
    /// The `if (!bank) throw new Error(...)` check in the TypeScript version is
    /// unnecessary in Rust because references are always valid.
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
            gt::INSTRUMENT,
            self.instrument_idx as f64,
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
    fn test_new_stores_preset_idx() {
        let z = BasicPresetZone::new(4, 2);
        assert_eq!(z.parent_preset_idx, 4);
    }

    #[test]
    fn test_new_stores_instrument_idx() {
        let z = BasicPresetZone::new(0, 8);
        assert_eq!(z.instrument_idx, 8);
    }

    #[test]
    fn test_new_zone_has_default_basic_zone() {
        let z = BasicPresetZone::new(0, 0);
        assert!(z.zone.generators.is_empty());
        assert!(z.zone.modulators.is_empty());
        assert!(!z.zone.has_key_range());
        assert!(!z.zone.has_vel_range());
    }

    // --- instrument_idx getter ---

    #[test]
    fn test_instrument_idx_getter_returns_stored_value() {
        let z = BasicPresetZone::new(0, 13);
        assert_eq!(z.instrument_idx(), 13);
    }

    // --- set_instrument setter ---

    #[test]
    fn test_set_instrument_updates_instrument_idx() {
        let mut z = BasicPresetZone::new(0, 1);
        z.set_instrument(20);
        assert_eq!(z.instrument_idx, 20);
    }

    #[test]
    fn test_set_instrument_does_not_change_preset_idx() {
        let mut z = BasicPresetZone::new(5, 1);
        z.set_instrument(99);
        assert_eq!(z.parent_preset_idx, 5);
    }

    // --- get_write_generators ---

    #[test]
    fn test_get_write_generators_appends_instrument() {
        let z = BasicPresetZone::new(0, 6);
        let gens = z.get_write_generators(&());
        assert!(
            gens.iter()
                .any(|g| g.generator_type == gt::INSTRUMENT && g.generator_value == 6),
            "instrument generator with value 6 should be present"
        );
    }

    #[test]
    fn test_get_write_generators_instrument_is_last() {
        let z = BasicPresetZone::new(0, 3);
        let gens = z.get_write_generators(&());
        assert_eq!(
            gens.last().map(|g| g.generator_type),
            Some(gt::INSTRUMENT),
            "instrument generator should be the last"
        );
    }

    #[test]
    fn test_get_write_generators_includes_base_zone_generators() {
        let mut z = BasicPresetZone::new(0, 0);
        z.zone.set_generator(gt::PAN, Some(30.0), true);
        let gens = z.get_write_generators(&());
        assert!(
            gens.iter().any(|g| g.generator_type == gt::PAN),
            "PAN generator from base zone should be present"
        );
    }

    #[test]
    fn test_get_write_generators_key_range_before_instrument() {
        let mut z = BasicPresetZone::new(0, 2);
        // min=0, max=60: (60 << 8) | 0 = 15360
        z.zone
            .add_generators(&[Generator::new_unvalidated(gt::KEY_RANGE, 15360.0)]);
        let gens = z.get_write_generators(&());
        let key_pos = gens
            .iter()
            .position(|g| g.generator_type == gt::KEY_RANGE)
            .unwrap();
        let inst_pos = gens
            .iter()
            .position(|g| g.generator_type == gt::INSTRUMENT)
            .unwrap();
        assert!(
            key_pos < inst_pos,
            "keyRange should appear before instrument"
        );
    }

    #[test]
    fn test_get_write_generators_does_not_mutate_zone() {
        let mut z = BasicPresetZone::new(0, 0);
        z.zone.set_generator(gt::PAN, Some(10.0), true);
        let _ = z.get_write_generators(&());
        assert_eq!(
            z.zone.generators.len(),
            1,
            "zone.generators should be unchanged"
        );
    }

    #[test]
    fn test_get_write_generators_vel_range_before_instrument() {
        let mut z = BasicPresetZone::new(0, 1);
        // min=20, max=80: (80 << 8) | 20 = 20500
        z.zone
            .add_generators(&[Generator::new_unvalidated(gt::VEL_RANGE, 20500.0)]);
        let gens = z.get_write_generators(&());
        let vel_pos = gens
            .iter()
            .position(|g| g.generator_type == gt::VEL_RANGE)
            .unwrap();
        let inst_pos = gens
            .iter()
            .position(|g| g.generator_type == gt::INSTRUMENT)
            .unwrap();
        assert!(
            vel_pos < inst_pos,
            "velRange should appear before instrument"
        );
    }

    // --- zone field passthrough: generators ---

    #[test]
    fn test_add_generators_to_zone() {
        let mut z = BasicPresetZone::new(0, 0);
        z.zone.add_generators(&[Generator::new(gt::PAN, 100.0)]);
        assert_eq!(z.zone.generators.len(), 1);
    }

    // --- zone field passthrough: modulators ---

    #[test]
    fn test_add_modulators_to_zone() {
        let mut z = BasicPresetZone::new(0, 0);
        z.zone.add_modulators(&[Modulator::default()]);
        assert_eq!(z.zone.modulators.len(), 1);
    }

    // --- clone ---

    #[test]
    fn test_clone_produces_independent_copy() {
        let mut z = BasicPresetZone::new(2, 5);
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
        let z = BasicPresetZone::new(7, 11);
        let cloned = z.clone();
        assert_eq!(cloned.parent_preset_idx, 7);
        assert_eq!(cloned.instrument_idx, 11);
    }
}
