/// dynamic_modulator_system.rs
/// purpose: DynamicModulatorSystem - Runtime dynamic modulator management for complex messages such as SysEx.
/// Ported from: src/synthesizer/audio_engine/engine_components/dynamic_modulator_system.ts
use crate::soundbank::basic_soundbank::generator_types::GeneratorType;
use crate::soundbank::basic_soundbank::modulator::Modulator;
use crate::soundbank::basic_soundbank::modulator_source::ModulatorSource;
use crate::soundbank::enums::{ModulatorSourceEnum, modulator_curve_types};
use crate::synthesizer::audio_engine::engine_components::controller_tables::NON_CC_INDEX_OFFSET;

// ---------------------------------------------------------------------------
// DynamicModulatorEntry
// ---------------------------------------------------------------------------

/// Struct corresponding to TypeScript's inline type `{ mod: Modulator; id: string }`.
pub struct DynamicModulatorEntry {
    /// The modulator itself. Corresponds to TypeScript's `mod` field.
    /// (Renamed to `modulator` since `mod` is a Rust reserved word)
    pub modulator: Modulator,
    /// Unique identifier string for this entry.
    pub id: String,
}

// ---------------------------------------------------------------------------
// DynamicModulatorSystem
// ---------------------------------------------------------------------------

/// Manages modulators dynamically assigned for complex messages such as SysEx.
/// Equivalent to: class DynamicModulatorSystem
#[derive(Default)]
pub struct DynamicModulatorSystem {
    /// List of currently active dynamic modulators.
    /// Equivalent to: modulatorList
    pub modulator_list: Vec<DynamicModulatorEntry>,
}

impl DynamicModulatorSystem {
    /// Creates a new DynamicModulatorSystem with an empty modulator list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears all dynamic modulators.
    /// Equivalent to: resetModulators()
    pub fn reset_modulators(&mut self) {
        self.modulator_list.clear();
    }

    /// Sets or updates a dynamic modulator.
    ///
    /// * `source` – like midiControllers, values below NON_CC_INDEX_OFFSET are CC,
    ///   values at or above are regular modulator sources.
    /// * `destination` – the generator type to modulate.
    /// * `amount` – modulation amount.
    /// * `is_bipolar` – true for bipolar (-1 to 1), false for unipolar (0 to 1).
    /// * `is_negative` – true for negative direction (1→0), false for positive (0→1).
    ///
    /// Equivalent to: setModulator(source, destination, amount, isBipolar, isNegative)
    pub fn set_modulator(
        &mut self,
        source: usize,
        destination: GeneratorType,
        amount: f64,
        is_bipolar: bool,
        is_negative: bool,
    ) {
        let id = Self::get_modulator_id(source, destination, is_bipolar, is_negative);

        if amount == 0.0 {
            self.delete_modulator(&id);
        }

        if let Some(entry) = self.modulator_list.iter_mut().find(|e| e.id == id) {
            entry.modulator.transform_amount = amount;
        } else {
            let (src_num, is_cc): (ModulatorSourceEnum, bool) = if source >= NON_CC_INDEX_OFFSET {
                ((source - NON_CC_INDEX_OFFSET) as ModulatorSourceEnum, false)
            } else {
                (source as ModulatorSourceEnum, true)
            };
            let modulator = Modulator::new(
                ModulatorSource::new(
                    src_num,
                    modulator_curve_types::LINEAR,
                    is_cc,
                    is_bipolar,
                    false,
                ),
                ModulatorSource::default(),
                destination,
                amount,
                0,
                false,
                false,
            );
            self.modulator_list
                .push(DynamicModulatorEntry { modulator, id });
        }
    }

    /// Generates a modulator ID.
    /// Equivalent to: getModulatorID(source, destination, isBipolar, isNegative)
    /// → `"${source}-${destination}-${isBipolar}-${isNegative}"`
    fn get_modulator_id(
        source: usize,
        destination: GeneratorType,
        is_bipolar: bool,
        is_negative: bool,
    ) -> String {
        format!("{}-{}-{}-{}", source, destination, is_bipolar, is_negative)
    }

    /// Deletes the modulator with the specified ID.
    /// Equivalent to: deleteModulator(id)
    fn delete_modulator(&mut self, id: &str) {
        self.modulator_list.retain(|e| e.id != id);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator_types::generator_types;
    use crate::synthesizer::audio_engine::engine_components::controller_tables::NON_CC_INDEX_OFFSET;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Test helper: returns the expected ID for a (source, destination) combination.
    fn expected_id(src: usize, dst: GeneratorType, bipolar: bool, negative: bool) -> String {
        format!("{}-{}-{}-{}", src, dst, bipolar, negative)
    }

    // -----------------------------------------------------------------------
    // new / default
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_empty_list() {
        let sys = DynamicModulatorSystem::new();
        assert!(sys.modulator_list.is_empty());
    }

    #[test]
    fn test_default_empty_list() {
        let sys = DynamicModulatorSystem::default();
        assert!(sys.modulator_list.is_empty());
    }

    // -----------------------------------------------------------------------
    // reset_modulators
    // -----------------------------------------------------------------------

    #[test]
    fn test_reset_clears_all() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(10, generator_types::PAN, 500.0, false, false);
        sys.set_modulator(7, generator_types::INITIAL_ATTENUATION, 960.0, false, false);
        assert_eq!(sys.modulator_list.len(), 2);
        sys.reset_modulators();
        assert!(sys.modulator_list.is_empty());
    }

    #[test]
    fn test_reset_on_empty_is_noop() {
        let mut sys = DynamicModulatorSystem::new();
        sys.reset_modulators();
        assert!(sys.modulator_list.is_empty());
    }

    // -----------------------------------------------------------------------
    // set_modulator: CC source (source < NON_CC_INDEX_OFFSET)
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_cc_source_adds_entry() {
        let mut sys = DynamicModulatorSystem::new();
        // CC 10 (pan), destination = PAN, amount = 500
        sys.set_modulator(10, generator_types::PAN, 500.0, false, false);
        assert_eq!(sys.modulator_list.len(), 1);
        let entry = &sys.modulator_list[0];
        assert_eq!(entry.modulator.transform_amount, 500.0);
        assert_eq!(entry.modulator.destination, generator_types::PAN);
        assert!(entry.modulator.primary_source.is_cc);
        assert_eq!(entry.modulator.primary_source.index, 10);
    }

    #[test]
    fn test_set_modulator_cc_source_id_is_correct() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(10, generator_types::PAN, 500.0, false, false);
        assert_eq!(
            sys.modulator_list[0].id,
            expected_id(10, generator_types::PAN, false, false)
        );
    }

    #[test]
    fn test_set_modulator_cc_primary_source_is_linear() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(7, generator_types::INITIAL_ATTENUATION, 960.0, false, false);
        assert_eq!(
            sys.modulator_list[0].modulator.primary_source.curve_type,
            modulator_curve_types::LINEAR
        );
    }

    // -----------------------------------------------------------------------
    // set_modulator: non-CC source (source >= NON_CC_INDEX_OFFSET)
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_non_cc_source() {
        let mut sys = DynamicModulatorSystem::new();
        // NON_CC_INDEX_OFFSET + 2 (note_on_velocity)
        let source = NON_CC_INDEX_OFFSET + 2;
        sys.set_modulator(
            source,
            generator_types::INITIAL_ATTENUATION,
            960.0,
            false,
            false,
        );
        let entry = &sys.modulator_list[0];
        assert!(!entry.modulator.primary_source.is_cc);
        assert_eq!(entry.modulator.primary_source.index, 2);
    }

    #[test]
    fn test_set_modulator_non_cc_id_uses_original_source() {
        let mut sys = DynamicModulatorSystem::new();
        let source = NON_CC_INDEX_OFFSET + 2;
        sys.set_modulator(
            source,
            generator_types::INITIAL_ATTENUATION,
            960.0,
            false,
            false,
        );
        assert_eq!(
            sys.modulator_list[0].id,
            expected_id(source, generator_types::INITIAL_ATTENUATION, false, false)
        );
    }

    // -----------------------------------------------------------------------
    // set_modulator: update (when an entry with the same ID exists)
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_updates_existing_amount() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(10, generator_types::PAN, 300.0, false, false);
        // Re-set with the same source/destination/polarity
        sys.set_modulator(10, generator_types::PAN, 700.0, false, false);
        // Only one entry
        assert_eq!(sys.modulator_list.len(), 1);
        assert_eq!(sys.modulator_list[0].modulator.transform_amount, 700.0);
    }

    #[test]
    fn test_set_modulator_different_bipolar_creates_separate_entries() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(10, generator_types::PAN, 300.0, false, false);
        sys.set_modulator(10, generator_types::PAN, 300.0, true, false);
        assert_eq!(sys.modulator_list.len(), 2);
    }

    #[test]
    fn test_set_modulator_different_negative_creates_separate_entries() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(10, generator_types::PAN, 300.0, false, false);
        sys.set_modulator(10, generator_types::PAN, 300.0, false, true);
        assert_eq!(sys.modulator_list.len(), 2);
    }

    #[test]
    fn test_set_modulator_different_destination_creates_separate_entries() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(10, generator_types::PAN, 300.0, false, false);
        sys.set_modulator(10, generator_types::INITIAL_ATTENUATION, 300.0, false, false);
        assert_eq!(sys.modulator_list.len(), 2);
    }

    // -----------------------------------------------------------------------
    // set_modulator: when amount=0 (TS behavior: delete then add new with amount=0)
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_amount_zero_on_existing_replaces_with_zero() {
        let mut sys = DynamicModulatorSystem::new();
        // First add an entry with amount=500
        sys.set_modulator(10, generator_types::PAN, 500.0, false, false);
        assert_eq!(sys.modulator_list.len(), 1);
        // Set with amount=0: TS behavior deletes then adds new (amount=0)
        sys.set_modulator(10, generator_types::PAN, 0.0, false, false);
        assert_eq!(sys.modulator_list.len(), 1);
        assert_eq!(sys.modulator_list[0].modulator.transform_amount, 0.0);
    }

    #[test]
    fn test_set_modulator_amount_zero_on_nonexistent_adds_zero_entry() {
        let mut sys = DynamicModulatorSystem::new();
        // No entry exists with amount=0 → same as TS, a zero-amount entry is added
        sys.set_modulator(10, generator_types::PAN, 0.0, false, false);
        assert_eq!(sys.modulator_list.len(), 1);
        assert_eq!(sys.modulator_list[0].modulator.transform_amount, 0.0);
    }

    // -----------------------------------------------------------------------
    // secondary_source should be default (zero value)
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_secondary_source_is_default() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(10, generator_types::PAN, 500.0, false, false);
        let sec = &sys.modulator_list[0].modulator.secondary_source;
        assert_eq!(*sec, ModulatorSource::default());
    }

    // -----------------------------------------------------------------------
    // bipolar / negative flag propagation
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_bipolar_flag_passed_to_primary_source() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(10, generator_types::PAN, 500.0, true, false);
        assert!(sys.modulator_list[0].modulator.primary_source.is_bipolar);
    }

    #[test]
    fn test_set_modulator_not_bipolar() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(10, generator_types::PAN, 500.0, false, false);
        assert!(!sys.modulator_list[0].modulator.primary_source.is_bipolar);
    }

    #[test]
    fn test_set_modulator_is_negative_not_passed_to_primary_source() {
        // is_negative is used for modulator ID generation, but
        // ModulatorSource::new always sets is_negative=false (same as TS)
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(10, generator_types::PAN, 500.0, false, true);
        assert!(!sys.modulator_list[0].modulator.primary_source.is_negative);
    }

    // -----------------------------------------------------------------------
    // ID format verification
    // -----------------------------------------------------------------------

    #[test]
    fn test_id_format_cc_unipolar_positive() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(10, generator_types::PAN, 100.0, false, false);
        assert_eq!(
            sys.modulator_list[0].id,
            format!("10-{}-false-false", generator_types::PAN)
        );
    }

    #[test]
    fn test_id_format_bipolar_negative() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(7, generator_types::INITIAL_ATTENUATION, 100.0, true, true);
        assert_eq!(
            sys.modulator_list[0].id,
            format!("7-{}-true-true", generator_types::INITIAL_ATTENUATION)
        );
    }

    // -----------------------------------------------------------------------
    // transform_type is always 0 (linear)
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_transform_type_is_zero() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(10, generator_types::PAN, 500.0, false, false);
        assert_eq!(sys.modulator_list[0].modulator.transform_type, 0);
    }

    // -----------------------------------------------------------------------
    // is_effect_modulator / is_default_resonant_modulator are always false
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_not_effect_modulator() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(10, generator_types::PAN, 500.0, false, false);
        assert!(!sys.modulator_list[0].modulator.is_effect_modulator);
    }

    #[test]
    fn test_set_modulator_not_default_resonant_modulator() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(10, generator_types::PAN, 500.0, false, false);
        assert!(
            !sys.modulator_list[0]
                .modulator
                .is_default_resonant_modulator
        );
    }

    // -----------------------------------------------------------------------
    // Independence of multiple entries
    // -----------------------------------------------------------------------

    #[test]
    fn test_multiple_entries_independent() {
        let mut sys = DynamicModulatorSystem::new();
        sys.set_modulator(1, generator_types::VIB_LFO_TO_PITCH, 50.0, false, false);
        sys.set_modulator(7, generator_types::INITIAL_ATTENUATION, 960.0, false, false);
        sys.set_modulator(11, generator_types::INITIAL_ATTENUATION, 960.0, false, false);
        assert_eq!(sys.modulator_list.len(), 3);
        // Verify the destination of each entry
        assert_eq!(
            sys.modulator_list[0].modulator.destination,
            generator_types::VIB_LFO_TO_PITCH
        );
        assert_eq!(
            sys.modulator_list[1].modulator.destination,
            generator_types::INITIAL_ATTENUATION
        );
    }
}
