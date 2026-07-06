/// midi.rs
/// purpose: Standard MIDI File (SMF) parser.
/// Ported from: src/midi/read/midi.ts (spessasynth_core 4.3.0)
///
/// TS 4.3.0 split the monolithic `midi_loader.ts` (4.2.0) into `read/midi.ts`, `read/rmidi.ts`,
/// and `read/xmf.ts`. This file is `read/midi.ts`'s `parseSMFInternal`, which now handles *only*
/// SMF parsing: the RIFF (RMIDI) / "XMF_" / plain-SMF format dispatch that `loadMIDIFromArrayBufferInternal`
/// used to do here moved to `BasicMIDI.fromArrayBuffer` (see `basic_midi.rs`), and `read/rmidi.ts`
/// now calls `parseSMFInternal` (this file) at the end of its own parsing instead of the other
/// way around — this Rust port's phase-1 structure had the call direction backwards (`read/midi.rs`
/// called into `read/rmidi.rs`); it is corrected here to match the real TS 4.3.0 layout.
///
/// TS 4.3.0 also inlined the message-length dispatch: instead of calling the (now file-local, no
/// longer exported) `getChannel` to classify the status byte into -1/-2/-3/voice, it does a
/// direct range check against `MIDIMessageTypes` bounds, and moved `dataBytesAmount` into a
/// private `DataBytesAmount` const local to this file (both ported below as `data_bytes_amount`,
/// a private fn distinct from the crate-public `midi_message::data_bytes_amount` kept for the
/// not-yet-ported `sequencer` module — see `midi_message.rs`'s header comment).
use crate::midi::basic_midi::BasicMidi;
use crate::midi::midi_message::MidiMessage;
use crate::midi::midi_track::MidiTrack;
use crate::midi::types::MidiFormat;
use crate::midi::enums::midi_message_types;
use crate::utils::byte_functions::big_endian::read_big_endian_indexed;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::loggin::SpessaLog;
use crate::utils::byte_functions::string::read_binary_string_indexed;
use crate::utils::byte_functions::variable_length_quantity::read_variable_length_quantity;

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Returns the number of data bytes for a given MIDI event high nibble (0x8–0xE).
/// Private to this file — TS 4.3.0's local `DataBytesAmount` const in `read/midi.ts`.
/// Equivalent to: DataBytesAmount[highNibble]
fn data_bytes_amount(high_nibble: u8) -> u8 {
    match high_nibble {
        0x8 | 0x9 | 0xA | 0xB | 0xE => 2,
        0xC | 0xD => 1,
        _ => 0,
    }
}

/// Reads a MIDI chunk (MThd or MTrk) header and data from the stream.
/// Returns `(chunk_type, data_size, data_as_IndexedByteArray)`.
/// Equivalent to the inner `readMIDIChunk` closure in TypeScript.
fn read_midi_chunk(
    file_byte_array: &mut IndexedByteArray,
) -> Result<(String, u32, IndexedByteArray), String> {
    let chunk_type = read_binary_string_indexed(file_byte_array, 4);
    let size = read_big_endian_indexed(file_byte_array, 4);
    let start = file_byte_array.current_index;
    let end = start + size as usize;
    if end > file_byte_array.len() {
        return Err(format!(
            "MIDI chunk '{}' claims size {} but only {} bytes remain",
            chunk_type,
            size,
            file_byte_array.len().saturating_sub(start)
        ));
    }
    let data = file_byte_array.slice(start, end);
    file_byte_array.current_index = end;
    Ok((chunk_type, size, data))
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Loads a Standard MIDI File (SMF) from given binary data into `output_midi`.
///
/// `smf_file_binary` must already be positioned at the start of the `MThd` chunk (the
/// RIFF/RMIDI/XMF format dispatch happens one level up, in `BasicMidi::from_array_buffer`; for
/// an RMIDI input it is the raw SMF bytes extracted from the RIFF `data` chunk by
/// `read::rmidi::parse_rmidi_internal`, which calls this function at the end of its own parsing).
///
/// Equivalent to: parseSMFInternal(outputMIDI, smfFileBinary, fileName)
pub fn parse_smf_internal(
    output_midi: &mut BasicMidi,
    smf_file_binary: &mut IndexedByteArray,
    file_name: &str,
) -> Result<(), String> {
    SpessaLog::group_collapsed("Parsing MIDI File...");

    output_midi.file_name = if file_name.is_empty() {
        None
    } else {
        Some(file_name.to_string())
    };

    let (header_type, header_size, mut header_data) =
        read_midi_chunk(smf_file_binary).inspect_err(|_| {
            SpessaLog::group_end();
        })?;

    if header_type != "MThd" {
        SpessaLog::group_end();
        return Err(format!(
            "Invalid MIDI Header! Expected \"MThd\", got \"{}\"",
            header_type
        ));
    }
    if header_size != 6 {
        SpessaLog::group_end();
        return Err(format!(
            "Invalid MIDI header chunk size! Expected 6, got {}",
            header_size
        ));
    }

    output_midi.format = match read_big_endian_indexed(&mut header_data, 2) {
        0 => MidiFormat::SingleTrack,
        1 => MidiFormat::MultiTrack,
        2 => MidiFormat::MultiPattern,
        v => {
            SpessaLog::warn(&format!("Unknown MIDI format: {}", v));
            MidiFormat::SingleTrack
        }
    };
    let track_count = read_big_endian_indexed(&mut header_data, 2) as usize;
    output_midi.time_division = read_big_endian_indexed(&mut header_data, 2);

    // ── Parse MTrk chunks ─────────────────────────────────────────────
    for i in 0..track_count {
        let mut track = MidiTrack::new();

        let (track_type, track_size, mut track_data) =
            read_midi_chunk(smf_file_binary).inspect_err(|_| {
                SpessaLog::group_end();
            })?;

        if track_type != "MTrk" {
            SpessaLog::group_end();
            return Err(format!(
                "Invalid track header! Expected \"MTrk\", got \"{}\"",
                track_type
            ));
        }

        // MIDI running status byte.
        let mut running_byte: Option<u8> = None;
        let mut total_ticks: u32 = 0;

        // Format 2: each track starts where the previous one ended.
        if output_midi.format == MidiFormat::MultiPattern
            && i > 0
            && let Some(last_event) = output_midi.tracks[i - 1].events.last()
        {
            total_ticks += last_event.ticks;
        }

        while track_data.current_index < track_size as usize {
            total_ticks += read_variable_length_quantity(&mut track_data);

            let status_byte_check = track_data[track_data.current_index];

            // Determine the actual status byte (handle running status).
            let mut status_byte: u8;
            if let Some(rb) = running_byte && status_byte_check < 0x80 {
                // Use the running status – do NOT advance the cursor.
                status_byte = rb;
            } else if status_byte_check < 0x80 {
                SpessaLog::group_end();
                return Err(format!(
                    "Unexpected byte with no running byte. ({})",
                    status_byte_check
                ));
            } else {
                status_byte = track_data[track_data.current_index];
                track_data.current_index += 1;
            }

            // Determine the message's length.
            // Equivalent to the inlined 4.3.0 dispatch (no more `getChannel` call): a direct
            // range check against `MIDIMessageTypes` bounds instead of a -1/-2/-3/voice
            // classification.
            let event_data_length: usize;

            if status_byte >= midi_message_types::NOTE_OFF
                && status_byte < midi_message_types::SYSTEM_EXCLUSIVE
            {
                // Voice message: fixed length from high nibble.
                event_data_length = data_bytes_amount(status_byte >> 4) as usize;
                // Save the status byte
                running_byte = Some(status_byte);
            } else if status_byte == midi_message_types::SYSTEM_EXCLUSIVE {
                // Sysex: VLQ length follows.
                event_data_length = read_variable_length_quantity(&mut track_data) as usize;
            } else if status_byte == 0xff {
                // Meta message (the next byte is the actual status byte).
                status_byte = track_data[track_data.current_index];
                track_data.current_index += 1;
                event_data_length = read_variable_length_quantity(&mut track_data) as usize;
            } else {
                // System common/realtime (no length).
                event_data_length = 0;
            }

            // Put the event data into the array.
            let start = track_data.current_index;
            let end = start + event_data_length;
            let event_data = track_data.slice(start, end).to_vec();

            track.push_event(MidiMessage::new(total_ticks, status_byte, event_data));

            // Advance the track chunk.
            track_data.current_index += event_data_length;
        }

        output_midi.tracks.push(track);

        SpessaLog::info(&format!(
            "Parsed {} / {}",
            output_midi.tracks.len(),
            track_count
        ));
    }

    SpessaLog::info("All tracks parsed correctly!");
    // Events from an SMF are already in sorted order per the spec; no need to re-sort.
    output_midi.flush(false);
    SpessaLog::group_end();
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::byte_functions::big_endian::write_big_endian;
    use crate::utils::byte_functions::variable_length_quantity::write_variable_length_quantity;

    /// Test helper: wraps `parse_smf_internal` with the `IndexedByteArray` construction that
    /// `BasicMidi::from_array_buffer` now does one level up.
    fn parse(midi: &mut BasicMidi, data: &[u8], file_name: &str) -> Result<(), String> {
        parse_smf_internal(midi, &mut IndexedByteArray::from_slice(data), file_name)
    }

    // ── Helpers to build minimal SMF binary ──────────────────────────

    /// Writes a big-endian u16.
    fn be16(v: u16) -> Vec<u8> {
        vec![(v >> 8) as u8, v as u8]
    }

    /// Writes a big-endian u32.
    fn be32(v: u32) -> Vec<u8> {
        write_big_endian(v, 4)
    }

    /// Builds an MThd chunk.
    fn mthd(format: u16, tracks: u16, division: u16) -> Vec<u8> {
        let mut b = b"MThd".to_vec();
        b.extend(be32(6)); // size always 6
        b.extend(be16(format));
        b.extend(be16(tracks));
        b.extend(be16(division));
        b
    }

    /// Builds an MTrk chunk from raw event bytes.
    fn mtrk(events_bytes: &[u8]) -> Vec<u8> {
        let mut b = b"MTrk".to_vec();
        b.extend(be32(events_bytes.len() as u32));
        b.extend_from_slice(events_bytes);
        b
    }

    /// VLQ-encodes a delta time.
    fn dt(v: u32) -> Vec<u8> {
        write_variable_length_quantity(v)
    }

    /// End-of-track meta event.
    fn eot() -> Vec<u8> {
        // delta=0, 0xFF 0x2F 0x00
        let mut b = dt(0);
        b.extend_from_slice(&[0xFF, 0x2F, 0x00]);
        b
    }

    // ── Basic SMF parsing ────────────────────────────────────────────

    #[test]
    fn test_parse_minimal_midi_format0() {
        // Single track: note-on at tick 0, note-off at tick 480, end-of-track.
        let mut events: Vec<u8> = Vec::new();
        // note-on ch0, note 60, vel 100
        events.extend(dt(0));
        events.extend_from_slice(&[0x90, 60, 100]);
        // note-off ch0, note 60, vel 0  (delta = 480 ticks)
        events.extend(dt(480));
        events.extend_from_slice(&[0x80, 60, 0]);
        events.extend(eot());

        let mut smf = mthd(0, 1, 480);
        smf.extend(mtrk(&events));

        let mut midi = BasicMidi::new();
        parse(&mut midi, &smf, "test.mid").unwrap();

        assert_eq!(midi.format, MidiFormat::SingleTrack);
        assert_eq!(midi.time_division, 480);
        assert_eq!(midi.tracks.len(), 1);
        assert_eq!(midi.first_note_on, 0);
    }

    #[test]
    fn test_parse_midi_format1_two_tracks() {
        // Track 0: tempo-only (conductor), Track 1: note.
        let mut t0: Vec<u8> = Vec::new();
        // Set tempo: 500000 µs/beat = 120 BPM
        t0.extend(dt(0));
        t0.extend_from_slice(&[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
        t0.extend(eot());

        let mut t1: Vec<u8> = Vec::new();
        t1.extend(dt(0));
        t1.extend_from_slice(&[0x90, 60, 100]);
        t1.extend(dt(480));
        t1.extend_from_slice(&[0x80, 60, 0]);
        t1.extend(eot());

        let mut smf = mthd(1, 2, 480);
        smf.extend(mtrk(&t0));
        smf.extend(mtrk(&t1));

        let mut midi = BasicMidi::new();
        parse(&mut midi, &smf, "test.mid").unwrap();

        assert_eq!(midi.format, MidiFormat::MultiTrack);
        assert_eq!(midi.tracks.len(), 2);
        assert_eq!(midi.first_note_on, 0);
        // last voice event (note off) is at tick 480 → 0.5 s at 120 BPM
        assert!((midi.duration - 0.5).abs() < 0.01, "duration = {}", midi.duration);
    }

    #[test]
    fn test_parse_running_status() {
        // Two note-ons using running status (second event omits status byte).
        let mut events: Vec<u8> = Vec::new();
        events.extend(dt(0));
        events.extend_from_slice(&[0x90, 60, 100]); // note-on with status
        events.extend(dt(10));
        events.extend_from_slice(&[64, 80]); // note-on via running status
        events.extend(eot());

        let mut smf = mthd(0, 1, 480);
        smf.extend(mtrk(&events));

        let mut midi = BasicMidi::new();
        parse(&mut midi, &smf, "test.mid").unwrap();

        // Both events should be note-ons
        let note_ons: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte == 0x90)
            .collect();
        assert_eq!(note_ons.len(), 2);
        assert_eq!(note_ons[0].data[0], 60);
        assert_eq!(note_ons[1].data[0], 64);
    }

    #[test]
    fn test_parse_meta_set_tempo() {
        // Verify SET_TEMPO meta event is parsed correctly.
        let mut events: Vec<u8> = Vec::new();
        // 500000 µs/beat = 120 BPM
        events.extend(dt(0));
        events.extend_from_slice(&[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
        events.extend(dt(480));
        events.extend_from_slice(&[0x90, 60, 100]);
        events.extend(eot());

        let mut smf = mthd(0, 1, 480);
        smf.extend(mtrk(&events));

        let mut midi = BasicMidi::new();
        parse(&mut midi, &smf, "test.mid").unwrap();

        // There should be at least one TempoChange (the parsed one + default 120)
        // After reversal, the tick-0 entry is at the end.
        assert!(!midi.tempo_changes.is_empty());
    }

    #[test]
    fn test_parse_sysex_event() {
        // SysEx event should be parsed without error.
        let mut events: Vec<u8> = Vec::new();
        // SysEx: F0 <len VLQ> <data bytes> (no F7 in SPessaSynth model)
        let sysex_data: &[u8] = &[0x43, 0x10, 0x4C, 0x00, 0x00, 0x7E, 0x00];
        events.extend(dt(0));
        events.push(0xF0);
        events.extend(write_variable_length_quantity(sysex_data.len() as u32));
        events.extend_from_slice(sysex_data);
        events.extend(eot());

        let mut smf = mthd(0, 1, 480);
        smf.extend(mtrk(&events));

        let mut midi = BasicMidi::new();
        parse(&mut midi, &smf, "test.mid").unwrap();

        let sysex_events: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte == 0xF0)
            .collect();
        assert_eq!(sysex_events.len(), 1);
        assert_eq!(sysex_events[0].data, sysex_data);
    }

    #[test]
    fn test_parse_bad_header_returns_err() {
        let bad: Vec<u8> = b"BADH\x00\x00\x00\x06\x00\x00\x00\x01\x01\xe0".to_vec();
        let mut midi = BasicMidi::new();
        let result = parse(&mut midi, &bad, "bad.mid");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("MThd"));
    }

    #[test]
    fn test_parse_bad_track_header_returns_err() {
        let mut smf = mthd(0, 1, 480);
        // Write a chunk with wrong type "XTRK"
        smf.extend(b"XTRK");
        smf.extend(be32(0));

        let mut midi = BasicMidi::new();
        let result = parse(&mut midi, &smf, "bad.mid");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("MTrk"));
    }

    #[test]
    fn test_file_name_stored() {
        let mut smf = mthd(0, 1, 480);
        let mut events = vec![];
        events.extend(eot());
        smf.extend(mtrk(&events));

        let mut midi = BasicMidi::new();
        parse(&mut midi, &smf, "song.mid").unwrap();
        assert_eq!(midi.file_name, Some("song.mid".to_string()));
    }

    #[test]
    fn test_empty_file_name_stored_as_none() {
        let mut smf = mthd(0, 1, 480);
        let mut events = vec![];
        events.extend(eot());
        smf.extend(mtrk(&events));

        let mut midi = BasicMidi::new();
        parse(&mut midi, &smf, "").unwrap();
        assert!(midi.file_name.is_none());
    }

    #[test]
    fn test_parse_midi_port_meta() {
        // MIDI port meta event 0xFF 0x21 0x01 <port>
        let mut events: Vec<u8> = Vec::new();
        events.extend(dt(0));
        events.extend_from_slice(&[0xFF, midi_message_types::MIDI_PORT, 0x01, 0x01]); // port 1
        events.extend(dt(10));
        events.extend_from_slice(&[0x90, 60, 100]);
        events.extend(eot());

        let mut smf = mthd(0, 1, 480);
        smf.extend(mtrk(&events));

        let mut midi = BasicMidi::new();
        parse(&mut midi, &smf, "test.mid").unwrap();

        // Port events are parsed; track will have port assigned.
        assert!(!midi.tracks.is_empty());
    }

    #[test]
    fn test_program_change_one_data_byte() {
        // Program change has only 1 data byte.
        let mut events: Vec<u8> = Vec::new();
        events.extend(dt(0));
        events.extend_from_slice(&[0xC0, 25]); // program change ch0, program 25
        events.extend(dt(10));
        events.extend_from_slice(&[0x90, 60, 100]);
        events.extend(eot());

        let mut smf = mthd(0, 1, 480);
        smf.extend(mtrk(&events));

        let mut midi = BasicMidi::new();
        parse(&mut midi, &smf, "test.mid").unwrap();

        let pc: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte == 0xC0)
            .collect();
        assert_eq!(pc.len(), 1);
        assert_eq!(pc[0].data, vec![25]);
    }

    #[test]
    fn test_parse_text_meta_event() {
        // Text meta event should be parseable.
        let text = b"Hello";
        let mut events: Vec<u8> = Vec::new();
        events.extend(dt(0));
        events.push(0xFF);
        events.push(midi_message_types::TEXT);
        events.extend(write_variable_length_quantity(text.len() as u32));
        events.extend_from_slice(text);
        events.extend(eot());

        let mut smf = mthd(0, 1, 480);
        smf.extend(mtrk(&events));

        let mut midi = BasicMidi::new();
        parse(&mut midi, &smf, "test.mid").unwrap();
        assert!(!midi.tracks.is_empty());
    }
}
