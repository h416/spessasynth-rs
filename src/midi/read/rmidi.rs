/// rmidi.rs
/// purpose: RIFF MIDI (RMIDI) container parser.
/// Ported from: src/midi/read/rmidi.ts (spessasynth_core 4.3.0)
///
/// TS 4.3.0 split the monolithic `midi_loader.ts` (4.2.0) into `read/midi.ts` (plain SMF
/// parsing), `read/rmidi.ts` (this file, RIFF/RMIDI container parsing), and `read/xmf.ts` (XMF,
/// not ported — out of scope). `parseRMIDIInternal` (this file's `parse_rmidi_internal`) is
/// called directly by `BasicMIDI.fromArrayBuffer` (see `basic_midi.rs`) when the input starts
/// with the "RIFF" FourCC, and at the very end of its own parsing calls `parseSMFInternal`
/// (`read/midi.rs`'s `parse_smf_internal`) to parse the embedded SMF bytes it just extracted
/// from the RIFF `data` chunk — the reverse of this Rust port's previous (phase-1) structure,
/// where `read/midi.rs` was the dispatcher and called into this file; that direction is
/// corrected here to match the real TS 4.3.0 call graph.
///
/// Besides the restructuring, this file's actual RMIDI-chunk-scanning logic is unchanged from
/// 4.2.0: only the utility APIs it calls were renamed in 4.3.0 — `readRIFFChunk` became
/// `RIFFChunk.read` (migrated below) and the free logging functions became `SpessaLog::` static
/// methods (migrated below).
use crate::midi::basic_midi::BasicMidi;
use crate::midi::read::midi::parse_smf_internal;
use crate::midi::types::RMIDInfoFourCC;
use crate::utils::byte_functions::little_endian::read_little_endian;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::loggin::SpessaLog;
use crate::utils::riff_chunk::RIFFChunk;
use crate::utils::byte_functions::string::read_binary_string_indexed;

// ─────────────────────────────────────────────────────────────────────────────
// RMIDI (RIFF MIDI) container parsing
// ─────────────────────────────────────────────────────────────────────────────

/// Loads a RIFF MIDI File (RMIDI) from given binary data.
///
/// `binary_data` must be positioned right after the initial "RIFF" FourCC has been peeked (i.e.
/// `current_index` unchanged from the caller's peek, per `BasicMidi::from_array_buffer`'s
/// contract): this function itself skips the "RIFF" FourCC + outer RIFF size (8 bytes total)
/// before reading "RMID". Validates the RMID header, extracts the raw SMF sub-chunk, scans the
/// remaining RIFF chunks for an embedded sound bank and INFO metadata, then hands the extracted
/// SMF bytes off to `parse_smf_internal` for the actual MIDI event parsing.
///
/// Equivalent to: parseRMIDIInternal(outputMIDI, binaryData, fileName)
pub fn parse_rmidi_internal(
    output_midi: &mut BasicMidi,
    binary_data: &mut IndexedByteArray,
    file_name: &str,
) -> Result<(), String> {
    // https://github.com/spessasus/sf2-rmidi-specification#readme
    // Skip size (we already verified "RIFF" if we're here).
    binary_data.current_index += 8;

    let rmid = read_binary_string_indexed(binary_data, 4);
    if rmid != "RMID" {
        SpessaLog::group_end();
        return Err(format!(
            "Invalid RMIDI Header! Expected \"RMID\", got \"{}\"",
            rmid
        ));
    }

    // The first sub-chunk must be "data" and contains the raw SMF bytes.
    let data_chunk = RIFFChunk::read(binary_data, true, false);
    if data_chunk.header != "data" {
        SpessaLog::group_end();
        return Err(format!(
            "Invalid RMIDI Chunk header! Expected \"data\", got \"{}\"",
            data_chunk.header
        ));
    }
    let mut smf_file_binary = data_chunk.data;

    let mut is_sf2_rmidi = false;
    let mut found_dbnk = false;

    // Scan remaining RMIDI chunks for embedded sound banks and INFO metadata.
    while binary_data.current_index < binary_data.len() {
        let start_index = binary_data.current_index;
        let mut current_chunk = RIFFChunk::read(binary_data, true, false);

        if current_chunk.header == "RIFF" {
            // The embedded chunk type is the first 4 bytes of the chunk data.
            let chunk_type =
                read_binary_string_indexed(&mut current_chunk.data, 4).to_lowercase();

            if chunk_type == "sfbk" || chunk_type == "sfpk" || chunk_type == "dls " {
                SpessaLog::info("Found embedded soundbank!");
                // Extract the complete embedded RIFF chunk bytes.
                // Note: matches TypeScript's slice(startIndex, startIndex + chunk.size).
                let end = (start_index + current_chunk.size as usize).min(binary_data.len());
                output_midi.embedded_sound_bank =
                    Some(binary_data.slice(start_index, end).to_vec());
            } else {
                SpessaLog::warn(&format!("Unknown RIFF chunk: \"{}\"", chunk_type));
            }

            if chunk_type == "dls " {
                // Assume bank offset of 0 by default. If we find any bank selects, then the
                // offset is 1.
                output_midi.is_dls_rmidi = true;
            } else {
                is_sf2_rmidi = true;
            }
        } else if current_chunk.header == "LIST" {
            let list_type = read_binary_string_indexed(&mut current_chunk.data, 4);

            if list_type == "INFO" {
                SpessaLog::info("Found RMIDI INFO chunk!");
                // Iterate sub-chunks inside the INFO list.
                while current_chunk.data.current_index < current_chunk.data.len() {
                    let info_chunk = RIFFChunk::read(&mut current_chunk.data, true, false);
                    let info_data: Vec<u8> = info_chunk.data.to_vec();

                    let header_typed = RMIDInfoFourCC::from_str(&info_chunk.header);
                    match header_typed {
                        Some(RMIDInfoFourCC::Inam) => {
                            output_midi
                                .rmidi_info
                                .insert("name".to_string(), info_data);
                        }
                        // Two possible FourCCs for album
                        Some(RMIDInfoFourCC::Ialb | RMIDInfoFourCC::Iprd) => {
                            output_midi
                                .rmidi_info
                                .insert("album".to_string(), info_data);
                        }
                        // Older spessasynth wrote ICRT instead of ICRD
                        Some(RMIDInfoFourCC::Icrt | RMIDInfoFourCC::Icrd) => {
                            output_midi
                                .rmidi_info
                                .insert("creationDate".to_string(), info_data);
                        }
                        Some(RMIDInfoFourCC::Iart) => {
                            output_midi
                                .rmidi_info
                                .insert("artist".to_string(), info_data);
                        }
                        Some(RMIDInfoFourCC::Ignr) => {
                            output_midi
                                .rmidi_info
                                .insert("genre".to_string(), info_data);
                        }
                        Some(RMIDInfoFourCC::Ipic) => {
                            output_midi
                                .rmidi_info
                                .insert("picture".to_string(), info_data);
                        }
                        Some(RMIDInfoFourCC::Icop) => {
                            output_midi
                                .rmidi_info
                                .insert("copyright".to_string(), info_data);
                        }
                        Some(RMIDInfoFourCC::Icmt) => {
                            output_midi
                                .rmidi_info
                                .insert("comment".to_string(), info_data);
                        }
                        Some(RMIDInfoFourCC::Ieng) => {
                            output_midi
                                .rmidi_info
                                .insert("engineer".to_string(), info_data);
                        }
                        Some(RMIDInfoFourCC::Isft) => {
                            output_midi
                                .rmidi_info
                                .insert("software".to_string(), info_data);
                        }
                        Some(RMIDInfoFourCC::Isbj) => {
                            output_midi
                                .rmidi_info
                                .insert("subject".to_string(), info_data);
                        }
                        Some(RMIDInfoFourCC::Ienc) => {
                            output_midi
                                .rmidi_info
                                .insert("infoEncoding".to_string(), info_data);
                        }
                        Some(RMIDInfoFourCC::Menc) => {
                            output_midi
                                .rmidi_info
                                .insert("midiEncoding".to_string(), info_data);
                        }
                        Some(RMIDInfoFourCC::Dbnk) => {
                            if info_data.len() >= 2 {
                                output_midi.bank_offset = read_little_endian(&info_data, 2, 0);
                            }
                            found_dbnk = true;
                        }
                        None => {
                            SpessaLog::warn(&format!(
                                "Unknown RMIDI Info: {}",
                                info_chunk.header
                            ));
                        }
                    }
                }
            }
        }
    }

    if is_sf2_rmidi && !found_dbnk {
        // Defaults to 1 according to the spec.
        output_midi.bank_offset = 1;
    }
    if output_midi.is_dls_rmidi {
        // Assume bank offset of 0 by default. If we find any bank selects (in the SMF parser),
        // then the offset is 1.
        output_midi.bank_offset = 0;
    }
    // If no embedded bank, assume 0.
    if output_midi.embedded_sound_bank.is_none() {
        output_midi.bank_offset = 0;
    }

    // Send the extracted SMF to the parser.
    parse_smf_internal(output_midi, &mut smf_file_binary, file_name)
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::byte_functions::big_endian::write_big_endian;
    use crate::utils::riff_chunk::RIFFChunk;

    /// Builds a minimal MThd + one empty MTrk SMF payload (used as the RMIDI "data" chunk).
    fn minimal_smf() -> Vec<u8> {
        let mut b = b"MThd".to_vec();
        b.extend(write_big_endian(6, 4));
        b.extend_from_slice(&[0x00, 0x00]); // format 0
        b.extend_from_slice(&[0x00, 0x01]); // 1 track
        b.extend_from_slice(&[0x01, 0xE0]); // division 480
        b.extend_from_slice(b"MTrk");
        b.extend(write_big_endian(4, 4));
        // delta=0, FF 2F 00 (EndOfTrack)
        b.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
        b
    }

    /// Builds a minimal well-formed RMIDI file: RIFF/RMID/data(SMF)/LIST-INFO(INAM).
    fn build_rmidi(name: Option<&str>) -> Vec<u8> {
        let smf = minimal_smf();
        let data_chunk = RIFFChunk::write("data", &smf, false, false).to_vec();

        let mut info_parts: Vec<u8> = Vec::new();
        if let Some(n) = name {
            let mut name_bytes = n.as_bytes().to_vec();
            name_bytes.push(0);
            info_parts.extend(RIFFChunk::write("INAM", &name_bytes, false, false).to_vec());
        }
        let info_refs: Vec<&[u8]> = vec![&info_parts];
        let list_info = RIFFChunk::write_parts("INFO", &info_refs, true).to_vec();

        let mut riff_body = Vec::new();
        riff_body.extend_from_slice(b"RMID");
        riff_body.extend_from_slice(&data_chunk);
        riff_body.extend_from_slice(&list_info);
        let riff_refs: Vec<&[u8]> = vec![&riff_body];
        RIFFChunk::write_parts("RIFF", &riff_refs, false).to_vec()
    }

    fn parse(data: &[u8], file_name: &str) -> Result<BasicMidi, String> {
        let mut midi = BasicMidi::new();
        // Simulate BasicMidi::from_array_buffer's peek: current_index starts at 0.
        let mut binary_data = IndexedByteArray::from_slice(data);
        parse_rmidi_internal(&mut midi, &mut binary_data, file_name)?;
        Ok(midi)
    }

    #[test]
    fn test_parse_rmidi_valid_file_ok() {
        let rmidi = build_rmidi(Some("Test Song"));
        let midi = parse(&rmidi, "test.rmi").unwrap();
        assert_eq!(midi.tracks.len(), 1);
        assert_eq!(midi.time_division, 480);
    }

    #[test]
    fn test_parse_rmidi_info_name_stored() {
        let rmidi = build_rmidi(Some("Test Song"));
        let midi = parse(&rmidi, "test.rmi").unwrap();
        let name = midi.rmidi_info.get("name").expect("name should be stored");
        assert_eq!(&name[..9], b"Test Song");
    }

    #[test]
    fn test_parse_rmidi_bank_offset_defaults_zero_without_soundbank() {
        let rmidi = build_rmidi(None);
        let midi = parse(&rmidi, "test.rmi").unwrap();
        // No embedded sound bank → bank offset forced to 0.
        assert_eq!(midi.bank_offset, 0);
        assert!(midi.embedded_sound_bank.is_none());
    }

    #[test]
    fn test_parse_rmidi_bad_rmid_header_returns_err() {
        // "RIFF" + size + "BADX" instead of "RMID"
        let mut data = b"RIFF".to_vec();
        data.extend(write_big_endian(4, 4));
        data.extend_from_slice(b"BADX");
        let result = parse(&data, "bad.rmi");
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("RMID"));
    }

    #[test]
    fn test_parse_rmidi_bad_data_chunk_header_returns_err() {
        // "RIFF" + size + "RMID" + a wrong first sub-chunk header "JUNK"
        let mut data = b"RIFF".to_vec();
        data.extend(write_big_endian(100, 4));
        data.extend_from_slice(b"RMID");
        data.extend_from_slice(b"JUNK");
        data.extend(write_big_endian(0, 4));
        let result = parse(&data, "bad.rmi");
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("data"));
    }

    #[test]
    fn test_parse_rmidi_calls_through_to_smf_parser() {
        // Verify the extracted SMF actually gets parsed (flush() ran, tracks populated).
        let rmidi = build_rmidi(Some("Song"));
        let midi = parse(&rmidi, "test.rmi").unwrap();
        assert_eq!(midi.format, crate::midi::types::MidiFormat::SingleTrack);
    }

    #[test]
    fn test_parse_rmidi_file_name_passed_through() {
        let rmidi = build_rmidi(None);
        let midi = parse(&rmidi, "myfile.rmi").unwrap();
        // Since there's no track name and no rmidi INAM, file_name becomes the fallback name
        // source used elsewhere; here we simply verify it was accepted without error and the
        // MIDI parsed. (file_name itself is not stored on BasicMidi directly by parseSMFInternal
        // unless binaryName is empty — this just exercises the parameter plumbing.)
        assert_eq!(midi.tracks.len(), 1);
    }
}
