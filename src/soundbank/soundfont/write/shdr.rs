/// shdr.rs
/// purpose: Build the SF2 shdr RIFF chunk (sample header records) from a sound bank.
/// Ported from: src/soundbank/soundfont/write/shdr.ts
use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
use crate::soundbank::soundfont::read::samples::SF3_BIT_FLIT;
use crate::soundbank::soundfont::write::types::ExtendedSF2Chunks;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::{write_dword, write_word};
use crate::utils::riff_chunk::write_riff_chunk_raw;
use crate::utils::string::write_binary_string_indexed;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Byte length of a single sample header record in the shdr chunk.
/// Equivalent to: `const sampleLength = 46`
///
/// Layout (all little-endian):
/// ```text
/// [  0.. 20)  sample name (ASCII, zero-padded)
/// [ 20.. 24)  dwStart  – sample start offset (u32)
/// [ 24.. 28)  dwEnd    – sample end offset   (u32)
/// [ 28.. 32)  loopStart (u32, absolute for SF2; relative for SF3)
/// [ 32.. 36)  loopEnd   (u32, absolute for SF2; relative for SF3)
/// [ 36.. 40)  sampleRate (u32)
/// [ 40..  41)  originalKey (u8)
/// [ 41..  42)  pitchCorrection (i8 stored as u8)
/// [ 42..  44)  sampleLink (u16, low 16 bits; high 16 bits go into xshdr)
/// [ 44..  46)  sampleType (u16, with SF3_BIT_FLIT set when Vorbis-compressed)
/// ```
const SAMPLE_RECORD_SIZE: usize = 46;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Builds the SF2 `shdr` RIFF chunk from the sound bank's samples.
///
/// Returns an [`ExtendedSF2Chunks`] pair:
/// - `pdta` – standard `shdr` chunk for SF2 / SF3.
/// - `xdta` – extended-limits `shdr` chunk for the XSF2 proposal
///   (<https://github.com/spessasus/soundfont-proposals/blob/main/extended_limits.md>).
///
/// Equivalent to: `export function getSHDR(...): ExtendedSF2Chunks`
///
/// # Parameters
/// - `bank`              – sound bank whose samples are encoded.
/// - `smpl_start_offsets` – start offset per sample (indexed by sample position).
/// - `smpl_end_offsets`   – end offset per sample (indexed by sample position).
pub fn get_shdr(
    bank: &BasicSoundBank,
    smpl_start_offsets: &[u64],
    smpl_end_offsets: &[u64],
) -> ExtendedSF2Chunks {
    // +1 for the EOS (end-of-section) sentinel record.
    let shdr_size = SAMPLE_RECORD_SIZE * (bank.samples.len() + 1);
    let mut shdr_data = IndexedByteArray::new(shdr_size);
    let mut xshdr_data = IndexedByteArray::new(shdr_size);

    for (index, sample) in bank.samples.iter().enumerate() {
        // --- Sample name -------------------------------------------------------
        // shdrData:  first 20 characters (zero-padded to 20 bytes).
        let name = &sample.name;
        let name_first20 = slice_str_bytes(name, 0, 20);
        write_binary_string_indexed(&mut shdr_data, name_first20, 20);

        // xshdrData: characters 20-39 (zero-padded to 20 bytes).
        let name_from20 = slice_str_bytes(name, 20, 20);
        write_binary_string_indexed(&mut xshdr_data, name_from20, 20);

        // --- Sample start offset -----------------------------------------------
        // Stored as u32 in both shdr (standard) and xshdr (extended limits).
        // The TypeScript writes only the lower 32 bits of dwStart to shdrData and
        // leaves the xshdrData field as zero (xshdrData.currentIndex += 4).
        let dw_start = smpl_start_offsets[index] as u32; // lower 32 bits
        write_dword(&mut shdr_data, dw_start);
        xshdr_data.current_index += 4; // skip – leave zero in xshdr

        // --- Sample end offset -------------------------------------------------
        let dw_end = smpl_end_offsets[index] as u32; // lower 32 bits
        write_dword(&mut shdr_data, dw_end);
        xshdr_data.current_index += 4; // skip

        // --- Loop points -------------------------------------------------------
        // SF2: loop values are stored as absolute sample-data-point offsets
        //      (loop_start + dw_start).
        // SF3 (Vorbis-compressed): loop values remain relative to the sample start,
        //      so no offset is added.
        //
        // TypeScript:
        //   let loopStart = sample.loopStart + dwStart;
        //   if (sample.isCompressed) { loopStart -= dwStart; }
        // → simplifies to: loopStart = loopStart + (if compressed { 0 } else { dwStart })
        let is_compressed = sample.is_compressed();
        let loop_start = if is_compressed {
            sample.loop_start
        } else {
            sample.loop_start.wrapping_add(dw_start)
        };
        let loop_end = if is_compressed {
            sample.loop_end
        } else {
            sample.loop_end.wrapping_add(dw_start)
        };
        write_dword(&mut shdr_data, loop_start);
        write_dword(&mut shdr_data, loop_end);

        // --- Sample rate -------------------------------------------------------
        write_dword(&mut shdr_data, sample.sample_rate);

        // --- Original pitch and pitch correction -------------------------------
        let idx = shdr_data.current_index;
        shdr_data[idx] = sample.original_key;
        shdr_data.current_index += 1;
        let idx = shdr_data.current_index;
        shdr_data[idx] = sample.pitch_correction as u8;
        shdr_data.current_index += 1;

        // Skip loop (8) + sample_rate (4) + original_key (1) + pitch_correction (1) = 14
        // bytes in xshdrData; all fields remain zero.
        xshdr_data.current_index += 14;

        // --- Sample link index -------------------------------------------------
        // For standard SF2 (shdrData): low 16 bits of the linked-sample index.
        // For extended limits (xshdrData): high 16 bits.
        let sample_link_index: usize = sample.linked_sample_idx.unwrap_or(0);
        write_word(&mut shdr_data, (sample_link_index & 0xFFFF) as u32);
        write_word(&mut xshdr_data, (sample_link_index >> 16) as u32);

        // --- Sample type -------------------------------------------------------
        // Set SF3_BIT_FLIT when the sample is Vorbis-compressed.
        let mut sample_type = sample.sample_type;
        if is_compressed {
            sample_type |= SF3_BIT_FLIT;
        }
        write_word(&mut shdr_data, sample_type as u32);
        xshdr_data.current_index += 2; // skip sampleType in xshdr
    }

    // --- EOS sentinel record --------------------------------------------------
    // "EOS" followed by 43 zero bytes to fill the remaining 46 bytes.
    write_binary_string_indexed(&mut shdr_data, "EOS", SAMPLE_RECORD_SIZE);
    write_binary_string_indexed(&mut xshdr_data, "EOS", SAMPLE_RECORD_SIZE);

    // --- Wrap in RIFF chunks --------------------------------------------------
    let shdr = write_riff_chunk_raw("shdr", &shdr_data, false, false);
    let xshdr = write_riff_chunk_raw("shdr", &xshdr_data, false, false);

    ExtendedSF2Chunks {
        pdta: shdr,
        xdta: xshdr,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Returns a byte-level sub-slice of `s` starting at byte `start` with at most
/// `max_len` bytes, ensuring the result is valid UTF-8 (safe for ASCII names).
///
/// For SoundFont sample names (ASCII only) this is equivalent to the JavaScript
/// `String.prototype.slice(start, start + max_len)`.
fn slice_str_bytes(s: &str, start: usize, max_len: usize) -> &str {
    let bytes = s.as_bytes();
    if start >= bytes.len() {
        return "";
    }
    let end = (start + max_len).min(bytes.len());
    // SAFETY: SF2 sample names are ASCII, so any byte boundary is valid UTF-8.
    // In the unlikely case of non-ASCII we fall back to the empty string.
    std::str::from_utf8(&bytes[start..end]).unwrap_or("")
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
    use crate::soundbank::soundfont::read::samples::SF3_BIT_FLIT;
    use crate::utils::little_endian::read_little_endian;
    use crate::utils::riff_chunk::read_riff_chunk;
    use crate::utils::string::read_binary_string;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Creates a minimal PCM sample with the given name.
    fn make_pcm_sample(name: &str) -> BasicSample {
        let mut s = BasicSample::new(
            name.to_string(),
            44_100,
            60,
            0,
            sample_types::MONO_SAMPLE,
            10,
            100,
        );
        s.set_audio_data(vec![0.0f32; 10], 44_100);
        s
    }

    /// Creates a Vorbis-compressed sample with the given name.
    fn make_compressed_sample(name: &str) -> BasicSample {
        let mut s = BasicSample::new(
            name.to_string(),
            44_100,
            60,
            0,
            sample_types::MONO_SAMPLE,
            5,
            50,
        );
        s.set_compressed_data(vec![0xABu8; 4]);
        s
    }

    /// Reads the raw `shdr` data bytes from an `ExtendedSF2Chunks::pdta`.
    fn pdta_data_bytes(chunks: &ExtendedSF2Chunks) -> Vec<u8> {
        (&*chunks.pdta).to_vec()
    }

    /// Returns the byte offset of sample `i` within the raw shdr payload
    /// (i.e. after the 8-byte RIFF header).
    fn record_offset(i: usize) -> usize {
        8 + i * SAMPLE_RECORD_SIZE
    }

    // -----------------------------------------------------------------------
    // RIFF chunk wrapper
    // -----------------------------------------------------------------------

    #[test]
    fn test_pdta_is_riff_shdr_chunk() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("Piano"));
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data: &[u8] = &chunks.pdta;
        // FourCC
        assert_eq!(&data[0..4], b"shdr");
    }

    #[test]
    fn test_xdta_is_riff_shdr_chunk() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("Piano"));
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data: &[u8] = &chunks.xdta;
        assert_eq!(&data[0..4], b"shdr");
    }

    #[test]
    fn test_empty_bank_chunk_size_is_one_record() {
        // 0 samples + 1 EOS = 1 record
        let bank = BasicSoundBank::default();
        let chunks = get_shdr(&bank, &[], &[]);
        let data: &[u8] = &chunks.pdta;
        // RIFF size field (4 bytes LE at offset 4) = SAMPLE_RECORD_SIZE
        let size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
        assert_eq!(size, SAMPLE_RECORD_SIZE);
    }

    #[test]
    fn test_one_sample_chunk_size_is_two_records() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A"));
        let chunks = get_shdr(&bank, &[0], &[20]);
        let data: &[u8] = &chunks.pdta;
        let size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
        assert_eq!(size, 2 * SAMPLE_RECORD_SIZE);
    }

    // -----------------------------------------------------------------------
    // Sample name fields
    // -----------------------------------------------------------------------

    #[test]
    fn test_sample_name_first20_chars_in_pdta() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("Piano"));
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data = pdta_data_bytes(&chunks);
        let name = read_binary_string(&data, 20, record_offset(0));
        assert_eq!(name, "Piano");
    }

    #[test]
    fn test_sample_name_exactly_20_chars() {
        let name20 = "ABCDEFGHIJKLMNOPQRST"; // exactly 20 chars
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample(name20));
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data = pdta_data_bytes(&chunks);
        let read_name = read_binary_string(&data, 20, record_offset(0));
        assert_eq!(read_name, name20);
    }

    #[test]
    fn test_sample_name_longer_than_20_truncates_in_pdta() {
        let long_name = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"; // 26 chars
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample(long_name));
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data = pdta_data_bytes(&chunks);
        let read_name = read_binary_string(&data, 20, record_offset(0));
        assert_eq!(read_name, "ABCDEFGHIJKLMNOPQRST");
    }

    #[test]
    fn test_sample_name_overflow_chars_in_xdta() {
        let long_name = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"; // 26 chars; chars 20..26 = "UVWXYZ"
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample(long_name));
        let chunks = get_shdr(&bank, &[0], &[10]);
        let xdata: &[u8] = &chunks.xdta;
        let overflow = read_binary_string(xdata, 20, record_offset(0));
        assert_eq!(overflow, "UVWXYZ");
    }

    #[test]
    fn test_sample_name_short_xdta_name_is_empty() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("Piano")); // 5 chars, no overflow
        let chunks = get_shdr(&bank, &[0], &[10]);
        let xdata: &[u8] = &chunks.xdta;
        let overflow = read_binary_string(xdata, 20, record_offset(0));
        assert_eq!(overflow, "");
    }

    // -----------------------------------------------------------------------
    // Start / end offsets
    // -----------------------------------------------------------------------

    #[test]
    fn test_dw_start_written_correctly() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A"));
        let chunks = get_shdr(&bank, &[42], &[100]);
        let data = pdta_data_bytes(&chunks);
        let dw_start = read_little_endian(&data, 4, record_offset(0) + 20);
        assert_eq!(dw_start, 42);
    }

    #[test]
    fn test_dw_end_written_correctly() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A"));
        let chunks = get_shdr(&bank, &[0], &[256]);
        let data = pdta_data_bytes(&chunks);
        let dw_end = read_little_endian(&data, 4, record_offset(0) + 24);
        assert_eq!(dw_end, 256);
    }

    #[test]
    fn test_xdta_start_offset_is_zero() {
        // xshdrData skips start/end offsets (leaves them zero).
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A"));
        let chunks = get_shdr(&bank, &[999], &[1000]);
        let xdata: &[u8] = &chunks.xdta;
        let x_start = read_little_endian(xdata, 4, record_offset(0) + 20);
        assert_eq!(x_start, 0);
    }

    // -----------------------------------------------------------------------
    // Loop points – SF2 (uncompressed)
    // -----------------------------------------------------------------------

    #[test]
    fn test_sf2_loop_start_is_absolute() {
        // loop_start=10, dw_start=50 → stored value = 60
        let mut bank = BasicSoundBank::default();
        let mut s = make_pcm_sample("A");
        s.loop_start = 10;
        bank.samples.push(s);
        let chunks = get_shdr(&bank, &[50], &[200]);
        let data = pdta_data_bytes(&chunks);
        let stored_loop_start = read_little_endian(&data, 4, record_offset(0) + 28);
        assert_eq!(stored_loop_start, 60); // 10 + 50
    }

    #[test]
    fn test_sf2_loop_end_is_absolute() {
        // loop_end=100, dw_start=50 → stored value = 150
        let mut bank = BasicSoundBank::default();
        let mut s = make_pcm_sample("A");
        s.loop_end = 100;
        bank.samples.push(s);
        let chunks = get_shdr(&bank, &[50], &[200]);
        let data = pdta_data_bytes(&chunks);
        let stored_loop_end = read_little_endian(&data, 4, record_offset(0) + 32);
        assert_eq!(stored_loop_end, 150); // 100 + 50
    }

    #[test]
    fn test_sf2_loop_start_zero_offset() {
        // dw_start=0 → loopStart stored as-is
        let mut bank = BasicSoundBank::default();
        let mut s = make_pcm_sample("A");
        s.loop_start = 15;
        bank.samples.push(s);
        let chunks = get_shdr(&bank, &[0], &[100]);
        let data = pdta_data_bytes(&chunks);
        let v = read_little_endian(&data, 4, record_offset(0) + 28);
        assert_eq!(v, 15);
    }

    // -----------------------------------------------------------------------
    // Loop points – SF3 (compressed)
    // -----------------------------------------------------------------------

    #[test]
    fn test_sf3_loop_start_is_relative() {
        // Compressed: loop_start=5, dw_start=50 → stored value remains 5 (relative).
        let mut bank = BasicSoundBank::default();
        let mut s = make_compressed_sample("V");
        s.loop_start = 5;
        bank.samples.push(s);
        let chunks = get_shdr(&bank, &[50], &[100]);
        let data = pdta_data_bytes(&chunks);
        let v = read_little_endian(&data, 4, record_offset(0) + 28);
        assert_eq!(v, 5);
    }

    #[test]
    fn test_sf3_loop_end_is_relative() {
        let mut bank = BasicSoundBank::default();
        let mut s = make_compressed_sample("V");
        s.loop_end = 30;
        bank.samples.push(s);
        let chunks = get_shdr(&bank, &[50], &[100]);
        let data = pdta_data_bytes(&chunks);
        let v = read_little_endian(&data, 4, record_offset(0) + 32);
        assert_eq!(v, 30);
    }

    // -----------------------------------------------------------------------
    // Sample rate
    // -----------------------------------------------------------------------

    #[test]
    fn test_sample_rate_written_correctly() {
        let mut bank = BasicSoundBank::default();
        let mut s = make_pcm_sample("A");
        s.sample_rate = 22_050;
        bank.samples.push(s);
        let chunks = get_shdr(&bank, &[0], &[20]);
        let data = pdta_data_bytes(&chunks);
        let rate = read_little_endian(&data, 4, record_offset(0) + 36);
        assert_eq!(rate, 22_050);
    }

    // -----------------------------------------------------------------------
    // Original key and pitch correction
    // -----------------------------------------------------------------------

    #[test]
    fn test_original_key_written_correctly() {
        let mut bank = BasicSoundBank::default();
        let mut s = make_pcm_sample("A");
        s.original_key = 69;
        bank.samples.push(s);
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data = pdta_data_bytes(&chunks);
        assert_eq!(data[record_offset(0) + 40], 69);
    }

    #[test]
    fn test_pitch_correction_positive() {
        let mut bank = BasicSoundBank::default();
        let mut s = make_pcm_sample("A");
        s.pitch_correction = 50;
        bank.samples.push(s);
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data = pdta_data_bytes(&chunks);
        assert_eq!(data[record_offset(0) + 41], 50);
    }

    #[test]
    fn test_pitch_correction_negative_stored_as_twos_complement() {
        // -50 as i8 → 206 as u8 (two's complement)
        let mut bank = BasicSoundBank::default();
        let mut s = make_pcm_sample("A");
        s.pitch_correction = -50;
        bank.samples.push(s);
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data = pdta_data_bytes(&chunks);
        assert_eq!(data[record_offset(0) + 41], (-50i8) as u8); // 206
    }

    // -----------------------------------------------------------------------
    // Sample link index
    // -----------------------------------------------------------------------

    #[test]
    fn test_sample_link_none_is_zero_in_pdta() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A"));
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data = pdta_data_bytes(&chunks);
        let link = read_little_endian(&data, 2, record_offset(0) + 42);
        assert_eq!(link, 0);
    }

    #[test]
    fn test_sample_link_low16_in_pdta() {
        let mut bank = BasicSoundBank::default();
        let mut s = make_pcm_sample("L");
        s.linked_sample_idx = Some(3);
        bank.samples.push(s);
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data = pdta_data_bytes(&chunks);
        let link = read_little_endian(&data, 2, record_offset(0) + 42);
        assert_eq!(link, 3 & 0xFFFF);
    }

    #[test]
    fn test_sample_link_high16_in_xdta() {
        let mut bank = BasicSoundBank::default();
        let mut s = make_pcm_sample("L");
        // High 16 bits: 0x0001_0000 = 65536
        s.linked_sample_idx = Some(0x0001_0003);
        bank.samples.push(s);
        let chunks = get_shdr(&bank, &[0], &[10]);
        let xdata: &[u8] = &chunks.xdta;
        let high = read_little_endian(xdata, 2, record_offset(0) + 42);
        assert_eq!(high, 1); // 0x0001_0003 >> 16 = 1
    }

    #[test]
    fn test_sample_link_low16_correct_when_large_index() {
        let mut bank = BasicSoundBank::default();
        let mut s = make_pcm_sample("L");
        s.linked_sample_idx = Some(0xABCD_EF01);
        bank.samples.push(s);
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data = pdta_data_bytes(&chunks);
        let low = read_little_endian(&data, 2, record_offset(0) + 42);
        assert_eq!(low as usize, 0xEF01);
    }

    // -----------------------------------------------------------------------
    // Sample type
    // -----------------------------------------------------------------------

    #[test]
    fn test_sample_type_mono_in_pdta() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A"));
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data = pdta_data_bytes(&chunks);
        let stype = read_little_endian(&data, 2, record_offset(0) + 44);
        assert_eq!(stype, sample_types::MONO_SAMPLE as u32);
    }

    #[test]
    fn test_sample_type_compressed_has_sf3_bit() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_compressed_sample("V"));
        let chunks = get_shdr(&bank, &[0], &[4]);
        let data = pdta_data_bytes(&chunks);
        let stype = read_little_endian(&data, 2, record_offset(0) + 44) as u16;
        assert_ne!(
            stype & SF3_BIT_FLIT,
            0,
            "SF3_BIT_FLIT must be set for compressed samples"
        );
    }

    #[test]
    fn test_sample_type_uncompressed_no_sf3_bit() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A"));
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data = pdta_data_bytes(&chunks);
        let stype = read_little_endian(&data, 2, record_offset(0) + 44) as u16;
        assert_eq!(stype & SF3_BIT_FLIT, 0);
    }

    #[test]
    fn test_sample_type_left_sample() {
        let mut bank = BasicSoundBank::default();
        let s = BasicSample::new(
            "L".to_string(),
            44_100,
            60,
            0,
            sample_types::LEFT_SAMPLE,
            0,
            10,
        );
        bank.samples.push(s);
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data = pdta_data_bytes(&chunks);
        let stype = read_little_endian(&data, 2, record_offset(0) + 44) as u16;
        assert_eq!(stype, sample_types::LEFT_SAMPLE);
    }

    // -----------------------------------------------------------------------
    // EOS sentinel record
    // -----------------------------------------------------------------------

    #[test]
    fn test_eos_record_name_in_pdta() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A"));
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data = pdta_data_bytes(&chunks);
        // EOS is the last record
        let eos_name = read_binary_string(&data, 3, record_offset(1));
        assert_eq!(eos_name, "EOS");
    }

    #[test]
    fn test_eos_record_rest_is_zero() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A"));
        let chunks = get_shdr(&bank, &[0], &[10]);
        let data = pdta_data_bytes(&chunks);
        let eos_start = record_offset(1);
        // Bytes after "EOS" (bytes 3..46) should be zero
        for i in (eos_start + 3)..(eos_start + SAMPLE_RECORD_SIZE) {
            assert_eq!(data[i], 0, "EOS byte {i} should be zero");
        }
    }

    #[test]
    fn test_eos_record_in_empty_bank() {
        let bank = BasicSoundBank::default();
        let chunks = get_shdr(&bank, &[], &[]);
        let data = pdta_data_bytes(&chunks);
        let eos_name = read_binary_string(&data, 3, record_offset(0));
        assert_eq!(eos_name, "EOS");
    }

    // -----------------------------------------------------------------------
    // Multiple samples
    // -----------------------------------------------------------------------

    #[test]
    fn test_multiple_samples_records_are_contiguous() {
        let mut bank = BasicSoundBank::default();
        let mut s1 = make_pcm_sample("First");
        s1.original_key = 60;
        let mut s2 = make_pcm_sample("Second");
        s2.original_key = 69;
        bank.samples.push(s1);
        bank.samples.push(s2);
        let chunks = get_shdr(&bank, &[0, 50], &[20, 80]);
        let data = pdta_data_bytes(&chunks);
        assert_eq!(data[record_offset(0) + 40], 60); // first sample original_key
        assert_eq!(data[record_offset(1) + 40], 69); // second sample original_key
    }

    #[test]
    fn test_multiple_samples_dw_start_per_sample() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A"));
        bank.samples.push(make_pcm_sample("B"));
        let chunks = get_shdr(&bank, &[10, 200], &[100, 400]);
        let data = pdta_data_bytes(&chunks);
        let dw_start_a = read_little_endian(&data, 4, record_offset(0) + 20);
        let dw_start_b = read_little_endian(&data, 4, record_offset(1) + 20);
        assert_eq!(dw_start_a, 10);
        assert_eq!(dw_start_b, 200);
    }

    // -----------------------------------------------------------------------
    // RIFF chunk readback
    // -----------------------------------------------------------------------

    #[test]
    fn test_pdta_chunk_readable_by_read_riff_chunk() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("Piano"));
        let chunks = get_shdr(&bank, &[0], &[10]);
        let raw = pdta_data_bytes(&chunks);
        let mut arr = IndexedByteArray::from_vec(raw);
        let chunk = read_riff_chunk(&mut arr, true, false);
        assert_eq!(chunk.header, "shdr");
        assert_eq!(chunk.size as usize, 2 * SAMPLE_RECORD_SIZE);
    }

    // -----------------------------------------------------------------------
    // Internal helper: slice_str_bytes
    // -----------------------------------------------------------------------

    #[test]
    fn test_slice_str_bytes_short_string() {
        assert_eq!(slice_str_bytes("Hi", 0, 20), "Hi");
    }

    #[test]
    fn test_slice_str_bytes_exact() {
        let s = "ABCDEFGHIJKLMNOPQRST"; // 20 chars
        assert_eq!(slice_str_bytes(s, 0, 20), s);
    }

    #[test]
    fn test_slice_str_bytes_truncates() {
        let s = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"; // 26 chars
        assert_eq!(slice_str_bytes(s, 0, 20), "ABCDEFGHIJKLMNOPQRST");
    }

    #[test]
    fn test_slice_str_bytes_from_offset() {
        let s = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        assert_eq!(slice_str_bytes(s, 20, 20), "UVWXYZ"); // only 6 chars left
    }

    #[test]
    fn test_slice_str_bytes_start_beyond_length() {
        assert_eq!(slice_str_bytes("Hi", 10, 20), "");
    }
}
