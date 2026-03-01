/// midi_track.rs
/// purpose: A single MIDI track with events and metadata.
/// Ported from: src/midi/midi_track.ts
use std::collections::HashSet;

use crate::midi::midi_message::MidiMessage;

/// A single MIDI track.
/// Equivalent to: class MIDITrack
pub struct MidiTrack {
    /// The name of this track.
    pub name: String,
    /// The MIDI port number used by the track.
    pub port: u32,
    /// Set of MIDI channels used by the track in the sequence.
    pub channels: HashSet<u8>,
    /// All the MIDI messages of this track.
    pub events: Vec<MidiMessage>,
}

impl MidiTrack {
    /// Creates a new empty MidiTrack.
    pub fn new() -> Self {
        Self {
            name: String::new(),
            port: 0,
            channels: HashSet::new(),
            events: Vec::new(),
        }
    }

    /// Creates a deep copy of the given track.
    /// Equivalent to: static copyFrom(track)
    pub fn copy_of(track: &MidiTrack) -> Self {
        let mut t = MidiTrack::new();
        t.copy_from(track);
        t
    }

    /// Copies the contents of another track into this one (deep copy).
    /// Equivalent to: copyFrom(track)
    pub fn copy_from(&mut self, track: &MidiTrack) {
        self.name = track.name.clone();
        self.port = track.port;
        self.channels = track.channels.clone();
        self.events = track.events.clone();
    }

    /// Inserts an event at the given index.
    /// Equivalent to: addEvent(event, index)
    pub fn add_event(&mut self, event: MidiMessage, index: usize) {
        self.events.insert(index, event);
    }

    /// Removes the event at the given index.
    /// Equivalent to: deleteEvent(index)
    pub fn delete_event(&mut self, index: usize) {
        self.events.remove(index);
    }

    /// Appends an event to the end of the track.
    /// Equivalent to: pushEvent(event)
    pub fn push_event(&mut self, event: MidiMessage) {
        self.events.push(event);
    }
}

impl Default for MidiTrack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::midi_message::MidiMessage;

    fn make_msg(ticks: u32, status_byte: u8, data: Vec<u8>) -> MidiMessage {
        MidiMessage::new(ticks, status_byte, data)
    }

    // --- MidiTrack::new / Default ---

    #[test]
    fn test_new_default_fields() {
        let t = MidiTrack::new();
        assert_eq!(t.name, "");
        assert_eq!(t.port, 0);
        assert!(t.channels.is_empty());
        assert!(t.events.is_empty());
    }

    #[test]
    fn test_default_equals_new() {
        let t = MidiTrack::default();
        assert_eq!(t.name, "");
        assert_eq!(t.port, 0);
        assert!(t.channels.is_empty());
        assert!(t.events.is_empty());
    }

    // --- push_event ---

    #[test]
    fn test_push_event_single() {
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        assert_eq!(t.events.len(), 1);
        assert_eq!(t.events[0].ticks, 0);
        assert_eq!(t.events[0].status_byte, 0x90);
        assert_eq!(t.events[0].data, vec![60, 100]);
    }

    #[test]
    fn test_push_event_order_preserved() {
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        t.push_event(make_msg(100, 0x80, vec![60, 0]));
        assert_eq!(t.events.len(), 2);
        assert_eq!(t.events[0].ticks, 0);
        assert_eq!(t.events[1].ticks, 100);
    }

    // --- add_event ---

    #[test]
    fn test_add_event_at_start() {
        let mut t = MidiTrack::new();
        t.push_event(make_msg(100, 0x90, vec![62, 80]));
        t.add_event(make_msg(0, 0x90, vec![60, 100]), 0);
        assert_eq!(t.events.len(), 2);
        assert_eq!(t.events[0].ticks, 0);
        assert_eq!(t.events[1].ticks, 100);
    }

    #[test]
    fn test_add_event_in_middle() {
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        t.push_event(make_msg(200, 0x90, vec![64, 100]));
        t.add_event(make_msg(100, 0x90, vec![62, 80]), 1);
        assert_eq!(t.events.len(), 3);
        assert_eq!(t.events[0].ticks, 0);
        assert_eq!(t.events[1].ticks, 100);
        assert_eq!(t.events[2].ticks, 200);
    }

    #[test]
    fn test_add_event_at_end() {
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        t.add_event(make_msg(200, 0xFF, vec![0x2F, 0]), 1);
        assert_eq!(t.events[1].status_byte, 0xFF);
    }

    // --- delete_event ---

    #[test]
    fn test_delete_event_first() {
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        t.push_event(make_msg(100, 0x80, vec![60, 0]));
        t.delete_event(0);
        assert_eq!(t.events.len(), 1);
        assert_eq!(t.events[0].ticks, 100);
    }

    #[test]
    fn test_delete_event_last() {
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        t.push_event(make_msg(100, 0x80, vec![60, 0]));
        t.delete_event(1);
        assert_eq!(t.events.len(), 1);
        assert_eq!(t.events[0].ticks, 0);
    }

    #[test]
    fn test_delete_only_event() {
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        t.delete_event(0);
        assert!(t.events.is_empty());
    }

    // --- copy_from ---

    #[test]
    fn test_copy_from_all_fields() {
        let mut original = MidiTrack::new();
        original.name = "Track 1".to_string();
        original.port = 2;
        original.channels.insert(0);
        original.channels.insert(3);
        original.push_event(make_msg(0, 0x90, vec![60, 100]));

        let mut copy = MidiTrack::new();
        copy.copy_from(&original);

        assert_eq!(copy.name, "Track 1");
        assert_eq!(copy.port, 2);
        assert!(copy.channels.contains(&0));
        assert!(copy.channels.contains(&3));
        assert_eq!(copy.events.len(), 1);
        assert_eq!(copy.events[0].ticks, 0);
        assert_eq!(copy.events[0].status_byte, 0x90);
    }

    #[test]
    fn test_copy_from_events_are_deep_copy() {
        let mut original = MidiTrack::new();
        original.push_event(make_msg(0, 0x90, vec![60, 100]));

        let mut copy = MidiTrack::new();
        copy.copy_from(&original);

        // Modifying the copied events does not affect the original
        copy.events[0].ticks = 999;
        assert_eq!(original.events[0].ticks, 0);
    }

    #[test]
    fn test_copy_from_channels_are_independent() {
        let mut original = MidiTrack::new();
        original.channels.insert(5);

        let mut copy = MidiTrack::new();
        copy.copy_from(&original);

        copy.channels.insert(6);
        assert!(!original.channels.contains(&6));
    }

    #[test]
    fn test_copy_from_multiple_events() {
        let mut original = MidiTrack::new();
        original.push_event(make_msg(0, 0x90, vec![60, 100]));
        original.push_event(make_msg(480, 0x80, vec![60, 0]));

        let mut copy = MidiTrack::new();
        copy.copy_from(&original);

        assert_eq!(copy.events.len(), 2);
        assert_eq!(copy.events[1].ticks, 480);
    }

    // --- copy_of (static constructor) ---

    #[test]
    fn test_copy_of_returns_independent_copy() {
        let mut original = MidiTrack::new();
        original.name = "Static Copy".to_string();
        original.push_event(make_msg(480, 0xC0, vec![25]));

        let copy = MidiTrack::copy_of(&original);

        assert_eq!(copy.name, "Static Copy");
        assert_eq!(copy.events.len(), 1);
        assert_eq!(copy.events[0].ticks, 480);
        assert_eq!(copy.events[0].status_byte, 0xC0);
    }

    #[test]
    fn test_copy_of_is_deep_copy() {
        let mut original = MidiTrack::new();
        original.push_event(make_msg(0, 0x90, vec![60, 100]));

        let mut copy = MidiTrack::copy_of(&original);
        copy.events[0].ticks = 999;
        assert_eq!(original.events[0].ticks, 0);
    }
}
