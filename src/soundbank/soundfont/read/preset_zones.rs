/// preset_zones.rs
/// purpose: reads preset zones from SoundFont and assigns instruments, generators, modulators.
/// Ported from: src/soundbank/soundfont/read/preset_zones.ts
///
/// # TypeScript vs Rust design differences
///
/// ## SoundFontPresetZone
/// TypeScript: `class SoundFontPresetZone extends BasicPresetZone`
/// Rust: `SoundFontPresetZone(BasicPresetZone)` newtype.
/// Handles the constructor logic (instrument generator search and validation),
/// and on success pushes the inner `BasicPresetZone` to the preset.
///
/// ## applyPresetZones
/// TypeScript: directly calls `SoundFontPreset`'s `createSoundFontZone` / `globalZone` methods.
/// Rust: since `SoundFontPreset` is not yet ported, the required operations are abstracted
/// into the `SoundFontPresetZoneSink` trait. Implement this trait when porting `SoundFontPreset`.
///
/// ## instruments parameter
/// TypeScript: passes `BasicInstrument[]` and indexes into it.
/// Rust: only `instrument_count: usize` (for index validation).
/// Actual instrument data is stored in `BasicSoundBank::instruments`, and
/// zones reference it via `instrument_idx`.
use crate::soundbank::basic_soundbank::basic_preset_zone::BasicPresetZone;
use crate::soundbank::basic_soundbank::basic_zone::BasicZone;
use crate::soundbank::basic_soundbank::generator::Generator;
use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
use crate::soundbank::basic_soundbank::modulator::Modulator;
use crate::soundbank::soundfont::read::zones::ZoneIndexes;
use crate::utils::loggin::spessa_synth_warn as warn_fn;

/// SoundFont preset zone.
/// A newtype over `BasicPresetZone` providing a constructor for SF2 file reading.
///
/// TypeScript: `class SoundFontPresetZone extends BasicPresetZone`
#[derive(Clone, Debug)]
pub struct SoundFontPresetZone(pub BasicPresetZone);

impl SoundFontPresetZone {
    /// Zone creation for SF2 reading.
    ///
    /// Searches generators for the `instrument` generator, builds a `BasicPresetZone`,
    /// and applies generators / modulators.
    ///
    /// # Errors
    /// - No `instrument` generator found in generators
    /// - `instrument` index is out of range of `instrument_count`
    ///
    /// Equivalent to:
    /// ```ts
    /// constructor(preset, modulators, generators, instruments)
    /// ```
    pub fn new(
        preset_idx: usize,
        modulators: &[Modulator],
        generators: &[Generator],
        instrument_count: usize,
    ) -> Result<Self, String> {
        // Search for the instrument generator
        // Equivalent to: generators.find(g => g.generatorType === generatorTypes.instrument)
        let instrument_id_gen = generators
            .iter()
            .find(|g| g.generator_type == gt::INSTRUMENT)
            .ok_or_else(|| "No instrument ID found in preset zone.".to_string())?;

        // generator_value is i16, but SF2 indices are always non-negative, so cast via u16
        // Equivalent to: instruments[instrumentID.generatorValue]
        let instrument_idx = instrument_id_gen.generator_value as u16 as usize;

        if instrument_idx >= instrument_count {
            return Err(format!(
                "Invalid instrument ID: {}, available instruments: {}",
                instrument_idx, instrument_count
            ));
        }

        // Equivalent to: super(preset, instrument)
        let mut zone = BasicPresetZone::new(preset_idx, instrument_idx);

        // Equivalent to: this.addGenerators(...generators)
        zone.zone.add_generators(generators);
        // Equivalent to: this.addModulators(...modulators)
        zone.zone.add_modulators(modulators);

        Ok(SoundFontPresetZone(zone))
    }
}

// ---------------------------------------------------------------------------
// Trait: SoundFontPresetZoneSink
// ---------------------------------------------------------------------------

/// Trait abstracting the operations that `apply_preset_zones` performs on `SoundFontPreset`.
///
/// Implement this trait when porting `SoundFontPreset`.
///
/// TypeScript methods used:
/// - `preset.zonesCount`          → `zones_count()`
/// - `preset.createSoundFontZone` → `push_zone()`
/// - `preset.globalZone`          → `global_zone_mut()`
pub trait SoundFontPresetZoneSink {
    /// Number of zones this preset has (pbag entry count).
    /// Equivalent to: `preset.zonesCount`
    fn zones_count(&self) -> usize;

    /// Adds a regular zone (non-global) to the preset.
    /// Equivalent to: push portion of `preset.createSoundFontZone(mods, gens, instruments)`
    fn push_zone(&mut self, zone: BasicPresetZone);

    /// Returns a mutable reference to the preset's global zone.
    /// Equivalent to: `preset.globalZone`
    fn global_zone_mut(&mut self) -> &mut BasicZone;
}

// ---------------------------------------------------------------------------
// apply_preset_zones
// ---------------------------------------------------------------------------

/// Reads preset zones and applies them to each preset.
///
/// Splits `preset_gens` / `preset_mods` according to `indexes`'s `gen_ndx` / `mod_ndx`,
/// and distributes them to each preset's zones.
///
/// - If an `instrument` generator is present → regular zone → `preset.push_zone()`
/// - If no `instrument` generator → global zone → `preset.global_zone_mut()`
///
/// `preset_idx` is obtained via enumerate and passed to `BasicPresetZone::new`.
///
/// # Warnings
/// Invalid zones (e.g., instrument index out of range) are skipped with a log warning instead of panicking.
///
/// Equivalent to: `applyPresetZones(indexes, presetGens, presetMods, instruments, presets)`
pub fn apply_preset_zones<P: SoundFontPresetZoneSink>(
    indexes: &ZoneIndexes,
    preset_gens: &[Generator],
    preset_mods: &[Modulator],
    instrument_count: usize,
    presets: &mut [P],
) {
    let mut mod_index: usize = 0;
    let mut gen_index: usize = 0;

    for (preset_idx, preset) in presets.iter_mut().enumerate() {
        for _ in 0..preset.zones_count() {
            // Extract the generators slice
            // Equivalent to: presetGens.slice(gensStart, gensEnd)
            let gens_start = indexes.gen_ndx[gen_index] as usize;
            gen_index += 1;
            let gens_end = indexes.gen_ndx[gen_index] as usize;
            let gens = &preset_gens[gens_start..gens_end];

            // Extract the modulators slice
            // Equivalent to: presetMods.slice(modsStart, modsEnd)
            let mods_start = indexes.mod_ndx[mod_index] as usize;
            mod_index += 1;
            let mods_end = indexes.mod_ndx[mod_index] as usize;
            let mods = &preset_mods[mods_start..mods_end];

            // Regular zone if instrument generator present, otherwise global zone
            // Equivalent to: if (gens.some(g => g.generatorType === generatorTypes.instrument))
            if gens.iter().any(|g| g.generator_type == gt::INSTRUMENT) {
                // Equivalent to: preset.createSoundFontZone(mods, gens, instruments)
                match SoundFontPresetZone::new(preset_idx, mods, gens, instrument_count) {
                    Ok(zone) => preset.push_zone(zone.0),
                    Err(e) => warn_fn(&e),
                }
            } else {
                // Equivalent to: preset.globalZone.addGenerators(...gens)
                //                preset.globalZone.addModulators(...mods)
                let global = preset.global_zone_mut();
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
    // Helper generators
    // -----------------------------------------------------------------------

    fn gen_instrument(id: i16) -> Generator {
        Generator::new_unvalidated(gt::INSTRUMENT, id as f64)
    }

    fn gen_pan(value: f64) -> Generator {
        Generator::new_unvalidated(gt::PAN, value)
    }

    // -----------------------------------------------------------------------
    // SoundFontPresetZone::new
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_ok_stores_instrument_idx() {
        let gens = vec![gen_instrument(4), gen_pan(0.0)];
        let zone = SoundFontPresetZone::new(0, &[], &gens, 10).unwrap();
        assert_eq!(zone.0.instrument_idx, 4);
    }

    #[test]
    fn test_new_ok_stores_preset_idx() {
        let gens = vec![gen_instrument(0)];
        let zone = SoundFontPresetZone::new(9, &[], &gens, 5).unwrap();
        assert_eq!(zone.0.parent_preset_idx, 9);
    }

    #[test]
    fn test_new_ok_adds_generators_to_zone() {
        let gens = vec![gen_instrument(1), gen_pan(50.0)];
        let zone = SoundFontPresetZone::new(0, &[], &gens, 5).unwrap();
        // PAN goes into zone.generators as a regular generator
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
    fn test_new_ok_instrument_not_in_generators_vec() {
        // BasicZone::add_generators ignores INSTRUMENT, so it won't appear in generators
        let gens = vec![gen_instrument(0)];
        let zone = SoundFontPresetZone::new(0, &[], &gens, 5).unwrap();
        assert!(
            !zone
                .0
                .zone
                .generators
                .iter()
                .any(|g| g.generator_type == gt::INSTRUMENT),
            "INSTRUMENT should not appear in zone.generators"
        );
    }

    #[test]
    fn test_new_ok_adds_modulators() {
        let gens = vec![gen_instrument(0)];
        let mods = vec![Modulator::default(), Modulator::default()];
        let zone = SoundFontPresetZone::new(0, &mods, &gens, 5).unwrap();
        assert_eq!(zone.0.zone.modulators.len(), 2);
    }

    #[test]
    fn test_new_err_no_instrument_id() {
        let gens = vec![gen_pan(0.0)];
        let result = SoundFontPresetZone::new(0, &[], &gens, 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No instrument ID"));
    }

    #[test]
    fn test_new_err_empty_generators() {
        let result = SoundFontPresetZone::new(0, &[], &[], 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No instrument ID"));
    }

    #[test]
    fn test_new_err_instrument_idx_out_of_range() {
        let gens = vec![gen_instrument(10)];
        let result = SoundFontPresetZone::new(0, &[], &gens, 5); // instrument_count=5, idx=10
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("Invalid instrument ID"), "got: {}", msg);
        assert!(msg.contains("10"), "got: {}", msg);
        assert!(msg.contains("5"), "got: {}", msg);
    }

    #[test]
    fn test_new_ok_boundary_instrument_idx() {
        // instrument_idx == instrument_count - 1 is valid
        let gens = vec![gen_instrument(4)];
        let result = SoundFontPresetZone::new(0, &[], &gens, 5);
        assert!(result.is_ok());
    }

    #[test]
    fn test_new_err_instrument_idx_equals_instrument_count() {
        // instrument_idx == instrument_count is invalid
        let gens = vec![gen_instrument(5)];
        let result = SoundFontPresetZone::new(0, &[], &gens, 5);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Integration tests via apply_preset_zones
    // -----------------------------------------------------------------------

    /// Test implementation of `SoundFontPresetZoneSink`
    struct MockPreset {
        zones_count: usize,
        global_zone: BasicZone,
        pushed_zones: Vec<BasicPresetZone>,
    }

    impl MockPreset {
        fn new(zones_count: usize) -> Self {
            Self {
                zones_count,
                global_zone: BasicZone::new(),
                pushed_zones: Vec::new(),
            }
        }
    }

    impl SoundFontPresetZoneSink for MockPreset {
        fn zones_count(&self) -> usize {
            self.zones_count
        }
        fn push_zone(&mut self, zone: BasicPresetZone) {
            self.pushed_zones.push(zone);
        }
        fn global_zone_mut(&mut self) -> &mut BasicZone {
            &mut self.global_zone
        }
    }

    fn make_indexes(gen_ndx: Vec<u32>, mod_ndx: Vec<u32>) -> ZoneIndexes {
        ZoneIndexes { gen_ndx, mod_ndx }
    }

    #[test]
    fn test_apply_one_preset_one_regular_zone() {
        let mut presets = vec![MockPreset::new(1)];
        let gens = vec![gen_instrument(2)];
        let mods: Vec<Modulator> = vec![];
        let indexes = make_indexes(vec![0, 1], vec![0, 0]);
        apply_preset_zones(&indexes, &gens, &mods, 5, &mut presets);
        assert_eq!(presets[0].pushed_zones.len(), 1);
        assert_eq!(presets[0].pushed_zones[0].instrument_idx, 2);
    }

    #[test]
    fn test_apply_one_preset_global_zone_only() {
        // No instrument generator → goes to global zone
        let mut presets = vec![MockPreset::new(1)];
        let gens = vec![gen_pan(100.0)];
        let mods: Vec<Modulator> = vec![];
        let indexes = make_indexes(vec![0, 1], vec![0, 0]);
        apply_preset_zones(&indexes, &gens, &mods, 5, &mut presets);
        assert_eq!(presets[0].pushed_zones.len(), 0);
        assert_eq!(presets[0].global_zone.generators.len(), 1);
        assert_eq!(presets[0].global_zone.generators[0].generator_type, gt::PAN);
    }

    #[test]
    fn test_apply_preset_idx_matches_enumerate() {
        // Verify that preset_idx is correctly set for 2 presets
        let mut presets = vec![MockPreset::new(1), MockPreset::new(1)];
        let gens = vec![gen_instrument(0), gen_instrument(1)];
        let mods: Vec<Modulator> = vec![];
        let indexes = make_indexes(vec![0, 1, 2], vec![0, 0, 0]);
        apply_preset_zones(&indexes, &gens, &mods, 5, &mut presets);
        assert_eq!(presets[0].pushed_zones[0].parent_preset_idx, 0);
        assert_eq!(presets[1].pushed_zones[0].parent_preset_idx, 1);
    }

    #[test]
    fn test_apply_two_zones_same_preset() {
        // 1 preset with 2 zones (global + regular)
        let mut presets = vec![MockPreset::new(2)];
        let gens = vec![gen_pan(50.0), gen_instrument(3)];
        let mods: Vec<Modulator> = vec![];
        let indexes = make_indexes(vec![0, 1, 2], vec![0, 0, 0]);
        apply_preset_zones(&indexes, &gens, &mods, 5, &mut presets);
        assert_eq!(
            presets[0].pushed_zones.len(),
            1,
            "only regular zone is pushed"
        );
        assert_eq!(presets[0].global_zone.generators.len(), 1);
    }

    #[test]
    fn test_apply_invalid_instrument_id_skips_zone() {
        // instrument_idx out of range → push_zone is not called (warning only)
        let mut presets = vec![MockPreset::new(1)];
        let gens = vec![gen_instrument(99)]; // instrument_count = 5
        let mods: Vec<Modulator> = vec![];
        let indexes = make_indexes(vec![0, 1], vec![0, 0]);
        apply_preset_zones(&indexes, &gens, &mods, 5, &mut presets);
        assert_eq!(
            presets[0].pushed_zones.len(),
            0,
            "invalid zone should be skipped"
        );
    }

    #[test]
    fn test_apply_zero_zones_preset() {
        let mut presets = vec![MockPreset::new(0)];
        let gens: Vec<Generator> = vec![];
        let mods: Vec<Modulator> = vec![];
        let indexes = make_indexes(vec![0], vec![0]);
        apply_preset_zones(&indexes, &gens, &mods, 5, &mut presets);
        assert_eq!(presets[0].pushed_zones.len(), 0);
        assert_eq!(presets[0].global_zone.generators.len(), 0);
    }

    #[test]
    fn test_apply_modulators_added_to_zone() {
        let mut presets = vec![MockPreset::new(1)];
        let gens = vec![gen_instrument(0)];
        let mods = vec![Modulator::default(), Modulator::default()];
        let indexes = make_indexes(vec![0, 1], vec![0, 2]);
        apply_preset_zones(&indexes, &gens, &mods, 5, &mut presets);
        assert_eq!(presets[0].pushed_zones[0].zone.modulators.len(), 2);
    }

    #[test]
    fn test_apply_modulators_added_to_global_zone() {
        let mut presets = vec![MockPreset::new(1)];
        let gens = vec![gen_pan(0.0)]; // global zone
        let mods = vec![Modulator::default()];
        let indexes = make_indexes(vec![0, 1], vec![0, 1]);
        apply_preset_zones(&indexes, &gens, &mods, 5, &mut presets);
        assert_eq!(presets[0].global_zone.modulators.len(), 1);
    }

    #[test]
    fn test_apply_multiple_presets_multiple_zones() {
        // preset0: 2 zones (global + regular), preset1: 1 zone (regular)
        let mut presets = vec![MockPreset::new(2), MockPreset::new(1)];
        let gens = vec![
            gen_pan(10.0),     // preset0 zone0: global
            gen_instrument(0), // preset0 zone1: regular -> instrument_idx=0
            gen_instrument(1), // preset1 zone0: regular -> instrument_idx=1
        ];
        let mods: Vec<Modulator> = vec![];
        // gen_ndx: [0,1,2,3]
        let indexes = make_indexes(vec![0, 1, 2, 3], vec![0, 0, 0, 0]);
        apply_preset_zones(&indexes, &gens, &mods, 5, &mut presets);
        assert_eq!(presets[0].pushed_zones.len(), 1);
        assert_eq!(presets[0].pushed_zones[0].instrument_idx, 0);
        assert_eq!(presets[0].global_zone.generators.len(), 1);
        assert_eq!(presets[1].pushed_zones.len(), 1);
        assert_eq!(presets[1].pushed_zones[0].instrument_idx, 1);
    }
}
