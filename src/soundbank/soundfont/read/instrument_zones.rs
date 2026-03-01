/// instrument_zones.rs
/// purpose: reads instrument zones from SoundFont and assigns samples, generators, modulators.
/// Ported from: src/soundbank/soundfont/read/instrument_zones.ts
///
/// # TypeScript vs Rust design differences
///
/// ## SoundFontInstrumentZone
/// TypeScript: `class SoundFontInstrumentZone extends BasicInstrumentZone`
/// Rust: `SoundFontInstrumentZone(BasicInstrumentZone)` newtype.
/// Handles the constructor logic (sampleID search and validation),
/// and on success pushes the inner `BasicInstrumentZone` to the instrument.
///
/// ## applyInstrumentZones
/// TypeScript: directly calls `SoundFontInstrument`'s `createSoundFontZone` / `globalZone` methods.
/// Rust: since `SoundFontInstrument` is not yet ported, the required operations are abstracted
/// into the `SoundFontInstrumentZoneSink` trait. Implement this trait when porting `SoundFontInstrument`.
///
/// ## samples parameter
/// TypeScript: passes `BasicSample[]` and indexes into it.
/// Rust: only `sample_count: usize` (for index validation).
/// Actual sample data is stored in `BasicSoundBank::samples`, and zones reference it via `sample_idx`.
use crate::soundbank::basic_soundbank::basic_instrument_zone::BasicInstrumentZone;
use crate::soundbank::basic_soundbank::basic_zone::BasicZone;
use crate::soundbank::basic_soundbank::generator::Generator;
use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
use crate::soundbank::basic_soundbank::modulator::Modulator;
use crate::soundbank::soundfont::read::zones::ZoneIndexes;
use crate::utils::loggin::spessa_synth_warn as warn_fn;

/// SoundFont instrument zone.
/// A newtype over `BasicInstrumentZone` providing a constructor for SF2 file reading.
///
/// TypeScript: `class SoundFontInstrumentZone extends BasicInstrumentZone`
#[derive(Clone, Debug)]
pub struct SoundFontInstrumentZone(pub BasicInstrumentZone);

impl SoundFontInstrumentZone {
    /// Zone creation for SF2 reading.
    ///
    /// Searches generators for sampleID, builds a `BasicInstrumentZone`,
    /// and applies generators / modulators.
    ///
    /// # Errors
    /// - No `sampleID` generator found in generators
    /// - `sampleID` is out of range of `sample_count`
    ///
    /// # Note
    /// `use_count` is initialized to 0 since presets are not yet linked at SF2 read time.
    /// (TypeScript: `this.useCount = instrument.useCount` → `instrument.linkedTo.length` is 0)
    ///
    /// Equivalent to:
    /// ```ts
    /// constructor(inst, modulators, generators, samples)
    /// ```
    pub fn new(
        instrument_idx: usize,
        modulators: &[Modulator],
        generators: &[Generator],
        sample_count: usize,
    ) -> Result<Self, String> {
        // Search for the sampleID generator
        // Equivalent to: generators.find(g => g.generatorType === generatorTypes.sampleID)
        let sample_id_gen = generators
            .iter()
            .find(|g| g.generator_type == gt::SAMPLE_ID)
            .ok_or_else(|| "No sample ID found in instrument zone.".to_string())?;

        // generator_value is i16, but SF2 sample indices are always non-negative, so cast via u16
        // Equivalent to: samples[sampleID.generatorValue]
        let sample_idx = sample_id_gen.generator_value as u16 as usize;

        if sample_idx >= sample_count {
            return Err(format!(
                "Invalid sample ID: {}, available samples: {}",
                sample_idx, sample_count
            ));
        }

        // Equivalent to: super(inst, sample)
        // use_count = 0 (presets are not yet linked at SF2 read time)
        let mut zone = BasicInstrumentZone::new(instrument_idx, 0, sample_idx);

        // Equivalent to: this.addGenerators(...generators)
        zone.zone.add_generators(generators);
        // Equivalent to: this.addModulators(...modulators)
        zone.zone.add_modulators(modulators);

        Ok(SoundFontInstrumentZone(zone))
    }
}

// ---------------------------------------------------------------------------
// Trait: SoundFontInstrumentZoneSink
// ---------------------------------------------------------------------------

/// Trait abstracting the operations that `apply_instrument_zones` performs on `SoundFontInstrument`.
///
/// Implement this trait when porting `SoundFontInstrument`.
///
/// TypeScript methods used:
/// - `instrument.zonesCount`          → `zones_count()`
/// - `instrument.createSoundFontZone` → `push_zone()`
/// - `instrument.globalZone`          → `global_zone_mut()`
pub trait SoundFontInstrumentZoneSink {
    /// Number of zones this instrument has (ibag entry count).
    /// Equivalent to: `instrument.zonesCount`
    fn zones_count(&self) -> usize;

    /// Adds a regular zone (non-global) to the instrument.
    /// Equivalent to: push portion of `instrument.createSoundFontZone(mods, gens, samples)`
    fn push_zone(&mut self, zone: BasicInstrumentZone);

    /// Returns a mutable reference to the instrument's global zone.
    /// Equivalent to: `instrument.globalZone`
    fn global_zone_mut(&mut self) -> &mut BasicZone;
}

// ---------------------------------------------------------------------------
// apply_instrument_zones
// ---------------------------------------------------------------------------

/// Reads instrument zones and applies them to each instrument.
///
/// Splits `instrument_generators` / `instrument_modulators` according to
/// `indexes`'s `gen_ndx` / `mod_ndx`, and distributes them to each instrument's zones.
///
/// - If a sampleID generator is present → regular zone → `instrument.push_zone()`
/// - If no sampleID generator → global zone → `instrument.global_zone_mut()`
///
/// `instrument_idx` is obtained via enumerate and passed to `BasicInstrumentZone::new`.
///
/// # Warnings
/// Invalid zones (e.g., sampleID out of range) are skipped with a log warning instead of panicking.
///
/// Equivalent to: `applyInstrumentZones(indexes, instrumentGenerators, instrumentModulators, samples, instruments)`
pub fn apply_instrument_zones<I: SoundFontInstrumentZoneSink>(
    indexes: &ZoneIndexes,
    instrument_generators: &[Generator],
    instrument_modulators: &[Modulator],
    sample_count: usize,
    instruments: &mut [I],
) {
    let mut mod_index: usize = 0;
    let mut gen_index: usize = 0;

    for (instrument_idx, instrument) in instruments.iter_mut().enumerate() {
        for _ in 0..instrument.zones_count() {
            // Extract the generators slice
            // Equivalent to: instrumentGenerators.slice(gensStart, gensEnd)
            let gens_start = indexes.gen_ndx[gen_index] as usize;
            gen_index += 1;
            let gens_end = indexes.gen_ndx[gen_index] as usize;
            let gens = &instrument_generators[gens_start..gens_end];

            // Extract the modulators slice
            // Equivalent to: instrumentModulators.slice(modsStart, modsEnd)
            let mods_start = indexes.mod_ndx[mod_index] as usize;
            mod_index += 1;
            let mods_end = indexes.mod_ndx[mod_index] as usize;
            let mods = &instrument_modulators[mods_start..mods_end];

            // Regular zone if sampleID present, otherwise global zone
            // Equivalent to: if (gens.some(g => g.generatorType === generatorTypes.sampleID))
            if gens.iter().any(|g| g.generator_type == gt::SAMPLE_ID) {
                // Equivalent to: instrument.createSoundFontZone(mods, gens, samples)
                match SoundFontInstrumentZone::new(instrument_idx, mods, gens, sample_count) {
                    Ok(zone) => instrument.push_zone(zone.0),
                    Err(e) => warn_fn(&e),
                }
            } else {
                // Equivalent to: instrument.globalZone.addGenerators(...gens)
                //                instrument.globalZone.addModulators(...mods)
                let global = instrument.global_zone_mut();
                global.add_generators(gens);
                global.add_modulators(mods);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator::Generator;
    use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
    use crate::soundbank::basic_soundbank::modulator::Modulator;
    use crate::soundbank::soundfont::read::zones::ZoneIndexes;

    // -----------------------------------------------------------------------
    // Helper: create generator arrays with sampleID mixed in
    // -----------------------------------------------------------------------
    fn gen_with_sample_id(sample_id: i16) -> Generator {
        Generator::new_unvalidated(gt::SAMPLE_ID, sample_id as f64)
    }

    fn gen_pan(value: f64) -> Generator {
        Generator::new_unvalidated(gt::PAN, value)
    }

    // -----------------------------------------------------------------------
    // SoundFontInstrumentZone::new
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_ok_stores_sample_idx() {
        let gens = vec![gen_with_sample_id(3), gen_pan(0.0)];
        let zone = SoundFontInstrumentZone::new(0, &[], &gens, 10).unwrap();
        assert_eq!(zone.0.sample_idx, 3);
    }

    #[test]
    fn test_new_ok_stores_instrument_idx() {
        let gens = vec![gen_with_sample_id(0)];
        let zone = SoundFontInstrumentZone::new(7, &[], &gens, 5).unwrap();
        assert_eq!(zone.0.parent_instrument_idx, 7);
    }

    #[test]
    fn test_new_ok_use_count_is_zero() {
        let gens = vec![gen_with_sample_id(0)];
        let zone = SoundFontInstrumentZone::new(0, &[], &gens, 5).unwrap();
        assert_eq!(zone.0.use_count, 0);
    }

    #[test]
    fn test_new_ok_adds_generators_to_zone() {
        let gens = vec![gen_with_sample_id(1), gen_pan(50.0)];
        let zone = SoundFontInstrumentZone::new(0, &[], &gens, 5).unwrap();
        // sampleID is ignored by BasicZone::add_generators so it does not remain in generators
        assert!(
            zone.0
                .zone
                .generators
                .iter()
                .any(|g| g.generator_type == gt::PAN),
            "PAN generator should be added to the zone"
        );
    }

    #[test]
    fn test_new_ok_sample_id_not_in_generators_vec() {
        // BasicZone::add_generators ignores SAMPLE_ID, so it won't appear in generators
        let gens = vec![gen_with_sample_id(2)];
        let zone = SoundFontInstrumentZone::new(0, &[], &gens, 5).unwrap();
        assert!(
            !zone
                .0
                .zone
                .generators
                .iter()
                .any(|g| g.generator_type == gt::SAMPLE_ID),
            "SAMPLE_ID should not appear in zone.generators"
        );
    }

    #[test]
    fn test_new_ok_adds_modulators() {
        let gens = vec![gen_with_sample_id(0)];
        let mods = vec![Modulator::default(), Modulator::default()];
        let zone = SoundFontInstrumentZone::new(0, &mods, &gens, 5).unwrap();
        assert_eq!(zone.0.zone.modulators.len(), 2);
    }

    #[test]
    fn test_new_err_no_sample_id() {
        let gens = vec![gen_pan(0.0)];
        let result = SoundFontInstrumentZone::new(0, &[], &gens, 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No sample ID"));
    }

    #[test]
    fn test_new_err_empty_generators() {
        let result = SoundFontInstrumentZone::new(0, &[], &[], 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No sample ID"));
    }

    #[test]
    fn test_new_err_sample_idx_out_of_range() {
        let gens = vec![gen_with_sample_id(10)];
        let result = SoundFontInstrumentZone::new(0, &[], &gens, 5); // sample_count = 5, idx = 10
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("Invalid sample ID"), "got: {}", msg);
        assert!(msg.contains("10"), "got: {}", msg);
        assert!(msg.contains("5"), "got: {}", msg);
    }

    #[test]
    fn test_new_ok_boundary_sample_idx() {
        // sample_idx == sample_count - 1 is valid
        let gens = vec![gen_with_sample_id(4)];
        let result = SoundFontInstrumentZone::new(0, &[], &gens, 5);
        assert!(result.is_ok());
    }

    #[test]
    fn test_new_err_sample_idx_equals_sample_count() {
        // sample_idx == sample_count is invalid
        let gens = vec![gen_with_sample_id(5)];
        let result = SoundFontInstrumentZone::new(0, &[], &gens, 5);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Integration tests via apply_instrument_zones
    // -----------------------------------------------------------------------

    /// Test implementation of `SoundFontInstrumentZoneSink`
    struct MockInstrument {
        zones_count: usize,
        global_zone: BasicZone,
        pushed_zones: Vec<BasicInstrumentZone>,
    }

    impl MockInstrument {
        fn new(zones_count: usize) -> Self {
            Self {
                zones_count,
                global_zone: BasicZone::new(),
                pushed_zones: Vec::new(),
            }
        }
    }

    impl SoundFontInstrumentZoneSink for MockInstrument {
        fn zones_count(&self) -> usize {
            self.zones_count
        }
        fn push_zone(&mut self, zone: BasicInstrumentZone) {
            self.pushed_zones.push(zone);
        }
        fn global_zone_mut(&mut self) -> &mut BasicZone {
            &mut self.global_zone
        }
    }

    /// Helper to manually construct ZoneIndexes
    fn make_indexes(gen_ndx: Vec<u32>, mod_ndx: Vec<u32>) -> ZoneIndexes {
        ZoneIndexes { gen_ndx, mod_ndx }
    }

    #[test]
    fn test_apply_one_instrument_one_regular_zone() {
        // instrument: 1 zone, sampleID = 2
        let mut instruments = vec![MockInstrument::new(1)];
        let gens = vec![gen_with_sample_id(2)];
        let mods: Vec<Modulator> = vec![];
        // gen_ndx: zone 0 is [0..1]
        let indexes = make_indexes(vec![0, 1], vec![0, 0]);
        apply_instrument_zones(&indexes, &gens, &mods, 5, &mut instruments);
        assert_eq!(instruments[0].pushed_zones.len(), 1);
        assert_eq!(instruments[0].pushed_zones[0].sample_idx, 2);
    }

    #[test]
    fn test_apply_one_instrument_global_zone_only() {
        // instrument: 1 zone, no sampleID → goes to global zone
        let mut instruments = vec![MockInstrument::new(1)];
        let gens = vec![gen_pan(100.0)];
        let mods: Vec<Modulator> = vec![];
        let indexes = make_indexes(vec![0, 1], vec![0, 0]);
        apply_instrument_zones(&indexes, &gens, &mods, 5, &mut instruments);
        // No regular zone is added
        assert_eq!(instruments[0].pushed_zones.len(), 0);
        // PAN is in the global zone
        assert_eq!(instruments[0].global_zone.generators.len(), 1);
        assert_eq!(
            instruments[0].global_zone.generators[0].generator_type,
            gt::PAN
        );
    }

    #[test]
    fn test_apply_instrument_idx_matches_enumerate() {
        // Verify that instrument_idx is correctly set for 2 instruments
        let mut instruments = vec![MockInstrument::new(1), MockInstrument::new(1)];
        let gens = vec![gen_with_sample_id(0), gen_with_sample_id(1)];
        let mods: Vec<Modulator> = vec![];
        // gen_ndx: inst0 → [0..1], inst1 → [1..2]
        let indexes = make_indexes(vec![0, 1, 2], vec![0, 0, 0]);
        apply_instrument_zones(&indexes, &gens, &mods, 5, &mut instruments);
        assert_eq!(instruments[0].pushed_zones[0].parent_instrument_idx, 0);
        assert_eq!(instruments[1].pushed_zones[0].parent_instrument_idx, 1);
    }

    #[test]
    fn test_apply_two_zones_same_instrument() {
        // 1 instrument with 2 zones (global + regular)
        let mut instruments = vec![MockInstrument::new(2)];
        // zone 0: no sampleID (global), zone 1: sampleID=3
        let gens = vec![gen_pan(50.0), gen_with_sample_id(3)];
        let mods: Vec<Modulator> = vec![];
        // gen_ndx: zone0 → [0..1], zone1 → [1..2]
        let indexes = make_indexes(vec![0, 1, 2], vec![0, 0, 0]);
        apply_instrument_zones(&indexes, &gens, &mods, 5, &mut instruments);
        assert_eq!(
            instruments[0].pushed_zones.len(),
            1,
            "only regular zone is pushed"
        );
        assert_eq!(instruments[0].global_zone.generators.len(), 1);
    }

    #[test]
    fn test_apply_invalid_sample_id_skips_zone() {
        // sampleID out of range → push_zone is not called (warning only)
        let mut instruments = vec![MockInstrument::new(1)];
        let gens = vec![gen_with_sample_id(99)]; // sample_count = 5
        let mods: Vec<Modulator> = vec![];
        let indexes = make_indexes(vec![0, 1], vec![0, 0]);
        apply_instrument_zones(&indexes, &gens, &mods, 5, &mut instruments);
        assert_eq!(
            instruments[0].pushed_zones.len(),
            0,
            "invalid zone should be skipped"
        );
    }

    #[test]
    fn test_apply_zero_zones_instrument() {
        // An instrument with zonesCount = 0 does nothing
        let mut instruments = vec![MockInstrument::new(0)];
        let gens: Vec<Generator> = vec![];
        let mods: Vec<Modulator> = vec![];
        let indexes = make_indexes(vec![0], vec![0]);
        apply_instrument_zones(&indexes, &gens, &mods, 5, &mut instruments);
        assert_eq!(instruments[0].pushed_zones.len(), 0);
        assert_eq!(instruments[0].global_zone.generators.len(), 0);
    }

    #[test]
    fn test_apply_modulators_added_to_zone() {
        let mut instruments = vec![MockInstrument::new(1)];
        let gens = vec![gen_with_sample_id(0)];
        let mods = vec![Modulator::default(), Modulator::default()];
        let indexes = make_indexes(vec![0, 1], vec![0, 2]);
        apply_instrument_zones(&indexes, &gens, &mods, 5, &mut instruments);
        assert_eq!(instruments[0].pushed_zones[0].zone.modulators.len(), 2);
    }

    #[test]
    fn test_apply_modulators_added_to_global_zone() {
        let mut instruments = vec![MockInstrument::new(1)];
        let gens = vec![gen_pan(0.0)]; // global zone
        let mods = vec![Modulator::default()];
        let indexes = make_indexes(vec![0, 1], vec![0, 1]);
        apply_instrument_zones(&indexes, &gens, &mods, 5, &mut instruments);
        assert_eq!(instruments[0].global_zone.modulators.len(), 1);
    }
}
