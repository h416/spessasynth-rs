/// midi_builder.rs
/// purpose: Convenience builder for constructing a standard MIDI file from scratch.
/// Ported from: src/midi/midi_tools/midi_builder.ts (spessasynth_core 4.3.0)
///
/// TS 4.3.0 renamed most methods to drop the redundant "add" prefix (`addNewTrack` ->
/// `addTrack`, `addSetTempo` -> `setTempo`, `addNoteOn` -> `noteOn`, `addNoteOff` -> `noteOff`,
/// `addProgramChange` -> `programChange`, `addControllerChange` -> `controllerChange`), merged
/// `addPitchWheel(ticks, track, channel, MSB, LSB)` into a single 14-bit
/// `pitchWheel(ticks, track, channel, pitch)`, and added three new methods: `systemExclusive`,
/// `registeredParameter` (RPN MSB/LSB + Data Entry MSB/LSB in one call), and
/// `nonRegisteredParameter` (same, for NRPN). `addEvent`'s `eventData` parameter widened from
/// `Uint8Array | Iterable<number>` to `ArrayLike<number>` (both map to Rust's `Vec<u8>`/`&[u8]`,
/// no functional change).
use crate::midi::basic_midi::BasicMidi;
use crate::midi::enums::{midi_controllers, midi_message_types, MidiMessageType};
use crate::midi::midi_message::MidiMessage;
use crate::midi::midi_track::MidiTrack;
use crate::midi::types::MidiFormat;

// ─────────────────────────────────────────────────────────────────────────────
// Options
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for a new [`MidiBuilder`].
/// Equivalent to: MIDIBuilderOptions
pub struct MidiBuilderOptions {
    /// Ticks per quarter note (PPQN).
    pub time_division: u32,
    /// Initial tempo in beats per minute (BPM).
    pub initial_tempo: f64,
    /// MIDI file format (0 = single-track, 1 = multi-track).  Format 2 is rejected.
    pub format: MidiFormat,
    /// Name written into the conductor track's TRACK_NAME meta event.
    pub name: String,
}

impl Default for MidiBuilderOptions {
    fn default() -> Self {
        Self {
            time_division: 480,
            initial_tempo: 120.0,
            format: MidiFormat::SingleTrack,
            name: "Untitled song".to_string(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MidiBuilder
// ─────────────────────────────────────────────────────────────────────────────

/// Builds a MIDI file from scratch by appending events and tracks.
///
/// The resulting data lives in `self.midi` and can be serialised with
/// [`crate::midi::midi_tools::midi_writer::write_midi_internal`].
///
/// Equivalent to: class MIDIBuilder extends BasicMIDI
pub struct MidiBuilder {
    /// The underlying MIDI data being built.
    pub midi: BasicMidi,
}

impl MidiBuilder {
    /// Creates a new MIDI builder with the given options.
    ///
    /// On success the builder already contains a conductor track (track 0) with
    /// a TRACK_NAME event and a SET_TEMPO event at tick 0.
    ///
    /// Returns `Err` when `options.format` is [`MidiFormat::MultiPattern`] (2).
    ///
    /// Equivalent to: constructor(options)
    pub fn new(options: MidiBuilderOptions) -> Result<Self, String> {
        if options.format == MidiFormat::MultiPattern {
            return Err(
                "MIDI format 2 is not supported in the MIDI builder. \
                 Consider using format 1."
                    .to_string(),
            );
        }

        let mut midi = BasicMidi::new();
        midi.rmidi_info
            .insert("midiEncoding".to_string(), b"utf-8".to_vec());
        midi.format = options.format;
        midi.time_division = options.time_division;
        midi.binary_name = Some(options.name.as_bytes().to_vec());

        let mut builder = Self { midi };

        // Create the conductor track, then set the initial tempo.
        builder.add_track(&options.name, 0)?;
        builder.set_tempo(0, options.initial_tempo)?;

        Ok(builder)
    }

    // ── Convenience constructor ───────────────────────────────────────────────

    /// Creates a new builder with default options.
    pub fn default() -> Result<Self, String> {
        Self::new(MidiBuilderOptions::default())
    }

    // ── Track management ──────────────────────────────────────────────────────

    /// Adds a new MIDI track with a name and port.
    ///
    /// Returns `Err` if the current format is 0 (single-track) and a track
    /// already exists.
    ///
    /// Equivalent to: addTrack(name, port)
    pub fn add_track(&mut self, name: &str, port: u32) -> Result<(), String> {
        if self.midi.format == MidiFormat::SingleTrack && !self.midi.tracks.is_empty() {
            return Err(
                "Can't add more tracks to MIDI format 0. Consider using format 1.".to_string(),
            );
        }

        let mut track = MidiTrack::new();
        track.name = name.to_string();
        track.port = port;
        self.midi.tracks.push(track);

        let track_idx = self.midi.tracks.len() - 1;
        self.add_event(0, track_idx, midi_message_types::TRACK_NAME, name.as_bytes().to_vec())?;
        self.add_event(0, track_idx, midi_message_types::MIDI_PORT, vec![port as u8])?;

        Ok(())
    }

    // ── Low-level event ───────────────────────────────────────────────────────

    /// Appends a raw MIDI event to the specified track.
    ///
    /// Returns `Err` if:
    /// - `track` does not exist, or
    /// - a voice message (status ≥ 0x80) is added to track 0 in format 1
    ///   (the conductor track must contain only meta events in format 1).
    ///
    /// Equivalent to: addEvent(ticks, track, event, eventData)
    pub fn add_event(
        &mut self,
        ticks: u32,
        track: usize,
        event: MidiMessageType,
        event_data: Vec<u8>,
    ) -> Result<(), String> {
        if self.midi.tracks.get(track).is_none() {
            return Err(format!(
                "Track {} does not exist. Add it via add_track.",
                track
            ));
        }
        if event >= midi_message_types::NOTE_OFF
            && self.midi.format == MidiFormat::MultiTrack
            && track == 0
        {
            return Err(
                "Can't add voice messages to the conductor track (0) in format 1. \
                 Consider using a different track."
                    .to_string(),
            );
        }
        self.midi.tracks[track].push_event(MidiMessage::new(ticks, event, event_data));
        Ok(())
    }

    // ── High-level event helpers ──────────────────────────────────────────────

    /// Adds a Set Tempo meta event.
    ///
    /// `tempo` is in beats per minute (BPM).
    /// Equivalent to: setTempo(ticks, tempo)
    pub fn set_tempo(&mut self, ticks: u32, tempo: f64) -> Result<(), String> {
        let tempo_us = (60_000_000.0 / tempo) as u32;
        let data = vec![
            ((tempo_us >> 16) & 0xFF) as u8,
            ((tempo_us >> 8) & 0xFF) as u8,
            (tempo_us & 0xFF) as u8,
        ];
        self.add_event(ticks, 0, midi_message_types::SET_TEMPO, data)
    }

    /// Adds a Note On event.
    ///
    /// `channel`, `midi_note`, and `velocity` are masked to their valid ranges
    /// (% 16 / % 128) before encoding.
    ///
    /// Equivalent to: noteOn(ticks, track, channel, midiNote, velocity)
    pub fn note_on(
        &mut self,
        ticks: u32,
        track: usize,
        channel: u8,
        midi_note: u8,
        velocity: u8,
    ) -> Result<(), String> {
        self.add_event(
            ticks,
            track,
            midi_message_types::NOTE_ON | (channel % 16),
            vec![midi_note % 128, velocity % 128],
        )
    }

    /// Adds a Note Off event.
    ///
    /// `velocity` is typically 64 (use 64 if unsure).
    /// Equivalent to: noteOff(ticks, track, channel, midiNote, velocity)
    pub fn note_off(
        &mut self,
        ticks: u32,
        track: usize,
        channel: u8,
        midi_note: u8,
        velocity: u8,
    ) -> Result<(), String> {
        self.add_event(
            ticks,
            track,
            midi_message_types::NOTE_OFF | (channel % 16),
            vec![midi_note % 128, velocity],
        )
    }

    /// Adds a Program Change event.
    /// Equivalent to: programChange(ticks, track, channel, programNumber)
    pub fn program_change(
        &mut self,
        ticks: u32,
        track: usize,
        channel: u8,
        program_number: u8,
    ) -> Result<(), String> {
        self.add_event(
            ticks,
            track,
            midi_message_types::PROGRAM_CHANGE | (channel % 16),
            vec![program_number % 128],
        )
    }

    /// Adds a Controller Change event.
    ///
    /// `controller`: the MIDI CC to use. `value`: the new CC value.
    /// Equivalent to: controllerChange(ticks, track, channel, controller, value)
    pub fn controller_change(
        &mut self,
        ticks: u32,
        track: usize,
        channel: u8,
        controller: u8,
        value: u8,
    ) -> Result<(), String> {
        self.add_event(
            ticks,
            track,
            midi_message_types::CONTROLLER_CHANGE | (channel % 16),
            vec![controller % 128, value % 128],
        )
    }

    /// Adds a Pitch Wheel event.
    ///
    /// `pitch` is the combined 14-bit pitch wheel value (0-16383).
    /// The data is written as `[pitch & 0x7f, (pitch >> 7) & 0x7f]` per the MIDI standard.
    ///
    /// Equivalent to: pitchWheel(ticks, track, channel, pitch)
    pub fn pitch_wheel(
        &mut self,
        ticks: u32,
        track: usize,
        channel: u8,
        pitch: u16,
    ) -> Result<(), String> {
        let pitch = pitch % 16_384;
        self.add_event(
            ticks,
            track,
            midi_message_types::PITCH_WHEEL | (channel % 16),
            vec![(pitch & 0x7f) as u8, ((pitch >> 7) & 0x7f) as u8],
        )
    }

    /// Adds a new System Exclusive event.
    ///
    /// `data` is the System Exclusive data, without the 0xF0 status byte.
    /// Equivalent to: systemExclusive(ticks, track, data)
    pub fn system_exclusive(
        &mut self,
        ticks: u32,
        track: usize,
        data: &[u8],
    ) -> Result<(), String> {
        self.add_event(
            ticks,
            track,
            midi_message_types::SYSTEM_EXCLUSIVE,
            data.to_vec(),
        )
    }

    /// Selects a new Registered Parameter Number and immediately writes its value.
    ///
    /// `parameter`: the 14-bit registered parameter number (e.g. 0 = pitch wheel range).
    /// `value`: the 14-bit value for this parameter.
    ///
    /// Equivalent to: registeredParameter(ticks, track, channel, parameter, value)
    pub fn registered_parameter(
        &mut self,
        ticks: u32,
        track: usize,
        channel: u8,
        parameter: u16,
        value: u16,
    ) -> Result<(), String> {
        self.controller_change(
            ticks,
            track,
            channel,
            midi_controllers::REGISTERED_PARAMETER_MSB,
            (parameter >> 7) as u8,
        )?;
        self.controller_change(
            ticks,
            track,
            channel,
            midi_controllers::REGISTERED_PARAMETER_LSB,
            (parameter & 0x7f) as u8,
        )?;
        self.controller_change(
            ticks,
            track,
            channel,
            midi_controllers::DATA_ENTRY_MSB,
            (value >> 7) as u8,
        )?;
        self.controller_change(
            ticks,
            track,
            channel,
            midi_controllers::DATA_ENTRY_LSB,
            (value & 0x7f) as u8,
        )
    }

    /// Selects a new Non-Registered Parameter Number and immediately writes its value.
    ///
    /// `parameter`: the 14-bit non-registered parameter number.
    /// `value`: the 14-bit value for this parameter.
    ///
    /// Equivalent to: nonRegisteredParameter(ticks, track, channel, parameter, value)
    pub fn non_registered_parameter(
        &mut self,
        ticks: u32,
        track: usize,
        channel: u8,
        parameter: u16,
        value: u16,
    ) -> Result<(), String> {
        self.controller_change(
            ticks,
            track,
            channel,
            midi_controllers::NON_REGISTERED_PARAMETER_MSB,
            (parameter >> 7) as u8,
        )?;
        self.controller_change(
            ticks,
            track,
            channel,
            midi_controllers::NON_REGISTERED_PARAMETER_LSB,
            (parameter & 0x7f) as u8,
        )?;
        self.controller_change(
            ticks,
            track,
            channel,
            midi_controllers::DATA_ENTRY_MSB,
            (value >> 7) as u8,
        )?;
        self.controller_change(
            ticks,
            track,
            channel,
            midi_controllers::DATA_ENTRY_LSB,
            (value & 0x7f) as u8,
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::enums::midi_message_types;
    use crate::midi::types::MidiFormat;

    // ── Constructor / defaults ────────────────────────────────────────────────

    #[test]
    fn test_default_creates_format0() {
        let b = MidiBuilder::default().unwrap();
        assert_eq!(b.midi.format, MidiFormat::SingleTrack);
    }

    #[test]
    fn test_default_time_division_480() {
        let b = MidiBuilder::default().unwrap();
        assert_eq!(b.midi.time_division, 480);
    }

    #[test]
    fn test_default_conductor_track_created() {
        let b = MidiBuilder::default().unwrap();
        assert_eq!(b.midi.tracks.len(), 1);
    }

    #[test]
    fn test_default_binary_name_set() {
        let b = MidiBuilder::default().unwrap();
        assert_eq!(
            b.midi.binary_name.as_deref().unwrap(),
            b"Untitled song"
        );
    }

    #[test]
    fn test_default_midi_encoding_set() {
        let b = MidiBuilder::default().unwrap();
        assert_eq!(
            b.midi.rmidi_info.get("midiEncoding").map(|v| v.as_slice()),
            Some(b"utf-8".as_slice())
        );
    }

    #[test]
    fn test_custom_name_in_binary_name() {
        let b = MidiBuilder::new(MidiBuilderOptions {
            name: "My Song".to_string(),
            ..MidiBuilderOptions::default()
        })
        .unwrap();
        assert_eq!(b.midi.binary_name.as_deref().unwrap(), b"My Song");
    }

    #[test]
    fn test_custom_time_division() {
        let b = MidiBuilder::new(MidiBuilderOptions {
            time_division: 960,
            ..MidiBuilderOptions::default()
        })
        .unwrap();
        assert_eq!(b.midi.time_division, 960);
    }

    #[test]
    fn test_format2_rejected() {
        let result = MidiBuilder::new(MidiBuilderOptions {
            format: MidiFormat::MultiPattern,
            ..MidiBuilderOptions::default()
        });
        assert!(result.is_err());
    }

    // ── Conductor track events ────────────────────────────────────────────────

    #[test]
    fn test_conductor_track_has_track_name_event() {
        let b = MidiBuilder::default().unwrap();
        let track_name_events: Vec<_> = b.midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte == midi_message_types::TRACK_NAME)
            .collect();
        assert_eq!(track_name_events.len(), 1);
        assert_eq!(track_name_events[0].data, b"Untitled song");
    }

    #[test]
    fn test_conductor_track_has_midi_port_event() {
        let b = MidiBuilder::default().unwrap();
        let port_events: Vec<_> = b.midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte == midi_message_types::MIDI_PORT)
            .collect();
        assert!(!port_events.is_empty());
        assert_eq!(port_events[0].data[0], 0); // port 0
    }

    #[test]
    fn test_conductor_track_has_set_tempo_event() {
        let b = MidiBuilder::default().unwrap();
        let tempo_events: Vec<_> = b.midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte == midi_message_types::SET_TEMPO)
            .collect();
        assert_eq!(tempo_events.len(), 1);
        // 120 BPM → 500000 µs = 0x07 0xA1 0x20
        assert_eq!(tempo_events[0].data, vec![0x07, 0xA1, 0x20]);
    }

    // ── set_tempo ─────────────────────────────────────────────────────────

    #[test]
    fn test_set_tempo_120bpm() {
        // 120 BPM → 500000 µs = 07 A1 20
        let mut b = MidiBuilder::default().unwrap();
        b.set_tempo(480, 120.0).unwrap();
        let ev = b.midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte == midi_message_types::SET_TEMPO && e.ticks == 480)
            .next()
            .unwrap();
        assert_eq!(ev.data, vec![0x07, 0xA1, 0x20]);
    }

    #[test]
    fn test_set_tempo_60bpm() {
        // 60 BPM → 1000000 µs = 0F 42 40
        let mut b = MidiBuilder::default().unwrap();
        b.set_tempo(0, 60.0).unwrap();
        let ev = b.midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte == midi_message_types::SET_TEMPO && e.ticks == 0)
            .last()
            .unwrap();
        assert_eq!(ev.data, vec![0x0F, 0x42, 0x40]);
    }

    // ── add_track ─────────────────────────────────────────────────────────

    #[test]
    fn test_add_track_format1() {
        let mut b = MidiBuilder::new(MidiBuilderOptions {
            format: MidiFormat::MultiTrack,
            ..MidiBuilderOptions::default()
        })
        .unwrap();
        b.add_track("Piano", 0).unwrap();
        assert_eq!(b.midi.tracks.len(), 2);
        assert_eq!(b.midi.tracks[1].name, "Piano");
    }

    #[test]
    fn test_add_track_format0_second_rejected() {
        let mut b = MidiBuilder::default().unwrap(); // format 0
        let result = b.add_track("Second", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_track_sets_port() {
        let mut b = MidiBuilder::new(MidiBuilderOptions {
            format: MidiFormat::MultiTrack,
            ..MidiBuilderOptions::default()
        })
        .unwrap();
        b.add_track("Port2", 2).unwrap();
        assert_eq!(b.midi.tracks[1].port, 2);
        // MIDI_PORT event data should contain port 2
        let port_ev = b.midi.tracks[1]
            .events
            .iter()
            .find(|e| e.status_byte == midi_message_types::MIDI_PORT)
            .unwrap();
        assert_eq!(port_ev.data[0], 2);
    }

    // ── add_event ─────────────────────────────────────────────────────────────

    #[test]
    fn test_add_event_invalid_track_returns_err() {
        let mut b = MidiBuilder::default().unwrap();
        let result = b.add_event(0, 99, 0x90, vec![60, 100]);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_event_voice_on_track0_format1_rejected() {
        let mut b = MidiBuilder::new(MidiBuilderOptions {
            format: MidiFormat::MultiTrack,
            ..MidiBuilderOptions::default()
        })
        .unwrap();
        // Track 0 is conductor; voice event (0x90) should be rejected
        let result = b.add_event(0, 0, 0x90, vec![60, 100]);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_event_voice_on_track0_format0_allowed() {
        // In format 0 there is only one track, voice events are fine there
        let mut b = MidiBuilder::default().unwrap();
        let result = b.add_event(0, 0, 0x90, vec![60, 100]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_add_event_meta_on_track0_format1_allowed() {
        let mut b = MidiBuilder::new(MidiBuilderOptions {
            format: MidiFormat::MultiTrack,
            ..MidiBuilderOptions::default()
        })
        .unwrap();
        // SET_TEMPO = 0x51 < NOTE_OFF = 0x80: allowed on conductor track
        let result = b.add_event(480, 0, midi_message_types::SET_TEMPO, vec![0x07, 0xA1, 0x20]);
        assert!(result.is_ok());
    }

    // ── note_on ───────────────────────────────────────────────────────────

    #[test]
    fn test_note_on_status_byte() {
        let mut b = MidiBuilder::default().unwrap();
        b.note_on(0, 0, 3, 60, 100).unwrap();
        let ev = b.midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == 0x93)
            .unwrap();
        assert_eq!(ev.data, vec![60, 100]);
    }

    #[test]
    fn test_note_on_channel_wrapped() {
        let mut b = MidiBuilder::default().unwrap();
        b.note_on(0, 0, 17, 60, 100).unwrap(); // 17 % 16 = 1 → ch1
        let ev = b.midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == 0x91)
            .unwrap();
        assert_eq!(ev.status_byte, 0x91);
    }

    #[test]
    fn test_note_on_velocity_wrapped() {
        let mut b = MidiBuilder::default().unwrap();
        b.note_on(0, 0, 0, 60, 200).unwrap(); // 200 % 128 = 72
        let ev = b.midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == 0x90)
            .unwrap();
        assert_eq!(ev.data[1], 200 % 128);
    }

    // ── note_off ──────────────────────────────────────────────────────────

    #[test]
    fn test_note_off_status_byte() {
        let mut b = MidiBuilder::default().unwrap();
        b.note_off(0, 0, 0, 60, 64).unwrap();
        let ev = b.midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == 0x80)
            .unwrap();
        assert_eq!(ev.data, vec![60, 64]);
    }

    // ── program_change ────────────────────────────────────────────────────

    #[test]
    fn test_program_change_status_byte() {
        let mut b = MidiBuilder::default().unwrap();
        b.program_change(0, 0, 2, 40).unwrap();
        let ev = b.midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == 0xC2)
            .unwrap();
        assert_eq!(ev.data, vec![40]);
    }

    #[test]
    fn test_program_change_program_wrapped() {
        let mut b = MidiBuilder::default().unwrap();
        b.program_change(0, 0, 0, 200).unwrap(); // 200 % 128 = 72
        let ev = b.midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == 0xC0)
            .unwrap();
        assert_eq!(ev.data[0], 200 % 128);
    }

    // ── controller_change ─────────────────────────────────────────────────

    #[test]
    fn test_controller_change_status_byte() {
        let mut b = MidiBuilder::default().unwrap();
        b.controller_change(0, 0, 5, 7, 100).unwrap(); // ch5, CC7, val=100
        let ev = b.midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == 0xB5)
            .unwrap();
        assert_eq!(ev.data, vec![7, 100]);
    }

    // ── pitch_wheel ───────────────────────────────────────────────────────────

    #[test]
    fn test_pitch_wheel_lsb_first() {
        // lsb must come before msb in the data bytes.
        // pitch = (10 << 7) | 20 -> msb=10, lsb=20
        let mut b = MidiBuilder::default().unwrap();
        b.pitch_wheel(0, 0, 0, (10 << 7) | 20).unwrap();
        let ev = b.midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == 0xE0)
            .unwrap();
        assert_eq!(ev.data[0], 20); // lsb first
        assert_eq!(ev.data[1], 10); // msb second
    }

    #[test]
    fn test_pitch_wheel_status_byte() {
        let mut b = MidiBuilder::default().unwrap();
        b.pitch_wheel(0, 0, 4, 0).unwrap(); // channel 4 → 0xE4
        let ev = b.midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == 0xE4)
            .unwrap();
        assert!(ev.status_byte == 0xE4);
    }

    // ── Round-trip with midi_writer ───────────────────────────────────────────

    #[test]
    fn test_round_trip_produces_valid_smf_header() {
        use crate::midi::write::midi::write_midi_internal;

        let mut b = MidiBuilder::new(MidiBuilderOptions {
            format: MidiFormat::MultiTrack,
            time_division: 480,
            name: "Test".to_string(),
            ..MidiBuilderOptions::default()
        })
        .unwrap();
        b.add_track("Piano", 0).unwrap();
        b.note_on(0, 1, 0, 60, 100).unwrap();
        b.note_off(480, 1, 0, 60, 64).unwrap();

        let bytes = write_midi_internal(&b.midi);
        // MThd magic
        assert_eq!(&bytes[0..4], b"MThd");
        // Format 1
        assert_eq!(u16::from_be_bytes([bytes[8], bytes[9]]), 1);
        // 2 tracks
        assert_eq!(u16::from_be_bytes([bytes[10], bytes[11]]), 2);
        // time division 480
        assert_eq!(u16::from_be_bytes([bytes[12], bytes[13]]), 480);
    }

    // ── system_exclusive ──────────────────────────────────────────────────────

    #[test]
    fn test_system_exclusive_status_byte_and_data() {
        let mut b = MidiBuilder::default().unwrap();
        b.system_exclusive(0, 0, &[0x41, 0x10, 0x42, 0x12, 0x40, 0x00, 0x7f, 0x00, 0x41, 0xf7])
            .unwrap();
        let ev = b.midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE)
            .unwrap();
        assert_eq!(ev.data[0], 0x41);
        assert_eq!(*ev.data.last().unwrap(), 0xf7);
    }

    // ── registered_parameter ──────────────────────────────────────────────────

    #[test]
    fn test_registered_parameter_emits_four_cc_events() {
        let mut b = MidiBuilder::default().unwrap();
        b.registered_parameter(0, 0, 0, 0, 2 << 7).unwrap(); // pitch wheel range = 2 semitones

        let ccs: Vec<(u8, u8)> = b.midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte == midi_message_types::CONTROLLER_CHANGE)
            .map(|e| (e.data[0], e.data[1]))
            .collect();
        assert!(ccs.contains(&(midi_controllers::REGISTERED_PARAMETER_MSB, 0)));
        assert!(ccs.contains(&(midi_controllers::REGISTERED_PARAMETER_LSB, 0)));
        assert!(ccs.contains(&(midi_controllers::DATA_ENTRY_MSB, 2)));
        assert!(ccs.contains(&(midi_controllers::DATA_ENTRY_LSB, 0)));
    }

    // ── non_registered_parameter ──────────────────────────────────────────────

    #[test]
    fn test_non_registered_parameter_emits_four_cc_events() {
        let mut b = MidiBuilder::default().unwrap();
        b.non_registered_parameter(0, 0, 0, 0x1808, 64).unwrap(); // drum pitch, part 8

        let ccs: Vec<(u8, u8)> = b.midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte == midi_message_types::CONTROLLER_CHANGE)
            .map(|e| (e.data[0], e.data[1]))
            .collect();
        assert!(ccs.contains(&(midi_controllers::NON_REGISTERED_PARAMETER_MSB, (0x1808 >> 7) as u8)));
        assert!(ccs.contains(&(midi_controllers::NON_REGISTERED_PARAMETER_LSB, (0x1808 & 0x7f) as u8)));
        assert!(ccs.contains(&(midi_controllers::DATA_ENTRY_MSB, 0)));
        assert!(ccs.contains(&(midi_controllers::DATA_ENTRY_LSB, 64)));
    }
}
