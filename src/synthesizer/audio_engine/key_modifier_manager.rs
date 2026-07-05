/// key_modifier_manager.rs
/// purpose: A manager for custom key overrides for channels.
/// Ported from: src/synthesizer/audio_engine/engine_components/key_modifier_manager.ts
use std::collections::HashMap;

use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;

/// A per-key patch override.
/// Mirrors the anonymous MIDIPatch literal inside KeyModifier in TypeScript,
/// where -1 on a numeric field means "no change".
/// Equivalent to: the `patch` field of KeyModifier
#[derive(Clone, Debug, PartialEq)]
pub struct KeyModifierPatch {
    /// -1 means unchanged.
    /// Equivalent to: bankLSB
    pub bank_lsb: i16,
    /// -1 means unchanged.  >= 0 indicates an active override.
    /// Equivalent to: bankMSB
    pub bank_msb: i16,
    /// -1 means unchanged.
    /// Equivalent to: program
    pub program: i16,
    /// Equivalent to: isGMGSDrum
    pub is_gm_gs_drum: bool,
}

impl KeyModifierPatch {
    /// Returns a "no override" patch (all fields set to -1 / false).
    pub fn no_override() -> Self {
        Self {
            bank_lsb: -1,
            bank_msb: -1,
            program: -1,
            is_gm_gs_drum: false,
        }
    }

    /// Returns true when this patch carries an active bank override (bankMSB >= 0).
    /// Equivalent to: the `bank >= 0` check in hasOverridePatch
    pub fn has_override(&self) -> bool {
        self.bank_msb >= 0
    }

    /// Converts the override patch into a MidiPatch.
    /// Panics if the fields are out of u8 range (should not happen in valid MIDI).
    pub fn to_midi_patch(&self) -> MidiPatch {
        MidiPatch {
            bank_lsb: self.bank_lsb as u8,
            bank_msb: self.bank_msb as u8,
            program: self.program as u8,
            is_gm_gs_drum: self.is_gm_gs_drum,
        }
    }
}

/// Holds velocity, patch and gain overrides for a single MIDI key on a channel.
/// Equivalent to: KeyModifier
#[derive(Clone, Debug, PartialEq)]
pub struct KeyModifier {
    /// The new override velocity. -1 means unchanged.
    /// Equivalent to: velocity
    pub velocity: i16,
    /// The MIDI patch this key uses. -1 on any property means unchanged.
    /// Equivalent to: patch
    pub patch: KeyModifierPatch,
    /// Linear gain override for the voice.
    /// Equivalent to: gain
    pub gain: f64,
}

impl Default for KeyModifier {
    fn default() -> Self {
        Self {
            velocity: -1,
            patch: KeyModifierPatch::no_override(),
            gain: 1.0,
        }
    }
}

/// Manages per-(channel, note) key modifier overrides.
/// Internally stored as a HashMap keyed by `(channel, midi_note)`.
/// Equivalent to: KeyModifierManager
pub struct KeyModifierManager {
    /// Equivalent to: keyMappings (private field)
    key_mappings: HashMap<(u8, u8), KeyModifier>,
}

impl KeyModifierManager {
    /// Creates a new, empty KeyModifierManager.
    pub fn new() -> Self {
        Self {
            key_mappings: HashMap::new(),
        }
    }

    /// Adds a mapping for a MIDI key.
    /// Equivalent to: addMapping
    pub fn add_mapping(&mut self, channel: u8, midi_note: u8, mapping: KeyModifier) {
        self.key_mappings.insert((channel, midi_note), mapping);
    }

    /// Removes the mapping for a MIDI key (no-op if not set).
    /// Equivalent to: deleteMapping
    pub fn delete_mapping(&mut self, channel: u8, midi_note: u8) {
        self.key_mappings.remove(&(channel, midi_note));
    }

    /// Clears all key mappings.
    /// Equivalent to: clearMappings
    pub fn clear_mappings(&mut self) {
        self.key_mappings.clear();
    }

    /// Replaces all mappings with the provided HashMap.
    /// Equivalent to: setMappings
    pub fn set_mappings(&mut self, mappings: HashMap<(u8, u8), KeyModifier>) {
        self.key_mappings = mappings;
    }

    /// Returns a reference to the current key mappings.
    /// Equivalent to: getMappings
    pub fn get_mappings(&self) -> &HashMap<(u8, u8), KeyModifier> {
        &self.key_mappings
    }

    /// Returns the velocity override for a MIDI key, or -1 if not set.
    /// Equivalent to: getVelocity
    pub fn get_velocity(&self, channel: u8, midi_note: u8) -> i16 {
        self.key_mappings
            .get(&(channel, midi_note))
            .map_or(-1, |m| m.velocity)
    }

    /// Returns the gain override for a MIDI key, or 1.0 if not set.
    /// Equivalent to: getGain
    pub fn get_gain(&self, channel: u8, midi_note: u8) -> f64 {
        self.key_mappings
            .get(&(channel, midi_note))
            .map_or(1.0, |m| m.gain)
    }

    /// Returns true if the key has an active patch override (bankMSB >= 0).
    /// Equivalent to: hasOverridePatch
    pub fn has_override_patch(&self, channel: u8, midi_note: u8) -> bool {
        self.key_mappings
            .get(&(channel, midi_note))
            .is_some_and(|m| m.patch.has_override())
    }

    /// Returns the patch override for a MIDI key.
    /// Returns `Err` if no modifier is set for that key.
    /// Equivalent to: getPatch (throws "No modifier." in TS)
    pub fn get_patch(&self, channel: u8, midi_note: u8) -> Result<MidiPatch, &'static str> {
        self.key_mappings
            .get(&(channel, midi_note))
            .map(|m| m.patch.to_midi_patch())
            .ok_or("No modifier.")
    }
}

impl Default for KeyModifierManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_modifier(velocity: i16, bank_msb: i16, program: i16, gain: f64) -> KeyModifier {
        KeyModifier {
            velocity,
            patch: KeyModifierPatch {
                bank_lsb: 0,
                bank_msb,
                program,
                is_gm_gs_drum: false,
            },
            gain,
        }
    }

    // ─── KeyModifierPatch ────────────────────────────────────────────────────

    #[test]
    fn no_override_patch_has_no_override() {
        let p = KeyModifierPatch::no_override();
        assert!(!p.has_override());
        assert_eq!(p.bank_msb, -1);
        assert_eq!(p.bank_lsb, -1);
        assert_eq!(p.program, -1);
        assert!(!p.is_gm_gs_drum);
    }

    #[test]
    fn patch_with_non_negative_bank_msb_has_override() {
        let mut p = KeyModifierPatch::no_override();
        p.bank_msb = 0;
        assert!(p.has_override());
    }

    #[test]
    fn patch_to_midi_patch_converts_fields() {
        let p = KeyModifierPatch {
            bank_lsb: 3,
            bank_msb: 5,
            program: 10,
            is_gm_gs_drum: true,
        };
        let mp = p.to_midi_patch();
        assert_eq!(mp.bank_lsb, 3);
        assert_eq!(mp.bank_msb, 5);
        assert_eq!(mp.program, 10);
        assert!(mp.is_gm_gs_drum);
    }

    // ─── KeyModifier default ─────────────────────────────────────────────────

    #[test]
    fn key_modifier_default_velocity_is_minus_one() {
        let m = KeyModifier::default();
        assert_eq!(m.velocity, -1);
    }

    #[test]
    fn key_modifier_default_gain_is_one() {
        let m = KeyModifier::default();
        assert!((m.gain - 1.0).abs() < 1e-10);
    }

    #[test]
    fn key_modifier_default_patch_has_no_override() {
        let m = KeyModifier::default();
        assert!(!m.patch.has_override());
    }

    // ─── add_mapping / get_velocity / get_gain ───────────────────────────────

    #[test]
    fn add_mapping_then_get_velocity() {
        let mut mgr = KeyModifierManager::new();
        mgr.add_mapping(0, 60, make_modifier(80, -1, -1, 1.0));
        assert_eq!(mgr.get_velocity(0, 60), 80);
    }

    #[test]
    fn add_mapping_then_get_gain() {
        let mut mgr = KeyModifierManager::new();
        mgr.add_mapping(1, 64, make_modifier(-1, -1, -1, 0.5));
        assert!((mgr.get_gain(1, 64) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn get_velocity_returns_minus_one_when_no_mapping() {
        let mgr = KeyModifierManager::new();
        assert_eq!(mgr.get_velocity(0, 60), -1);
    }

    #[test]
    fn get_gain_returns_one_when_no_mapping() {
        let mgr = KeyModifierManager::new();
        assert!((mgr.get_gain(0, 60) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn different_channels_are_independent() {
        let mut mgr = KeyModifierManager::new();
        mgr.add_mapping(0, 60, make_modifier(100, -1, -1, 1.0));
        mgr.add_mapping(1, 60, make_modifier(50, -1, -1, 1.0));
        assert_eq!(mgr.get_velocity(0, 60), 100);
        assert_eq!(mgr.get_velocity(1, 60), 50);
    }

    #[test]
    fn different_notes_same_channel_are_independent() {
        let mut mgr = KeyModifierManager::new();
        mgr.add_mapping(0, 60, make_modifier(100, -1, -1, 1.0));
        mgr.add_mapping(0, 61, make_modifier(64, -1, -1, 1.0));
        assert_eq!(mgr.get_velocity(0, 60), 100);
        assert_eq!(mgr.get_velocity(0, 61), 64);
    }

    // ─── delete_mapping ──────────────────────────────────────────────────────

    #[test]
    fn delete_mapping_removes_entry() {
        let mut mgr = KeyModifierManager::new();
        mgr.add_mapping(0, 60, make_modifier(80, -1, -1, 1.0));
        mgr.delete_mapping(0, 60);
        assert_eq!(mgr.get_velocity(0, 60), -1);
    }

    #[test]
    fn delete_mapping_no_op_when_not_set() {
        let mut mgr = KeyModifierManager::new();
        // Should not panic
        mgr.delete_mapping(0, 60);
        assert_eq!(mgr.get_velocity(0, 60), -1);
    }

    #[test]
    fn delete_mapping_only_removes_specified_key() {
        let mut mgr = KeyModifierManager::new();
        mgr.add_mapping(0, 60, make_modifier(80, -1, -1, 1.0));
        mgr.add_mapping(0, 61, make_modifier(64, -1, -1, 1.0));
        mgr.delete_mapping(0, 60);
        assert_eq!(mgr.get_velocity(0, 60), -1);
        assert_eq!(mgr.get_velocity(0, 61), 64); // untouched
    }

    // ─── clear_mappings ──────────────────────────────────────────────────────

    #[test]
    fn clear_mappings_removes_all() {
        let mut mgr = KeyModifierManager::new();
        mgr.add_mapping(0, 60, make_modifier(80, -1, -1, 1.0));
        mgr.add_mapping(1, 72, make_modifier(100, 0, 10, 2.0));
        mgr.clear_mappings();
        assert_eq!(mgr.get_velocity(0, 60), -1);
        assert_eq!(mgr.get_velocity(1, 72), -1);
        assert!(mgr.get_mappings().is_empty());
    }

    // ─── set_mappings / get_mappings ─────────────────────────────────────────

    #[test]
    fn set_mappings_replaces_existing() {
        let mut mgr = KeyModifierManager::new();
        mgr.add_mapping(0, 60, make_modifier(80, -1, -1, 1.0));

        let mut new_map = HashMap::new();
        new_map.insert((2u8, 48u8), make_modifier(127, 0, 5, 1.5));
        mgr.set_mappings(new_map);

        // Old mapping gone
        assert_eq!(mgr.get_velocity(0, 60), -1);
        // New mapping present
        assert_eq!(mgr.get_velocity(2, 48), 127);
    }

    #[test]
    fn get_mappings_reflects_current_state() {
        let mut mgr = KeyModifierManager::new();
        mgr.add_mapping(0, 60, make_modifier(80, -1, -1, 1.0));
        assert_eq!(mgr.get_mappings().len(), 1);
        assert!(mgr.get_mappings().contains_key(&(0, 60)));
    }

    // ─── has_override_patch ──────────────────────────────────────────────────

    #[test]
    fn has_override_patch_false_when_no_mapping() {
        let mgr = KeyModifierManager::new();
        assert!(!mgr.has_override_patch(0, 60));
    }

    #[test]
    fn has_override_patch_false_when_bank_msb_is_minus_one() {
        let mut mgr = KeyModifierManager::new();
        mgr.add_mapping(0, 60, make_modifier(-1, -1, -1, 1.0));
        assert!(!mgr.has_override_patch(0, 60));
    }

    #[test]
    fn has_override_patch_true_when_bank_msb_is_zero() {
        let mut mgr = KeyModifierManager::new();
        mgr.add_mapping(0, 60, make_modifier(-1, 0, 5, 1.0));
        assert!(mgr.has_override_patch(0, 60));
    }

    #[test]
    fn has_override_patch_true_when_bank_msb_positive() {
        let mut mgr = KeyModifierManager::new();
        mgr.add_mapping(0, 60, make_modifier(-1, 64, 10, 1.0));
        assert!(mgr.has_override_patch(0, 60));
    }

    // ─── get_patch ───────────────────────────────────────────────────────────

    #[test]
    fn get_patch_returns_err_when_no_mapping() {
        let mgr = KeyModifierManager::new();
        assert!(mgr.get_patch(0, 60).is_err());
    }

    #[test]
    fn get_patch_returns_correct_midi_patch() {
        let mut mgr = KeyModifierManager::new();
        let modifier = KeyModifier {
            velocity: -1,
            patch: KeyModifierPatch {
                bank_lsb: 2,
                bank_msb: 5,
                program: 10,
                is_gm_gs_drum: false,
            },
            gain: 1.0,
        };
        mgr.add_mapping(0, 60, modifier);
        let patch = mgr.get_patch(0, 60).unwrap();
        assert_eq!(patch.bank_lsb, 2);
        assert_eq!(patch.bank_msb, 5);
        assert_eq!(patch.program, 10);
        assert!(!patch.is_gm_gs_drum);
    }

    #[test]
    fn get_patch_error_message() {
        let mgr = KeyModifierManager::new();
        assert_eq!(mgr.get_patch(0, 60).unwrap_err(), "No modifier.");
    }

    // ─── overwrite existing mapping ──────────────────────────────────────────

    #[test]
    fn add_mapping_overwrites_previous() {
        let mut mgr = KeyModifierManager::new();
        mgr.add_mapping(0, 60, make_modifier(80, -1, -1, 1.0));
        mgr.add_mapping(0, 60, make_modifier(127, -1, -1, 2.0));
        assert_eq!(mgr.get_velocity(0, 60), 127);
        assert!((mgr.get_gain(0, 60) - 2.0).abs() < 1e-10);
    }
}
