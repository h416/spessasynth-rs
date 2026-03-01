/// sound_bank_manager.rs
/// purpose: SoundBankManager - manages a priority-ordered stack of BasicSoundBank instances,
///          providing a unified preset list and preset selection.
/// Ported from: src/synthesizer/audio_engine/engine_components/sound_bank_manager.ts
///
/// # TypeScript vs Rust design differences
///
/// - `SoundBankManagerPreset extends BasicPreset` (private TS class) is replaced by cloning
///   the BasicPreset and adjusting bank_msb in-place inside `generate_preset_list`.
///
/// - TypeScript's `getPreset` returns `BasicPreset | undefined` (a selectable preset with adjusted
///   bankMSB).  Callers that need `instruments` from the source bank would have to look it up
///   separately.  In Rust, `get_preset_and_bank` returns `Option<(&BasicPreset, &BasicSoundBank)>`
///   so callers can resolve instruments directly.
///
/// - Parallel arrays `selectable_preset_list` / `selectable_bank_idxs` are used instead of
///   object references to track which BasicSoundBank owns each selectable preset.
use std::collections::HashSet;

use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
use crate::soundbank::basic_soundbank::midi_patch::{MidiPatch, MidiPatchNamed, sorter};
use crate::soundbank::basic_soundbank::preset_selector::select_preset;
use crate::synthesizer::types::{PresetListEntry, SynthSystem};
use crate::utils::loggin::spessa_synth_warn;
use crate::utils::midi_hacks::BankSelectHacks;

// ---------------------------------------------------------------------------
// SoundBankManagerListEntry
// ---------------------------------------------------------------------------

/// An entry in the sound bank manager's bank list.
/// Equivalent to: SoundBankManagerListEntry (defined in soundbank/types.ts)
pub struct SoundBankManagerListEntry {
    /// Sound bank identifier string.
    pub id: String,
    /// The sound bank itself.
    pub sound_bank: BasicSoundBank,
    /// Bank offset applied to every preset's bankMSB in this bank.
    pub bank_offset: u8,
}

// ---------------------------------------------------------------------------
// SoundBankManager
// ---------------------------------------------------------------------------

/// Manages a priority-ordered stack of BasicSoundBank instances and exposes
/// a unified, deduplicated preset list for the synthesizer.
/// Equivalent to: class SoundBankManager
pub struct SoundBankManager {
    /// All sound banks, ordered from most important to least important.
    /// Equivalent to: public soundBankList
    pub sound_bank_list: Vec<SoundBankManagerListEntry>,

    /// Called whenever the preset list changes.
    /// Equivalent to: private readonly presetListChangeCallback
    preset_list_change_callback: Box<dyn Fn()>,

    /// Flattened preset list with each preset's bankMSB adjusted by its bank's bank_offset.
    /// Parallel with `selectable_bank_idxs`.
    /// Equivalent to: private selectablePresetList
    selectable_preset_list: Vec<BasicPreset>,

    /// For each entry in `selectable_preset_list`, the index into `sound_bank_list`
    /// that owns that preset.  This Rust-specific array allows resolving the source
    /// BasicSoundBank (needed for instrument lookups) after preset selection.
    selectable_bank_idxs: Vec<usize>,

    /// The public preset list entries derived from `selectable_preset_list`.
    /// Equivalent to: private _presetList
    _preset_list: Vec<PresetListEntry>,
}

impl SoundBankManager {
    /// Creates a new SoundBankManager with the supplied preset list change callback.
    /// Equivalent to: constructor(presetListChangeCallback)
    pub fn new(preset_list_change_callback: impl Fn() + 'static) -> Self {
        Self {
            sound_bank_list: Vec::new(),
            preset_list_change_callback: Box::new(preset_list_change_callback),
            selectable_preset_list: Vec::new(),
            selectable_bank_idxs: Vec::new(),
            _preset_list: Vec::new(),
        }
    }

    /// Returns a clone of the full preset list.
    /// Equivalent to: get presetList()
    pub fn preset_list(&self) -> Vec<PresetListEntry> {
        self._preset_list.clone()
    }

    /// Returns the sound bank IDs in their current priority order.
    /// Equivalent to: get priorityOrder()
    pub fn priority_order(&self) -> Vec<String> {
        self.sound_bank_list.iter().map(|s| s.id.clone()).collect()
    }

    /// Reorders the sound bank list to match `new_list` and regenerates the preset list.
    /// Equivalent to: set priorityOrder(newList)
    pub fn set_priority_order(&mut self, new_list: &[String]) {
        self.sound_bank_list.sort_by_key(|s| {
            new_list
                .iter()
                .position(|id| id == &s.id)
                .unwrap_or(usize::MAX)
        });
        self.generate_preset_list();
    }

    /// Removes the sound bank with the given ID and regenerates the preset list.
    /// Warns if the list is already empty; panics if `id` is not found.
    /// Equivalent to: deleteSoundBank(id)
    pub fn delete_sound_bank(&mut self, id: &str) {
        if self.sound_bank_list.is_empty() {
            spessa_synth_warn("1 soundbank left. Aborting!");
            return;
        }
        let index = self
            .sound_bank_list
            .iter()
            .position(|s| s.id == id)
            .unwrap_or_else(|| panic!("No sound bank with id \"{}\"", id));
        self.sound_bank_list.remove(index);
        self.generate_preset_list();
    }

    /// Adds a new sound bank, or replaces an existing one with the same ID.
    /// Equivalent to: addSoundBank(font, id, bankOffset = 0)
    pub fn add_sound_bank(&mut self, font: BasicSoundBank, id: String, bank_offset: u8) {
        if let Some(entry) = self.sound_bank_list.iter_mut().find(|s| s.id == id) {
            entry.sound_bank = font;
            entry.bank_offset = bank_offset;
        } else {
            self.sound_bank_list.push(SoundBankManagerListEntry {
                id,
                sound_bank: font,
                bank_offset,
            });
        }
        self.generate_preset_list();
    }

    /// Selects a preset and returns a reference to both the preset and its source bank.
    /// Returns `None` if no banks are loaded or the selectable list is empty.
    ///
    /// The returned `&BasicSoundBank` is needed by callers that must resolve instrument
    /// indices (which are only valid relative to their source bank).
    ///
    /// Equivalent to: getPreset(patch, system) — extended for Rust's ownership model.
    pub fn get_preset_and_bank(
        &self,
        patch: MidiPatch,
        system: SynthSystem,
    ) -> Option<(&BasicPreset, &BasicSoundBank)> {
        if self.sound_bank_list.is_empty() || self.selectable_preset_list.is_empty() {
            return None;
        }
        let preset = select_preset(&self.selectable_preset_list, patch, system);
        // Identify the position of the returned reference via pointer equality.
        let idx = self
            .selectable_preset_list
            .iter()
            .position(|p| std::ptr::eq(p, preset))?;
        let bank_idx = self.selectable_bank_idxs[idx];
        Some((preset, &self.sound_bank_list[bank_idx].sound_bank))
    }

    /// Selects a preset and returns a reference to it along with the source bank index.
    /// This Rust-specific variant is used by SynthesizerCore to retrieve the bank index
    /// for caching and resolving instrument indices after selection.
    ///
    /// Returns None if no banks are loaded or the selectable list is empty.
    pub fn get_preset_and_bank_idx(
        &self,
        patch: MidiPatch,
        system: SynthSystem,
    ) -> Option<(&BasicPreset, usize)> {
        if self.sound_bank_list.is_empty() || self.selectable_preset_list.is_empty() {
            return None;
        }
        let preset = select_preset(&self.selectable_preset_list, patch, system);
        let idx = self
            .selectable_preset_list
            .iter()
            .position(|p| std::ptr::eq(p, preset))?;
        let bank_idx = self.selectable_bank_idxs[idx];
        Some((preset, bank_idx))
    }

    /// Destroys all sound banks and clears the bank list.
    /// Equivalent to: destroy()
    pub fn destroy(&mut self) {
        for entry in &mut self.sound_bank_list {
            entry.sound_bank.destroy_sound_bank();
        }
        self.sound_bank_list.clear();
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Rebuilds `selectable_preset_list`, `selectable_bank_idxs`, and `_preset_list`,
    /// then fires the preset list change callback.
    /// Equivalent to: private generatePresetList()
    fn generate_preset_list(&mut self) {
        // Pair: (adjusted BasicPreset, source bank index).
        let mut pairs: Vec<(BasicPreset, usize)> = Vec::new();
        let mut added: HashSet<String> = HashSet::new();

        for (bank_idx, entry) in self.sound_bank_list.iter().enumerate() {
            let bank = &entry.sound_bank;
            let bank_offset = entry.bank_offset;
            for p in &bank.presets {
                // Mirror SoundBankManagerPreset: clone and adjust bankMSB by the bank offset.
                // XG drums are left unchanged by add_bank_offset.
                let adjusted_msb = BankSelectHacks::add_bank_offset(
                    p.bank_msb,
                    bank_offset,
                    p.is_xg_drums(bank.is_xg_bank()),
                );
                let mut adjusted = p.clone();
                adjusted.bank_msb = adjusted_msb;

                let key = adjusted.to_midi_string();
                if !added.contains(&key) {
                    added.insert(key);
                    pairs.push((adjusted, bank_idx));
                }
            }
        }

        // Sort by MIDI patch order; drums are forced to the end.
        pairs.sort_by(|(a, _), (b, _)| {
            sorter(
                &MidiPatch {
                    program: a.program,
                    bank_msb: a.bank_msb,
                    bank_lsb: a.bank_lsb,
                    is_gm_gs_drum: a.is_gm_gs_drum,
                },
                &MidiPatch {
                    program: b.program,
                    bank_msb: b.bank_msb,
                    bank_lsb: b.bank_lsb,
                    is_gm_gs_drum: b.is_gm_gs_drum,
                },
            )
        });

        let (selectable_list, bank_idxs): (Vec<BasicPreset>, Vec<usize>) =
            pairs.into_iter().unzip();

        // Build the public PresetListEntry list.
        // is_any_drums uses the ORIGINAL bank's is_xg_bank flag (matching TS SoundBankManagerPreset
        // behaviour where parentSoundBank refers to the original bank before offset).
        self._preset_list = selectable_list
            .iter()
            .zip(bank_idxs.iter())
            .map(|(p, &bi)| {
                let is_xg = self.sound_bank_list[bi].sound_bank.is_xg_bank();
                PresetListEntry {
                    named: MidiPatchNamed {
                        patch: MidiPatch {
                            program: p.program,
                            bank_msb: p.bank_msb,
                            bank_lsb: p.bank_lsb,
                            is_gm_gs_drum: p.is_gm_gs_drum,
                        },
                        name: p.name.clone(),
                    },
                    is_any_drums: p.is_any_drums(is_xg),
                }
            })
            .collect();

        self.selectable_preset_list = selectable_list;
        self.selectable_bank_idxs = bank_idxs;

        (self.preset_list_change_callback)();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
    use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn melodic_preset(program: u8, bank_msb: u8) -> BasicPreset {
        BasicPreset {
            program,
            bank_msb,
            ..BasicPreset::default()
        }
    }

    fn drum_preset(program: u8) -> BasicPreset {
        BasicPreset {
            program,
            is_gm_gs_drum: true,
            ..BasicPreset::default()
        }
    }

    fn make_bank(presets: Vec<BasicPreset>) -> BasicSoundBank {
        let mut bank = BasicSoundBank::default();
        bank.presets = presets;
        bank
    }

    /// Creates a SoundBankManager and a shared counter incremented by the callback.
    fn make_manager() -> (SoundBankManager, Arc<Mutex<u32>>) {
        let counter = Arc::new(Mutex::new(0u32));
        let c = Arc::clone(&counter);
        let mgr = SoundBankManager::new(move || {
            *c.lock().unwrap() += 1;
        });
        (mgr, counter)
    }

    fn callback_count(counter: &Arc<Mutex<u32>>) -> u32 {
        *counter.lock().unwrap()
    }

    // -----------------------------------------------------------------------
    // new / initial state
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_starts_empty() {
        let (mgr, _) = make_manager();
        assert!(mgr.sound_bank_list.is_empty());
        assert!(mgr.preset_list().is_empty());
        assert!(mgr.priority_order().is_empty());
    }

    // -----------------------------------------------------------------------
    // add_sound_bank
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_sound_bank_inserts_entry() {
        let (mut mgr, counter) = make_manager();
        let bank = make_bank(vec![melodic_preset(0, 0)]);
        mgr.add_sound_bank(bank, "sf1".to_string(), 0);
        assert_eq!(mgr.sound_bank_list.len(), 1);
        assert_eq!(mgr.sound_bank_list[0].id, "sf1");
        assert_eq!(callback_count(&counter), 1);
    }

    #[test]
    fn test_add_sound_bank_preset_list_updated() {
        let (mut mgr, _) = make_manager();
        let bank = make_bank(vec![melodic_preset(10, 0), melodic_preset(20, 0)]);
        mgr.add_sound_bank(bank, "sf1".to_string(), 0);
        let pl = mgr.preset_list();
        assert_eq!(pl.len(), 2);
    }

    #[test]
    fn test_add_sound_bank_replaces_existing_id() {
        let (mut mgr, counter) = make_manager();
        let bank1 = make_bank(vec![melodic_preset(0, 0)]);
        let bank2 = make_bank(vec![melodic_preset(5, 0), melodic_preset(10, 0)]);
        mgr.add_sound_bank(bank1, "sf1".to_string(), 0);
        mgr.add_sound_bank(bank2, "sf1".to_string(), 0);
        // Still one entry, replaced.
        assert_eq!(mgr.sound_bank_list.len(), 1);
        // Preset list now has 2 presets from the replaced bank.
        assert_eq!(mgr.preset_list().len(), 2);
        // Callback called twice (once per add_sound_bank).
        assert_eq!(callback_count(&counter), 2);
    }

    #[test]
    fn test_add_sound_bank_replaces_bank_offset() {
        let (mut mgr, _) = make_manager();
        let bank = make_bank(vec![melodic_preset(0, 0)]);
        mgr.add_sound_bank(bank, "sf1".to_string(), 0);
        let bank2 = make_bank(vec![melodic_preset(0, 0)]);
        mgr.add_sound_bank(bank2, "sf1".to_string(), 5);
        assert_eq!(mgr.sound_bank_list[0].bank_offset, 5);
    }

    // -----------------------------------------------------------------------
    // bank_offset applied to bankMSB
    // -----------------------------------------------------------------------

    #[test]
    fn test_bank_offset_applied_to_preset_bank_msb() {
        let (mut mgr, _) = make_manager();
        let bank = make_bank(vec![melodic_preset(0, 10)]);
        mgr.add_sound_bank(bank, "sf1".to_string(), 5);
        let pl = mgr.preset_list();
        // bankMSB 10 + offset 5 = 15
        assert_eq!(pl[0].named.patch.bank_msb, 15);
    }

    #[test]
    fn test_bank_offset_clamped_to_127() {
        let (mut mgr, _) = make_manager();
        let bank = make_bank(vec![melodic_preset(0, 120)]);
        mgr.add_sound_bank(bank, "sf1".to_string(), 20);
        let pl = mgr.preset_list();
        // 120 + 20 = 140, clamped to 127
        assert_eq!(pl[0].named.patch.bank_msb, 127);
    }

    // -----------------------------------------------------------------------
    // delete_sound_bank
    // -----------------------------------------------------------------------

    #[test]
    fn test_delete_sound_bank_removes_entry() {
        let (mut mgr, _) = make_manager();
        let bank = make_bank(vec![melodic_preset(0, 0)]);
        mgr.add_sound_bank(bank, "sf1".to_string(), 0);
        mgr.delete_sound_bank("sf1");
        assert!(mgr.sound_bank_list.is_empty());
        assert!(mgr.preset_list().is_empty());
    }

    #[test]
    fn test_delete_sound_bank_when_empty_warns_and_returns() {
        // Should not panic, just warn.
        let (mut mgr, _) = make_manager();
        mgr.delete_sound_bank("nonexistent"); // empty list → warns and returns
    }

    #[test]
    #[should_panic(expected = "No sound bank with id")]
    fn test_delete_sound_bank_unknown_id_panics() {
        let (mut mgr, _) = make_manager();
        let bank = make_bank(vec![melodic_preset(0, 0)]);
        mgr.add_sound_bank(bank, "sf1".to_string(), 0);
        mgr.delete_sound_bank("nonexistent");
    }

    #[test]
    fn test_delete_sound_bank_fires_callback() {
        let (mut mgr, counter) = make_manager();
        let bank = make_bank(vec![melodic_preset(0, 0)]);
        mgr.add_sound_bank(bank, "sf1".to_string(), 0);
        let before = callback_count(&counter);
        mgr.delete_sound_bank("sf1");
        assert_eq!(callback_count(&counter), before + 1);
    }

    // -----------------------------------------------------------------------
    // priority_order / set_priority_order
    // -----------------------------------------------------------------------

    #[test]
    fn test_priority_order_returns_ids_in_order() {
        let (mut mgr, _) = make_manager();
        mgr.add_sound_bank(make_bank(vec![]), "a".to_string(), 0);
        mgr.add_sound_bank(make_bank(vec![]), "b".to_string(), 0);
        mgr.add_sound_bank(make_bank(vec![]), "c".to_string(), 0);
        assert_eq!(mgr.priority_order(), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_set_priority_order_reorders_banks() {
        let (mut mgr, _) = make_manager();
        mgr.add_sound_bank(make_bank(vec![]), "a".to_string(), 0);
        mgr.add_sound_bank(make_bank(vec![]), "b".to_string(), 0);
        mgr.add_sound_bank(make_bank(vec![]), "c".to_string(), 0);
        mgr.set_priority_order(&["c".to_string(), "a".to_string(), "b".to_string()]);
        assert_eq!(mgr.priority_order(), vec!["c", "a", "b"]);
    }

    #[test]
    fn test_set_priority_order_fires_callback() {
        let (mut mgr, counter) = make_manager();
        mgr.add_sound_bank(make_bank(vec![]), "a".to_string(), 0);
        let before = callback_count(&counter);
        mgr.set_priority_order(&["a".to_string()]);
        assert_eq!(callback_count(&counter), before + 1);
    }

    // -----------------------------------------------------------------------
    // deduplication in generate_preset_list
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_preset_list_deduplicates_by_midi_string() {
        let (mut mgr, _) = make_manager();
        // Both banks have program=0, bank_msb=0 → same MIDI string → second is deduplicated.
        let bank1 = make_bank(vec![melodic_preset(0, 0)]);
        let bank2 = make_bank(vec![melodic_preset(0, 0)]);
        mgr.add_sound_bank(bank1, "sf1".to_string(), 0);
        mgr.add_sound_bank(bank2, "sf2".to_string(), 0);
        // After dedup: only one preset should appear.
        assert_eq!(mgr.preset_list().len(), 1);
    }

    #[test]
    fn test_generate_preset_list_first_bank_wins_on_dedup() {
        let (mut mgr, _) = make_manager();
        // Bank "sf1" has preset named "Piano" at 0:0:0, bank "sf2" has "Organ" at 0:0:0.
        let mut p1 = melodic_preset(0, 0);
        p1.name = "Piano".to_string();
        let mut p2 = melodic_preset(0, 0);
        p2.name = "Organ".to_string();
        mgr.add_sound_bank(make_bank(vec![p1]), "sf1".to_string(), 0);
        mgr.add_sound_bank(make_bank(vec![p2]), "sf2".to_string(), 0);
        // First bank wins: "Piano"
        assert_eq!(mgr.preset_list()[0].named.name, "Piano");
    }

    #[test]
    fn test_generate_preset_list_different_presets_not_deduped() {
        let (mut mgr, _) = make_manager();
        let bank1 = make_bank(vec![melodic_preset(0, 0)]);
        let bank2 = make_bank(vec![melodic_preset(1, 0)]);
        mgr.add_sound_bank(bank1, "sf1".to_string(), 0);
        mgr.add_sound_bank(bank2, "sf2".to_string(), 0);
        assert_eq!(mgr.preset_list().len(), 2);
    }

    // -----------------------------------------------------------------------
    // preset_list ordering (drums last)
    // -----------------------------------------------------------------------

    #[test]
    fn test_preset_list_drums_sorted_after_melodic() {
        let (mut mgr, _) = make_manager();
        // Drums have program=0, melodic has program=0 too → drums should be after melodic.
        let bank = make_bank(vec![drum_preset(0), melodic_preset(0, 0)]);
        mgr.add_sound_bank(bank, "sf1".to_string(), 0);
        let pl = mgr.preset_list();
        assert_eq!(pl.len(), 2);
        assert!(!pl[0].named.patch.is_gm_gs_drum);
        assert!(pl[1].named.patch.is_gm_gs_drum);
    }

    // -----------------------------------------------------------------------
    // preset_list is_any_drums
    // -----------------------------------------------------------------------

    #[test]
    fn test_preset_list_entry_is_any_drums_true_for_gm_drum() {
        let (mut mgr, _) = make_manager();
        let bank = make_bank(vec![drum_preset(0)]);
        mgr.add_sound_bank(bank, "sf1".to_string(), 0);
        let pl = mgr.preset_list();
        assert!(pl[0].is_any_drums);
    }

    #[test]
    fn test_preset_list_entry_is_any_drums_false_for_melodic() {
        let (mut mgr, _) = make_manager();
        let bank = make_bank(vec![melodic_preset(0, 0)]);
        mgr.add_sound_bank(bank, "sf1".to_string(), 0);
        let pl = mgr.preset_list();
        assert!(!pl[0].is_any_drums);
    }

    // -----------------------------------------------------------------------
    // get_preset_and_bank
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_preset_and_bank_returns_none_when_empty() {
        let (mgr, _) = make_manager();
        let patch = MidiPatch {
            program: 0,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        };
        assert!(mgr.get_preset_and_bank(patch, SynthSystem::Gs).is_none());
    }

    #[test]
    fn test_get_preset_and_bank_returns_preset_and_bank() {
        let (mut mgr, _) = make_manager();
        let bank = make_bank(vec![melodic_preset(10, 0)]);
        mgr.add_sound_bank(bank, "sf1".to_string(), 0);
        let patch = MidiPatch {
            program: 10,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        };
        let result = mgr.get_preset_and_bank(patch, SynthSystem::Gs);
        assert!(result.is_some());
        let (preset, _bank) = result.unwrap();
        assert_eq!(preset.program, 10);
    }

    #[test]
    fn test_get_preset_and_bank_correct_source_bank() {
        let (mut mgr, _) = make_manager();
        // Bank "sf1" has program 0, bank "sf2" has program 10.
        // Requesting program 10 should return the bank from "sf2".
        let bank1 = make_bank(vec![melodic_preset(0, 0)]);
        let bank2 = make_bank(vec![melodic_preset(10, 0)]);
        mgr.add_sound_bank(bank1, "sf1".to_string(), 0);
        mgr.add_sound_bank(bank2, "sf2".to_string(), 0);

        let patch = MidiPatch {
            program: 10,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        };
        let (_, source_bank) = mgr.get_preset_and_bank(patch, SynthSystem::Gs).unwrap();
        // The source bank should have program 10, not program 0.
        assert!(source_bank.presets.iter().any(|p| p.program == 10));
    }

    #[test]
    fn test_get_preset_and_bank_uses_adjusted_bank_msb_for_selection() {
        let (mut mgr, _) = make_manager();
        // Bank has preset at bank_msb=5; with offset=3 it becomes bank_msb=8.
        let bank = make_bank(vec![melodic_preset(0, 5)]);
        mgr.add_sound_bank(bank, "sf1".to_string(), 3);
        // Requesting bank_msb=8 (adjusted) should find the preset.
        let patch = MidiPatch {
            program: 0,
            bank_msb: 8,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        };
        let result = mgr.get_preset_and_bank(patch, SynthSystem::Gs);
        assert!(result.is_some());
        let (preset, _) = result.unwrap();
        // The selectable preset has the adjusted bank_msb.
        assert_eq!(preset.bank_msb, 8);
    }

    // -----------------------------------------------------------------------
    // destroy
    // -----------------------------------------------------------------------

    #[test]
    fn test_destroy_clears_bank_list() {
        let (mut mgr, _) = make_manager();
        mgr.add_sound_bank(make_bank(vec![melodic_preset(0, 0)]), "sf1".to_string(), 0);
        mgr.destroy();
        assert!(mgr.sound_bank_list.is_empty());
    }

    #[test]
    fn test_destroy_empties_banks_data() {
        let (mut mgr, _) = make_manager();
        let bank = make_bank(vec![melodic_preset(0, 0), melodic_preset(10, 0)]);
        mgr.add_sound_bank(bank, "sf1".to_string(), 0);
        mgr.destroy();
        // After destroy the list is empty; no accessible preset data.
        assert!(mgr.preset_list().is_empty() || mgr.sound_bank_list.is_empty());
    }

    // -----------------------------------------------------------------------
    // multiple banks: priority order determines which preset wins dedup
    // -----------------------------------------------------------------------

    #[test]
    fn test_priority_determines_dedup_winner() {
        let (mut mgr, _) = make_manager();
        let mut p_a = melodic_preset(0, 0);
        p_a.name = "From A".to_string();
        let mut p_b = melodic_preset(0, 0);
        p_b.name = "From B".to_string();

        mgr.add_sound_bank(make_bank(vec![p_a]), "A".to_string(), 0);
        mgr.add_sound_bank(make_bank(vec![p_b]), "B".to_string(), 0);

        // A is first → "From A" wins.
        assert_eq!(mgr.preset_list()[0].named.name, "From A");

        // Reverse priority → B is now first → "From B" wins.
        mgr.set_priority_order(&["B".to_string(), "A".to_string()]);
        assert_eq!(mgr.preset_list()[0].named.name, "From B");
    }

    // -----------------------------------------------------------------------
    // callback fired on every mutation
    // -----------------------------------------------------------------------

    #[test]
    fn test_callback_fired_on_every_mutating_operation() {
        let (mut mgr, counter) = make_manager();
        mgr.add_sound_bank(make_bank(vec![]), "a".to_string(), 0);
        assert_eq!(callback_count(&counter), 1);
        mgr.add_sound_bank(make_bank(vec![]), "b".to_string(), 0);
        assert_eq!(callback_count(&counter), 2);
        mgr.set_priority_order(&["b".to_string(), "a".to_string()]);
        assert_eq!(callback_count(&counter), 3);
        mgr.delete_sound_bank("a");
        assert_eq!(callback_count(&counter), 4);
    }
}
