/// midi_loader.rs
/// purpose: MIDI file (SMF, RMIDI) parser.
/// XMF format is not supported (panics with unimplemented!).
/// Ported from: src/midi/midi_loader.ts
use crate::midi::basic_midi::BasicMidi;
use crate::midi::midi_message::{data_bytes_amount, get_channel, MidiMessage};
use crate::midi::midi_track::MidiTrack;
use crate::midi::types::MidiFormat;
use crate::utils::byte_functions::big_endian::read_big_endian_indexed;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::loggin::{
    spessa_synth_group_collapsed, spessa_synth_group_end, spessa_synth_info, spessa_synth_warn,
};
use crate::utils::byte_functions::string::{read_binary_string, read_binary_string_indexed};
use crate::utils::byte_functions::variable_length_quantity::read_variable_length_quantity;
use super::rmidi::parse_rmidi_container_internal;

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

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

/// Loads a MIDI file (SMF or RMIDI) from a raw byte slice into `output_midi`.
/// XMF files are not supported and will panic via `unimplemented!`.
///
/// Equivalent to: loadMIDIFromArrayBufferInternal(outputMIDI, arrayBuffer, fileName)
pub fn load_midi_from_array_buffer_internal(
    output_midi: &mut BasicMidi,
    data: &[u8],
    file_name: &str,
) -> Result<(), String> {
    spessa_synth_group_collapsed("Parsing MIDI File...");

    output_midi.file_name = if file_name.is_empty() {
        None
    } else {
        Some(file_name.to_string())
    };

    let mut binary_data = IndexedByteArray::from_slice(data);
    let mut smf_file_binary;

    // Peek at the first 4 bytes without advancing the cursor.
    let initial_string = read_binary_string(&binary_data, 4, 0);

    if initial_string == "RIFF" {
        // ── RMIDI (Resource-Interchangeable MIDI) ─────────────────────
        smf_file_binary = parse_rmidi_container_internal(output_midi, &mut binary_data)?;
    } else if initial_string == "XMF_" {
        // XMF is not needed for midi→wav; stub it out.
        spessa_synth_group_end();
        unimplemented!("XMF not supported");
    } else {
        // Plain SMF file – use the whole buffer as the SMF data.
        smf_file_binary = binary_data;
    }

    // ── Parse Standard MIDI File (SMF) ────────────────────────────────

    let (header_type, header_size, mut header_data) =
        read_midi_chunk(&mut smf_file_binary).inspect_err(|_| {
            spessa_synth_group_end();
        })?;

    if header_type != "MThd" {
        spessa_synth_group_end();
        return Err(format!(
            "Invalid MIDI Header! Expected \"MThd\", got \"{}\"",
            header_type
        ));
    }
    if header_size != 6 {
        spessa_synth_group_end();
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
            spessa_synth_warn(&format!("Unknown MIDI format: {}", v));
            MidiFormat::SingleTrack
        }
    };
    let track_count = read_big_endian_indexed(&mut header_data, 2) as usize;
    output_midi.time_division = read_big_endian_indexed(&mut header_data, 2);

    // ── Parse MTrk chunks ─────────────────────────────────────────────
    for i in 0..track_count {
        let mut track = MidiTrack::new();

        let (track_type, track_size, mut track_data) =
            read_midi_chunk(&mut smf_file_binary).inspect_err(|_| {
                spessa_synth_group_end();
            })?;

        if track_type != "MTrk" {
            spessa_synth_group_end();
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
            let status_byte: u8;
            if let Some(rb) = running_byte && status_byte_check < 0x80 {
                // Use the running status – do NOT advance the cursor.
                status_byte = rb;
            } else if status_byte_check < 0x80 {
                spessa_synth_group_end();
                return Err(format!(
                    "Unexpected byte with no running byte. ({})",
                    status_byte_check
                ));
            } else {
                status_byte = track_data[track_data.current_index];
                track_data.current_index += 1;
            }

            let channel = get_channel(status_byte);

            // Determine event data length and final status byte.
            let event_data_length: usize;
            let final_status_byte: u8;

            match channel {
                -1 => {
                    // System common / realtime – no data bytes.
                    event_data_length = 0;
                    final_status_byte = status_byte;
                }
                -2 => {
                    // Meta event: read meta type, then VLQ length.
                    final_status_byte = track_data[track_data.current_index];
                    track_data.current_index += 1;
                    event_data_length =
                        read_variable_length_quantity(&mut track_data) as usize;
                }
                -3 => {
                    // SysEx: VLQ length follows.
                    event_data_length =
                        read_variable_length_quantity(&mut track_data) as usize;
                    final_status_byte = status_byte;
                }
                _ => {
                    // Voice message: fixed length from high nibble.
                    event_data_length =
                        data_bytes_amount(status_byte >> 4) as usize;
                    running_byte = Some(status_byte);
                    final_status_byte = status_byte;
                }
            }

            // Read event data bytes.
            let start = track_data.current_index;
            let end = start + event_data_length;
            let event_data = track_data.slice(start, end).to_vec();

            let event = MidiMessage::new(total_ticks, final_status_byte, event_data);
            track.push_event(event);

            track_data.current_index += event_data_length;
        }

        output_midi.tracks.push(track);

        spessa_synth_info(&format!(
            "Parsed {} / {}",
            output_midi.tracks.len(),
            track_count
        ));
    }

    spessa_synth_info("All tracks parsed correctly!");
    // Events from an SMF are already in sorted order per the spec; no need to re-sort.
    output_midi.flush(false);
    spessa_synth_group_end();
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::enums::midi_message_types;
    use crate::utils::byte_functions::big_endian::write_big_endian;
    use crate::utils::byte_functions::variable_length_quantity::write_variable_length_quantity;

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
        load_midi_from_array_buffer_internal(&mut midi, &smf, "test.mid").unwrap();

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
        load_midi_from_array_buffer_internal(&mut midi, &smf, "test.mid").unwrap();

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
        load_midi_from_array_buffer_internal(&mut midi, &smf, "test.mid").unwrap();

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
        load_midi_from_array_buffer_internal(&mut midi, &smf, "test.mid").unwrap();

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
        load_midi_from_array_buffer_internal(&mut midi, &smf, "test.mid").unwrap();

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
        let result = load_midi_from_array_buffer_internal(&mut midi, &bad, "bad.mid");
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
        let result = load_midi_from_array_buffer_internal(&mut midi, &smf, "bad.mid");
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
        load_midi_from_array_buffer_internal(&mut midi, &smf, "song.mid").unwrap();
        assert_eq!(midi.file_name, Some("song.mid".to_string()));
    }

    #[test]
    fn test_empty_file_name_stored_as_none() {
        let mut smf = mthd(0, 1, 480);
        let mut events = vec![];
        events.extend(eot());
        smf.extend(mtrk(&events));

        let mut midi = BasicMidi::new();
        load_midi_from_array_buffer_internal(&mut midi, &smf, "").unwrap();
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
        load_midi_from_array_buffer_internal(&mut midi, &smf, "test.mid").unwrap();

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
        load_midi_from_array_buffer_internal(&mut midi, &smf, "test.mid").unwrap();

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
        load_midi_from_array_buffer_internal(&mut midi, &smf, "test.mid").unwrap();
        assert!(!midi.tracks.is_empty());
    }
}
