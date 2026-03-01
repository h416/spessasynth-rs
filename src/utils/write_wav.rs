use crate::utils::WaveWriteOptions;
/// write_wav.rs
/// purpose: Writes audio data to WAV file format (RIFF/PCM).
/// Ported from: src/utils/write_wav.ts
///
/// Note: `fillWithDefaults` from TypeScript is replaced by `Option::unwrap_or_default()`.
/// `WaveWriteOptions` / `WaveMetadata` are defined in `utils/mod.rs` (exports.ts mapping).
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::write_little_endian_indexed;
use crate::utils::riff_chunk::{write_riff_chunk_parts, write_riff_chunk_raw};
use crate::utils::string::write_binary_string_indexed;
use std::ops::Deref;

/// Writes audio data into a valid WAV file and returns the raw bytes.
///
/// Equivalent to: `audioToWav(audioData, sampleRate, options)`
///
/// # Arguments
/// * `audio_data` - Per-channel sample data as `f32` values in [-1.0, 1.0].
/// * `sample_rate` - Sample rate in Hz (e.g. 44100).
/// * `options`     - Optional write options; `None` uses `WaveWriteOptions::default()`.
///
/// # Returns
/// A `Vec<u8>` containing the complete WAV file.
pub fn audio_to_wav(
    audio_data: &[Vec<f32>],
    sample_rate: u32,
    options: Option<WaveWriteOptions>,
) -> (Vec<u8>, usize) {
    let length = audio_data[0].len();
    let num_channels = audio_data.len();
    let bytes_per_sample: usize = 2; // 16-bit PCM

    // fillWithDefaults(options, DEFAULT_WAV_WRITE_OPTIONS)
    let full_options = options.unwrap_or_default();
    let loop_points = full_options.loop_points.as_ref();
    let metadata = &full_options.metadata;

    // --- Prepare INFO chunk (metadata) ---
    // infoOn = Object.keys(metadata).length > 0
    let info_on = metadata.title.is_some()
        || metadata.artist.is_some()
        || metadata.album.is_some()
        || metadata.genre.is_some();

    let info_chunk: Vec<u8> = if info_on {
        let mut sub_chunks: Vec<IndexedByteArray> = Vec::new();

        // "Created with SpessaSynth" comment is always added when any metadata is present
        sub_chunks.push(write_riff_chunk_raw(
            "ICMT",
            b"Created with SpessaSynth",
            true,
            false,
        ));
        if let Some(artist) = &metadata.artist {
            sub_chunks.push(write_riff_chunk_raw("IART", artist.as_bytes(), true, false));
        }
        if let Some(album) = &metadata.album {
            sub_chunks.push(write_riff_chunk_raw("IPRD", album.as_bytes(), true, false));
        }
        if let Some(genre) = &metadata.genre {
            sub_chunks.push(write_riff_chunk_raw("IGNR", genre.as_bytes(), true, false));
        }
        if let Some(title) = &metadata.title {
            sub_chunks.push(write_riff_chunk_raw("INAM", title.as_bytes(), true, false));
        }

        let sub_refs: Vec<&[u8]> = sub_chunks.iter().map(|c| c.deref()).collect();
        write_riff_chunk_parts("INFO", &sub_refs, true).to_vec()
    } else {
        vec![]
    };

    // --- Prepare CUE chunk (loop points) ---
    // cueOn = loop?.end !== undefined && loop?.start !== undefined
    let cue_on = loop_points.is_some();

    let cue_chunk: Vec<u8> = if cue_on {
        let lp = loop_points.unwrap();
        let loop_start_samples = (lp.start * sample_rate as f64).floor() as u32;
        let loop_end_samples = (lp.end * sample_rate as f64).floor() as u32;

        // CUE start point (24 bytes)
        let mut cue_start = IndexedByteArray::new(24);
        write_little_endian_indexed(&mut cue_start, 0, 4); // DwIdentifier
        write_little_endian_indexed(&mut cue_start, 0, 4); // DwPosition
        write_binary_string_indexed(&mut cue_start, "data", 0); // Cue point ID (4 bytes)
        write_little_endian_indexed(&mut cue_start, 0, 4); // ChunkStart (always 0)
        write_little_endian_indexed(&mut cue_start, 0, 4); // BlockStart (always 0)
        write_little_endian_indexed(&mut cue_start, loop_start_samples, 4); // SampleOffset

        // CUE end point (24 bytes)
        let mut cue_end = IndexedByteArray::new(24);
        write_little_endian_indexed(&mut cue_end, 1, 4); // DwIdentifier
        write_little_endian_indexed(&mut cue_end, 0, 4); // DwPosition
        write_binary_string_indexed(&mut cue_end, "data", 0); // Cue point ID (4 bytes)
        write_little_endian_indexed(&mut cue_end, 0, 4); // ChunkStart (always 0)
        write_little_endian_indexed(&mut cue_end, 0, 4); // BlockStart (always 0)
        write_little_endian_indexed(&mut cue_end, loop_end_samples, 4); // SampleOffset

        let cue_count = IndexedByteArray::from_vec(vec![2, 0, 0, 0]); // 2 cue points (LE u32)
        let sub_refs: Vec<&[u8]> = vec![&*cue_count, &*cue_start, &*cue_end];
        write_riff_chunk_parts("cue ", &sub_refs, false).to_vec()
    } else {
        vec![]
    };

    // --- Build WAV header (44 bytes) ---
    let header_size: usize = 44;
    let data_size = length * num_channels * bytes_per_sample; // 16-bit per channel
    // Total file size minus the first 8 bytes ("RIFF" + size field)
    let file_size = header_size + data_size + info_chunk.len() + cue_chunk.len() - 8;
    let total_size = file_size + 8;

    let mut wav_data = vec![0u8; total_size];

    // "RIFF"
    wav_data[0..4].copy_from_slice(b"RIFF");
    // File length (LE u32)
    wav_data[4..8].copy_from_slice(&(file_size as u32).to_le_bytes());
    // "WAVE"
    wav_data[8..12].copy_from_slice(b"WAVE");
    // "fmt "
    wav_data[12..16].copy_from_slice(b"fmt ");
    // fmt chunk size = 16 (PCM)
    wav_data[16..20].copy_from_slice(&16u32.to_le_bytes());
    // Audio format = 1 (PCM)
    wav_data[20..22].copy_from_slice(&1u16.to_le_bytes());
    // Number of channels
    wav_data[22..24].copy_from_slice(&(num_channels as u16).to_le_bytes());
    // Sample rate
    wav_data[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    // Byte rate = sample_rate * num_channels * bytes_per_sample
    let byte_rate = sample_rate * num_channels as u32 * bytes_per_sample as u32;
    wav_data[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    // Block align = num_channels * bytes_per_sample
    wav_data[32..34]
        .copy_from_slice(&(num_channels as u16 * bytes_per_sample as u16).to_le_bytes());
    // Bits per sample = 16
    wav_data[34..36].copy_from_slice(&16u16.to_le_bytes());
    // "data"
    wav_data[36..40].copy_from_slice(b"data");
    // Data chunk size
    wav_data[40..44].copy_from_slice(&(data_size as u32).to_le_bytes());

    // --- Normalize and interleave audio data as 16-bit PCM ---
    // Find peak amplitude for normalization
    // Use f64 to match JS behavior: Float32Array values are promoted to f64 (JS number)
    // before Math.abs() and division.
    let mut multiplier: f64 = 32_767.0;
    if full_options.normalize_audio {
        let mut max_abs_value: f64 = 0.0;
        for ch in audio_data {
            for &s in ch {
                let abs_s = (s as f64).abs();
                if abs_s > max_abs_value {
                    max_abs_value = abs_s;
                }
            }
        }
        multiplier = if max_abs_value > 0.0 {
            32_767.0 / max_abs_value
        } else {
            1.0
        };
    }

    // Interleave channels: [ch0[0], ch1[0], ch0[1], ch1[1], ...]
    // Use f64 multiplication to match JS behavior: d[i] * multiplier is computed in f64.
    let mut offset = header_size;
    let mut clipped_count: usize = 0;
    for i in 0..length {
        for ch in audio_data {
            let raw = ch[i] as f64 * multiplier;
            if raw > 32_767.0 || raw < -32_768.0 {
                clipped_count += 1;
            }
            // Clamp to [-32768, 32767] then convert to signed 16-bit LE
            let sample = raw.clamp(-32_768.0, 32_767.0) as i16;
            let [lo, hi] = sample.to_le_bytes();
            wav_data[offset] = lo;
            wav_data[offset + 1] = hi;
            offset += 2;
        }
    }

    // --- Append optional chunks ---
    if info_on {
        wav_data[offset..offset + info_chunk.len()].copy_from_slice(&info_chunk);
        offset += info_chunk.len();
    }
    if cue_on {
        wav_data[offset..offset + cue_chunk.len()].copy_from_slice(&cue_chunk);
    }

    (wav_data, clipped_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{WaveLoopPoints, WaveMetadata};

    // Helper: read a little-endian u32 from a byte slice at the given offset.
    fn read_u32_le(data: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
    }

    // Helper: read a little-endian u16 from a byte slice at the given offset.
    fn read_u16_le(data: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap())
    }

    // Helper: read a signed little-endian i16 from a byte slice at the given offset.
    fn read_i16_le(data: &[u8], offset: usize) -> i16 {
        i16::from_le_bytes(data[offset..offset + 2].try_into().unwrap())
    }

    // --- WAV header structure ---

    #[test]
    fn test_riff_header_magic() {
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4]], 44100, None);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
    }

    #[test]
    fn test_fmt_chunk_pcm_fields() {
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4]], 44100, None);
        assert_eq!(read_u32_le(&wav, 16), 16); // fmt chunk size
        assert_eq!(read_u16_le(&wav, 20), 1); // PCM format
        assert_eq!(read_u16_le(&wav, 34), 16); // bits per sample
    }

    #[test]
    fn test_mono_channel_count() {
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4]], 44100, None);
        assert_eq!(read_u16_le(&wav, 22), 1); // num channels
    }

    #[test]
    fn test_stereo_channel_count() {
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4], vec![0.0f32; 4]], 44100, None);
        assert_eq!(read_u16_le(&wav, 22), 2); // num channels
    }

    #[test]
    fn test_sample_rate_field() {
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4]], 48000, None);
        assert_eq!(read_u32_le(&wav, 24), 48000);
    }

    #[test]
    fn test_byte_rate_mono() {
        // byte_rate = sample_rate * num_channels * bytes_per_sample
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4]], 44100, None);
        assert_eq!(read_u32_le(&wav, 28), 44100 * 1 * 2);
    }

    #[test]
    fn test_byte_rate_stereo() {
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4], vec![0.0f32; 4]], 44100, None);
        assert_eq!(read_u32_le(&wav, 28), 44100 * 2 * 2);
    }

    #[test]
    fn test_block_align_mono() {
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4]], 44100, None);
        assert_eq!(read_u16_le(&wav, 32), 1 * 2); // 1 channel * 2 bytes
    }

    #[test]
    fn test_block_align_stereo() {
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4], vec![0.0f32; 4]], 44100, None);
        assert_eq!(read_u16_le(&wav, 32), 2 * 2); // 2 channels * 2 bytes
    }

    // --- File size consistency ---

    #[test]
    fn test_total_size_mono() {
        // total = 44 (header) + 4 samples * 1 ch * 2 bytes
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4]], 44100, None);
        assert_eq!(wav.len(), 44 + 4 * 1 * 2);
    }

    #[test]
    fn test_total_size_stereo() {
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4], vec![0.0f32; 4]], 44100, None);
        assert_eq!(wav.len(), 44 + 4 * 2 * 2);
    }

    #[test]
    fn test_riff_size_field_mono() {
        // RIFF size field = total - 8
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4]], 44100, None);
        let reported = read_u32_le(&wav, 4) as usize;
        assert_eq!(reported, wav.len() - 8);
    }

    #[test]
    fn test_data_chunk_size_field() {
        // data chunk size at offset 40 = num_samples * num_channels * 2
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4]], 44100, None);
        assert_eq!(read_u32_le(&wav, 40), 4 * 1 * 2);
    }

    // --- Sample data / normalization ---

    #[test]
    fn test_silence_writes_zeros() {
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4]], 44100, None);
        // All 8 PCM bytes (4 samples * 2 bytes) should be zero
        for &b in &wav[44..52] {
            assert_eq!(b, 0);
        }
    }

    #[test]
    fn test_normalize_peak_sample() {
        // With normalization, the peak f32 sample (0.5) should map to ≈ 32767
        let samples = vec![0.0f32, 0.5, -0.5, 0.25];
        let (wav, _) = audio_to_wav(&[samples], 44100, None); // normalize_audio = true by default
        let peak = read_i16_le(&wav, 44 + 2); // second sample (0.5) → should be 32767
        assert_eq!(peak, 32767);
    }

    #[test]
    fn test_no_normalize_uses_raw_multiplier() {
        // Without normalization, a 1.0 sample maps to 32767
        let opts = WaveWriteOptions {
            normalize_audio: false,
            ..Default::default()
        };
        let (wav, _) = audio_to_wav(&[vec![1.0f32]], 44100, Some(opts));
        let sample = read_i16_le(&wav, 44);
        assert_eq!(sample, 32767);
    }

    #[test]
    fn test_full_negative_sample_no_normalize() {
        // multiplier = 32_767 (not 32_768), so -1.0 * 32767 = -32767
        let opts = WaveWriteOptions {
            normalize_audio: false,
            ..Default::default()
        };
        let (wav, _) = audio_to_wav(&[vec![-1.0f32]], 44100, Some(opts));
        let sample = read_i16_le(&wav, 44);
        assert_eq!(sample, -32767);
    }

    #[test]
    fn test_stereo_interleaving() {
        // left=[1.0], right=[-1.0] → interleaved as [left_sample, right_sample]
        // multiplier = 32_767, so -1.0 * 32767 = -32767
        let opts = WaveWriteOptions {
            normalize_audio: false,
            ..Default::default()
        };
        let left = vec![1.0f32];
        let right = vec![-1.0f32];
        let (wav, _) = audio_to_wav(&[left, right], 44100, Some(opts));
        let l = read_i16_le(&wav, 44);
        let r = read_i16_le(&wav, 46);
        assert_eq!(l, 32767);
        assert_eq!(r, -32767);
    }

    // --- INFO chunk (metadata) ---

    #[test]
    fn test_no_metadata_no_info_chunk() {
        // Default options → no metadata → total size = 44 + data only
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 2]], 44100, None);
        assert_eq!(wav.len(), 44 + 2 * 2);
    }

    #[test]
    fn test_metadata_adds_list_chunk() {
        let opts = WaveWriteOptions {
            metadata: WaveMetadata {
                title: Some("Test".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 2]], 44100, Some(opts));
        // Should be larger than header + data alone
        assert!(wav.len() > 44 + 2 * 2);
        // The INFO LIST chunk begins right after the data
        let data_end = 44 + 2 * 2;
        assert_eq!(&wav[data_end..data_end + 4], b"LIST");
    }

    #[test]
    fn test_riff_size_with_metadata() {
        let opts = WaveWriteOptions {
            metadata: WaveMetadata {
                artist: Some("Artist".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 2]], 44100, Some(opts));
        let reported = read_u32_le(&wav, 4) as usize;
        assert_eq!(reported, wav.len() - 8);
    }

    // --- CUE chunk (loop points) ---

    #[test]
    fn test_no_loop_no_cue_chunk() {
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4]], 44100, None);
        assert_eq!(wav.len(), 44 + 4 * 2);
    }

    #[test]
    fn test_loop_adds_cue_chunk() {
        let opts = WaveWriteOptions {
            loop_points: Some(WaveLoopPoints {
                start: 0.0,
                end: 1.0,
            }),
            ..Default::default()
        };
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4]], 44100, Some(opts));
        assert!(wav.len() > 44 + 4 * 2);
        let data_end = 44 + 4 * 2;
        assert_eq!(&wav[data_end..data_end + 4], b"cue ");
    }

    #[test]
    fn test_cue_chunk_sample_offsets() {
        // loop 1.0s–2.0s at 44100 Hz → start=44100, end=88200
        let opts = WaveWriteOptions {
            loop_points: Some(WaveLoopPoints {
                start: 1.0,
                end: 2.0,
            }),
            normalize_audio: false,
            ..Default::default()
        };
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4]], 44100, Some(opts));
        let data_end = 44 + 4 * 2;
        // cue chunk: "cue " (4) + size (4) + count (4) + cue_start (24) + cue_end (24)
        // cue_start SampleOffset is at data_end + 8 + 4 + 20 = data_end + 32
        let start_offset = data_end + 8 + 4 + 20; // skip header, count, and first 20 bytes of cue_start
        let end_offset = data_end + 8 + 4 + 24 + 20; // same for cue_end
        assert_eq!(read_u32_le(&wav, start_offset), 44100);
        assert_eq!(read_u32_le(&wav, end_offset), 88200);
    }

    #[test]
    fn test_riff_size_with_loop() {
        let opts = WaveWriteOptions {
            loop_points: Some(WaveLoopPoints {
                start: 0.0,
                end: 0.5,
            }),
            ..Default::default()
        };
        let (wav, _) = audio_to_wav(&[vec![0.0f32; 4]], 44100, Some(opts));
        let reported = read_u32_le(&wav, 4) as usize;
        assert_eq!(reported, wav.len() - 8);
    }
}
