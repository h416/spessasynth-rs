/// midi_message.rs
/// purpose: MIDI message struct and status byte parsing utilities.
/// Ported from: src/midi/midi_message.ts
/// A single MIDI message.
/// Equivalent to: class MIDIMessage
#[derive(Clone)]
pub struct MidiMessage {
    /// Absolute number of MIDI ticks from the start of the track.
    pub ticks: u32,
    /// The MIDI message status byte. For meta events, this is the second byte (not 0xFF).
    pub status_byte: u8,
    /// Message's binary data.
    pub data: Vec<u8>,
}

impl MidiMessage {
    /// Equivalent to: new MIDIMessage(ticks, byte, data)
    pub fn new(ticks: u32, status_byte: u8, data: Vec<u8>) -> Self {
        Self {
            ticks,
            status_byte,
            data,
        }
    }
}

/// Return value of get_event().
/// Equivalent to: { channel: number, status: number }
pub struct MidiEventInfo {
    /// MIDI channel (0–15), or -1 for system/meta/sysex messages.
    pub channel: i8,
    /// Status byte (masked to high nibble for channel messages, original byte otherwise).
    pub status: u8,
}

/// Returns the MIDI channel encoded in a status byte.
/// - 0–15 : channel message channel
/// -    -1 : system message
/// -    -2 : meta event / reset
/// -    -3 : SysEx
///
/// Equivalent to: getChannel(statusByte)
pub fn get_channel(status_byte: u8) -> i8 {
    let event_type = status_byte & 0xF0;
    let channel = (status_byte & 0x0F) as i8;

    match event_type {
        0x80 | 0x90 | 0xA0 | 0xB0 | 0xC0 | 0xD0 | 0xE0 => channel,
        0xF0 => match channel {
            0x0 => -3, // SysEx
            0xF => -2, // Meta / Reset
            _ => -1,   // Other system messages
        },
        _ => -1,
    }
}

/// Splits a status byte into event status and channel.
/// For channel messages (0x80–0xEF): status = high nibble, channel = low nibble.
/// For system messages: status = original byte, channel = -1.
/// Equivalent to: getEvent(statusByte)
pub fn get_event(status_byte: u8) -> MidiEventInfo {
    let status = status_byte & 0xF0;
    let channel = (status_byte & 0x0F) as i8;

    if (0x80..=0xE0).contains(&status) {
        MidiEventInfo { channel, status }
    } else {
        MidiEventInfo {
            channel: -1,
            status: status_byte,
        }
    }
}

/// Returns the number of data bytes for a given MIDI event high nibble (0x8–0xE).
/// Equivalent to: dataBytesAmount[highNibble]
pub fn data_bytes_amount(high_nibble: u8) -> u8 {
    match high_nibble {
        0x8 | 0x9 | 0xA | 0xB | 0xE => 2,
        0xC | 0xD => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- MidiMessage::new ---

    #[test]
    fn test_new() {
        let msg = MidiMessage::new(100, 0x90, vec![60, 100]);
        assert_eq!(msg.ticks, 100);
        assert_eq!(msg.status_byte, 0x90);
        assert_eq!(msg.data, vec![60, 100]);
    }

    // --- get_channel ---

    #[test]
    fn test_channel_note_on_ch0() {
        assert_eq!(get_channel(0x90), 0);
    }

    #[test]
    fn test_channel_note_on_ch15() {
        assert_eq!(get_channel(0x9F), 15);
    }

    #[test]
    fn test_channel_cc_ch3() {
        assert_eq!(get_channel(0xB3), 3);
    }

    #[test]
    fn test_channel_sysex() {
        assert_eq!(get_channel(0xF0), -3);
    }

    #[test]
    fn test_channel_meta_reset() {
        assert_eq!(get_channel(0xFF), -2);
    }

    #[test]
    fn test_channel_timecode() {
        assert_eq!(get_channel(0xF1), -1);
    }

    #[test]
    fn test_channel_active_sensing() {
        assert_eq!(get_channel(0xFE), -1);
    }

    #[test]
    fn test_channel_unknown() {
        assert_eq!(get_channel(0x00), -1);
    }

    // --- get_event ---

    #[test]
    fn test_event_note_on_ch2() {
        let e = get_event(0x92);
        assert_eq!(e.status, 0x90);
        assert_eq!(e.channel, 2);
    }

    #[test]
    fn test_event_cc_ch15() {
        let e = get_event(0xBF);
        assert_eq!(e.status, 0xB0);
        assert_eq!(e.channel, 15);
    }

    #[test]
    fn test_event_sysex() {
        let e = get_event(0xF0);
        assert_eq!(e.status, 0xF0);
        assert_eq!(e.channel, -1);
    }

    #[test]
    fn test_event_meta() {
        let e = get_event(0xFF);
        assert_eq!(e.status, 0xFF);
        assert_eq!(e.channel, -1);
    }

    // --- data_bytes_amount ---

    #[test]
    fn test_data_bytes_note_off() {
        assert_eq!(data_bytes_amount(0x8), 2);
    }

    #[test]
    fn test_data_bytes_note_on() {
        assert_eq!(data_bytes_amount(0x9), 2);
    }

    #[test]
    fn test_data_bytes_program_change() {
        assert_eq!(data_bytes_amount(0xC), 1);
    }

    #[test]
    fn test_data_bytes_channel_pressure() {
        assert_eq!(data_bytes_amount(0xD), 1);
    }

    #[test]
    fn test_data_bytes_pitch_wheel() {
        assert_eq!(data_bytes_amount(0xE), 2);
    }

    #[test]
    fn test_data_bytes_unknown() {
        assert_eq!(data_bytes_amount(0x0), 0);
    }
}
