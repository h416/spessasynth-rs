/// midi_message.rs
/// purpose: MIDI message struct.
/// Ported from: src/midi/midi_message.ts (spessasynth_core 4.3.0)
///
/// TS 4.3.0 removed `getChannel`, `getEvent`, and `dataBytesAmount` from this file entirely:
/// `read/midi.ts`'s `parseSMFInternal` inlines an equivalent range-check + a private
/// `DataBytesAmount` const instead of calling `getChannel` (this Rust crate's `read/midi.rs`
/// counterpart already matches — it defines its own private `data_bytes_amount` and never called
/// any of the three), and the sequencer (`process_event.ts`/`set_time_to.ts`) — 4.2.0's only
/// caller of `getEvent` — was rewritten in 4.3.0 to inline the same channel/status split instead
/// of calling `getEvent`.
///
/// Task 19 (sequencer 4.3.0 port) removed this file's `get_event`/`get_channel`/
/// `data_bytes_amount` and `MidiEventInfo` accordingly, since `sequencer/process_event.rs` and
/// `sequencer/set_time_to.rs` no longer call `get_event` (their last remaining caller) and now
/// inline the equivalent split, matching TS.
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
