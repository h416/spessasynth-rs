/// dls_verifier.rs
/// purpose: Shared validation helpers for DLS file parsing.
/// Ported from: src/soundbank/downloadable_sounds/dls_verifier.ts
///
/// TypeScript uses an abstract class with protected static methods.
/// Rust has no abstract classes; equivalent logic is expressed as module-level pub functions.
/// TypeScript's `throw new Error(...)` maps to `Err(String)` in Rust.
use crate::utils::loggin::spessa_synth_group_end;
use crate::utils::riff_chunk::{RIFFChunk, read_riff_chunk};
use crate::utils::string::read_binary_string_indexed;

/// Assembles a DLS parse error message and closes the group log.
/// The caller should return the error as `Err(parsing_error(...))`.
/// Equivalent to: `DLSVerifier.parsingError(error)`
pub fn parsing_error(error: &str) -> String {
    spessa_synth_group_end();
    format!("DLS parse error: {error} The file may be corrupted.")
}

/// Validates that `chunk.header` matches any of the `expected` values (case-insensitive).
/// Returns `Err` if no match is found.
/// Equivalent to: `DLSVerifier.verifyHeader(chunk, ...expected)`
pub fn verify_header(chunk: &RIFFChunk, expected: &[&str]) -> Result<(), String> {
    let header_lc = chunk.header.to_lowercase();
    for &exp in expected {
        if header_lc == exp.to_lowercase() {
            return Ok(());
        }
    }
    let expected_str = expected.join(", or ");
    Err(parsing_error(&format!(
        "Invalid DLS chunk header! Expected \"{expected_str}\" got \"{header_lc}\""
    )))
}

/// Validates that `text` matches any of the `expected` values (case-insensitive).
/// Returns `Err` if no match is found.
/// Equivalent to: `DLSVerifier.verifyText(text, ...expected)`
pub fn verify_text(text: &str, expected: &[&str]) -> Result<(), String> {
    let text_lc = text.to_lowercase();
    for &exp in expected {
        if text_lc == exp.to_lowercase() {
            return Ok(());
        }
    }
    let expected_str = expected.join(", or ");
    Err(parsing_error(&format!(
        "FourCC error: Expected \"{expected_str}\" got \"{text_lc}\""
    )))
}

/// Validates the header and list type of a LIST chunk, reads all sub-chunks, and returns them.
/// Equivalent to: `DLSVerifier.verifyAndReadList(chunk, ...type)`
pub fn verify_and_read_list(
    chunk: &mut RIFFChunk,
    types: &[&str],
) -> Result<Vec<RIFFChunk>, String> {
    verify_header(chunk, &["LIST"])?;
    chunk.data.current_index = 0;
    let list_type = read_binary_string_indexed(&mut chunk.data, 4);
    verify_text(&list_type, types)?;
    let mut chunks: Vec<RIFFChunk> = Vec::new();
    while chunk.data.len() > chunk.data.current_index {
        chunks.push(read_riff_chunk(&mut chunk.data, true, false));
    }
    Ok(chunks)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::indexed_array::IndexedByteArray;
    use crate::utils::riff_chunk::RIFFChunk;

    /// Creates a RIFFChunk with the given header and data.
    fn make_chunk(header: &str, data: &[u8]) -> RIFFChunk {
        RIFFChunk::new(
            header.to_string(),
            data.len() as u32,
            IndexedByteArray::from_vec(data.to_vec()),
        )
    }

    /// Generates the byte sequence for a RIFF sub-chunk (including even-byte padding).
    fn make_sub_chunk_bytes(header: &str, data: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(header.as_bytes());
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(data);
        if data.len() % 2 != 0 {
            bytes.push(0); // padding
        }
        bytes
    }

    /// Creates a LIST chunk. data = [list_type 4B][sub_chunks_bytes]
    fn make_list_chunk(list_type: &str, sub_chunks_bytes: &[u8]) -> RIFFChunk {
        let mut data = Vec::new();
        data.extend_from_slice(list_type.as_bytes());
        data.extend_from_slice(sub_chunks_bytes);
        RIFFChunk::new(
            "LIST".to_string(),
            data.len() as u32,
            IndexedByteArray::from_vec(data),
        )
    }

    // --- parsing_error ---

    #[test]
    fn test_parsing_error_message_format() {
        let msg = parsing_error("something went wrong");
        assert_eq!(
            msg,
            "DLS parse error: something went wrong The file may be corrupted."
        );
    }

    #[test]
    fn test_parsing_error_empty_string() {
        let msg = parsing_error("");
        assert!(msg.starts_with("DLS parse error:"));
        assert!(msg.ends_with("The file may be corrupted."));
    }

    // --- verify_header ---

    #[test]
    fn test_verify_header_match() {
        let chunk = make_chunk("LIST", &[]);
        assert!(verify_header(&chunk, &["LIST"]).is_ok());
    }

    #[test]
    fn test_verify_header_mismatch() {
        let chunk = make_chunk("data", &[]);
        let result = verify_header(&chunk, &["LIST"]);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("Invalid DLS chunk header!"));
        assert!(msg.contains("LIST"));
        assert!(msg.contains("data"));
    }

    #[test]
    fn test_verify_header_case_insensitive() {
        // matches even if chunk.header is uppercase
        let chunk = make_chunk("RIFF", &[]);
        assert!(verify_header(&chunk, &["riff"]).is_ok());
    }

    #[test]
    fn test_verify_header_expected_case_insensitive() {
        // matches even if expected is uppercase and chunk.header is lowercase
        let chunk = make_chunk("list", &[]);
        assert!(verify_header(&chunk, &["LIST"]).is_ok());
    }

    #[test]
    fn test_verify_header_multiple_expected_first_matches() {
        let chunk = make_chunk("fmt ", &[]);
        assert!(verify_header(&chunk, &["fmt ", "data"]).is_ok());
    }

    #[test]
    fn test_verify_header_multiple_expected_second_matches() {
        let chunk = make_chunk("data", &[]);
        assert!(verify_header(&chunk, &["fmt ", "data"]).is_ok());
    }

    #[test]
    fn test_verify_header_multiple_expected_none_match() {
        let chunk = make_chunk("shdr", &[]);
        let result = verify_header(&chunk, &["fmt ", "data"]);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("fmt , or data"));
        assert!(msg.contains("shdr"));
    }

    #[test]
    fn test_verify_header_error_contains_dls_parse_error() {
        let chunk = make_chunk("xxxx", &[]);
        let msg = verify_header(&chunk, &["LIST"]).unwrap_err();
        assert!(msg.contains("DLS parse error:"));
        assert!(msg.contains("The file may be corrupted."));
    }

    // --- verify_text ---

    #[test]
    fn test_verify_text_match() {
        assert!(verify_text("dlsd", &["dlsd"]).is_ok());
    }

    #[test]
    fn test_verify_text_mismatch() {
        let result = verify_text("wvpl", &["lins"]);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("FourCC error:"));
        assert!(msg.contains("lins"));
        assert!(msg.contains("wvpl"));
    }

    #[test]
    fn test_verify_text_case_insensitive_text_lower() {
        assert!(verify_text("dls ", &["DLS "]).is_ok());
    }

    #[test]
    fn test_verify_text_case_insensitive_expected_lower() {
        assert!(verify_text("DLS ", &["dls "]).is_ok());
    }

    #[test]
    fn test_verify_text_multiple_expected_matches_first() {
        assert!(verify_text("lins", &["lins", "wvpl"]).is_ok());
    }

    #[test]
    fn test_verify_text_multiple_expected_matches_second() {
        assert!(verify_text("wvpl", &["lins", "wvpl"]).is_ok());
    }

    #[test]
    fn test_verify_text_multiple_expected_none_match() {
        let result = verify_text("rgnh", &["lins", "wvpl"]);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("lins, or wvpl"));
        assert!(msg.contains("rgnh"));
    }

    #[test]
    fn test_verify_text_error_contains_dls_parse_error() {
        let msg = verify_text("xxxx", &["yyyy"]).unwrap_err();
        assert!(msg.contains("DLS parse error:"));
        assert!(msg.contains("The file may be corrupted."));
    }

    // --- verify_and_read_list ---

    #[test]
    fn test_verify_and_read_list_empty_list() {
        // LIST chunk with no sub-chunks
        let mut chunk = make_list_chunk("wvpl", &[]);
        let result = verify_and_read_list(&mut chunk, &["wvpl"]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_verify_and_read_list_wrong_header() {
        // header is not LIST → Err
        let mut chunk = make_chunk("RIFF", &[b'w', b'v', b'p', b'l']);
        let result = verify_and_read_list(&mut chunk, &["wvpl"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid DLS chunk header!"));
    }

    #[test]
    fn test_verify_and_read_list_wrong_list_type() {
        // LIST chunk but list_type does not match expected → Err
        let mut chunk = make_list_chunk("lins", &[]);
        let result = verify_and_read_list(&mut chunk, &["wvpl"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("FourCC error:"));
    }

    #[test]
    fn test_verify_and_read_list_reads_one_subchunk() {
        let sub = make_sub_chunk_bytes("wsmp", &[1, 2, 3, 4]);
        let mut chunk = make_list_chunk("wvpl", &sub);
        let result = verify_and_read_list(&mut chunk, &["wvpl"]);
        assert!(result.is_ok());
        let chunks = result.unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].header, "wsmp");
        assert_eq!(chunks[0].size, 4);
    }

    #[test]
    fn test_verify_and_read_list_reads_multiple_subchunks() {
        let mut sub = make_sub_chunk_bytes("rgnh", &[0xAA, 0xBB]);
        sub.extend(make_sub_chunk_bytes("wlnk", &[0xCC, 0xDD]));
        let mut chunk = make_list_chunk("lrgn", &sub);
        let result = verify_and_read_list(&mut chunk, &["lrgn"]);
        assert!(result.is_ok());
        let chunks = result.unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].header, "rgnh");
        assert_eq!(chunks[1].header, "wlnk");
    }

    #[test]
    fn test_verify_and_read_list_multiple_accepted_types() {
        // when multiple types are passed, it is OK if any one matches
        let mut chunk = make_list_chunk("lart", &[]);
        let result = verify_and_read_list(&mut chunk, &["lart", "lar2"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_and_read_list_resets_cursor_to_zero() {
        // verify_and_read_list resets current_index to 0 internally before reading
        let sub = make_sub_chunk_bytes("dlid", &[1, 2]);
        let mut chunk = make_list_chunk("dls ", &sub);
        // advance the cursor beforehand
        chunk.data.current_index = 999;
        let result = verify_and_read_list(&mut chunk, &["dls "]);
        // cursor should be reset and data read correctly
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }
}
