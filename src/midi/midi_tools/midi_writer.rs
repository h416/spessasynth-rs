/// midi_writer.rs
/// purpose: Serialize a BasicMidi into a standard MIDI file (SMF) byte sequence.
/// Ported from: src/midi/midi_tools/midi_writer.ts
use crate::midi::basic_midi::BasicMidi;
use crate::midi::enums::midi_message_types;
use crate::utils::big_endian::write_big_endian;
use crate::utils::variable_length_quantity::write_variable_length_quantity;

/// Serializes `midi` as a standard MIDI file and returns the bytes.
/// Equivalent to: writeMIDIInternal(midi)
pub fn write_midi_internal(midi: &BasicMidi) -> Vec<u8> {
    // Encode each track into SMF binary
    let mut binary_track_data: Vec<Vec<u8>> = Vec::new();

    for track in &midi.tracks {
        let mut binary_track: Vec<u8> = Vec::new();
        let mut current_tick = 0u32;
        let mut running_byte: Option<u8> = None;

        for event in &track.events {
            // Ticks in BasicMidi are absolute; SMF stores relative (delta) ticks.
            let delta_ticks = event.ticks.saturating_sub(current_tick);

            // EndOfTrack is written automatically at the end of the track.
            if event.status_byte == midi_message_types::END_OF_TRACK {
                current_tick += delta_ticks;
                continue;
            }

            let mut message_data: Vec<u8> = Vec::new();

            if event.status_byte <= midi_message_types::SEQUENCE_SPECIFIC {
                // Meta event: FF <type> <vlq_length> <data>
                // RP-001: meta events cancel any running status.
                message_data.push(0xFF);
                message_data.push(event.status_byte);
                message_data.extend(write_variable_length_quantity(event.data.len() as u32));
                message_data.extend_from_slice(&event.data);
                running_byte = None;
            } else if event.status_byte == midi_message_types::SYSTEM_EXCLUSIVE {
                // Sysex event: F0 <vlq_length> <data>
                // RP-001: sysex events cancel any running status.
                message_data.push(0xF0);
                message_data.extend(write_variable_length_quantity(event.data.len() as u32));
                message_data.extend_from_slice(&event.data);
                running_byte = None;
            } else {
                // Voice message: apply running status compression.
                if running_byte != Some(event.status_byte) {
                    running_byte = Some(event.status_byte);
                    message_data.push(event.status_byte);
                }
                message_data.extend_from_slice(&event.data);
            }

            // Write VLQ-encoded delta ticks followed by the message.
            binary_track.extend(write_variable_length_quantity(delta_ticks));
            binary_track.extend(message_data);
            current_tick += delta_ticks;
        }

        // Write EndOfTrack marker: delta=0, FF 2F 00
        binary_track.extend_from_slice(&[0x00, 0xFF, midi_message_types::END_OF_TRACK, 0x00]);
        binary_track_data.push(binary_track);
    }

    // Build the complete SMF file
    let mut binary_data: Vec<u8> = Vec::new();

    // MThd header: "MThd" + length(6) + format(2 bytes) + num_tracks(2) + time_div(2)
    binary_data.extend_from_slice(b"MThd");
    binary_data.extend(write_big_endian(6, 4));
    binary_data.extend(write_big_endian(midi.format.as_u8() as u32, 2));
    binary_data.extend(write_big_endian(midi.tracks.len() as u32, 2));
    binary_data.extend(write_big_endian(midi.time_division, 2));

    // MTrk chunks
    for track in &binary_track_data {
        binary_data.extend_from_slice(b"MTrk");
        binary_data.extend(write_big_endian(track.len() as u32, 4));
        binary_data.extend_from_slice(track);
    }

    binary_data
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::basic_midi::BasicMidi;
    use crate::midi::midi_message::MidiMessage;
    use crate::midi::midi_track::MidiTrack;
    use crate::midi::types::MidiFormat;

    fn make_msg(ticks: u32, status: u8, data: Vec<u8>) -> MidiMessage {
        MidiMessage::new(ticks, status, data)
    }

    fn push_event(track: &mut MidiTrack, ticks: u32, status: u8, data: Vec<u8>) {
        track.push_event(make_msg(ticks, status, data));
    }

    /// Parse a minimal SMF back out and verify high-level properties.
    fn parse_mthd(bytes: &[u8]) -> (u16, u16, u16) {
        assert_eq!(&bytes[0..4], b"MThd");
        let length = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(length, 6);
        let format = u16::from_be_bytes([bytes[8], bytes[9]]);
        let num_tracks = u16::from_be_bytes([bytes[10], bytes[11]]);
        let time_div = u16::from_be_bytes([bytes[12], bytes[13]]);
        (format, num_tracks, time_div)
    }

    fn find_mtrk(bytes: &[u8], index: usize) -> &[u8] {
        // Walk past MThd (14 bytes header) and previous MTrk chunks.
        let mut pos = 14;
        for _ in 0..index {
            assert_eq!(&bytes[pos..pos + 4], b"MTrk");
            let len = u32::from_be_bytes([
                bytes[pos + 4],
                bytes[pos + 5],
                bytes[pos + 6],
                bytes[pos + 7],
            ]) as usize;
            pos += 8 + len;
        }
        assert_eq!(&bytes[pos..pos + 4], b"MTrk");
        let len = u32::from_be_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        &bytes[pos + 8..pos + 8 + len]
    }

    // ── Header tests ─────────────────────────────────────────────────────────

    #[test]
    fn test_header_format0_single_track() {
        let mut midi = BasicMidi::new();
        midi.format = MidiFormat::SingleTrack;
        midi.time_division = 480;
        let mut t = MidiTrack::new();
        push_event(&mut t, 0, midi_message_types::END_OF_TRACK, vec![]);
        midi.tracks.push(t);

        let bytes = write_midi_internal(&midi);
        let (fmt, ntracks, tdiv) = parse_mthd(&bytes);
        assert_eq!(fmt, 0);
        assert_eq!(ntracks, 1);
        assert_eq!(tdiv, 480);
    }

    #[test]
    fn test_header_format1_two_tracks() {
        let mut midi = BasicMidi::new();
        midi.format = MidiFormat::MultiTrack;
        midi.time_division = 960;
        midi.tracks.push(MidiTrack::new());
        midi.tracks.push(MidiTrack::new());

        let bytes = write_midi_internal(&midi);
        let (fmt, ntracks, tdiv) = parse_mthd(&bytes);
        assert_eq!(fmt, 1);
        assert_eq!(ntracks, 2);
        assert_eq!(tdiv, 960);
    }

    #[test]
    fn test_header_magic_mthd() {
        let mut midi = BasicMidi::new();
        midi.tracks.push(MidiTrack::new());
        let bytes = write_midi_internal(&midi);
        assert_eq!(&bytes[0..4], b"MThd");
    }

    // ── EndOfTrack handling ───────────────────────────────────────────────────

    #[test]
    fn test_eot_always_appended() {
        // Even with an empty track, EOT must be written.
        let mut midi = BasicMidi::new();
        midi.tracks.push(MidiTrack::new());
        let bytes = write_midi_internal(&midi);
        let track = find_mtrk(&bytes, 0);
        // delta=0, FF 2F 00
        assert_eq!(track, &[0x00, 0xFF, 0x2F, 0x00]);
    }

    #[test]
    fn test_eot_event_skipped_automatic_appended() {
        // An existing END_OF_TRACK event should be skipped; a fresh one appended.
        let mut midi = BasicMidi::new();
        let mut t = MidiTrack::new();
        push_event(&mut t, 0, midi_message_types::END_OF_TRACK, vec![]);
        midi.tracks.push(t);
        let bytes = write_midi_internal(&midi);
        let track = find_mtrk(&bytes, 0);
        assert_eq!(track, &[0x00, 0xFF, 0x2F, 0x00]);
    }

    // ── Meta events ───────────────────────────────────────────────────────────

    #[test]
    fn test_meta_set_tempo_encoding() {
        // SET_TEMPO (0x51) with 3-byte payload 07 A1 20 → 500000 µs/beat = 120 BPM
        let mut midi = BasicMidi::new();
        let mut t = MidiTrack::new();
        push_event(
            &mut t,
            0,
            midi_message_types::SET_TEMPO,
            vec![0x07, 0xA1, 0x20],
        );
        push_event(&mut t, 0, midi_message_types::END_OF_TRACK, vec![]);
        midi.tracks.push(t);

        let bytes = write_midi_internal(&midi);
        let track = find_mtrk(&bytes, 0);
        // delta=0x00, FF 51 03 07 A1 20, then EOT
        assert_eq!(
            track,
            &[0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20, 0x00, 0xFF, 0x2F, 0x00]
        );
    }

    #[test]
    fn test_meta_track_name_encoding() {
        let mut midi = BasicMidi::new();
        let mut t = MidiTrack::new();
        push_event(
            &mut t,
            0,
            midi_message_types::TRACK_NAME,
            b"Test".to_vec(),
        );
        midi.tracks.push(t);

        let bytes = write_midi_internal(&midi);
        let track = find_mtrk(&bytes, 0);
        // delta=0x00, FF 03 04 T e s t, then EOT
        assert_eq!(
            &track[..8],
            &[0x00, 0xFF, 0x03, 0x04, b'T', b'e', b's', b't']
        );
    }

    // ── Voice messages ────────────────────────────────────────────────────────

    #[test]
    fn test_note_on_with_status_byte() {
        let mut midi = BasicMidi::new();
        let mut t = MidiTrack::new();
        push_event(&mut t, 0, 0x90, vec![60, 100]); // note-on ch1 C4 vel100
        midi.tracks.push(t);

        let bytes = write_midi_internal(&midi);
        let track = find_mtrk(&bytes, 0);
        // delta=0, 90 3C 64, then EOT
        assert_eq!(&track[..4], &[0x00, 0x90, 60, 100]);
    }

    #[test]
    fn test_running_status_compression() {
        // Two consecutive note-ons on the same channel: second omits status byte.
        let mut midi = BasicMidi::new();
        let mut t = MidiTrack::new();
        push_event(&mut t, 0, 0x90, vec![60, 100]);
        push_event(&mut t, 10, 0x90, vec![64, 80]);
        midi.tracks.push(t);

        let bytes = write_midi_internal(&midi);
        let track = find_mtrk(&bytes, 0);
        // First event: delta=0, 90 3C 64
        // Second event: delta=10 (VLQ: 0x0A), 40 50  (no status byte – running status)
        assert_eq!(&track[..7], &[0x00, 0x90, 60, 100, 0x0A, 64, 80]);
    }

    #[test]
    fn test_running_status_reset_by_meta() {
        // A meta event must reset running status.
        let mut midi = BasicMidi::new();
        let mut t = MidiTrack::new();
        push_event(&mut t, 0, 0x90, vec![60, 100]);
        push_event(&mut t, 0, midi_message_types::TRACK_NAME, b"X".to_vec()); // meta → resets
        push_event(&mut t, 0, 0x90, vec![64, 80]); // must re-emit status byte
        midi.tracks.push(t);

        let bytes = write_midi_internal(&midi);
        let track = find_mtrk(&bytes, 0);
        // note-on: 00 90 3C 64
        assert_eq!(&track[0..4], &[0x00, 0x90, 60, 100]);
        // meta track-name "X": 00 FF 03 01 58
        assert_eq!(&track[4..9], &[0x00, 0xFF, 0x03, 0x01, b'X']);
        // note-on (must include status byte again): 00 90 40 50
        assert_eq!(&track[9..13], &[0x00, 0x90, 64, 80]);
    }

    #[test]
    fn test_running_status_reset_by_sysex() {
        let mut midi = BasicMidi::new();
        let mut t = MidiTrack::new();
        push_event(&mut t, 0, 0x90, vec![60, 100]);
        push_event(&mut t, 0, midi_message_types::SYSTEM_EXCLUSIVE, vec![0x41, 0xF7]);
        push_event(&mut t, 0, 0x90, vec![64, 80]); // must re-emit status
        midi.tracks.push(t);

        let bytes = write_midi_internal(&midi);
        let track = find_mtrk(&bytes, 0);
        // note-on: 00 90 3C 64
        assert_eq!(&track[0..4], &[0x00, 0x90, 60, 100]);
        // sysex F0 02 41 F7 (length=2)
        assert_eq!(&track[4..9], &[0x00, 0xF0, 0x02, 0x41, 0xF7]);
        // note-on re-emits status: 00 90 40 50
        assert_eq!(&track[9..13], &[0x00, 0x90, 64, 80]);
    }

    // ── Delta tick encoding ───────────────────────────────────────────────────

    #[test]
    fn test_delta_ticks_converted_from_absolute() {
        let mut midi = BasicMidi::new();
        let mut t = MidiTrack::new();
        // Absolute ticks: 0, 480
        push_event(&mut t, 0, 0x90, vec![60, 100]);
        push_event(&mut t, 480, 0x80, vec![60, 0]); // note-off at abs tick 480
        midi.tracks.push(t);

        let bytes = write_midi_internal(&midi);
        let track = find_mtrk(&bytes, 0);
        // First delta=0, second delta=480 (VLQ: 83 60)
        assert_eq!(&track[0..4], &[0x00, 0x90, 60, 100]);
        assert_eq!(&track[4..8], &[0x83, 0x60, 0x80, 60]); // 480 as VLQ = 83 60
    }

    #[test]
    fn test_delta_ticks_large_vlq() {
        // delta = 16383 → VLQ = FF 7F
        let mut midi = BasicMidi::new();
        let mut t = MidiTrack::new();
        push_event(&mut t, 0, 0x90, vec![60, 100]);
        push_event(&mut t, 16383, 0x80, vec![60, 0]);
        midi.tracks.push(t);

        let bytes = write_midi_internal(&midi);
        let track = find_mtrk(&bytes, 0);
        // First event: 00 90 3C 64
        // Second delta = 16383 → VLQ FF 7F, then running-status note-off: 80 3C 00
        assert_eq!(&track[4..8], &[0xFF, 0x7F, 0x80, 60]);
    }

    // ── Sysex ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_sysex_encoding() {
        let mut midi = BasicMidi::new();
        let mut t = MidiTrack::new();
        push_event(
            &mut t,
            0,
            midi_message_types::SYSTEM_EXCLUSIVE,
            vec![0x7E, 0x7F, 0xF7],
        );
        midi.tracks.push(t);

        let bytes = write_midi_internal(&midi);
        let track = find_mtrk(&bytes, 0);
        // delta=0, F0 03 7E 7F F7, then EOT
        assert_eq!(&track[..6], &[0x00, 0xF0, 0x03, 0x7E, 0x7F, 0xF7]);
    }

    // ── MTrk structure ────────────────────────────────────────────────────────

    #[test]
    fn test_mtrk_magic() {
        let mut midi = BasicMidi::new();
        midi.tracks.push(MidiTrack::new());
        let bytes = write_midi_internal(&midi);
        assert_eq!(&bytes[14..18], b"MTrk");
    }

    #[test]
    fn test_mtrk_length_field() {
        let mut midi = BasicMidi::new();
        midi.tracks.push(MidiTrack::new());
        let bytes = write_midi_internal(&midi);
        // Empty track: only EOT (4 bytes)
        let len = u32::from_be_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]);
        assert_eq!(len, 4);
    }

    #[test]
    fn test_multiple_tracks_ordering() {
        let mut midi = BasicMidi::new();
        midi.format = MidiFormat::MultiTrack;
        midi.time_division = 480;

        // Track 0: SET_TEMPO only
        let mut t0 = MidiTrack::new();
        push_event(
            &mut t0,
            0,
            midi_message_types::SET_TEMPO,
            vec![0x07, 0xA1, 0x20],
        );
        midi.tracks.push(t0);

        // Track 1: note-on only
        let mut t1 = MidiTrack::new();
        push_event(&mut t1, 0, 0x90, vec![60, 100]);
        midi.tracks.push(t1);

        let bytes = write_midi_internal(&midi);

        // Check header says 2 tracks
        let (_, ntracks, _) = parse_mthd(&bytes);
        assert_eq!(ntracks, 2);

        // Both MTrk chunks exist
        let _t0 = find_mtrk(&bytes, 0);
        let _t1 = find_mtrk(&bytes, 1);
    }
}
