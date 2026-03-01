/// basic_instrument.rs
/// purpose: BasicInstrument struct - an SF2 instrument with zones, a global zone,
///          and back-references to presets.
/// Ported from: src/soundbank/basic_soundbank/basic_instrument.ts
///
/// # TypeScript vs Rust design differences
///
/// The TypeScript version stores actual object references to `BasicPreset[]` in `linkedTo`, but
/// the Rust version uses `Vec<usize>` (indices into `BasicSoundBank::presets`) to avoid circular ownership.
///
/// - `linkedTo: BasicPreset[]`  → `linked_to: Vec<usize>`  (preset index)
/// - `createZone(sample: BasicSample)` `sample` argument → `sample_idx: usize` + `samples: &mut Vec<BasicSample>`
/// - `deleteUnusedZones` / `delete` / `deleteZone` take `instrument_idx` and `samples`
use std::collections::HashMap;

use crate::soundbank::basic_soundbank::basic_instrument_zone::BasicInstrumentZone;
use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
use crate::soundbank::basic_soundbank::basic_zone::BasicZone;
use crate::soundbank::basic_soundbank::generator_types::{
    GENERATOR_LIMITS, GeneratorType, generator_types as gt,
};
use crate::soundbank::basic_soundbank::modulator::Modulator;
use crate::soundbank::soundfont::write::types::ExtendedSF2Chunks;
use crate::utils::little_endian::write_word;
use crate::utils::string::write_binary_string_indexed;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// SF2 INST record size in bytes.
/// Equivalent to: export const INST_BYTE_SIZE = 22
pub const INST_BYTE_SIZE: usize = 22;

// ---------------------------------------------------------------------------
// Module-private helpers
// ---------------------------------------------------------------------------

/// Generator types that must NOT be moved to the global zone.
/// Equivalent to: const notGlobalizedTypes = new Set([...])
fn is_not_globalizable(gen_type: GeneratorType) -> bool {
    matches!(
        gen_type,
        gt::VEL_RANGE
            | gt::KEY_RANGE
            | gt::INSTRUMENT
            | gt::SAMPLE_ID
            | gt::EXCLUSIVE_CLASS
            | gt::END_OPER
            | gt::SAMPLE_MODES
            | gt::STARTLOOP_ADDRS_OFFSET
            | gt::STARTLOOP_ADDRS_COARSE_OFFSET
            | gt::ENDLOOP_ADDRS_OFFSET
            | gt::ENDLOOP_ADDRS_COARSE_OFFSET
            | gt::START_ADDRS_OFFSET
            | gt::START_ADDRS_COARSE_OFFSET
            | gt::END_ADDR_OFFSET
            | gt::END_ADDRS_COARSE_OFFSET
            | gt::INITIAL_ATTENUATION
            | gt::FINE_TUNE
            | gt::COARSE_TUNE
            | gt::KEY_NUM_TO_VOL_ENV_HOLD
            | gt::KEY_NUM_TO_VOL_ENV_DECAY
            | gt::KEY_NUM_TO_MOD_ENV_HOLD
            | gt::KEY_NUM_TO_MOD_ENV_DECAY
    )
}

/// Returns `Some(value)` if `gen_type` exists in the zone, `None` otherwise.
/// Equivalent to: zone.getGenerator(type, undefined)
fn zone_get_generator_opt(zone: &BasicZone, gen_type: GeneratorType) -> Option<i32> {
    zone.generators
        .iter()
        .find(|g| g.generator_type == gen_type)
        .map(|g| g.generator_value as i32)
}

// ---------------------------------------------------------------------------
// BasicInstrument
// ---------------------------------------------------------------------------

/// Represents a single SF2 instrument.
/// Equivalent to: class BasicInstrument
#[derive(Clone, Debug)]
pub struct BasicInstrument {
    /// Instrument name.
    /// Equivalent to: public name = ""
    pub name: String,

    /// Instrument zones (non-global).
    /// Equivalent to: public zones: BasicInstrumentZone[] = []
    pub zones: Vec<BasicInstrumentZone>,

    /// Global zone (generators/modulators applied to all zones).
    /// `BasicGlobalZone` is a type alias for `BasicZone`.
    /// Equivalent to: public readonly globalZone: BasicGlobalZone = new BasicGlobalZone()
    pub global_zone: BasicZone,

    /// Preset indices that use this instrument (back-references).
    /// Duplicates allowed: one preset can use the same instrument multiple times.
    /// Equivalent to: public readonly linkedTo: BasicPreset[] = []
    pub linked_to: Vec<usize>,
}

impl Default for BasicInstrument {
    fn default() -> Self {
        Self {
            name: String::new(),
            zones: Vec::new(),
            global_zone: BasicZone::new(),
            linked_to: Vec::new(),
        }
    }
}

impl BasicInstrument {
    /// Creates a new, empty BasicInstrument.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a BasicInstrument with the given name.
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    // -----------------------------------------------------------------------
    // useCount getter
    // -----------------------------------------------------------------------

    /// How many presets use this instrument.
    /// Equivalent to: public get useCount(): number
    pub fn use_count(&self) -> usize {
        self.linked_to.len()
    }

    // -----------------------------------------------------------------------
    // createZone
    // -----------------------------------------------------------------------

    /// Creates a new instrument zone referencing `sample_idx` and appends it to `self.zones`.
    /// Also links the sample back to this instrument via `sample.link_to(instrument_idx)`.
    /// Returns the index of the newly created zone.
    ///
    /// Equivalent to: public createZone(sample: BasicSample): BasicInstrumentZone
    pub fn create_zone(
        &mut self,
        instrument_idx: usize,
        sample_idx: usize,
        samples: &mut [BasicSample],
    ) -> usize {
        let use_count = self.use_count() as u32;
        let zone = BasicInstrumentZone::new(instrument_idx, use_count, sample_idx);
        self.zones.push(zone);
        if let Some(sample) = samples.get_mut(sample_idx) {
            sample.link_to(instrument_idx);
        }
        self.zones.len() - 1
    }

    // -----------------------------------------------------------------------
    // linkTo / unlinkFrom
    // -----------------------------------------------------------------------

    /// Links this instrument to a preset (by index).
    /// Increments `use_count` on all child zones.
    /// Equivalent to: public linkTo(preset: BasicPreset)
    pub fn link_to(&mut self, preset_idx: usize) {
        self.linked_to.push(preset_idx);
        for z in &mut self.zones {
            z.use_count += 1;
        }
    }

    /// Unlinks this instrument from a preset (by index).
    /// Removes the **first** occurrence of `preset_idx`.
    /// Decrements `use_count` on all child zones.
    /// Equivalent to: public unlinkFrom(preset: BasicPreset)
    pub fn unlink_from(&mut self, preset_idx: usize) {
        if let Some(pos) = self.linked_to.iter().position(|&p| p == preset_idx) {
            self.linked_to.remove(pos);
            for z in &mut self.zones {
                z.use_count = z.use_count.saturating_sub(1);
            }
        } else {
            eprintln!(
                "Cannot unlink preset {} from instrument {}: not linked.",
                preset_idx, self.name
            );
        }
    }

    // -----------------------------------------------------------------------
    // deleteUnusedZones
    // -----------------------------------------------------------------------

    /// Removes all zones whose `use_count == 0`, unlinking the associated sample.
    /// Equivalent to: public deleteUnusedZones()
    pub fn delete_unused_zones(&mut self, instrument_idx: usize, samples: &mut [BasicSample]) {
        let mut i = 0;
        while i < self.zones.len() {
            if self.zones[i].use_count == 0 {
                let sample_idx = self.zones[i].sample_idx;
                if let Some(sample) = samples.get_mut(sample_idx) {
                    sample.unlink_from(instrument_idx);
                }
                self.zones.remove(i);
            } else {
                i += 1;
            }
        }
    }

    // -----------------------------------------------------------------------
    // delete
    // -----------------------------------------------------------------------

    /// Unlinks all zones' samples from this instrument.
    /// Panics if `use_count > 0` (still referenced by a preset).
    /// Equivalent to: public delete()
    pub fn delete(&self, instrument_idx: usize, samples: &mut [BasicSample]) {
        assert!(
            self.use_count() == 0,
            "Cannot delete instrument '{}' that is still used by {} preset(s)",
            self.name,
            self.use_count()
        );
        for z in &self.zones {
            if let Some(sample) = samples.get_mut(z.sample_idx) {
                sample.unlink_from(instrument_idx);
            }
        }
    }

    // -----------------------------------------------------------------------
    // deleteZone
    // -----------------------------------------------------------------------

    /// Decrements `use_count` of zone at `index`, then removes it if `use_count < 1` or `force`.
    /// Returns `true` if the zone was removed.
    /// Equivalent to: public deleteZone(index: number, force = false): boolean
    pub fn delete_zone(
        &mut self,
        index: usize,
        force: bool,
        instrument_idx: usize,
        samples: &mut [BasicSample],
    ) -> bool {
        // Saturating decrement matches TS behavior: if useCount was 0, result 0 → still deletes.
        self.zones[index].use_count = self.zones[index].use_count.saturating_sub(1);
        let should_delete = self.zones[index].use_count == 0 || force;
        if should_delete {
            let sample_idx = self.zones[index].sample_idx;
            if let Some(sample) = samples.get_mut(sample_idx) {
                sample.unlink_from(instrument_idx);
            }
            self.zones.remove(index);
            return true;
        }
        false
    }

    // -----------------------------------------------------------------------
    // globalize
    // -----------------------------------------------------------------------

    /// Moves repeated generators / modulators to the global zone to reduce redundancy.
    /// Equivalent to: public globalize()
    pub fn globalize(&mut self) {
        // ── Generator globalization ─────────────────────────────────────────
        for checked_type in 0i16..58 {
            if is_not_globalizable(checked_type) {
                continue;
            }

            let default_for_checked: i32 = GENERATOR_LIMITS
                .get(checked_type as usize)
                .and_then(|l| *l)
                .map(|l| l.def)
                .unwrap_or(0);

            // occurrences: generator_value → count
            // The default starts with count 0 to ensure it's always a candidate.
            let mut occurrences: HashMap<i32, i32> = HashMap::new();
            occurrences.insert(default_for_checked, 0);

            // Iterate over zones immutably to build occurrence counts.
            let mut cleared = false;
            'zone_loop: for zone in &self.zones {
                let value = zone_get_generator_opt(&zone.zone, checked_type);
                match value {
                    None => {
                        *occurrences.entry(default_for_checked).or_insert(0) += 1;
                    }
                    Some(v) => {
                        *occurrences.entry(v).or_insert(0) += 1;
                    }
                }

                // Check relative counterpart: if present in any zone, this type cannot be globalized.
                let relative_type: Option<GeneratorType> = match checked_type {
                    gt::DECAY_VOL_ENV => Some(gt::KEY_NUM_TO_VOL_ENV_DECAY),
                    gt::HOLD_VOL_ENV => Some(gt::KEY_NUM_TO_VOL_ENV_HOLD),
                    gt::DECAY_MOD_ENV => Some(gt::KEY_NUM_TO_MOD_ENV_DECAY),
                    gt::HOLD_MOD_ENV => Some(gt::KEY_NUM_TO_MOD_ENV_HOLD),
                    _ => None,
                };

                if let Some(rel_type) = relative_type
                    && zone_get_generator_opt(&zone.zone, rel_type).is_some()
                {
                    occurrences.clear();
                    cleared = true;
                    break 'zone_loop;
                }
            }

            if cleared || occurrences.is_empty() {
                continue;
            }

            // Find the value with the highest occurrence count.
            // Ties: first winner wins (matches TS Object.entries iteration starting with default).
            // Initial: (0, 0) mirrors TypeScript's `let valueToGlobalize: [string, number] = ["0", 0]`.
            let mut target_value: i32 = 0;
            let mut max_count: i32 = 0;
            for (&val, &count) in &occurrences {
                if count > max_count {
                    max_count = count;
                    target_value = val;
                }
            }

            // Add to global zone (only if differs from default).
            if target_value != default_for_checked {
                self.global_zone
                    .set_generator(checked_type, Some(target_value as f64), false);
            }

            // Update individual zones.
            for zone in &mut self.zones {
                let gen_value = zone_get_generator_opt(&zone.zone, checked_type);
                match gen_value {
                    None => {
                        // Generator absent in this zone; add the default to make the override explicit.
                        if target_value != default_for_checked {
                            zone.zone.set_generator(
                                checked_type,
                                Some(default_for_checked as f64),
                                true,
                            );
                        }
                    }
                    Some(v) => {
                        if v == target_value {
                            // Value matches global; remove from local zone.
                            zone.zone.set_generator(checked_type, None, true);
                        }
                    }
                }
            }
        }

        // ── Modulator globalization ─────────────────────────────────────────
        if self.zones.is_empty() {
            return;
        }

        // Clone first zone's modulators; these are candidates for globalization.
        let modulators_to_check: Vec<Modulator> = self.zones[0]
            .zone
            .modulators
            .iter()
            .map(Modulator::copy_from)
            .collect();

        for checked_modulator in &modulators_to_check {
            // Check if this modulator exists in every zone.
            let exists_for_all = self.zones.iter().all(|z| {
                z.zone
                    .modulators
                    .iter()
                    .any(|m| Modulator::is_identical(m, checked_modulator, false))
            });

            if exists_for_all {
                // Move to global zone.
                self.global_zone
                    .modulators
                    .push(Modulator::copy_from(checked_modulator));

                // Remove from each local zone (if transform amount matches).
                for zone in &mut self.zones {
                    if let Some(pos) = zone
                        .zone
                        .modulators
                        .iter()
                        .position(|m| Modulator::is_identical(m, checked_modulator, false))
                        && zone.zone.modulators[pos].transform_amount
                            == checked_modulator.transform_amount
                    {
                        zone.zone.modulators.remove(pos);
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // write
    // -----------------------------------------------------------------------

    /// Writes this instrument's INST record to `inst_data`.
    /// `index` is the starting bag (zone) index for this instrument.
    /// Equivalent to: public write(instData: ExtendedSF2Chunks, index: number)
    pub fn write(&self, inst_data: &mut ExtendedSF2Chunks, index: usize) {
        // Name: first 20 chars to pdta, next 20 chars to xdta.
        let first_20: String = self.name.chars().take(20).collect();
        let rest: String = self.name.chars().skip(20).collect();
        write_binary_string_indexed(&mut inst_data.pdta, &first_20, 20);
        write_binary_string_indexed(&mut inst_data.xdta, &rest, 20);
        // Bag start index: low 16 bits to pdta, high 16 bits to xdta.
        write_word(&mut inst_data.pdta, (index & 0xFFFF) as u32);
        write_word(&mut inst_data.xdta, (index >> 16) as u32);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
    use crate::soundbank::soundfont::write::types::ExtendedSF2Chunks;
    use crate::utils::indexed_array::IndexedByteArray;

    // ── helpers ─────────────────────────────────────────────────────────────

    fn make_chunks() -> ExtendedSF2Chunks {
        ExtendedSF2Chunks {
            pdta: IndexedByteArray::new(64),
            xdta: IndexedByteArray::new(64),
        }
    }

    fn make_sample(name: &str) -> BasicSample {
        BasicSample::new(
            name.to_string(),
            44100,
            60,
            0,
            crate::soundbank::enums::sample_types::MONO_SAMPLE,
            0,
            0,
        )
    }

    fn make_samples(n: usize) -> Vec<BasicSample> {
        (0..n)
            .map(|i| make_sample(&format!("sample{}", i)))
            .collect()
    }

    // ── new / default ────────────────────────────────────────────────────────

    #[test]
    fn test_new_name_empty() {
        let inst = BasicInstrument::new();
        assert_eq!(inst.name, "");
    }

    #[test]
    fn test_new_zones_empty() {
        let inst = BasicInstrument::new();
        assert!(inst.zones.is_empty());
    }

    #[test]
    fn test_new_linked_to_empty() {
        let inst = BasicInstrument::new();
        assert!(inst.linked_to.is_empty());
    }

    #[test]
    fn test_with_name() {
        let inst = BasicInstrument::with_name("Piano");
        assert_eq!(inst.name, "Piano");
    }

    // ── use_count ────────────────────────────────────────────────────────────

    #[test]
    fn test_use_count_zero_initially() {
        let inst = BasicInstrument::new();
        assert_eq!(inst.use_count(), 0);
    }

    #[test]
    fn test_use_count_reflects_linked_to_length() {
        let mut inst = BasicInstrument::new();
        inst.linked_to.push(0);
        inst.linked_to.push(1);
        assert_eq!(inst.use_count(), 2);
    }

    // ── create_zone ──────────────────────────────────────────────────────────

    #[test]
    fn test_create_zone_returns_index() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(2);
        let idx = inst.create_zone(0, 0, &mut samples);
        assert_eq!(idx, 0);
        let idx2 = inst.create_zone(0, 1, &mut samples);
        assert_eq!(idx2, 1);
    }

    #[test]
    fn test_create_zone_appends_to_zones() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(1);
        inst.create_zone(0, 0, &mut samples);
        assert_eq!(inst.zones.len(), 1);
        assert_eq!(inst.zones[0].sample_idx, 0);
    }

    #[test]
    fn test_create_zone_links_sample() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(1);
        inst.create_zone(7, 0, &mut samples);
        assert!(samples[0].linked_to.contains(&7));
    }

    #[test]
    fn test_create_zone_use_count_matches_instrument_use_count() {
        let mut inst = BasicInstrument::new();
        inst.linked_to.push(0); // use_count = 1
        let mut samples = make_samples(1);
        inst.create_zone(0, 0, &mut samples);
        // New zone's use_count should be the instrument's use_count at creation time (= 1).
        assert_eq!(inst.zones[0].use_count, 1);
    }

    // ── link_to ──────────────────────────────────────────────────────────────

    #[test]
    fn test_link_to_appends_preset_idx() {
        let mut inst = BasicInstrument::new();
        inst.link_to(5);
        assert_eq!(inst.linked_to, vec![5]);
    }

    #[test]
    fn test_link_to_allows_duplicates() {
        let mut inst = BasicInstrument::new();
        inst.link_to(3);
        inst.link_to(3);
        assert_eq!(inst.linked_to.len(), 2);
    }

    #[test]
    fn test_link_to_increments_zone_use_count() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(1);
        inst.create_zone(0, 0, &mut samples);
        // zone.use_count == 0 after creation (instrument had use_count 0 at that time)
        assert_eq!(inst.zones[0].use_count, 0);
        inst.link_to(1);
        assert_eq!(inst.zones[0].use_count, 1);
    }

    #[test]
    fn test_link_to_increments_all_zones() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(2);
        inst.create_zone(0, 0, &mut samples);
        inst.create_zone(0, 1, &mut samples);
        inst.link_to(1);
        assert_eq!(inst.zones[0].use_count, 1);
        assert_eq!(inst.zones[1].use_count, 1);
    }

    // ── unlink_from ──────────────────────────────────────────────────────────

    #[test]
    fn test_unlink_from_removes_preset_idx() {
        let mut inst = BasicInstrument::new();
        inst.link_to(2);
        inst.unlink_from(2);
        assert!(inst.linked_to.is_empty());
    }

    #[test]
    fn test_unlink_from_removes_only_first_occurrence() {
        let mut inst = BasicInstrument::new();
        inst.link_to(2);
        inst.link_to(2);
        inst.unlink_from(2);
        assert_eq!(inst.linked_to.len(), 1);
        assert_eq!(inst.linked_to[0], 2);
    }

    #[test]
    fn test_unlink_from_decrements_zone_use_count() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(1);
        inst.link_to(0); // use_count = 1 before zone creation
        inst.create_zone(0, 0, &mut samples); // zone.use_count = 1
        inst.unlink_from(0);
        assert_eq!(inst.zones[0].use_count, 0);
    }

    #[test]
    fn test_unlink_from_unknown_preset_is_noop() {
        // Should not panic, just print a warning.
        let mut inst = BasicInstrument::new();
        inst.link_to(1);
        inst.unlink_from(99); // not linked
        assert_eq!(inst.linked_to.len(), 1);
    }

    // ── delete_unused_zones ──────────────────────────────────────────────────

    #[test]
    fn test_delete_unused_zones_removes_zero_use_count() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(1);
        inst.create_zone(0, 0, &mut samples);
        // zones[0].use_count == 0 (instrument had no presets)
        inst.delete_unused_zones(0, &mut samples);
        assert!(inst.zones.is_empty());
    }

    #[test]
    fn test_delete_unused_zones_unlinks_sample() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(1);
        inst.create_zone(0, 0, &mut samples);
        assert!(samples[0].linked_to.contains(&0));
        inst.delete_unused_zones(0, &mut samples);
        assert!(!samples[0].linked_to.contains(&0));
    }

    #[test]
    fn test_delete_unused_zones_keeps_used_zones() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(2);
        inst.link_to(0); // inst.use_count = 1, so zones created after this have use_count = 1
        inst.create_zone(0, 0, &mut samples); // use_count = 1
        // Manually add a zone with use_count 0
        let zero_zone = BasicInstrumentZone::new(0, 0, 1);
        inst.zones.push(zero_zone);
        // samples[1] not linked; manually set it to simulate an old link
        samples[1].link_to(0);

        inst.delete_unused_zones(0, &mut samples);
        // Zone 0 (use_count=1) should remain; zone 1 (use_count=0) should be removed.
        assert_eq!(inst.zones.len(), 1);
        assert_eq!(inst.zones[0].sample_idx, 0);
    }

    // ── delete ───────────────────────────────────────────────────────────────

    #[test]
    fn test_delete_unlinks_all_samples() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(2);
        inst.create_zone(0, 0, &mut samples);
        inst.create_zone(0, 1, &mut samples);
        assert!(samples[0].linked_to.contains(&0));
        assert!(samples[1].linked_to.contains(&0));

        inst.delete(0, &mut samples);
        assert!(!samples[0].linked_to.contains(&0));
        assert!(!samples[1].linked_to.contains(&0));
    }

    #[test]
    #[should_panic(expected = "Cannot delete instrument")]
    fn test_delete_panics_when_still_used() {
        let mut inst = BasicInstrument::with_name("TestInst");
        inst.link_to(0);
        let mut samples = Vec::new();
        inst.delete(0, &mut samples);
    }

    // ── delete_zone ──────────────────────────────────────────────────────────

    #[test]
    fn test_delete_zone_removes_when_use_count_reaches_zero() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(1);
        inst.create_zone(0, 0, &mut samples);
        // zone has use_count = 0, decrement → still 0 → deletes
        let removed = inst.delete_zone(0, false, 0, &mut samples);
        assert!(removed);
        assert!(inst.zones.is_empty());
    }

    #[test]
    fn test_delete_zone_does_not_remove_when_use_count_positive() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(1);
        inst.link_to(0); // use_count = 1
        inst.create_zone(0, 0, &mut samples); // zone.use_count = 1
        // Decrement: 1 → 0 → deletes. So this zone WILL be deleted.
        // Let's manually set a higher use_count:
        inst.zones[0].use_count = 2;
        let removed = inst.delete_zone(0, false, 0, &mut samples);
        assert!(!removed); // use_count 2 → 1, still ≥ 1
        assert_eq!(inst.zones.len(), 1);
    }

    #[test]
    fn test_delete_zone_force_removes_even_with_positive_use_count() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(1);
        inst.link_to(0);
        inst.create_zone(0, 0, &mut samples);
        inst.zones[0].use_count = 5;
        let removed = inst.delete_zone(0, true, 0, &mut samples);
        assert!(removed);
        assert!(inst.zones.is_empty());
    }

    #[test]
    fn test_delete_zone_unlinks_sample() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(1);
        inst.create_zone(0, 0, &mut samples);
        assert!(samples[0].linked_to.contains(&0));
        inst.delete_zone(0, false, 0, &mut samples);
        assert!(!samples[0].linked_to.contains(&0));
    }

    // ── globalize ────────────────────────────────────────────────────────────

    #[test]
    fn test_globalize_empty_zones_no_panic() {
        let mut inst = BasicInstrument::new();
        inst.globalize(); // should not panic
    }

    #[test]
    fn test_globalize_common_generator_moved_to_global() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(2);
        inst.create_zone(0, 0, &mut samples);
        inst.create_zone(0, 1, &mut samples);
        // Both zones get the same PAN value.
        inst.zones[0]
            .zone
            .set_generator(gt::PAN, Some(100.0), false);
        inst.zones[1]
            .zone
            .set_generator(gt::PAN, Some(100.0), false);

        inst.globalize();

        // PAN should now be in the global zone and removed from local zones.
        assert_eq!(inst.global_zone.get_generator(gt::PAN, i32::MIN), 100);
        // Zones should not have it anymore (or have default 0).
        for z in &inst.zones {
            // Either absent or explicitly set to default (0).
            let v = zone_get_generator_opt(&z.zone, gt::PAN);
            assert!(v.is_none() || v == Some(0));
        }
    }

    #[test]
    fn test_globalize_not_globalizable_types_unchanged() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(1);
        inst.create_zone(0, 0, &mut samples);
        // INITIAL_ATTENUATION is in notGlobalizedTypes.
        inst.zones[0]
            .zone
            .set_generator(gt::INITIAL_ATTENUATION, Some(200.0), false);

        inst.globalize();

        // Should NOT be moved to global zone.
        assert_eq!(
            inst.global_zone
                .get_generator(gt::INITIAL_ATTENUATION, i32::MIN),
            i32::MIN
        );
        // Should remain in local zone.
        assert_eq!(
            inst.zones[0]
                .zone
                .get_generator(gt::INITIAL_ATTENUATION, i32::MIN),
            200
        );
    }

    #[test]
    fn test_globalize_different_values_most_common_wins() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(3);
        inst.create_zone(0, 0, &mut samples);
        inst.create_zone(0, 1, &mut samples);
        inst.create_zone(0, 2, &mut samples);
        // Zones 0 and 1 have PAN=50; zone 2 has PAN=100.
        inst.zones[0].zone.set_generator(gt::PAN, Some(50.0), false);
        inst.zones[1].zone.set_generator(gt::PAN, Some(50.0), false);
        inst.zones[2]
            .zone
            .set_generator(gt::PAN, Some(100.0), false);

        inst.globalize();

        // PAN=50 occurs twice, so it should win and go to global zone.
        assert_eq!(inst.global_zone.get_generator(gt::PAN, i32::MIN), 50);
    }

    #[test]
    fn test_globalize_default_value_not_added_to_global() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(2);
        inst.create_zone(0, 0, &mut samples);
        inst.create_zone(0, 1, &mut samples);
        // PAN default = 0. Both zones have PAN=0 (the default). After globalization,
        // the global zone should NOT have a PAN generator (since default values are not added).
        inst.zones[0].zone.set_generator(gt::PAN, Some(0.0), false);
        inst.zones[1].zone.set_generator(gt::PAN, Some(0.0), false);

        inst.globalize();

        assert_eq!(
            inst.global_zone.get_generator(gt::PAN, i32::MIN),
            i32::MIN,
            "PAN=0 (default) should not be added to global zone"
        );
    }

    #[test]
    fn test_globalize_modulators_moved_when_identical_in_all_zones() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(2);
        inst.create_zone(0, 0, &mut samples);
        inst.create_zone(0, 1, &mut samples);

        let mod1 = Modulator::default();
        inst.zones[0].zone.add_modulators(&[mod1.clone()]);
        inst.zones[1].zone.add_modulators(&[mod1.clone()]);

        inst.globalize();

        // Modulator should be in the global zone.
        assert_eq!(inst.global_zone.modulators.len(), 1);
        // And removed from local zones.
        assert!(inst.zones[0].zone.modulators.is_empty());
        assert!(inst.zones[1].zone.modulators.is_empty());
    }

    #[test]
    fn test_globalize_modulators_not_moved_when_missing_from_some_zones() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(2);
        inst.create_zone(0, 0, &mut samples);
        inst.create_zone(0, 1, &mut samples);

        let mod1 = Modulator::default();
        // Only zone 0 has the modulator.
        inst.zones[0].zone.add_modulators(&[mod1.clone()]);

        inst.globalize();

        // Should NOT be moved to global zone.
        assert!(inst.global_zone.modulators.is_empty());
        // Should remain in zone 0.
        assert_eq!(inst.zones[0].zone.modulators.len(), 1);
    }

    #[test]
    fn test_globalize_keynum_relative_blocks_globalization() {
        let mut inst = BasicInstrument::new();
        let mut samples = make_samples(2);
        inst.create_zone(0, 0, &mut samples);
        inst.create_zone(0, 1, &mut samples);

        // Both zones share decayVolEnv=100, but one also has keyNumToVolEnvDecay set.
        inst.zones[0]
            .zone
            .set_generator(gt::DECAY_VOL_ENV, Some(100.0), false);
        inst.zones[0]
            .zone
            .set_generator(gt::KEY_NUM_TO_VOL_ENV_DECAY, Some(50.0), false);
        inst.zones[1]
            .zone
            .set_generator(gt::DECAY_VOL_ENV, Some(100.0), false);

        inst.globalize();

        // decayVolEnv should NOT be moved to global zone (blocked by keyNumToVolEnvDecay).
        assert_eq!(
            inst.global_zone.get_generator(gt::DECAY_VOL_ENV, i32::MIN),
            i32::MIN
        );
    }

    // ── write ────────────────────────────────────────────────────────────────

    #[test]
    fn test_write_encodes_name_in_pdta() {
        let inst = BasicInstrument::with_name("Piano");
        let mut chunks = make_chunks();
        inst.write(&mut chunks, 0);
        // First 5 bytes of pdta should be ASCII "Piano"
        assert_eq!(chunks.pdta[0], b'P');
        assert_eq!(chunks.pdta[1], b'i');
        assert_eq!(chunks.pdta[2], b'a');
        assert_eq!(chunks.pdta[3], b'n');
        assert_eq!(chunks.pdta[4], b'o');
    }

    #[test]
    fn test_write_pads_name_to_20_bytes_in_pdta() {
        let inst = BasicInstrument::with_name("AB");
        let mut chunks = make_chunks();
        inst.write(&mut chunks, 0);
        // After 2 chars, should be null-padded up to 20 bytes, then 2-byte word.
        // Bytes 2-19 are null, bytes 20-21 are the index word.
        assert_eq!(chunks.pdta[2], 0u8);
        assert_eq!(chunks.pdta[19], 0u8);
    }

    #[test]
    fn test_write_encodes_long_name_truncated_to_20_in_pdta() {
        // Name longer than 20 chars: first 20 go to pdta, rest to xdta.
        let inst = BasicInstrument::with_name("ABCDEFGHIJKLMNOPQRSTUVWXYZ");
        let mut chunks = make_chunks();
        inst.write(&mut chunks, 0);
        // pdta[0..20] should be "ABCDEFGHIJKLMNOPQRST"
        let expected: Vec<u8> = b"ABCDEFGHIJKLMNOPQRST".to_vec();
        assert_eq!(&(*chunks.pdta)[..20], expected.as_slice());
        // xdta[0..6] should be "UVWXYZ"
        assert_eq!(chunks.xdta[0], b'U');
        assert_eq!(chunks.xdta[5], b'Z');
    }

    #[test]
    fn test_write_encodes_index_low16_in_pdta() {
        let inst = BasicInstrument::with_name("Test");
        let mut chunks = make_chunks();
        inst.write(&mut chunks, 0x1234);
        // After 20 bytes name in pdta, 2 bytes for index.
        let idx_lo = u16::from_le_bytes([chunks.pdta[20], chunks.pdta[21]]);
        assert_eq!(idx_lo, 0x1234u16);
    }

    #[test]
    fn test_write_encodes_index_high16_in_xdta() {
        let inst = BasicInstrument::with_name("Test");
        let mut chunks = make_chunks();
        inst.write(&mut chunks, 0x0001_2345);
        // xdta bytes 20-21 = index >> 16 = 1
        let idx_hi = u16::from_le_bytes([chunks.xdta[20], chunks.xdta[21]]);
        assert_eq!(idx_hi, 1u16);
    }

    #[test]
    fn test_write_index_zero() {
        let inst = BasicInstrument::with_name("Test");
        let mut chunks = make_chunks();
        inst.write(&mut chunks, 0);
        let idx_lo = u16::from_le_bytes([chunks.pdta[20], chunks.pdta[21]]);
        let idx_hi = u16::from_le_bytes([chunks.xdta[20], chunks.xdta[21]]);
        assert_eq!(idx_lo, 0);
        assert_eq!(idx_hi, 0);
    }

    // ── is_not_globalizable ──────────────────────────────────────────────────

    #[test]
    fn test_vel_range_is_not_globalizable() {
        assert!(is_not_globalizable(gt::VEL_RANGE));
    }

    #[test]
    fn test_key_range_is_not_globalizable() {
        assert!(is_not_globalizable(gt::KEY_RANGE));
    }

    #[test]
    fn test_sample_id_is_not_globalizable() {
        assert!(is_not_globalizable(gt::SAMPLE_ID));
    }

    #[test]
    fn test_initial_attenuation_is_not_globalizable() {
        assert!(is_not_globalizable(gt::INITIAL_ATTENUATION));
    }

    #[test]
    fn test_pan_is_globalizable() {
        assert!(!is_not_globalizable(gt::PAN));
    }

    #[test]
    fn test_scale_tuning_is_globalizable() {
        assert!(!is_not_globalizable(gt::SCALE_TUNING));
    }

    // ── zone_get_generator_opt ───────────────────────────────────────────────

    #[test]
    fn test_zone_get_generator_opt_absent_returns_none() {
        let zone = BasicZone::new();
        assert_eq!(zone_get_generator_opt(&zone, gt::PAN), None);
    }

    #[test]
    fn test_zone_get_generator_opt_present_returns_some() {
        let mut zone = BasicZone::new();
        zone.set_generator(gt::PAN, Some(42.0), false);
        assert_eq!(zone_get_generator_opt(&zone, gt::PAN), Some(42));
    }

    // ── INST_BYTE_SIZE ───────────────────────────────────────────────────────

    #[test]
    fn test_inst_byte_size() {
        assert_eq!(INST_BYTE_SIZE, 22);
    }
}
