//! dynamic_modulator_system.rs
//! purpose: DynamicModulatorSystem - Runtime dynamic modulator management for complex messages such as SysEx.
//! Ported from: src/synthesizer/audio_engine/engine_components/dynamic_modulator_system.ts

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
        // Copy, so changing the amount does not affect other channels
        let id = Self::get_modulator_id(&modulator.primary_source, modulator.destination);
        vec![DynamicModulatorEntry { modulator, id }]
    }

    /// Configures the dynamic modulators from an XG part setup controller matrix message.
    /// The lower nibble of `addr3` selects the parameter.
    /// Equivalent to: setupReceiverXG(addr3, data, source, isCC, sourceName, bipolar = false)
    pub fn setup_receiver_xg(
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

        // Value is tested in xg_controller_matrix
        // Ensure the test matches s-yxg50 before changing
        match addr3 & 0x0f {
            0x00 => {
                let v = centered_value.clamp(-24.0, 24.0);
                // Pitch Control
                self.set_modulator(source, is_cc, generator_types::FINE_TUNE, v * 100.0, bipolar, false, false);
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
                // Amplitude
                // In XG it behaves like CC volume (exponential) but we can't have positive attenuation
                if centered_normalized > 0.0 {
                    self.set_modulator(
                        source,
                        is_cc,
                        generator_types::AMPLITUDE,
                        centered_normalized * 1000.0, // Generator is 1/10%
                        bipolar,
                        false,
                        false,
                    );
                    self.set_modulator(source, is_cc, generator_types::INITIAL_ATTENUATION, 0.0, bipolar, false, true);
                } else {
                    self.set_modulator(
                        source,
                        is_cc,
                        generator_types::INITIAL_ATTENUATION,
                        centered_normalized * -960.0,
                        bipolar,
                        false,
                        true,
                    );
                    self.set_modulator(source, is_cc, generator_types::AMPLITUDE, 0.0, bipolar, false, false);
                }
                spessa_synth_info(&format!(
                    "Channel {} {} amplitude control {} %",
                    self.channel,
                    source_name,
                    centered_normalized * 100.0
                ));
            }
            0x03 => {
                // LFO pitch depth
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::VIB_LFO_TO_PITCH,
                    normalized_not_centered * 600.0,
                    bipolar,
                    false,
                    false,
                );
                spessa_synth_info(&format!(
                    "Channel {} {} LFO pitch depth control {} cents",
                    self.channel,
                    source_name,
                    normalized_not_centered * 600.0
                ));
            }
            0x04 => {
                // LFO filter depth
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::VIB_LFO_TO_FILTER_FC,
                    normalized_not_centered * 9600.0,
                    bipolar,
                    false,
                    false,
                );
                spessa_synth_info(&format!(
                    "Channel {} {} LFO filter depth control {} cents",
                    self.channel,
                    source_name,
                    normalized_not_centered * 9600.0
                ));
            }
            0x05 => {
                // LFO amplitude depth
                // Value is tested in xg_controller_matrix
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::VIB_LFO_TO_VOLUME,
                    normalized_not_centered * 200.0,
                    bipolar,
                    false,
                    true,
                );
                // In XG the LFO is only negative (only attenuates), emulate that here
                self.set_modulator(
                    source,
                    is_cc,
                    generator_types::INITIAL_ATTENUATION,
                    normalized_not_centered * 200.0,
                    bipolar,
                    false,
                    true,
                );
                spessa_synth_info(&format!(
                    "Channel {} {} LFO amplitude depth control {} dB",
                    self.channel,
                    source_name,
                    normalized_not_centered * 200.0
                ));
            }
            _ => {}
        }
    }

    /// Configures the dynamic modulators from a GS "patch parameter" receiver
    /// setup message. The lower nibble of `addr3` selects the parameter.
    /// Equivalent to: setupReceiverGS(addr3, data, source, isCC, sourceName, bipolar = false)
    ///
    /// * `addr3` – third address byte of the GS message (parameter selector).
    /// * `data` – the raw data byte (0..127).
    /// * `source` – the raw modulator source index (CC number or SF2 source index).
    /// * `is_cc` – whether `source` is a MIDI CC source or an SF2 source.
    /// * `source_name` – human-readable source name (for logging only).
    /// * `bipolar` – whether the resulting modulation is bipolar.
    #[allow(clippy::too_many_arguments)]
    pub fn setup_receiver_gs(
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
    /// * `is_concave` – true for a concave (exponential) curve.
    ///
    /// Equivalent to: setModulator(source, isCC, destination, amount, isBipolar, isNegative, isConcave)
    #[allow(clippy::too_many_arguments)]
    pub fn set_modulator(
        &mut self,
        source: usize,
        is_cc: bool,
        destination: GeneratorType,
        amount: f64,
        is_bipolar: bool,
        is_negative: bool,
        is_concave: bool,
    ) {
        let primary_source = ModulatorSource::new(
            source as ModulatorControllerSource,
            if is_concave { modulator_curve_types::CONCAVE } else { modulator_curve_types::LINEAR },
            is_cc,
            is_bipolar,
            is_negative,
        );
        let id = Self::get_modulator_id(&primary_source, destination);
        if amount == 0.0 {
            self.delete_modulator(&id);
            return;
        }

        if let Some(entry) = self.modulator_list.iter_mut().find(|e| e.id == id) {
            entry.modulator.transform_amount = amount;
        } else {
            let modulator = Modulator::new(
                primary_source,
                ModulatorSource::default(),
                destination,
                amount,
                0,
                false,
                false,
            );
            self.modulator_list.push(DynamicModulatorEntry { modulator, id });
        }
    }

    /// Generates a modulator ID.
    /// The source enum includes the CC flag, the curve and the polarity.
    /// Equivalent to: getModulatorID(source, destination) → `"${source.toSourceEnum()}-${destination}"`
    fn get_modulator_id(source: &ModulatorSource, destination: GeneratorType) -> String {
        format!("{}-{}", source.to_source_enum(), destination)
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

    fn cc(index: usize) -> ModulatorSource {
        ModulatorSource::new(index as ModulatorControllerSource, modulator_curve_types::LINEAR, true, false, false)
    }

    #[test]
    fn test_reset_seeds_initial_modulators() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 500.0, false, false, false);
        sys.setup_receiver_gs(0x00, 100, 1, true, "mod wheel", false);
        assert!(sys.active);
        sys.reset_modulators();
        assert_eq!(sys.modulator_list.len(), 1);
        assert!(!sys.active);
        let entry = &sys.modulator_list[0];
        assert_eq!(entry.modulator.destination, generator_types::VIB_LFO_RATE);
        assert_eq!(entry.modulator.transform_amount, 1000.0);
        assert!(entry.modulator.primary_source.is_bipolar);
    }

    #[test]
    fn test_id_is_source_enum_and_destination() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 500.0, false, false, false);
        assert_eq!(
            sys.modulator_list[0].id,
            format!("{}-{}", cc(10).to_source_enum(), generator_types::PAN)
        );
    }

    #[test]
    fn test_set_modulator_updates_existing_amount() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 300.0, false, false, false);
        sys.set_modulator(10, true, generator_types::PAN, 700.0, false, false, false);
        assert_eq!(sys.modulator_list.len(), 1);
        assert_eq!(sys.modulator_list[0].modulator.transform_amount, 700.0);
    }

    #[test]
    fn test_polarity_and_curve_create_separate_entries() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 300.0, false, false, false);
        sys.set_modulator(10, true, generator_types::PAN, 300.0, true, false, false);
        sys.set_modulator(10, true, generator_types::PAN, 300.0, false, true, false);
        sys.set_modulator(10, true, generator_types::PAN, 300.0, false, false, true);
        assert_eq!(sys.modulator_list.len(), 4);
        assert!(sys.modulator_list[2].modulator.primary_source.is_negative);
        assert_eq!(
            sys.modulator_list[3].modulator.primary_source.curve_type,
            modulator_curve_types::CONCAVE
        );
    }

    #[test]
    fn test_amount_zero_deletes_and_does_not_add() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.set_modulator(10, true, generator_types::PAN, 500.0, false, false, false);
        sys.set_modulator(10, true, generator_types::PAN, 0.0, false, false, false);
        assert!(sys.modulator_list.is_empty());
        sys.set_modulator(10, true, generator_types::PAN, 0.0, false, false, false);
        assert!(sys.modulator_list.is_empty());
    }

    #[test]
    fn test_xg_amplitude_swaps_attenuation_and_amplitude() {
        let mut sys = DynamicModulatorSystem::new(0);
        // Negative: concave attenuation only
        sys.setup_receiver_xg(0x02, 0, 1, true, "mod wheel", false);
        assert_eq!(sys.modulator_list.len(), 1);
        assert_eq!(sys.modulator_list[0].modulator.destination, generator_types::INITIAL_ATTENUATION);
        assert_eq!(sys.modulator_list[0].modulator.transform_amount, 960.0);
        // Positive: amplitude only, the attenuation is deleted
        sys.setup_receiver_xg(0x02, 127, 1, true, "mod wheel", false);
        assert_eq!(sys.modulator_list.len(), 1);
        assert_eq!(sys.modulator_list[0].modulator.destination, generator_types::AMPLITUDE);
    }

    #[test]
    fn test_gs_lfo1_pitch_depth() {
        let mut sys = DynamicModulatorSystem::new(0);
        sys.setup_receiver_gs(0x04, 127, 1, true, "mod wheel", false);
        let entry = &sys.modulator_list[0];
        assert_eq!(entry.modulator.destination, generator_types::VIB_LFO_TO_PITCH);
        assert_eq!(entry.modulator.transform_amount, 600.0);
    }
}
