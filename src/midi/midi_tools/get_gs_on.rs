/// get_gs_on.rs
/// purpose: Utility for generating GS ON SysEx messages.
/// Ported from: src/midi/midi_tools/get_gs_on.ts
use crate::midi::enums::midi_message_types;
use crate::midi::midi_message::MidiMessage;

/// Generates and returns a Roland GS ON SysEx message.
///
/// GS ON is a SysEx message that enables Roland GS mode.
/// Equivalent to: getGsOn(ticks)
pub fn get_gs_on(ticks: u32) -> MidiMessage {
    MidiMessage::new(
        ticks,
        midi_message_types::SYSTEM_EXCLUSIVE,
        vec![
            0x41, // Roland
            0x10, // Device ID (Roland default is 16)
            0x42, // GS
            0x12, // Command ID (DT1)
            0x40, // System parameter - Address
            0x00, // Global parameter - Address
            0x7f, // GS Change - Address
            0x00, // Turn on - Data
            0x41, // Checksum
            0xf7, // End of exclusive
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::enums::midi_message_types;

    #[test]
    fn test_ticks_zero() {
        let msg = get_gs_on(0);
        assert_eq!(msg.ticks, 0);
    }

    #[test]
    fn test_ticks_nonzero() {
        let msg = get_gs_on(480);
        assert_eq!(msg.ticks, 480);
    }

    #[test]
    fn test_status_byte_is_sysex() {
        let msg = get_gs_on(0);
        assert_eq!(msg.status_byte, midi_message_types::SYSTEM_EXCLUSIVE);
    }

    #[test]
    fn test_data_length() {
        let msg = get_gs_on(0);
        assert_eq!(msg.data.len(), 10);
    }

    #[test]
    fn test_data_roland_id() {
        let msg = get_gs_on(0);
        assert_eq!(msg.data[0], 0x41); // Roland
    }

    #[test]
    fn test_data_device_id() {
        let msg = get_gs_on(0);
        assert_eq!(msg.data[1], 0x10); // Device ID 16
    }

    #[test]
    fn test_data_gs_byte() {
        let msg = get_gs_on(0);
        assert_eq!(msg.data[2], 0x42); // GS
    }

    #[test]
    fn test_data_command_id() {
        let msg = get_gs_on(0);
        assert_eq!(msg.data[3], 0x12); // DT1
    }

    #[test]
    fn test_data_address_bytes() {
        let msg = get_gs_on(0);
        assert_eq!(msg.data[4], 0x40);
        assert_eq!(msg.data[5], 0x00);
        assert_eq!(msg.data[6], 0x7f);
    }

    #[test]
    fn test_data_turn_on_byte() {
        let msg = get_gs_on(0);
        assert_eq!(msg.data[7], 0x00); // Turn on
    }

    #[test]
    fn test_data_checksum() {
        let msg = get_gs_on(0);
        assert_eq!(msg.data[8], 0x41); // Checksum
    }

    #[test]
    fn test_data_end_of_exclusive() {
        let msg = get_gs_on(0);
        assert_eq!(msg.data[9], 0xf7); // End of exclusive
    }

    #[test]
    fn test_full_sysex_payload() {
        let msg = get_gs_on(0);
        assert_eq!(
            msg.data,
            vec![0x41, 0x10, 0x42, 0x12, 0x40, 0x00, 0x7f, 0x00, 0x41, 0xf7]
        );
    }
}
