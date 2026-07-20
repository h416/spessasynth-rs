/// dynamic_modulator_system.rs
/// purpose: DynamicModulatorSystem - Runtime dynamic modulator management for complex messages such as SysEx.
/// Ported from: src/synthesizer/audio_engine/engine_components/dynamic_modulator_system.ts
use crate::midi::enums::midi_controllers;
use crate::soundbank::basic_soundbank::generator_types::{GeneratorType, generator_types};
use crate::soundbank::basic_soundbank::modulator::{Modulator, get_mod_source_enum};
use crate::soundbank::basic_soundbank::modulator_source::ModulatorSource;
use crate::soundbank::enums::{ModulatorControllerSource, modulator_curve_types};
use crate::utils::loggin::spessa_synth_info;

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
/// Equivalent to: class DynamicModulatorManager
pub struct DynamicModulatorSystem {
    /// List of currently active dynamic modulators.
    /// Equivalent to: modulatorList
    pub modulator_list: Vec<DynamicModulatorEntry>,
    /// Whether any dynamic modulator has been assigned to this channel.
    /// Equivalent to: active
    pub active: bool,
    /// The channel this manager belongs to (used for logging).
    channel: usize,
}

impl DynamicModulatorSystem {
    /// Creates a new DynamicModulatorSystem with an empty modulator list.
    /// Equivalent to: constructor(channel)
    pub fn new(channel: usize) -> Self {
        Self {
            modulator_list: Vec::new(),
            active: false,
            channel,
        }
    }

    /// Resets the dynamic modulators to the initial set.
    ///
    /// Seeds the list with `INITIAL_MODULATORS` (currently a single entry that
    /// maps the GS vibrato-rate CC to the vibrato LFO rate in bare Hz, needed
    /// for special cases such as J-Cycle.mid) and clears the `active` flag.
    /// Equivalent to: resetModulators()
    pub fn reset_modulators(&mut self) {
        self.modulator_list = Self::initial_modulators();
        self.active = false;
    }

    /// Builds the initial modulator list.
    ///
    /// Equivalent to: INITIAL_MODULATORS mapped into `{ mod, id }` entries.
    /// The single initial modulator is `vibratoRate` (linear, forward, bipolar)
    /// → `vibLfoRate`, amount 1000. Its ID uses the primary source's raw source
    /// enum (matching `getModulatorID(m.primarySource.toSourceEnum(), ...)`).
    fn initial_modulators() -> Vec<DynamicModulatorEntry> {
        // getModSourceEnum(linear, isBipolar=true, isNegative=false, isCC=true, vibratoRate)
        let src_enum = get_mod_source_enum(
            modulator_curve_types::LINEAR,
            true,
            false,
            true,
            midi_controllers::VIBRATO_RATE,
        );
        let modulator = Modulator::new(
            ModulatorSource::from_source_enum(src_enum),
            ModulatorSource::from_source_enum(0x0),
            generator_types::VIB_LFO_RATE,
            1000.0,
            0,
            false,
            false,
        );
        let ps = &modulator.primary_source;
        let id = Self::get_modulator_id(
            ps.to_source_enum() as usize,
            modulator.destination,
            ps.is_bipolar,
            ps.is_negative,
        );
        vec![DynamicModulatorEntry { modulator, id }]
    }

    /// Configures the dynamic modulators from a GS "patch parameter" receiver
    /// setup message. The lower nibble of `addr3` selects the parameter.
    /// Equivalent to: setupReceiver(addr3, data, source, isCC, sourceName, bipolar = false)
    ///
    /// * `addr3` – third address byte of the GS message (parameter selector).
    /// * `data` – the raw data byte (0..127).
    /// * `source` – the raw modulator source index (CC number or SF2 source index).
    /// * `is_cc` – whether `source` is a MIDI CC source or an SF2 source.
    /// * `source_name` – human-readable source name (for logging only).
    /// * `bipolar` – whether the resulting modulation is bipolar.
    #[allow(clippy::too_many_arguments)]
    pub fn setup_receiver(
        &mut self,
        addr3: u8,
        data: u8,
        source: usize,
        is_cc: bool,
        source_name: &str,
        bipolar: bool,
    ) {
        self.active = true;
        let centered_value = data as f64 - 64.0;
        let centered_normalized = centered_value / 64.0;
        let normalized_not_centered = data as f64 / 127.0;
        match addr3 & 0x0f {
            0x00 => {
                // Pitch Control.
                // Clamp to [-24; 24] semitones: TS 4.3.14 caps the centered value before
                // using it, to avoid absurd pitch offsets from malformed/extreme SysEx data.
                let v = centered_value.clamp(-24.0, 24.0);
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::FINE_TUNE,
                    v * 100.0,
                    bipolar,
                    false,
                );
                spessa_synth_info(&format!(
                    "Channel {} {} pitch control {} semitones",
                    self.channel, source_name, v
                ));
            }
            0x01 => {
                // Cutoff
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::INITIAL_FILTER_FC,
                    centered_normalized * 9600.0,
                    bipolar,
                    false,
                );
                spessa_synth_info(&format!(
                    "Channel {} {} filter control {} cents",
                    self.channel,
                    source_name,
                    centered_normalized * 9600.0
                ));
            }
            0x02 => {
                // Amplitude (generator is 1/10%)
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::AMPLITUDE,
                    centered_normalized * 1000.0,
                    bipolar,
                    false,
                );
                spessa_synth_info(&format!(
                    "Channel {} {} amplitude control {} %",
                    self.channel,
                    source_name,
                    centered_normalized * 100.0
                ));
            }
            0x03 => {
                // LFO1 Rate (generator is 1/100Hz)
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::VIB_LFO_RATE,
                    centered_normalized * 1000.0,
                    bipolar,
                    false,
                );
                spessa_synth_info(&format!(
                    "Channel {} {} LFO1 rate control {} Hz",
                    self.channel,
                    source_name,
                    centered_normalized * 10.0
                ));
            }
            0x04 => {
                // LFO1 pitch depth
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::VIB_LFO_TO_PITCH,
                    normalized_not_centered * 600.0,
                    bipolar,
                    false,
                );
                spessa_synth_info(&format!(
                    "Channel {} {} LFO1 pitch depth control {} cents",
                    self.channel,
                    source_name,
                    normalized_not_centered * 600.0
                ));
            }
            0x05 => {
                // LFO1 filter depth
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::VIB_LFO_TO_FILTER_FC,
                    normalized_not_centered * 2400.0,
                    bipolar,
                    false,
                );
                spessa_synth_info(&format!(
                    "Channel {} {} LFO1 filter depth control {} cents",
                    self.channel,
                    source_name,
                    normalized_not_centered * 2400.0
                ));
            }
            0x06 => {
                // LFO1 amplitude depth (generator is 1/10%)
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::VIB_LFO_AMPLITUDE_DEPTH,
                    normalized_not_centered * 1000.0,
                    bipolar,
                    false,
                );
                spessa_synth_info(&format!(
                    "Channel {} {} LFO1 amplitude depth control {} %",
                    self.channel,
                    source_name,
                    normalized_not_centered * 100.0
                ));
            }
            0x07 => {
                // LFO2 Rate (generator is 1/100Hz)
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::MOD_LFO_RATE,
                    centered_normalized * 1000.0,
                    bipolar,
                    false,
                );
                spessa_synth_info(&format!(
                    "Channel {} {} LFO2 rate control {} Hz",
                    self.channel,
                    source_name,
                    centered_normalized * 10.0
                ));
            }
            0x08 => {
                // LFO2 pitch depth
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::MOD_LFO_TO_PITCH,
                    normalized_not_centered * 600.0,
                    bipolar,
                    false,
                );
                spessa_synth_info(&format!(
                    "Channel {} {} LFO2 pitch depth control {} cents",
                    self.channel,
                    source_name,
                    normalized_not_centered * 600.0
                ));
            }
            0x09 => {
                // LFO2 filter depth
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::MOD_LFO_TO_FILTER_FC,
                    normalized_not_centered * 2400.0,
                    bipolar,
                    false,
                );
                spessa_synth_info(&format!(
                    "Channel {} {} LFO2 filter depth control {} cents",
                    self.channel,
                    source_name,
                    normalized_not_centered * 2400.0
                ));
            }
            0x0a => {
                // LFO2 amplitude depth (generator is 1/10%)
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::MOD_LFO_AMPLITUDE_DEPTH,
                    normalized_not_centered * 1000.0,
                    bipolar,
                    false,
                );
                spessa_synth_info(&format!(
                    "Channel {} {} LFO2 amplitude depth control {} %",
                    self.channel,
                    source_name,
                    normalized_not_centered * 100.0
                ));
            }
            _ => {}
        }
    }

    /// Sets or updates a dynamic modulator.
    ///
    /// * `source` – the raw modulator source index (CC number or SF2 source index).
    /// * `is_cc` – whether the source is a MIDI CC source or an SF2 source.
    /// * `destination` – the generator type to modulate.
    /// * `amount` – modulation amount.
    /// * `is_bipolar` – true for bipolar (-1 to 1), false for unipolar (0 to 1).
    /// * `is_negative` – true for negative direction (1→0), false for positive (0→1).
    ///
    /// Equivalent to: setModulator(source, isCC, destination, amount, isBipolar, isNegative)
    pub fn set_modulator(
        &mut self,
        source: usize,
        is_cc: bool,
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
            let modulator = Modulator::new(
                ModulatorSource::new(
                    source as ModulatorControllerSource,
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

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Test helper: returns the expected ID for a (source, destination) combination.
    fn expected_id(src: usize, dst: GeneratorType, bipolar: bool, negative: bool) -> String {
        format!("{}-{}-{}-{}", src, dst, bipolar, negative)
    }

    // -----------------------------------------------------------------------
    // new
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_empty_list() {
        let sys = DynamicModulatorSystem::new(0);
        assert!(sys.modulator_list.is_empty());
        assert!(!sys.active);
    }

    // -----------------------------------------------------------------------
    // reset_modulators
    // -----------------------------------------------------------------------

    #[test]
    fn test_reset_seeds_initial_modulators() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 500.0, false, false);
        sys.set_modulator(7, true, generator_types::INITIAL_ATTENUATION, 960.0, false, false);
        assert_eq!(sys.modulator_list.len(), 2);
        sys.setup_receiver(0x00, 100, 1, true, "mod wheel", false);
        assert!(sys.active);
        // Reset restores the single initial (vibrato rate) modulator and clears active.
        sys.reset_modulators();
        assert_eq!(sys.modulator_list.len(), 1);
        assert!(!sys.active);
    }

    #[test]
    fn test_reset_on_new_seeds_one_initial() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.reset_modulators();
        assert_eq!(sys.modulator_list.len(), 1);
    }

    #[test]
    fn test_initial_modulator_maps_vibrato_rate_to_vib_lfo_rate() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.reset_modulators();
        let entry = &sys.modulator_list[0];
        assert_eq!(entry.modulator.destination, generator_types::VIB_LFO_RATE);
        assert_eq!(entry.modulator.transform_amount, 1000.0);
        assert!(entry.modulator.primary_source.is_cc);
        assert_eq!(
            entry.modulator.primary_source.index,
            crate::midi::enums::midi_controllers::VIBRATO_RATE
        );
        assert!(entry.modulator.primary_source.is_bipolar);
    }

    // -----------------------------------------------------------------------
    // setup_receiver
    // -----------------------------------------------------------------------

    #[test]
    fn test_setup_receiver_sets_active() {
        let mut sys = DynamicModulatorSystem::new(0);
        assert!(!sys.active);
        // addr3 lower nibble 0x04 = LFO1 pitch depth
        sys.setup_receiver(0x04, 127, 1, true, "mod wheel", false);
        assert!(sys.active);
        assert_eq!(sys.modulator_list.len(), 1);
        let entry = &sys.modulator_list[0];
        assert_eq!(entry.modulator.destination, generator_types::VIB_LFO_TO_PITCH);
        // normalizedNotCentered * 600 = (127/127) * 600 = 600
        assert_eq!(entry.modulator.transform_amount, 600.0);
        assert!(entry.modulator.primary_source.is_cc);
        assert_eq!(entry.modulator.primary_source.index, 1);
    }

    #[test]
    fn test_setup_receiver_amplitude_uses_centered_normalized() {
        let mut sys = DynamicModulatorSystem::new(0);
        // addr3 lower nibble 0x02 = amplitude, data 127 → centeredNormalized = 63/64
        sys.setup_receiver(0x02, 127, 1, true, "mod wheel", false);
        let entry = &sys.modulator_list[0];
        assert_eq!(entry.modulator.destination, generator_types::AMPLITUDE);
        assert_eq!(entry.modulator.transform_amount, (63.0 / 64.0) * 1000.0);
    }

    // -----------------------------------------------------------------------
    // set_modulator: CC source
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_cc_source_adds_entry() {
        let mut sys = DynamicModulatorSystem::new(0);
        // CC 10 (pan), destination = PAN, amount = 500
        sys.set_modulator(10, true, generator_types::PAN, 500.0, false, false);
        assert_eq!(sys.modulator_list.len(), 1);
        let entry = &sys.modulator_list[0];
        assert_eq!(entry.modulator.transform_amount, 500.0);
        assert_eq!(entry.modulator.destination, generator_types::PAN);
        assert!(entry.modulator.primary_source.is_cc);
        assert_eq!(entry.modulator.primary_source.index, 10);
    }

    #[test]
    fn test_set_modulator_cc_source_id_is_correct() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 500.0, false, false);
        assert_eq!(
            sys.modulator_list[0].id,
            expected_id(10, generator_types::PAN, false, false)
        );
    }

    #[test]
    fn test_set_modulator_cc_primary_source_is_linear() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(7, true, generator_types::INITIAL_ATTENUATION, 960.0, false, false);
        assert_eq!(
            sys.modulator_list[0].modulator.primary_source.curve_type,
            modulator_curve_types::LINEAR
        );
    }

    // -----------------------------------------------------------------------
    // set_modulator: non-CC source (is_cc = false, raw source index)
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_non_cc_source() {
        let mut sys = DynamicModulatorSystem::new(0);
        // Source index 2 (note_on_velocity), is_cc = false
        sys.set_modulator(
            2,
            false,
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
    fn test_set_modulator_non_cc_id_uses_raw_source() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(
            2,
            false,
            generator_types::INITIAL_ATTENUATION,
            960.0,
            false,
            false,
        );
        assert_eq!(
            sys.modulator_list[0].id,
            expected_id(2, generator_types::INITIAL_ATTENUATION, false, false)
        );
    }

    // -----------------------------------------------------------------------
    // set_modulator: update (when an entry with the same ID exists)
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_updates_existing_amount() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 300.0, false, false);
        // Re-set with the same source/destination/polarity
        sys.set_modulator(10, true, generator_types::PAN, 700.0, false, false);
        // Only one entry
        assert_eq!(sys.modulator_list.len(), 1);
        assert_eq!(sys.modulator_list[0].modulator.transform_amount, 700.0);
    }

    #[test]
    fn test_set_modulator_different_bipolar_creates_separate_entries() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 300.0, false, false);
        sys.set_modulator(10, true, generator_types::PAN, 300.0, true, false);
        assert_eq!(sys.modulator_list.len(), 2);
    }

    #[test]
    fn test_set_modulator_different_negative_creates_separate_entries() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 300.0, false, false);
        sys.set_modulator(10, true, generator_types::PAN, 300.0, false, true);
        assert_eq!(sys.modulator_list.len(), 2);
    }

    #[test]
    fn test_set_modulator_different_destination_creates_separate_entries() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 300.0, false, false);
        sys.set_modulator(10, true, generator_types::INITIAL_ATTENUATION, 300.0, false, false);
        assert_eq!(sys.modulator_list.len(), 2);
    }

    // -----------------------------------------------------------------------
    // set_modulator: when amount=0 (TS behavior: delete then add new with amount=0)
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_amount_zero_on_existing_replaces_with_zero() {
        let mut sys = DynamicModulatorSystem::new(0);
        // First add an entry with amount=500
        sys.set_modulator(10, true, generator_types::PAN, 500.0, false, false);
        assert_eq!(sys.modulator_list.len(), 1);
        // Set with amount=0: TS behavior deletes then adds new (amount=0)
        sys.set_modulator(10, true, generator_types::PAN, 0.0, false, false);
        assert_eq!(sys.modulator_list.len(), 1);
        assert_eq!(sys.modulator_list[0].modulator.transform_amount, 0.0);
    }

    #[test]
    fn test_set_modulator_amount_zero_on_nonexistent_adds_zero_entry() {
        let mut sys = DynamicModulatorSystem::new(0);
        // No entry exists with amount=0 → same as TS, a zero-amount entry is added
        sys.set_modulator(10, true, generator_types::PAN, 0.0, false, false);
        assert_eq!(sys.modulator_list.len(), 1);
        assert_eq!(sys.modulator_list[0].modulator.transform_amount, 0.0);
    }

    // -----------------------------------------------------------------------
    // secondary_source should be default (zero value)
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_secondary_source_is_default() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 500.0, false, false);
        let sec = &sys.modulator_list[0].modulator.secondary_source;
        assert_eq!(*sec, ModulatorSource::default());
    }

    // -----------------------------------------------------------------------
    // bipolar / negative flag propagation
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_bipolar_flag_passed_to_primary_source() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 500.0, true, false);
        assert!(sys.modulator_list[0].modulator.primary_source.is_bipolar);
    }

    #[test]
    fn test_set_modulator_not_bipolar() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 500.0, false, false);
        assert!(!sys.modulator_list[0].modulator.primary_source.is_bipolar);
    }

    #[test]
    fn test_set_modulator_is_negative_not_passed_to_primary_source() {
        // is_negative is used for modulator ID generation, but
        // ModulatorSource::new always sets is_negative=false (same as TS)
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 500.0, false, true);
        assert!(!sys.modulator_list[0].modulator.primary_source.is_negative);
    }

    // -----------------------------------------------------------------------
    // ID format verification
    // -----------------------------------------------------------------------

    #[test]
    fn test_id_format_cc_unipolar_positive() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 100.0, false, false);
        assert_eq!(
            sys.modulator_list[0].id,
            format!("10-{}-false-false", generator_types::PAN)
        );
    }

    #[test]
    fn test_id_format_bipolar_negative() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(7, true, generator_types::INITIAL_ATTENUATION, 100.0, true, true);
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
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 500.0, false, false);
        assert_eq!(sys.modulator_list[0].modulator.transform_type, 0);
    }

    // -----------------------------------------------------------------------
    // is_effect_modulator / is_default_resonant_modulator are always false
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_modulator_not_effect_modulator() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 500.0, false, false);
        assert!(!sys.modulator_list[0].modulator.is_effect_modulator);
    }

    #[test]
    fn test_set_modulator_not_default_resonant_modulator() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 500.0, false, false);
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
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(1, true, generator_types::VIB_LFO_TO_PITCH, 50.0, false, false);
        sys.set_modulator(7, true, generator_types::INITIAL_ATTENUATION, 960.0, false, false);
        sys.set_modulator(11, true, generator_types::INITIAL_ATTENUATION, 960.0, false, false);
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
