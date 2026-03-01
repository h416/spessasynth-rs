/// types.rs
/// purpose: Sequencer event data types and discriminated union.
/// Ported from: src/sequencer/types.ts
use crate::midi::basic_midi::BasicMidi;
use crate::midi::midi_message::MidiMessage;

// ─────────────────────────────────────────────────────────────────────────────
// Event data structs
// ─────────────────────────────────────────────────────────────────────────────

/// Data for a midiMessage event.
/// Called when a MIDI message is sent while `externalMIDIPlayback` is true.
/// Equivalent to: SequencerEventData["midiMessage"]
#[derive(Clone, Debug)]
pub struct MidiMessageEventData {
    /// Binary MIDI message.
    /// Equivalent to: message: Iterable<number>
    pub message: Vec<u8>,
    /// The synthesizer's current time (in seconds) when this event was sent.
    /// Used for scheduling to external MIDI devices.
    pub time: f64,
}

/// Data for a timeChange event.
/// Called when the time changes (including on song change).
/// Equivalent to: SequencerEventData["timeChange"]
#[derive(Clone, Copy, Debug)]
pub struct TimeChangeEventData {
    /// The new time (in seconds).
    pub new_time: f64,
}

/// Data for a pause event (deprecated — use SongEnded instead).
/// Called when playback stops.
/// Equivalent to: SequencerEventData["pause"]
#[derive(Clone, Copy, Debug)]
pub struct PauseEventData {
    /// `true` if the song finished and stopped, `false` if manually stopped.
    pub is_finished: bool,
}

/// Data for a songChange event.
/// Called when the song changes.
/// Equivalent to: SequencerEventData["songChange"]
#[derive(Clone, Copy, Debug)]
pub struct SongChangeEventData {
    /// The index of the new song in the song list.
    pub song_index: usize,
}

/// Data for a songListChange event.
/// Called when the song list changes.
/// Equivalent to: SequencerEventData["songListChange"]
pub struct SongListChangeEventData {
    /// The new song list.
    pub new_song_list: Vec<BasicMidi>,
}

/// Data for a metaEvent event.
/// Called when a MIDI meta event is encountered.
/// Equivalent to: SequencerEventData["metaEvent"]
pub struct MetaEventEventData {
    /// The MIDI message of the meta event.
    pub event: MidiMessage,
    /// The track index where the meta event was found.
    pub track_index: usize,
}

/// Data for a loopCountChange event.
/// Called when the loop count changes (decreases).
/// Equivalent to: SequencerEventData["loopCountChange"]
#[derive(Clone, Copy, Debug)]
pub struct LoopCountChangeEventData {
    /// The new loop count.
    pub new_count: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// SequencerEvent — discriminated union
// ─────────────────────────────────────────────────────────────────────────────

/// Discriminated union of all events emitted by the sequencer.
/// Equivalent to: SequencerEvent
pub enum SequencerEvent {
    /// Fired when a MIDI message is sent while `externalMIDIPlayback` is true.
    MidiMessage(MidiMessageEventData),
    /// Fired when the time changes (including on song change).
    TimeChange(TimeChangeEventData),
    /// Fired when playback stops (deprecated — use SongEnded instead).
    Pause(PauseEventData),
    /// Fired when the song ends.
    /// Equivalent to: songEnded: object
    SongEnded,
    /// Fired when the song changes.
    SongChange(SongChangeEventData),
    /// Fired when the song list changes.
    SongListChange(SongListChangeEventData),
    /// Fired when a MIDI meta event is encountered.
    MetaEvent(MetaEventEventData),
    /// Fired when the loop count changes (decreases).
    LoopCountChange(LoopCountChangeEventData),
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::midi_message::MidiMessage;

    // ── MidiMessageEventData ──────────────────────────────────────────────────

    #[test]
    fn test_midi_message_event_data_fields() {
        let data = MidiMessageEventData {
            message: vec![0x90, 60, 100],
            time: 1.5,
        };
        assert_eq!(data.message, vec![0x90, 60, 100]);
        assert_eq!(data.time, 1.5);
    }

    #[test]
    fn test_midi_message_event_data_clone() {
        let a = MidiMessageEventData {
            message: vec![0x80, 60, 0],
            time: 0.0,
        };
        let b = a.clone();
        assert_eq!(a.message, b.message);
        assert_eq!(a.time, b.time);
    }

    #[test]
    fn test_midi_message_event_data_empty_message() {
        let data = MidiMessageEventData {
            message: vec![],
            time: 0.0,
        };
        assert!(data.message.is_empty());
    }

    // ── TimeChangeEventData ───────────────────────────────────────────────────

    #[test]
    fn test_time_change_event_data_fields() {
        let data = TimeChangeEventData { new_time: 3.0 };
        assert_eq!(data.new_time, 3.0);
    }

    #[test]
    fn test_time_change_event_data_copy() {
        let a = TimeChangeEventData { new_time: 2.5 };
        let b = a; // Copy
        assert_eq!(a.new_time, b.new_time);
    }

    #[test]
    fn test_time_change_event_data_zero() {
        let data = TimeChangeEventData { new_time: 0.0 };
        assert_eq!(data.new_time, 0.0);
    }

    // ── PauseEventData ────────────────────────────────────────────────────────

    #[test]
    fn test_pause_event_data_finished() {
        let data = PauseEventData { is_finished: true };
        assert!(data.is_finished);
    }

    #[test]
    fn test_pause_event_data_not_finished() {
        let data = PauseEventData { is_finished: false };
        assert!(!data.is_finished);
    }

    #[test]
    fn test_pause_event_data_copy() {
        let a = PauseEventData { is_finished: true };
        let b = a; // Copy
        assert_eq!(a.is_finished, b.is_finished);
    }

    // ── SongChangeEventData ───────────────────────────────────────────────────

    #[test]
    fn test_song_change_event_data_index() {
        let data = SongChangeEventData { song_index: 3 };
        assert_eq!(data.song_index, 3);
    }

    #[test]
    fn test_song_change_event_data_first_song() {
        let data = SongChangeEventData { song_index: 0 };
        assert_eq!(data.song_index, 0);
    }

    #[test]
    fn test_song_change_event_data_copy() {
        let a = SongChangeEventData { song_index: 5 };
        let b = a; // Copy
        assert_eq!(a.song_index, b.song_index);
    }

    // ── SongListChangeEventData ───────────────────────────────────────────────

    #[test]
    fn test_song_list_change_event_data_empty() {
        let data = SongListChangeEventData {
            new_song_list: vec![],
        };
        assert!(data.new_song_list.is_empty());
    }

    #[test]
    fn test_song_list_change_event_data_len() {
        let data = SongListChangeEventData {
            new_song_list: vec![BasicMidi::new(), BasicMidi::new(), BasicMidi::new()],
        };
        assert_eq!(data.new_song_list.len(), 3);
    }

    // ── MetaEventEventData ────────────────────────────────────────────────────

    #[test]
    fn test_meta_event_event_data_fields() {
        let msg = MidiMessage::new(100, 0x51, vec![0x07, 0xA1, 0x20]);
        let data = MetaEventEventData {
            event: msg,
            track_index: 2,
        };
        assert_eq!(data.event.ticks, 100);
        assert_eq!(data.event.status_byte, 0x51);
        assert_eq!(data.track_index, 2);
    }

    #[test]
    fn test_meta_event_event_data_track0() {
        let msg = MidiMessage::new(0, 0x58, vec![4, 2, 24, 8]);
        let data = MetaEventEventData {
            event: msg,
            track_index: 0,
        };
        assert_eq!(data.track_index, 0);
        assert_eq!(data.event.data, vec![4, 2, 24, 8]);
    }

    // ── LoopCountChangeEventData ──────────────────────────────────────────────

    #[test]
    fn test_loop_count_change_event_data_fields() {
        let data = LoopCountChangeEventData { new_count: 5 };
        assert_eq!(data.new_count, 5);
    }

    #[test]
    fn test_loop_count_change_event_data_zero() {
        let data = LoopCountChangeEventData { new_count: 0 };
        assert_eq!(data.new_count, 0);
    }

    #[test]
    fn test_loop_count_change_event_data_copy() {
        let a = LoopCountChangeEventData { new_count: 2 };
        let b = a; // Copy
        assert_eq!(a.new_count, b.new_count);
    }

    // ── SequencerEvent variants ────────────────────────────────────────────

    #[test]
    fn test_sequencer_event_midi_message_variant() {
        let ev = SequencerEvent::MidiMessage(MidiMessageEventData {
            message: vec![0x90, 60, 80],
            time: 0.5,
        });
        if let SequencerEvent::MidiMessage(d) = ev {
            assert_eq!(d.message[0], 0x90);
            assert_eq!(d.time, 0.5);
        } else {
            panic!("expected MidiMessage variant");
        }
    }

    #[test]
    fn test_sequencer_event_time_change_variant() {
        let ev = SequencerEvent::TimeChange(TimeChangeEventData { new_time: 10.0 });
        if let SequencerEvent::TimeChange(d) = ev {
            assert_eq!(d.new_time, 10.0);
        } else {
            panic!("expected TimeChange variant");
        }
    }

    #[test]
    fn test_sequencer_event_pause_variant() {
        let ev = SequencerEvent::Pause(PauseEventData { is_finished: true });
        if let SequencerEvent::Pause(d) = ev {
            assert!(d.is_finished);
        } else {
            panic!("expected Pause variant");
        }
    }

    #[test]
    fn test_sequencer_event_song_ended_unit_variant() {
        let ev = SequencerEvent::SongEnded;
        assert!(matches!(ev, SequencerEvent::SongEnded));
    }

    #[test]
    fn test_sequencer_event_song_change_variant() {
        let ev = SequencerEvent::SongChange(SongChangeEventData { song_index: 1 });
        if let SequencerEvent::SongChange(d) = ev {
            assert_eq!(d.song_index, 1);
        } else {
            panic!("expected SongChange variant");
        }
    }

    #[test]
    fn test_sequencer_event_song_list_change_variant_empty() {
        let ev = SequencerEvent::SongListChange(SongListChangeEventData {
            new_song_list: vec![],
        });
        if let SequencerEvent::SongListChange(d) = ev {
            assert!(d.new_song_list.is_empty());
        } else {
            panic!("expected SongListChange variant");
        }
    }

    #[test]
    fn test_sequencer_event_meta_event_variant() {
        let msg = MidiMessage::new(0, 0x58, vec![4, 2, 24, 8]);
        let ev = SequencerEvent::MetaEvent(MetaEventEventData {
            event: msg,
            track_index: 0,
        });
        if let SequencerEvent::MetaEvent(d) = ev {
            assert_eq!(d.event.status_byte, 0x58);
            assert_eq!(d.track_index, 0);
        } else {
            panic!("expected MetaEvent variant");
        }
    }

    #[test]
    fn test_sequencer_event_loop_count_change_variant() {
        let ev = SequencerEvent::LoopCountChange(LoopCountChangeEventData { new_count: 3 });
        if let SequencerEvent::LoopCountChange(d) = ev {
            assert_eq!(d.new_count, 3);
        } else {
            panic!("expected LoopCountChange variant");
        }
    }

    #[test]
    fn test_sequencer_event_all_variants_matchable() {
        // Verify that all variants can be listed in match arms
        let events: Vec<SequencerEvent> = vec![
            SequencerEvent::MidiMessage(MidiMessageEventData {
                message: vec![],
                time: 0.0,
            }),
            SequencerEvent::TimeChange(TimeChangeEventData { new_time: 0.0 }),
            SequencerEvent::Pause(PauseEventData { is_finished: false }),
            SequencerEvent::SongEnded,
            SequencerEvent::SongChange(SongChangeEventData { song_index: 0 }),
            SequencerEvent::SongListChange(SongListChangeEventData {
                new_song_list: vec![],
            }),
            SequencerEvent::MetaEvent(MetaEventEventData {
                event: MidiMessage::new(0, 0x00, vec![]),
                track_index: 0,
            }),
            SequencerEvent::LoopCountChange(LoopCountChangeEventData { new_count: 0 }),
        ];
        assert_eq!(events.len(), 8);
    }
}
