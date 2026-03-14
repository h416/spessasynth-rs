/// enums.rs
/// purpose: MIDI message type and controller number constants.
/// Ported from: src/midi/enums.ts
/// MIDI message type constants.
/// Equivalent to: midiMessageTypes
pub mod midi_message_types {
    // --- Channel messages (status bytes) ---
    pub const NOTE_OFF: u8 = 0x80;
    pub const NOTE_ON: u8 = 0x90;
    pub const POLY_PRESSURE: u8 = 0xA0;
    pub const CONTROLLER_CHANGE: u8 = 0xB0;
    pub const PROGRAM_CHANGE: u8 = 0xC0;
    pub const CHANNEL_PRESSURE: u8 = 0xD0;
    pub const PITCH_WHEEL: u8 = 0xE0;

    // --- System messages ---
    pub const SYSTEM_EXCLUSIVE: u8 = 0xF0;
    pub const TIMECODE: u8 = 0xF1;
    pub const SONG_POSITION: u8 = 0xF2;
    pub const SONG_SELECT: u8 = 0xF3;
    pub const TUNE_REQUEST: u8 = 0xF6;
    pub const CLOCK: u8 = 0xF8;
    pub const START: u8 = 0xFA;
    pub const CONTINUE: u8 = 0xFB;
    pub const STOP: u8 = 0xFC;
    pub const ACTIVE_SENSING: u8 = 0xFE;
    pub const RESET: u8 = 0xFF;

    // --- Meta event types ---
    pub const SEQUENCE_NUMBER: u8 = 0x00;
    pub const TEXT: u8 = 0x01;
    pub const COPYRIGHT: u8 = 0x02;
    pub const TRACK_NAME: u8 = 0x03;
    pub const INSTRUMENT_NAME: u8 = 0x04;
    pub const LYRIC: u8 = 0x05;
    pub const MARKER: u8 = 0x06;
    pub const CUE_POINT: u8 = 0x07;
    pub const PROGRAM_NAME: u8 = 0x08;
    pub const MIDI_CHANNEL_PREFIX: u8 = 0x20;
    pub const MIDI_PORT: u8 = 0x21;
    pub const END_OF_TRACK: u8 = 0x2F;
    pub const SET_TEMPO: u8 = 0x51;
    pub const SMPTE_OFFSET: u8 = 0x54;
    pub const TIME_SIGNATURE: u8 = 0x58;
    pub const KEY_SIGNATURE: u8 = 0x59;
    pub const SEQUENCE_SPECIFIC: u8 = 0x7F;
}

/// Equivalent to: MIDIMessageType
pub type MidiMessageType = u8;

/// MIDI controller number constants.
/// Equivalent to: midiControllers
pub mod midi_controllers {
    pub const BANK_SELECT: u8 = 0;
    pub const MODULATION_WHEEL: u8 = 1;
    pub const BREATH_CONTROLLER: u8 = 2;
    pub const UNDEFINED_CC3: u8 = 3;
    pub const FOOT_CONTROLLER: u8 = 4;
    pub const PORTAMENTO_TIME: u8 = 5;
    pub const DATA_ENTRY_MSB: u8 = 6;
    pub const MAIN_VOLUME: u8 = 7;
    pub const BALANCE: u8 = 8;
    pub const UNDEFINED_CC9: u8 = 9;
    pub const PAN: u8 = 10;
    pub const EXPRESSION_CONTROLLER: u8 = 11;
    pub const EFFECT_CONTROL1: u8 = 12;
    pub const EFFECT_CONTROL2: u8 = 13;
    pub const UNDEFINED_CC14: u8 = 14;
    pub const UNDEFINED_CC15: u8 = 15;
    pub const GENERAL_PURPOSE_CONTROLLER1: u8 = 16;
    pub const GENERAL_PURPOSE_CONTROLLER2: u8 = 17;
    pub const GENERAL_PURPOSE_CONTROLLER3: u8 = 18;
    pub const GENERAL_PURPOSE_CONTROLLER4: u8 = 19;
    pub const UNDEFINED_CC20: u8 = 20;
    pub const UNDEFINED_CC21: u8 = 21;
    pub const UNDEFINED_CC22: u8 = 22;
    pub const UNDEFINED_CC23: u8 = 23;
    pub const UNDEFINED_CC24: u8 = 24;
    pub const UNDEFINED_CC25: u8 = 25;
    pub const UNDEFINED_CC26: u8 = 26;
    pub const UNDEFINED_CC27: u8 = 27;
    pub const UNDEFINED_CC28: u8 = 28;
    pub const UNDEFINED_CC29: u8 = 29;
    pub const UNDEFINED_CC30: u8 = 30;
    pub const UNDEFINED_CC31: u8 = 31;
    pub const BANK_SELECT_LSB: u8 = 32;
    pub const MODULATION_WHEEL_LSB: u8 = 33;
    pub const BREATH_CONTROLLER_LSB: u8 = 34;
    pub const UNDEFINED_CC3_LSB: u8 = 35;
    pub const FOOT_CONTROLLER_LSB: u8 = 36;
    pub const PORTAMENTO_TIME_LSB: u8 = 37;
    pub const DATA_ENTRY_LSB: u8 = 38;
    pub const MAIN_VOLUME_LSB: u8 = 39;
    pub const BALANCE_LSB: u8 = 40;
    pub const UNDEFINED_CC9_LSB: u8 = 41;
    pub const PAN_LSB: u8 = 42;
    pub const EXPRESSION_CONTROLLER_LSB: u8 = 43;
    pub const EFFECT_CONTROL1_LSB: u8 = 44;
    pub const EFFECT_CONTROL2_LSB: u8 = 45;
    pub const UNDEFINED_CC14_LSB: u8 = 46;
    pub const UNDEFINED_CC15_LSB: u8 = 47;
    pub const UNDEFINED_CC16_LSB: u8 = 48;
    pub const UNDEFINED_CC17_LSB: u8 = 49;
    pub const UNDEFINED_CC18_LSB: u8 = 50;
    pub const UNDEFINED_CC19_LSB: u8 = 51;
    pub const UNDEFINED_CC20_LSB: u8 = 52;
    pub const UNDEFINED_CC21_LSB: u8 = 53;
    pub const UNDEFINED_CC22_LSB: u8 = 54;
    pub const UNDEFINED_CC23_LSB: u8 = 55;
    pub const UNDEFINED_CC24_LSB: u8 = 56;
    pub const UNDEFINED_CC25_LSB: u8 = 57;
    pub const UNDEFINED_CC26_LSB: u8 = 58;
    pub const UNDEFINED_CC27_LSB: u8 = 59;
    pub const UNDEFINED_CC28_LSB: u8 = 60;
    pub const UNDEFINED_CC29_LSB: u8 = 61;
    pub const UNDEFINED_CC30_LSB: u8 = 62;
    pub const UNDEFINED_CC31_LSB: u8 = 63;
    pub const SUSTAIN_PEDAL: u8 = 64;
    pub const PORTAMENTO_ON_OFF: u8 = 65;
    pub const SOSTENUTO_PEDAL: u8 = 66;
    pub const SOFT_PEDAL: u8 = 67;
    pub const LEGATO_FOOTSWITCH: u8 = 68;
    pub const HOLD2_PEDAL: u8 = 69;
    pub const SOUND_VARIATION: u8 = 70;
    pub const FILTER_RESONANCE: u8 = 71;
    pub const RELEASE_TIME: u8 = 72;
    pub const ATTACK_TIME: u8 = 73;
    pub const BRIGHTNESS: u8 = 74;
    pub const DECAY_TIME: u8 = 75;
    pub const VIBRATO_RATE: u8 = 76;
    pub const VIBRATO_DEPTH: u8 = 77;
    pub const VIBRATO_DELAY: u8 = 78;
    pub const SOUND_CONTROLLER10: u8 = 79;
    pub const GENERAL_PURPOSE_CONTROLLER5: u8 = 80;
    pub const GENERAL_PURPOSE_CONTROLLER6: u8 = 81;
    pub const GENERAL_PURPOSE_CONTROLLER7: u8 = 82;
    pub const GENERAL_PURPOSE_CONTROLLER8: u8 = 83;
    pub const PORTAMENTO_CONTROL: u8 = 84;
    pub const UNDEFINED_CC85: u8 = 85;
    pub const UNDEFINED_CC86: u8 = 86;
    pub const UNDEFINED_CC87: u8 = 87;
    pub const UNDEFINED_CC88: u8 = 88;
    pub const UNDEFINED_CC89: u8 = 89;
    pub const UNDEFINED_CC90: u8 = 90;
    pub const REVERB_DEPTH: u8 = 91;
    pub const TREMOLO_DEPTH: u8 = 92;
    pub const CHORUS_DEPTH: u8 = 93;
    pub const VARIATION_DEPTH: u8 = 94;
    pub const PHASER_DEPTH: u8 = 95;
    pub const DATA_INCREMENT: u8 = 96;
    pub const DATA_DECREMENT: u8 = 97;
    pub const NON_REGISTERED_PARAMETER_LSB: u8 = 98;
    pub const NON_REGISTERED_PARAMETER_MSB: u8 = 99;
    pub const REGISTERED_PARAMETER_LSB: u8 = 100;
    pub const REGISTERED_PARAMETER_MSB: u8 = 101;
    pub const UNDEFINED_CC102_LSB: u8 = 102;
    pub const UNDEFINED_CC103_LSB: u8 = 103;
    pub const UNDEFINED_CC104_LSB: u8 = 104;
    pub const UNDEFINED_CC105_LSB: u8 = 105;
    pub const UNDEFINED_CC106_LSB: u8 = 106;
    pub const UNDEFINED_CC107_LSB: u8 = 107;
    pub const UNDEFINED_CC108_LSB: u8 = 108;
    pub const UNDEFINED_CC109_LSB: u8 = 109;
    pub const UNDEFINED_CC110_LSB: u8 = 110;
    pub const UNDEFINED_CC111_LSB: u8 = 111;
    pub const UNDEFINED_CC112_LSB: u8 = 112;
    pub const UNDEFINED_CC113_LSB: u8 = 113;
    pub const UNDEFINED_CC114_LSB: u8 = 114;
    pub const UNDEFINED_CC115_LSB: u8 = 115;
    pub const UNDEFINED_CC116_LSB: u8 = 116;
    pub const UNDEFINED_CC117_LSB: u8 = 117;
    pub const UNDEFINED_CC118_LSB: u8 = 118;
    pub const UNDEFINED_CC119_LSB: u8 = 119;
    pub const ALL_SOUND_OFF: u8 = 120;
    pub const RESET_ALL_CONTROLLERS: u8 = 121;
    pub const LOCAL_CONTROL_ON_OFF: u8 = 122;
    pub const ALL_NOTES_OFF: u8 = 123;
    pub const OMNI_MODE_OFF: u8 = 124;
    pub const OMNI_MODE_ON: u8 = 125;
    pub const MONO_MODE_ON: u8 = 126;
    pub const POLY_MODE_ON: u8 = 127;
}

/// Equivalent to: MIDIController
pub type MidiController = u8;

#[cfg(test)]
mod tests {
    use super::midi_controllers as cc;
    use super::midi_message_types as msg;

    // --- midi_message_types ---

    #[test]
    fn test_note_off() {
        assert_eq!(msg::NOTE_OFF, 0x80);
    }

    #[test]
    fn test_note_on() {
        assert_eq!(msg::NOTE_ON, 0x90);
    }

    #[test]
    fn test_controller_change() {
        assert_eq!(msg::CONTROLLER_CHANGE, 0xB0);
    }

    #[test]
    fn test_system_exclusive() {
        assert_eq!(msg::SYSTEM_EXCLUSIVE, 0xF0);
    }

    #[test]
    fn test_end_of_track() {
        assert_eq!(msg::END_OF_TRACK, 0x2F);
    }

    #[test]
    fn test_set_tempo() {
        assert_eq!(msg::SET_TEMPO, 0x51);
    }

    #[test]
    fn test_time_signature() {
        assert_eq!(msg::TIME_SIGNATURE, 0x58);
    }

    // --- midi_controllers ---

    #[test]
    fn test_bank_select() {
        assert_eq!(cc::BANK_SELECT, 0);
    }

    #[test]
    fn test_modulation_wheel() {
        assert_eq!(cc::MODULATION_WHEEL, 1);
    }

    #[test]
    fn test_sustain_pedal() {
        assert_eq!(cc::SUSTAIN_PEDAL, 64);
    }

    #[test]
    fn test_all_notes_off() {
        assert_eq!(cc::ALL_NOTES_OFF, 123);
    }

    #[test]
    fn test_poly_mode_on() {
        assert_eq!(cc::POLY_MODE_ON, 127);
    }

    #[test]
    fn test_registered_parameter_lsb() {
        assert_eq!(cc::REGISTERED_PARAMETER_LSB, 100);
    }

    #[test]
    fn test_registered_parameter_msb() {
        assert_eq!(cc::REGISTERED_PARAMETER_MSB, 101);
    }
}
