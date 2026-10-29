//! midi_message.rs
//! purpose: MIDI message struct.
//! Ported from: src/midi/midi_message.ts

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

    #[test]
    fn test_new_empty_data() {
        let msg = MidiMessage::new(0, 0x2f, vec![]);
        assert_eq!(msg.ticks, 0);
        assert_eq!(msg.status_byte, 0x2f);
        assert!(msg.data.is_empty());
    }

    #[test]
    fn test_clone() {
        let msg = MidiMessage::new(10, 0xb0, vec![7, 100]);
        let cloned = msg.clone();
        assert_eq!(cloned.ticks, msg.ticks);
        assert_eq!(cloned.status_byte, msg.status_byte);
        assert_eq!(cloned.data, msg.data);
    }
}
