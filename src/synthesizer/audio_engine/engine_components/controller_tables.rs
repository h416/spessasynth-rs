/// controller_tables.rs
/// purpose: Default MIDI controller values and custom controller reset arrays.
/// Ported from: src/synthesizer/audio_engine/engine_components/controller_tables.ts
use crate::midi::enums::midi_controllers;
use crate::soundbank::enums::modulator_sources;
use crate::synthesizer::enums::custom_controllers;

/*
 * A bit of explanation:
 * The controller table is stored as an i16 array, it stores 14-bit values.
 * This controller table is then extended with the modulatorSources section,
 * for example, pitch range and pitch range depth.
 * This allows us for precise control range and supports full pitch-wheel resolution.
 */

/// Equivalent to: NON_CC_INDEX_OFFSET
pub const NON_CC_INDEX_OFFSET: usize = 128;

/// Equivalent to: CONTROLLER_TABLE_SIZE
pub const CONTROLLER_TABLE_SIZE: usize = 147;

/// Compute default MIDI controller values at compile time.
/// Equivalent to: defaultMIDIControllerValues initialization + setResetValue calls
const fn build_default_midi_controller_values() -> [i16; CONTROLLER_TABLE_SIZE] {
    let mut arr = [0i16; CONTROLLER_TABLE_SIZE];

    // setResetValue(i, v) => arr[i] = v << 7

    // Values come from Falcosoft MidiPlayer 6
    arr[midi_controllers::MAIN_VOLUME as usize] = 100 << 7;
    arr[midi_controllers::BALANCE as usize] = 64 << 7;
    arr[midi_controllers::EXPRESSION_CONTROLLER as usize] = 127 << 7;
    arr[midi_controllers::PAN as usize] = 64 << 7;

    // Portamento is on by default, but time is set to 0 so it's effectively off
    arr[midi_controllers::PORTAMENTO_ON_OFF as usize] = 127 << 7;

    arr[midi_controllers::FILTER_RESONANCE as usize] = 64 << 7;
    arr[midi_controllers::RELEASE_TIME as usize] = 64 << 7;
    arr[midi_controllers::ATTACK_TIME as usize] = 64 << 7;
    arr[midi_controllers::BRIGHTNESS as usize] = 64 << 7;

    arr[midi_controllers::DECAY_TIME as usize] = 64 << 7;
    arr[midi_controllers::VIBRATO_RATE as usize] = 64 << 7;
    arr[midi_controllers::VIBRATO_DEPTH as usize] = 64 << 7;
    arr[midi_controllers::VIBRATO_DELAY as usize] = 64 << 7;
    arr[midi_controllers::GENERAL_PURPOSE_CONTROLLER6 as usize] = 64 << 7;
    arr[midi_controllers::GENERAL_PURPOSE_CONTROLLER8 as usize] = 64 << 7;

    arr[midi_controllers::REVERB_DEPTH as usize] = 40 << 7;

    arr[midi_controllers::REGISTERED_PARAMETER_LSB as usize] = 127 << 7;
    arr[midi_controllers::REGISTERED_PARAMETER_MSB as usize] = 127 << 7;
    arr[midi_controllers::NON_REGISTERED_PARAMETER_LSB as usize] = 127 << 7;
    arr[midi_controllers::NON_REGISTERED_PARAMETER_MSB as usize] = 127 << 7;

    // Pitch wheel
    arr[NON_CC_INDEX_OFFSET + modulator_sources::PITCH_WHEEL as usize] = 64 << 7;
    arr[NON_CC_INDEX_OFFSET + modulator_sources::PITCH_WHEEL_RANGE as usize] = 2 << 7;

    arr
}

/// An array with the default MIDI controller values. Note that these are 14-bit, not 7-bit.
/// Equivalent to: defaultMIDIControllerValues
pub const DEFAULT_MIDI_CONTROLLER_VALUES: [i16; CONTROLLER_TABLE_SIZE] =
    build_default_midi_controller_values();

/// Equivalent to: CUSTOM_CONTROLLER_TABLE_SIZE (= Object.keys(customControllers).length = 7)
pub const CUSTOM_CONTROLLER_TABLE_SIZE: usize = 7;

/// Build the custom controller reset array at compile time.
/// Equivalent to: customResetArray initialization
const fn build_custom_reset_array() -> [f32; CUSTOM_CONTROLLER_TABLE_SIZE] {
    let mut arr = [0.0f32; CUSTOM_CONTROLLER_TABLE_SIZE];
    arr[custom_controllers::MODULATION_MULTIPLIER as usize] = 1.0;
    arr
}

/// Equivalent to: customResetArray
pub const CUSTOM_RESET_ARRAY: [f32; CUSTOM_CONTROLLER_TABLE_SIZE] = build_custom_reset_array();

#[cfg(test)]
mod tests {
    use super::*;

    // --- Constants ---

    #[test]
    fn test_non_cc_index_offset() {
        assert_eq!(NON_CC_INDEX_OFFSET, 128);
    }

    #[test]
    fn test_controller_table_size() {
        assert_eq!(CONTROLLER_TABLE_SIZE, 147);
    }

    #[test]
    fn test_custom_controller_table_size() {
        assert_eq!(CUSTOM_CONTROLLER_TABLE_SIZE, 7);
    }

    // --- DEFAULT_MIDI_CONTROLLER_VALUES: array length ---

    #[test]
    fn test_default_midi_controller_values_length() {
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES.len(), CONTROLLER_TABLE_SIZE);
    }

    // --- DEFAULT_MIDI_CONTROLLER_VALUES: non-zero entries ---

    #[test]
    fn test_main_volume() {
        // mainVolume = 7, value = 100 << 7 = 12800
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[7], 12800);
    }

    #[test]
    fn test_balance() {
        // balance = 8, value = 64 << 7 = 8192
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[8], 8192);
    }

    #[test]
    fn test_pan() {
        // pan = 10, value = 64 << 7 = 8192
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[10], 8192);
    }

    #[test]
    fn test_expression_controller() {
        // expressionController = 11, value = 127 << 7 = 16256
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[11], 16256);
    }

    #[test]
    fn test_portamento_on_off() {
        // portamentoOnOff = 65, value = 127 << 7 = 16256
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[65], 16256);
    }

    #[test]
    fn test_filter_resonance() {
        // filterResonance = 71, value = 64 << 7 = 8192
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[71], 8192);
    }

    #[test]
    fn test_release_time() {
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[72], 8192);
    }

    #[test]
    fn test_attack_time() {
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[73], 8192);
    }

    #[test]
    fn test_brightness() {
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[74], 8192);
    }

    #[test]
    fn test_decay_time() {
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[75], 8192);
    }

    #[test]
    fn test_vibrato_rate() {
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[76], 8192);
    }

    #[test]
    fn test_vibrato_depth() {
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[77], 8192);
    }

    #[test]
    fn test_vibrato_delay() {
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[78], 8192);
    }

    #[test]
    fn test_general_purpose_controller6() {
        // generalPurposeController6 = 81
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[81], 8192);
    }

    #[test]
    fn test_general_purpose_controller8() {
        // generalPurposeController8 = 83
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[83], 8192);
    }

    #[test]
    fn test_non_registered_parameter_lsb() {
        // nonRegisteredParameterLSB = 98, value = 127 << 7 = 16256
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[98], 16256);
    }

    #[test]
    fn test_non_registered_parameter_msb() {
        // nonRegisteredParameterMSB = 99
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[99], 16256);
    }

    #[test]
    fn test_registered_parameter_lsb() {
        // registeredParameterLSB = 100
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[100], 16256);
    }

    #[test]
    fn test_registered_parameter_msb() {
        // registeredParameterMSB = 101
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[101], 16256);
    }

    #[test]
    fn test_pitch_wheel() {
        // NON_CC_INDEX_OFFSET + pitchWheel = 128 + 14 = 142, value = 64 << 7 = 8192
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[142], 8192);
    }

    #[test]
    fn test_pitch_wheel_range() {
        // NON_CC_INDEX_OFFSET + pitchWheelRange = 128 + 16 = 144, value = 2 << 7 = 256
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[144], 256);
    }

    // --- DEFAULT_MIDI_CONTROLLER_VALUES: zero entries ---

    #[test]
    fn test_zero_entries_bank_select() {
        // bankSelect = 0 should be 0
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[0], 0);
    }

    #[test]
    fn test_zero_entries_modulation_wheel() {
        // modulationWheel = 1 should be 0
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[1], 0);
    }

    #[test]
    fn test_zero_entries_sustain_pedal() {
        // sustainPedal = 64 should be 0
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[64], 0);
    }

    #[test]
    fn test_zero_entries_last() {
        // last entry (146) should be 0
        assert_eq!(DEFAULT_MIDI_CONTROLLER_VALUES[146], 0);
    }

    // --- CUSTOM_RESET_ARRAY ---

    #[test]
    fn test_custom_reset_array_length() {
        assert_eq!(CUSTOM_RESET_ARRAY.len(), CUSTOM_CONTROLLER_TABLE_SIZE);
    }

    #[test]
    fn test_custom_reset_array_modulation_multiplier() {
        // modulationMultiplier = 2 => 1.0
        assert_eq!(CUSTOM_RESET_ARRAY[2], 1.0f32);
    }

    #[test]
    fn test_custom_reset_array_channel_tuning_zero() {
        // channelTuning = 0 => 0.0
        assert_eq!(CUSTOM_RESET_ARRAY[0], 0.0f32);
    }

    #[test]
    fn test_custom_reset_array_master_tuning_zero() {
        // masterTuning = 3 => 0.0
        assert_eq!(CUSTOM_RESET_ARRAY[3], 0.0f32);
    }

    #[test]
    fn test_custom_reset_array_last_zero() {
        // sf2NPRNGeneratorLSB = 6 => 0.0
        assert_eq!(CUSTOM_RESET_ARRAY[6], 0.0f32);
    }
}
