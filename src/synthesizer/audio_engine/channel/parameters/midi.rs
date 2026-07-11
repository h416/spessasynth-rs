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

    // Note: TS 4.3.0 removed the default reverb depth (CC91). It now defaults to 0,
    // matching DEFAULT_MIDI_CONTROLLERS in reset.ts (previously 4.2.0 set it to 40).

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

// ---------------------------------------------------------------------------
// ChannelMidiParameter
// ---------------------------------------------------------------------------

use crate::midi::enums::MidiController;

/// Per-channel MIDI parameters (state driven by MIDI messages / SysEx).
/// Ported from: src/synthesizer/audio_engine/channel/parameters/midi.ts
/// Equivalent to: interface ChannelMIDIParameter
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelMidiParameter {
    /// The current pressure (aftertouch) of this channel.
    /// Equivalent to: pressure
    pub pressure: i32,

    /// The current pitch wheel value (0-16,383) of this channel.
    /// Equivalent to: pitchWheel
    pub pitch_wheel: i32,

    /// The current pitch wheel range, in semitones.
    /// Equivalent to: pitchWheelRange
    pub pitch_wheel_range: f64,

    /// The multiplier of the modulation wheel modulator.
    /// The MIDI spec assumes the default modulation depth is 50 cents, but it
    /// may vary for different sound banks.
    /// Equivalent to: modulationDepth
    pub modulation_depth: f64,

    /// The channel's receiving number (0-based index).
    /// Only used when customChannelNumbers is enabled.
    /// Equivalent to: rxChannel
    pub rx_channel: u8,

    /// If the channel is in the poly mode.
    /// - `true` - POLY ON - regular playback.
    /// - `false` - MONO ON - one note per channel, others are killed on Note On.
    /// Equivalent to: polyMode
    pub poly_mode: bool,

    /// The key shift of the channel (in semitones). Drum channels ignore this.
    /// Equivalent to: keyShift
    pub key_shift: f64,

    /// Cents, RPN/SysEx for fine-tuning. Drum channels ignore this value.
    /// Equivalent to: fineTune
    pub fine_tune: f64,

    /// Enables random panning for every note played on this channel.
    /// Equivalent to: randomPan
    pub random_pan: bool,

    /// Assign mode for the channel (voice assignment behavior on overlap).
    /// 0 = Single, 1 = LimitedMulti, 2 = FullMulti.
    /// Equivalent to: assignMode
    pub assign_mode: u8,

    /// Indicates whether this channel uses the insertion EFX processor.
    /// Equivalent to: efxAssign
    pub efx_assign: bool,

    /// CC1 for GS controller matrix (arbitrary MIDI controller). Default 16.
    /// Equivalent to: cc1
    pub cc1: MidiController,

    /// CC2 for GS controller matrix (arbitrary MIDI controller). Default 17.
    /// Equivalent to: cc2
    pub cc2: MidiController,

    /// Drum map for GS system exclusive tracking (0 melodic, 1 or 2 drum).
    /// Equivalent to: drumMap
    pub drum_map: u8,
}

/// A typed (parameter, value) pair for `MidiChannel::set_midi_parameter`.
/// Rust equivalent of the TS generic `setMIDIParameter<P>(parameter, value)`.
#[derive(Clone, Debug, PartialEq)]
pub enum ChannelMidiParameterValue {
    Pressure(i32),
    PitchWheel(i32),
    PitchWheelRange(f64),
    ModulationDepth(f64),
    RxChannel(u8),
    PolyMode(bool),
    KeyShift(f64),
    FineTune(f64),
    RandomPan(bool),
    AssignMode(u8),
    EfxAssign(bool),
    Cc1(MidiController),
    Cc2(MidiController),
    DrumMap(u8),
}

/// The default MIDI parameters of a channel.
/// Equivalent to: DEFAULT_CHANNEL_MIDI_PARAMETERS
pub const DEFAULT_CHANNEL_MIDI_PARAMETERS: ChannelMidiParameter = ChannelMidiParameter {
    pressure: 0,
    pitch_wheel: 8192,
    pitch_wheel_range: 2.0,
    modulation_depth: 1.0,
    rx_channel: 0,
    poly_mode: true,
    key_shift: 0.0,
    fine_tune: 0.0,
    random_pan: false,
    assign_mode: 2,
    efx_assign: false,
    cc1: 0x10,
    cc2: 0x11,
    drum_map: 0,
};

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
