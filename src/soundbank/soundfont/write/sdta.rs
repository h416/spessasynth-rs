/// sdta.rs
/// purpose: Build the SF2 sdta LIST chunk from a sound bank's samples.
/// Ported from: src/soundbank/soundfont/write/sdta.ts
///
/// # Differences from TypeScript
///
/// - Async behaviour is removed: the function is synchronous.
/// - `compress` + `vorbisFunc` parameters are omitted (vorbis encoding is out of
///   MIDI→WAV scope and requires an unimplemented TODO crate).
/// - `progressFunc` parameter is omitted (no async progress callbacks in Rust).
/// - `decompress` parameter is kept: when `true` the sample is decoded to PCM
///   before writing (mirrors `s.setAudioData(s.getAudioData(), s.sampleRate)`).
use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::write_little_endian_indexed;
use crate::utils::loggin::spessa_synth_info;
use crate::utils::string::write_binary_string_indexed;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Byte distance from the start of the sdta LIST chunk to the first sample byte.
///
/// ```text
/// "LIST"   4 bytes
/// <size>   4 bytes   (LE u32)
/// "sdta"   4 bytes
/// "smpl"   4 bytes
/// <size>   4 bytes   (LE u32)
/// ──────────────────────────
///          20 bytes  total
/// ```
///
/// Equivalent to: `const SDTA_TO_DATA_OFFSET`
const SDTA_TO_DATA_OFFSET: usize = 4 + 4 + 4 + 4 + 4; // 20

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Builds the SF2 `sdta` LIST chunk (LIST/sdta/smpl) from the sound bank samples.
///
/// Appends one entry per sample to `smpl_start_offsets` and `smpl_end_offsets`:
/// - **SF2 (uncompressed)**: offsets are in *sample data points* (i16 units).
///   Each sample is followed by 46 zero-valued data points (= 92 bytes) as
///   required by §6.1 of the SF2.1 spec.
/// - **SF3 (Vorbis-compressed)**: offsets are in *bytes*; no padding is added.
///
/// Returns the complete raw bytes of the sdta LIST chunk.
///
/// Equivalent to: `export async function getSDTA(...): Promise<Uint8Array>`
///
/// # Parameters
/// - `bank` -- sound bank whose samples are encoded.
/// - `smpl_start_offsets` -- output: receives the inclusive start offset of every sample.
/// - `smpl_end_offsets` -- output: receives the exclusive end offset of every sample.
/// - `decompress` -- when `true`, forces Vorbis-compressed samples to be decoded
///   to PCM before writing.
pub fn get_sdta(
    bank: &mut BasicSoundBank,
    smpl_start_offsets: &mut Vec<u64>,
    smpl_end_offsets: &mut Vec<u64>,
    decompress: bool,
) -> Vec<u8> {
    let n = bank.samples.len();

    // --- Pass 1: encode each sample and accumulate the smpl chunk size ----------

    let mut smpl_chunk_size: usize = 0;
    // Collect (raw_bytes, is_compressed) so we do not borrow bank again later.
    let mut sample_infos: Vec<(Vec<u8>, bool)> = Vec::with_capacity(n);

    for (i, sample) in bank.samples.iter_mut().enumerate() {
        // Force PCM decompression when requested.
        if decompress {
            let audio = sample.get_audio_data().to_vec();
            let rate = sample.sample_rate;
            sample.set_audio_data(audio, rate);
        }

        let raw = sample.get_raw_data(true);
        let is_compressed = sample.is_compressed();
        let name = sample.name.clone();

        spessa_synth_info(&format!(
            "Encoded sample {}. {} of {}. Compressed: {}.",
            i + 1,
            name,
            n,
            is_compressed,
        ));

        // SF2.1 §6.1: each uncompressed sample must be followed by 46 zero-valued
        // sample data points (= 92 bytes).  SF3 compressed samples need no padding.
        smpl_chunk_size += raw.len() + if is_compressed { 0 } else { 92 };
        sample_infos.push((raw, is_compressed));
    }

    // smpl chunk size must be word-aligned (even number of bytes).
    #[allow(clippy::manual_is_multiple_of)]
    if smpl_chunk_size % 2 != 0 {
        smpl_chunk_size += 1;
    }

    // --- Pass 2: build the RIFF sdta LIST chunk ---------------------------------

    let total_size = smpl_chunk_size + SDTA_TO_DATA_OFFSET;
    let mut sdta = IndexedByteArray::new(total_size);

    // Header – written with the indexed helpers so current_index advances exactly
    // SDTA_TO_DATA_OFFSET (= 20) bytes.
    write_binary_string_indexed(&mut sdta, "LIST", 0);
    // LIST payload = "sdta" (4) + "smpl" (4) + smpl_size (4) + smpl_data = total - 8
    write_little_endian_indexed(
        &mut sdta,
        (smpl_chunk_size + SDTA_TO_DATA_OFFSET - 8) as u32,
        4,
    );
    write_binary_string_indexed(&mut sdta, "sdta", 0);
    write_binary_string_indexed(&mut sdta, "smpl", 0);
    write_little_endian_indexed(&mut sdta, smpl_chunk_size as u32, 4);

    // Write sample payloads and record offsets.
    let mut byte_offset: usize = 0; // current write position within the smpl data area

    for (data, is_compressed) in &sample_infos {
        let dest = byte_offset + SDTA_TO_DATA_OFFSET;
        // IndexedByteArray only implements Index<usize>; use explicit Deref to
        // access the underlying &mut [u8] slice for bulk copy.
        (&mut *sdta)[dest..dest + data.len()].copy_from_slice(data);

        let start_offset: u64;
        let end_offset: u64;

        if *is_compressed {
            // SF3: offsets in bytes, contiguous (no padding).
            start_offset = byte_offset as u64;
            end_offset = start_offset + data.len() as u64;
        } else {
            // SF2: offsets in sample data points (divide byte position by 2).
            start_offset = (byte_offset / 2) as u64; // inclusive
            end_offset = start_offset + (data.len() / 2) as u64; // exclusive
            byte_offset += 92; // 46 zero data points
        }
        byte_offset += data.len();

        smpl_start_offsets.push(start_offset);
        smpl_end_offsets.push(end_offset);
    }

    sdta.to_vec()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
    use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
    use crate::soundbank::enums::sample_types;
    use crate::utils::little_endian::read_little_endian;
    use crate::utils::string::read_binary_string;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Creates a PCM sample whose audio data consists of `num_samples` zero-value
    /// floats (encoding to s16le produces `num_samples * 2` zero bytes).
    fn make_pcm_sample(name: &str, num_samples: usize) -> BasicSample {
        let mut s = BasicSample::new(
            name.to_string(),
            44_100,
            60,
            0,
            sample_types::MONO_SAMPLE,
            0,
            num_samples.saturating_sub(1) as u32,
        );
        s.set_audio_data(vec![0.0f32; num_samples], 44_100);
        s
    }

    /// Creates a Vorbis-compressed sample with the given raw byte payload.
    fn make_compressed_sample(name: &str, payload: Vec<u8>) -> BasicSample {
        let mut s = BasicSample::new(
            name.to_string(),
            44_100,
            60,
            0,
            sample_types::MONO_SAMPLE,
            0,
            0,
        );
        s.set_compressed_data(payload);
        s
    }

    // -----------------------------------------------------------------------
    // Header structure
    // -----------------------------------------------------------------------

    #[test]
    fn test_header_starts_with_list() {
        let mut bank = BasicSoundBank::default();
        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);
        assert_eq!(&out[0..4], b"LIST");
    }

    #[test]
    fn test_header_contains_sdta_tag() {
        let mut bank = BasicSoundBank::default();
        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);
        assert_eq!(&out[8..12], b"sdta");
    }

    #[test]
    fn test_header_contains_smpl_tag() {
        let mut bank = BasicSoundBank::default();
        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);
        assert_eq!(&out[12..16], b"smpl");
    }

    #[test]
    fn test_empty_bank_total_size_is_sdta_offset() {
        let mut bank = BasicSoundBank::default();
        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);
        // No samples → smpl_chunk_size = 0 → total = SDTA_TO_DATA_OFFSET
        assert_eq!(out.len(), SDTA_TO_DATA_OFFSET);
    }

    #[test]
    fn test_empty_bank_smpl_chunk_size_is_zero() {
        let mut bank = BasicSoundBank::default();
        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);
        let smpl_size = read_little_endian(&out, 4, 16);
        assert_eq!(smpl_size, 0);
    }

    #[test]
    fn test_list_size_field_is_total_minus_8() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A", 10)); // 10 * 2 = 20 bytes + 92 padding = 112
        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);
        let list_size = read_little_endian(&out, 4, 4);
        assert_eq!(list_size as usize, out.len() - 8);
    }

    #[test]
    fn test_smpl_size_matches_sample_data() {
        // 4 PCM samples → 8 bytes raw + 92 padding = 100
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A", 4));
        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);
        let smpl_size = read_little_endian(&out, 4, 16) as usize;
        assert_eq!(smpl_size, 4 * 2 + 92); // 8 + 92 = 100
    }

    // -----------------------------------------------------------------------
    // Sample data placement
    // -----------------------------------------------------------------------

    #[test]
    fn test_sample_data_placed_at_sdta_offset() {
        let num = 4usize;
        let mut bank = BasicSoundBank::default();
        // Non-zero audio to distinguish from padding zeros
        let mut s = BasicSample::new(
            "S".to_string(),
            44_100,
            60,
            0,
            sample_types::MONO_SAMPLE,
            0,
            num as u32 - 1,
        );
        // Use 0.5 so all encoded bytes are non-zero (0x00 0x40 per sample)
        s.set_audio_data(vec![0.5f32; num], 44_100);
        bank.samples.push(s);

        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);

        // The first 2 bytes at SDTA_TO_DATA_OFFSET should be 0x00, 0x40 (0.5 → 16384 LE)
        assert_eq!(out[SDTA_TO_DATA_OFFSET], 0x00);
        assert_eq!(out[SDTA_TO_DATA_OFFSET + 1], 0x40);
    }

    #[test]
    fn test_padding_zeros_after_uncompressed_sample() {
        let num = 2usize;
        let mut bank = BasicSoundBank::default();
        let mut s = BasicSample::new(
            "S".to_string(),
            44_100,
            60,
            0,
            sample_types::MONO_SAMPLE,
            0,
            0,
        );
        s.set_audio_data(vec![0.0f32; num], 44_100);
        bank.samples.push(s);

        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);

        // raw data = 4 bytes (2 samples × 2), then 92 zero-padding bytes
        let data_end = SDTA_TO_DATA_OFFSET + num * 2;
        let padding_end = data_end + 92;
        for i in data_end..padding_end {
            assert_eq!(out[i], 0, "padding byte at position {i} should be zero");
        }
    }

    #[test]
    fn test_compressed_sample_placed_without_padding() {
        let payload = vec![0xABu8, 0xCD, 0xEF];
        let mut bank = BasicSoundBank::default();
        bank.samples
            .push(make_compressed_sample("V", payload.clone()));

        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);

        let smpl_size = read_little_endian(&out, 4, 16) as usize;
        // No padding: smpl size should equal payload length (even → 4 due to alignment)
        // Actually 3 bytes → padded to 4 (odd → +1)
        assert_eq!(smpl_size, 4); // 3 padded to 4

        // Payload bytes placed at SDTA_TO_DATA_OFFSET
        assert_eq!(
            &out[SDTA_TO_DATA_OFFSET..SDTA_TO_DATA_OFFSET + 3],
            &payload[..]
        );
    }

    // -----------------------------------------------------------------------
    // Offset vectors – SF2 (uncompressed)
    // -----------------------------------------------------------------------

    #[test]
    fn test_sf2_single_sample_start_offset_is_zero() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A", 10));
        let mut starts = vec![];
        let mut ends = vec![];
        get_sdta(&mut bank, &mut starts, &mut ends, false);
        assert_eq!(starts[0], 0);
    }

    #[test]
    fn test_sf2_single_sample_end_offset_is_num_samples() {
        // 10 float32 samples → 20 raw bytes → end offset = 20/2 = 10
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A", 10));
        let mut starts = vec![];
        let mut ends = vec![];
        get_sdta(&mut bank, &mut starts, &mut ends, false);
        assert_eq!(ends[0], 10);
    }

    #[test]
    fn test_sf2_second_sample_start_offset_accounts_for_padding() {
        // First sample: 4 float32 → 8 raw bytes + 92 padding = 100 bytes
        // Second sample start in sample points: 100 / 2 = 50
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A", 4));
        bank.samples.push(make_pcm_sample("B", 6));
        let mut starts = vec![];
        let mut ends = vec![];
        get_sdta(&mut bank, &mut starts, &mut ends, false);
        assert_eq!(starts[1], 50); // (8 + 92) / 2 = 50
    }

    #[test]
    fn test_sf2_second_sample_end_offset() {
        // Second sample: 6 float32 → 12 raw bytes → end = 50 + 12/2 = 56
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A", 4));
        bank.samples.push(make_pcm_sample("B", 6));
        let mut starts = vec![];
        let mut ends = vec![];
        get_sdta(&mut bank, &mut starts, &mut ends, false);
        assert_eq!(ends[1], 56); // 50 + 6
    }

    #[test]
    fn test_sf2_offsets_count_matches_sample_count() {
        let mut bank = BasicSoundBank::default();
        for i in 0..5 {
            bank.samples.push(make_pcm_sample(&format!("S{i}"), 10));
        }
        let mut starts = vec![];
        let mut ends = vec![];
        get_sdta(&mut bank, &mut starts, &mut ends, false);
        assert_eq!(starts.len(), 5);
        assert_eq!(ends.len(), 5);
    }

    // -----------------------------------------------------------------------
    // Offset vectors – SF3 (compressed)
    // -----------------------------------------------------------------------

    #[test]
    fn test_sf3_single_sample_start_offset_is_zero() {
        let mut bank = BasicSoundBank::default();
        bank.samples
            .push(make_compressed_sample("V", vec![1, 2, 3, 4]));
        let mut starts = vec![];
        let mut ends = vec![];
        get_sdta(&mut bank, &mut starts, &mut ends, false);
        assert_eq!(starts[0], 0);
    }

    #[test]
    fn test_sf3_single_sample_end_offset_is_byte_length() {
        let mut bank = BasicSoundBank::default();
        bank.samples
            .push(make_compressed_sample("V", vec![1, 2, 3, 4]));
        let mut starts = vec![];
        let mut ends = vec![];
        get_sdta(&mut bank, &mut starts, &mut ends, false);
        assert_eq!(ends[0], 4);
    }

    #[test]
    fn test_sf3_second_sample_start_follows_first() {
        // First compressed: 4 bytes, second starts at byte 4
        let mut bank = BasicSoundBank::default();
        bank.samples
            .push(make_compressed_sample("V1", vec![1, 2, 3, 4]));
        bank.samples.push(make_compressed_sample("V2", vec![5, 6]));
        let mut starts = vec![];
        let mut ends = vec![];
        get_sdta(&mut bank, &mut starts, &mut ends, false);
        assert_eq!(starts[1], 4);
        assert_eq!(ends[1], 6);
    }

    // -----------------------------------------------------------------------
    // Word-alignment of odd smpl chunk size
    // -----------------------------------------------------------------------

    #[test]
    fn test_odd_smpl_chunk_size_padded_to_even() {
        // 3-byte compressed payload → smpl_chunk_size = 3 → padded to 4
        let mut bank = BasicSoundBank::default();
        bank.samples
            .push(make_compressed_sample("V", vec![1, 2, 3]));
        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);
        let smpl_size = read_little_endian(&out, 4, 16) as usize;
        assert_eq!(smpl_size % 2, 0);
        assert_eq!(smpl_size, 4); // 3 → 4
    }

    #[test]
    fn test_even_smpl_chunk_size_not_changed() {
        // 4-byte compressed payload → smpl_chunk_size = 4 (already even)
        let mut bank = BasicSoundBank::default();
        bank.samples
            .push(make_compressed_sample("V", vec![1, 2, 3, 4]));
        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);
        let smpl_size = read_little_endian(&out, 4, 16) as usize;
        assert_eq!(smpl_size, 4);
    }

    // -----------------------------------------------------------------------
    // decompress flag
    // -----------------------------------------------------------------------

    #[test]
    fn test_decompress_flag_converts_compressed_to_pcm() {
        // A compressed sample with audio_data set (via decode stub) should be
        // re-written as s16le when decompress=true.
        let mut bank = BasicSoundBank::default();
        let mut s = make_compressed_sample("V", vec![0xFF, 0xFF]);
        // Manually set audio_data so get_audio_data() returns something sensible.
        s.audio_data = Some(vec![0.0f32; 2]);
        bank.samples.push(s);

        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, true);

        // After decompression the sample is treated as PCM.
        // 2 float32 → 4 bytes raw + 92 padding = 96 smpl bytes (even).
        let smpl_size = read_little_endian(&out, 4, 16) as usize;
        assert_eq!(smpl_size, 4 + 92); // 96
    }

    // -----------------------------------------------------------------------
    // sdta_to_data_offset constant
    // -----------------------------------------------------------------------

    #[test]
    fn test_sdta_to_data_offset_is_20() {
        assert_eq!(SDTA_TO_DATA_OFFSET, 20);
    }

    // -----------------------------------------------------------------------
    // Total output length
    // -----------------------------------------------------------------------

    #[test]
    fn test_output_length_equals_header_plus_smpl_data() {
        let num = 8usize;
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A", num));
        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);
        let expected = SDTA_TO_DATA_OFFSET + num * 2 + 92;
        assert_eq!(out.len(), expected);
    }

    #[test]
    fn test_multiple_samples_output_length() {
        // 2 PCM samples of 4 floats each
        // Each: 8 raw bytes + 92 padding = 100 → total smpl = 200
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A", 4));
        bank.samples.push(make_pcm_sample("B", 4));
        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);
        assert_eq!(out.len(), SDTA_TO_DATA_OFFSET + 200);
    }

    // -----------------------------------------------------------------------
    // Offset vectors are appended (not reset)
    // -----------------------------------------------------------------------

    #[test]
    fn test_offsets_are_appended_to_existing_vecs() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A", 4));

        let mut starts = vec![999u64]; // pre-existing entry
        let mut ends = vec![888u64];
        get_sdta(&mut bank, &mut starts, &mut ends, false);

        // Original entries must be preserved
        assert_eq!(starts[0], 999);
        assert_eq!(ends[0], 888);
        // New entry appended
        assert_eq!(starts.len(), 2);
        assert_eq!(ends.len(), 2);
    }

    // -----------------------------------------------------------------------
    // String tags at expected byte positions
    // -----------------------------------------------------------------------

    #[test]
    fn test_header_bytes_are_consistent() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("Z", 2));
        let mut starts = vec![];
        let mut ends = vec![];
        let out = get_sdta(&mut bank, &mut starts, &mut ends, false);

        assert_eq!(read_binary_string(&out, 4, 0), "LIST");
        assert_eq!(read_binary_string(&out, 4, 8), "sdta");
        assert_eq!(read_binary_string(&out, 4, 12), "smpl");
    }
}
