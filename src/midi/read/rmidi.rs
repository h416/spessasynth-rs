/// midi_loader.rs
/// purpose: MIDI file (SMF, RMIDI) parser.
/// XMF format is not supported (panics with unimplemented!).
/// Ported from: src/midi/midi_loader.ts
use crate::midi::basic_midi::BasicMidi;
use crate::utils::byte_functions::little_endian::read_little_endian;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::loggin::{spessa_synth_group_end, spessa_synth_info, spessa_synth_warn};
use crate::utils::riff_chunk::read_riff_chunk;
use crate::utils::byte_functions::string::read_binary_string_indexed;

// ─────────────────────────────────────────────────────────────────────────────
// RMIDI (RIFF MIDI) container parsing
// ─────────────────────────────────────────────────────────────────────────────

/// Parses the RMIDI (RIFF MIDI) container starting right after the initial
/// "RIFF" FourCC has been peeked: validates the RMID header, extracts the raw
/// SMF sub-chunk, and scans the remaining RIFF chunks for an embedded sound
/// bank and INFO metadata. Returns the extracted SMF binary data for further
/// parsing by the SMF chunk parser.
///
/// Equivalent to the "RIFF" branch of: loadMIDIFromArrayBufferInternal(outputMIDI, arrayBuffer, fileName)
pub(crate) fn parse_rmidi_container_internal(
    output_midi: &mut BasicMidi,
    binary_data: &mut IndexedByteArray,
) -> Result<IndexedByteArray, String> {
    // ── RMIDI (Resource-Interchangeable MIDI) ─────────────────────
    // Skip "RIFF" FourCC (4 B) + outer RIFF size (4 B).
    binary_data.current_index += 8;

    let rmid = read_binary_string_indexed(binary_data, 4);
    if rmid != "RMID" {
        spessa_synth_group_end();
        return Err(format!(
            "Invalid RMIDI Header! Expected \"RMID\", got \"{}\"",
            rmid
        ));
    }

    // The first sub-chunk must be "data" and contains the raw SMF bytes.
    let data_chunk = read_riff_chunk(binary_data, true, false);
    if data_chunk.header != "data" {
        spessa_synth_group_end();
        return Err(format!(
            "Invalid RMIDI Chunk header! Expected \"data\", got \"{}\"",
            data_chunk.header
        ));
    }
    let smf_file_binary = data_chunk.data;

    let mut is_sf2_rmidi = false;
    let mut found_dbnk = false;

    // Scan remaining RMIDI chunks for embedded sound banks and INFO metadata.
    while binary_data.current_index < binary_data.len() {
        let start_index = binary_data.current_index;
        let mut current_chunk = read_riff_chunk(binary_data, true, false);

        if current_chunk.header == "RIFF" {
            // The embedded chunk type is the first 4 bytes of the chunk data.
            let chunk_type =
                read_binary_string_indexed(&mut current_chunk.data, 4).to_lowercase();

            if chunk_type == "sfbk" || chunk_type == "sfpk" || chunk_type == "dls " {
                spessa_synth_info("Found embedded soundbank!");
                // Extract the complete embedded RIFF chunk bytes.
                // Note: matches TypeScript's slice(startIndex, startIndex + chunk.size).
                let end = (start_index + current_chunk.size as usize).min(binary_data.len());
                output_midi.embedded_sound_bank =
                    Some(binary_data.slice(start_index, end).to_vec());
            } else {
                spessa_synth_warn(&format!("Unknown RIFF chunk: \"{}\"", chunk_type));
            }

            if chunk_type == "dls " {
                output_midi.is_dls_rmidi = true;
            } else {
                is_sf2_rmidi = true;
            }
        } else if current_chunk.header == "LIST" {
            let list_type =
                read_binary_string_indexed(&mut current_chunk.data, 4);

            if list_type == "INFO" {
                spessa_synth_info("Found RMIDI INFO chunk!");
                // Iterate sub-chunks inside the INFO list.
                while current_chunk.data.current_index < current_chunk.data.len() {
                    let info_chunk =
                        read_riff_chunk(&mut current_chunk.data, true, false);
                    let info_data: Vec<u8> = info_chunk.data.to_vec();

                    match info_chunk.header.as_str() {
                        "INAM" => {
                            output_midi
                                .rmidi_info
                                .insert("name".to_string(), info_data);
                        }
                        // Two possible FourCCs for album
                        "IALB" | "IPRD" => {
                            output_midi
                                .rmidi_info
                                .insert("album".to_string(), info_data);
                        }
                        // Older spessasynth wrote ICRT instead of ICRD
                        "ICRT" | "ICRD" => {
                            output_midi
                                .rmidi_info
                                .insert("creationDate".to_string(), info_data);
                        }
                        "IART" => {
                            output_midi
                                .rmidi_info
                                .insert("artist".to_string(), info_data);
                        }
                        "IGNR" => {
                            output_midi
                                .rmidi_info
                                .insert("genre".to_string(), info_data);
                        }
                        "IPIC" => {
                            output_midi
                                .rmidi_info
                                .insert("picture".to_string(), info_data);
                        }
                        "ICOP" => {
                            output_midi
                                .rmidi_info
                                .insert("copyright".to_string(), info_data);
                        }
                        "ICMT" => {
                            output_midi
                                .rmidi_info
                                .insert("comment".to_string(), info_data);
                        }
                        "IENG" => {
                            output_midi
                                .rmidi_info
                                .insert("engineer".to_string(), info_data);
                        }
                        "ISFT" => {
                            output_midi
                                .rmidi_info
                                .insert("software".to_string(), info_data);
                        }
                        "ISBJ" => {
                            output_midi
                                .rmidi_info
                                .insert("subject".to_string(), info_data);
                        }
                        "IENC" => {
                            output_midi
                                .rmidi_info
                                .insert("infoEncoding".to_string(), info_data);
                        }
                        "MENC" => {
                            output_midi
                                .rmidi_info
                                .insert("midiEncoding".to_string(), info_data);
                        }
                        "DBNK" => {
                            if info_data.len() >= 2 {
                                output_midi.bank_offset =
                                    read_little_endian(&info_data, 2, 0);
                            }
                            found_dbnk = true;
                        }
                        _ => {
                            spessa_synth_warn(&format!(
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
        output_midi.bank_offset = 1; // SF2 RMIDI default
    }
    if output_midi.is_dls_rmidi {
        output_midi.bank_offset = 0;
    }
    if output_midi.embedded_sound_bank.is_none() {
        output_midi.bank_offset = 0;
    }

    Ok(smf_file_binary)
}
