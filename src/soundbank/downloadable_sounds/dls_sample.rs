/// dls_sample.rs
/// purpose: DLS audio sample with on-demand PCM/A-law decoding.
/// Ported from: src/soundbank/downloadable_sounds/dls_sample.ts
///
/// # TypeScript vs Rust design differences
///
/// TypeScript uses `class DLSSample extends BasicSample`.
/// Rust uses composition: `DlsSample` contains a `BasicSample` (`sample` field) plus
/// the DLS-specific fields (`w_format_tag`, `bytes_per_sample`, `raw_data`).
///
/// Callers that need `BasicSample` access use `dls_sample.sample`.
/// The overridden `getAudioData` / `getRawData` methods are exposed on `DlsSample`.
use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
use crate::soundbank::enums::sample_types;
use crate::utils::loggin::spessa_synth_warn;

// ---------------------------------------------------------------------------
// wFormatTag constants
// ---------------------------------------------------------------------------

/// Wave format tag constants.
/// Equivalent to: W_FORMAT_TAG
pub mod w_format_tag {
    /// Uncompressed PCM audio.
    pub const PCM: u16 = 0x01;
    /// ITU-T G.711 A-law companded audio.
    pub const ALAW: u16 = 0x06;
}

// ---------------------------------------------------------------------------
// Private decode helpers
// ---------------------------------------------------------------------------

/// Decodes raw bytes as PCM audio into f32 samples in [-1.0, 1.0].
/// Equivalent to: readPCM(data, bytesPerSample)
fn read_pcm(data: &[u8], bytes_per_sample: usize) -> Vec<f32> {
    let sample_length = data.len() / bytes_per_sample;
    let mut sample_data = Vec::with_capacity(sample_length);

    if bytes_per_sample == 2 {
        // Optimized path for s16le (most common)
        for i in 0..sample_length {
            let s16 = i16::from_le_bytes([data[i * 2], data[i * 2 + 1]]);
            sample_data.push((s16 as f64 / 32_768.0) as f32);
        }
    } else if bytes_per_sample == 1 {
        // 8-bit unsigned: normalize [0, 255] → [-0.5, 0.5]
        let normalization_factor: f64 = 255.0;
        for &byte in data {
            sample_data.push((byte as f64 / normalization_factor - 0.5) as f32);
        }
    } else {
        // General path for other bit depths
        let max_sample_value = 1i64 << (bytes_per_sample * 8 - 1);
        let max_unsigned = 1i64 << (bytes_per_sample * 8);
        let normalization_factor = max_sample_value as f64;
        let mut offset = 0usize;
        for _ in 0..sample_length {
            let mut value: u32 = 0;
            for j in 0..bytes_per_sample {
                value |= (data[offset + j] as u32) << (j * 8);
            }
            offset += bytes_per_sample;
            let signed = if value as i64 >= max_sample_value {
                value as i64 - max_unsigned
            } else {
                value as i64
            };
            sample_data.push((signed as f64 / normalization_factor) as f32);
        }
    }

    sample_data
}

/// Decodes raw bytes as A-law (G.711) audio into f32 samples.
/// See: https://en.wikipedia.org/wiki/G.711#A-law
/// Equivalent to: readALAW(data, bytesPerSample)
///
/// Note: the divisor 32_678 is preserved from the original TypeScript source (typo for 32_768).
fn read_alaw(data: &[u8], bytes_per_sample: usize) -> Vec<f32> {
    let sample_length = data.len() / bytes_per_sample;
    let mut sample_data = Vec::with_capacity(sample_length);
    let mut offset = 0usize;

    for _ in 0..sample_length {
        // Read bytes_per_sample bytes as little-endian unsigned integer
        let mut input: u32 = 0;
        for j in 0..bytes_per_sample {
            input |= (data[offset + j] as u32) << (j * 8);
        }
        offset += bytes_per_sample;

        // Re-toggle toggled bits
        let mut sample = input ^ 0x55;
        // Remove sign bit
        sample &= 0x7f;

        // Extract exponent and mantissa
        let exponent = sample >> 4;
        let mut mantissa = sample & 0xf;
        if exponent > 0 {
            mantissa += 16; // Add leading '1' if exponent > 0
        }

        mantissa = (mantissa << 4) + 0x8;
        if exponent > 1 {
            mantissa <<= exponent - 1;
        }

        // Apply sign based on the original input's sign bit
        // Note: divisor 32_678 is preserved from the original TypeScript (intentional typo)
        let s16sample: i32 = if input > 127 {
            mantissa as i32
        } else {
            -(mantissa as i32)
        };
        sample_data.push((s16sample as f64 / 32_678.0) as f32);
    }

    sample_data
}

// ---------------------------------------------------------------------------
// DlsSample
// ---------------------------------------------------------------------------

/// A DLS audio sample that decodes PCM or A-law audio on demand.
/// Equivalent to: class DLSSample extends BasicSample
///
/// Because Rust does not support class inheritance, `DlsSample` uses composition:
/// it contains a `BasicSample` (`sample` field) and delegates or overrides behavior
/// through its own methods.
#[derive(Clone, Debug)]
pub struct DlsSample {
    /// The underlying sample metadata and decoded audio data.
    pub sample: BasicSample,

    /// The wave format tag (PCM = 0x01, A-law = 0x06).
    /// Equivalent to: protected wFormatTag: number
    pub w_format_tag: u16,

    /// Number of bytes per audio sample frame.
    /// Equivalent to: protected bytesPerSample: number
    pub bytes_per_sample: u8,

    /// Raw encoded audio bytes before decoding.
    /// Equivalent to: protected rawData: IndexedByteArray
    raw_data: Vec<u8>,
}

impl DlsSample {
    /// Creates a new `DlsSample` from a RIFF chunk's data bytes.
    ///
    /// Equivalent to: new DLSSample(name, rate, pitch, pitchCorrection,
    ///                              loopStart, loopEnd, dataChunk, wFormatTag, bytesPerSample)
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        rate: u32,
        pitch: u8,
        pitch_correction: i8,
        loop_start: u32,
        loop_end: u32,
        raw_data: Vec<u8>,
        w_format_tag: u16,
        bytes_per_sample: u8,
    ) -> Self {
        let mut sample = BasicSample::new(
            name,
            rate,
            pitch,
            pitch_correction,
            sample_types::MONO_SAMPLE,
            loop_start,
            loop_end,
        );
        // DLS samples are not overridden until the user explicitly sets audio data
        sample.data_overridden = false;
        Self {
            sample,
            w_format_tag,
            bytes_per_sample,
            raw_data,
        }
    }

    /// Returns the decoded f32 PCM audio data, decoding from raw bytes on first call.
    ///
    /// If `raw_data` is empty, an empty slice is returned.
    /// After decoding, the result is cached in `self.sample.audio_data`.
    ///
    /// Equivalent to: DLSSample.getAudioData()
    pub fn get_audio_data(&mut self) -> &[f32] {
        if self.raw_data.is_empty() {
            // No raw data: cache an empty vec and return it
            if self.sample.audio_data.is_none() {
                self.sample.audio_data = Some(Vec::new());
            }
            return self.sample.audio_data.as_deref().unwrap();
        }

        if self.sample.audio_data.is_none() {
            let bps = self.bytes_per_sample as usize;
            let sample_data = match self.w_format_tag {
                w_format_tag::PCM => read_pcm(&self.raw_data, bps),
                w_format_tag::ALAW => read_alaw(&self.raw_data, bps),
                tag => {
                    spessa_synth_warn(&format!(
                        "Failed to decode sample. Unknown wFormatTag: {}",
                        tag
                    ));
                    // Return silence for unknown formats
                    vec![0.0f32; self.raw_data.len() / bps]
                }
            };
            let rate = self.sample.sample_rate;
            self.sample.set_audio_data(sample_data, rate);
        }

        self.sample.audio_data.as_deref().unwrap()
    }

    /// Returns the raw bytes for writing.
    ///
    /// - If `data_overridden` or compressed: delegates to `BasicSample::get_raw_data`.
    /// - If PCM with 2 bytes/sample: returns the raw bytes directly (no re-encoding needed).
    /// - Otherwise: decodes then re-encodes as s16le.
    ///
    /// Equivalent to: DLSSample.getRawData(allowVorbis)
    pub fn get_raw_data(&mut self, allow_vorbis: bool) -> Vec<u8> {
        if self.sample.data_overridden || self.sample.is_compressed() {
            return self.sample.get_raw_data(allow_vorbis);
        }
        if self.w_format_tag == w_format_tag::PCM && self.bytes_per_sample == 2 {
            // Already in the target format; copy straight through
            return self.raw_data.clone();
        }
        // Decode (if not already decoded) then encode as s16le
        let _ = self.get_audio_data();
        self.sample.encode_s16le()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- helpers ---

    fn pcm_s16le_bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|&s| s.to_le_bytes()).collect()
    }

    fn pcm_u8_bytes(samples: &[u8]) -> Vec<u8> {
        samples.to_vec()
    }

    fn make_pcm_s16_sample(samples: &[i16]) -> DlsSample {
        DlsSample::new(
            "test".to_string(),
            44_100,
            60,
            0,
            0,
            samples.len() as u32 - 1,
            pcm_s16le_bytes(samples),
            w_format_tag::PCM,
            2,
        )
    }

    fn make_pcm_u8_sample(samples: &[u8]) -> DlsSample {
        DlsSample::new(
            "test".to_string(),
            44_100,
            60,
            0,
            0,
            samples.len() as u32 - 1,
            pcm_u8_bytes(samples),
            w_format_tag::PCM,
            1,
        )
    }

    fn make_alaw_sample(raw: &[u8]) -> DlsSample {
        DlsSample::new(
            "alaw".to_string(),
            44_100,
            60,
            0,
            0,
            raw.len() as u32 - 1,
            raw.to_vec(),
            w_format_tag::ALAW,
            1,
        )
    }

    // --- DlsSample::new ---

    #[test]
    fn test_new_stores_name() {
        let s = make_pcm_s16_sample(&[0]);
        assert_eq!(s.sample.name, "test");
    }

    #[test]
    fn test_new_stores_sample_rate() {
        let s = make_pcm_s16_sample(&[0]);
        assert_eq!(s.sample.sample_rate, 44_100);
    }

    #[test]
    fn test_new_stores_w_format_tag() {
        let s = make_pcm_s16_sample(&[0]);
        assert_eq!(s.w_format_tag, w_format_tag::PCM);
    }

    #[test]
    fn test_new_stores_bytes_per_sample() {
        let s = make_pcm_s16_sample(&[0]);
        assert_eq!(s.bytes_per_sample, 2);
    }

    #[test]
    fn test_new_sample_type_is_mono() {
        let s = make_pcm_s16_sample(&[0]);
        assert_eq!(s.sample.sample_type, sample_types::MONO_SAMPLE);
    }

    #[test]
    fn test_new_data_overridden_is_false() {
        let s = make_pcm_s16_sample(&[0]);
        assert!(!s.sample.data_overridden);
    }

    #[test]
    fn test_new_audio_data_is_none() {
        let s = make_pcm_s16_sample(&[0]);
        assert!(s.sample.audio_data.is_none());
    }

    // --- w_format_tag constants ---

    #[test]
    fn test_w_format_tag_pcm_value() {
        assert_eq!(w_format_tag::PCM, 0x01);
    }

    #[test]
    fn test_w_format_tag_alaw_value() {
        assert_eq!(w_format_tag::ALAW, 0x06);
    }

    // --- read_pcm: 16-bit ---

    #[test]
    fn test_read_pcm_s16_zero() {
        let bytes = pcm_s16le_bytes(&[0]);
        let result = read_pcm(&bytes, 2);
        assert_eq!(result.len(), 1);
        assert!((result[0] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_read_pcm_s16_max_positive() {
        let bytes = pcm_s16le_bytes(&[32_767]);
        let result = read_pcm(&bytes, 2);
        // 32767 / 32768 ≈ 0.9999695...
        assert!((result[0] - (32_767.0f32 / 32_768.0)).abs() < 1e-5);
    }

    #[test]
    fn test_read_pcm_s16_min_negative() {
        let bytes = pcm_s16le_bytes(&[-32_768]);
        let result = read_pcm(&bytes, 2);
        assert!((result[0] - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_read_pcm_s16_multiple_samples() {
        let bytes = pcm_s16le_bytes(&[0, 16_384, -16_384]);
        let result = read_pcm(&bytes, 2);
        assert_eq!(result.len(), 3);
        assert!((result[1] - 0.5).abs() < 1e-5);
        assert!((result[2] - (-0.5)).abs() < 1e-5);
    }

    #[test]
    fn test_read_pcm_s16_output_length() {
        let bytes = pcm_s16le_bytes(&[1, 2, 3, 4, 5]);
        let result = read_pcm(&bytes, 2);
        assert_eq!(result.len(), 5);
    }

    // --- read_pcm: 8-bit unsigned ---

    #[test]
    fn test_read_pcm_u8_min_value() {
        // 0 / 255 - 0.5 = -0.5
        let result = read_pcm(&[0], 1);
        assert!((result[0] - (-0.5)).abs() < 1e-6);
    }

    #[test]
    fn test_read_pcm_u8_max_value() {
        // 255 / 255 - 0.5 = 0.5
        let result = read_pcm(&[255], 1);
        assert!((result[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_read_pcm_u8_midpoint() {
        // 128 / 255 - 0.5 ≈ 0.00196
        let result = read_pcm(&[128], 1);
        let expected = 128.0f32 / 255.0 - 0.5;
        assert!((result[0] - expected).abs() < 1e-6);
    }

    #[test]
    fn test_read_pcm_u8_multiple_samples() {
        let result = read_pcm(&[0, 128, 255], 1);
        assert_eq!(result.len(), 3);
    }

    // --- read_alaw ---

    #[test]
    fn test_read_alaw_output_length() {
        let data = vec![0u8; 4];
        let result = read_alaw(&data, 1);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_read_alaw_zero_input() {
        // input=0: sample = 0^0x55 = 0x55 = 85; &0x7f = 85; exponent=5; mantissa=5+16=21
        // mantissa = (21<<4)+8 = 336+8 = 344; exponent>1 → 344<<4 = 5504
        // s16sample = input(0) > 127? no → -5504
        let result = read_alaw(&[0], 1);
        let expected = -5504.0f32 / 32_678.0;
        assert!((result[0] - expected).abs() < 1e-5);
    }

    #[test]
    fn test_read_alaw_high_byte_is_positive() {
        // input > 127 → s16sample is positive
        let result = read_alaw(&[0xD5], 1); // 0xD5 = 213 > 127
        assert!(result[0] >= 0.0);
    }

    #[test]
    fn test_read_alaw_low_byte_is_negative() {
        // input <= 127 → s16sample is negative
        let result = read_alaw(&[0x00], 1); // 0 <= 127
        assert!(result[0] <= 0.0);
    }

    #[test]
    fn test_read_alaw_uses_32678_divisor() {
        // Verify divisor is 32_678 (preserved typo from TypeScript), not 32_768
        // For input = 0xD5 (213): xor 0x55 = 0x80, &0x7f = 0, exp=0, man=0
        // man = (0<<4)+8 = 8, exp not > 1
        // s16 = 8 (positive since 213 > 127)
        // result = 8 / 32_678 ≈ 0.0002449...
        let result = read_alaw(&[0xD5], 1);
        let with_correct = 8.0f32 / 32_768.0;
        let with_typo = 8.0f32 / 32_678.0;
        // result should be closer to the typo divisor
        assert!((result[0] - with_typo).abs() < (result[0] - with_correct).abs());
    }

    // --- get_audio_data: PCM s16 ---

    #[test]
    fn test_get_audio_data_pcm_s16_zero() {
        let mut s = make_pcm_s16_sample(&[0]);
        let data = s.get_audio_data();
        assert_eq!(data.len(), 1);
        assert!((data[0] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_get_audio_data_pcm_s16_multiple() {
        let mut s = make_pcm_s16_sample(&[0, 16_384, -16_384]);
        let data = s.get_audio_data();
        assert_eq!(data.len(), 3);
        assert!((data[1] - 0.5).abs() < 1e-5);
        assert!((data[2] - (-0.5)).abs() < 1e-5);
    }

    #[test]
    fn test_get_audio_data_caches_result() {
        let mut s = make_pcm_s16_sample(&[0, 1000]);
        let _ = s.get_audio_data(); // first call decodes
        // Second call should return the same cached data
        let data = s.get_audio_data();
        assert_eq!(data.len(), 2);
    }

    #[test]
    fn test_get_audio_data_sets_data_overridden() {
        let mut s = make_pcm_s16_sample(&[0]);
        assert!(!s.sample.data_overridden);
        let _ = s.get_audio_data();
        // After decoding, set_audio_data is called which sets data_overridden = true
        assert!(s.sample.data_overridden);
    }

    #[test]
    fn test_get_audio_data_empty_raw_data_returns_empty() {
        let mut s = DlsSample::new(
            "empty".to_string(),
            44_100,
            60,
            0,
            0,
            0,
            Vec::new(), // no raw data
            w_format_tag::PCM,
            2,
        );
        let data = s.get_audio_data();
        assert!(data.is_empty());
    }

    #[test]
    fn test_get_audio_data_empty_raw_caches_empty() {
        let mut s = DlsSample::new(
            "empty".to_string(),
            44_100,
            60,
            0,
            0,
            0,
            Vec::new(),
            w_format_tag::PCM,
            2,
        );
        let _ = s.get_audio_data();
        assert!(s.sample.audio_data.is_some());
        assert!(s.sample.audio_data.as_ref().unwrap().is_empty());
    }

    // --- get_audio_data: A-law ---

    #[test]
    fn test_get_audio_data_alaw_decodes() {
        let mut s = make_alaw_sample(&[0xD5, 0x00]);
        let data = s.get_audio_data();
        assert_eq!(data.len(), 2);
    }

    // --- get_audio_data: unknown format ---

    #[test]
    fn test_get_audio_data_unknown_format_returns_silence() {
        let mut s = DlsSample::new(
            "test".to_string(),
            44_100,
            60,
            0,
            0,
            1,
            vec![0x00, 0x00, 0x00, 0x00], // 4 bytes, 2 bytes_per_sample → 2 samples
            0xFF,                         // unknown format tag
            2,
        );
        let data = s.get_audio_data();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0], 0.0);
        assert_eq!(data[1], 0.0);
    }

    // --- get_raw_data: PCM s16 (direct copy) ---

    #[test]
    fn test_get_raw_data_pcm_s16_returns_raw_bytes_directly() {
        let samples = [100i16, -200, 300];
        let raw = pcm_s16le_bytes(&samples);
        let mut s = make_pcm_s16_sample(&samples);
        let result = s.get_raw_data(false);
        assert_eq!(result, raw);
    }

    #[test]
    fn test_get_raw_data_pcm_s16_no_decoding_needed() {
        // For PCM s16, get_raw_data should NOT set audio_data (no decoding)
        let mut s = make_pcm_s16_sample(&[1000, -1000]);
        let _ = s.get_raw_data(false);
        // audio_data should not be populated (raw bytes returned directly)
        assert!(s.sample.audio_data.is_none());
    }

    // --- get_raw_data: non-s16 PCM (encode after decode) ---

    #[test]
    fn test_get_raw_data_pcm_u8_encodes_as_s16le() {
        let mut s = make_pcm_u8_sample(&[0, 128, 255]);
        let result = s.get_raw_data(false);
        // 3 f32 samples → 3 × 2 = 6 bytes
        assert_eq!(result.len(), 6);
    }

    #[test]
    fn test_get_raw_data_pcm_u8_decodes_before_encoding() {
        let mut s = make_pcm_u8_sample(&[0]);
        let _ = s.get_raw_data(false);
        // After get_raw_data on non-s16 PCM, audio_data should be populated
        assert!(s.sample.audio_data.is_some());
    }

    // --- get_raw_data: data_overridden delegates to BasicSample ---

    #[test]
    fn test_get_raw_data_delegates_when_data_overridden() {
        let mut s = make_pcm_s16_sample(&[0]);
        s.sample.data_overridden = true;
        // With data_overridden=true, delegates to BasicSample::get_raw_data
        // BasicSample will try to encode, but audio_data is None → set some data first
        s.sample.audio_data = Some(vec![0.0]);
        let result = s.get_raw_data(false);
        assert_eq!(result.len(), 2); // 1 f32 → 2 bytes s16le
    }

    // --- get_raw_data: compressed delegates to BasicSample ---

    #[test]
    fn test_get_raw_data_delegates_when_compressed() {
        let mut s = make_pcm_s16_sample(&[0]);
        s.sample.set_compressed_data(vec![0xAB, 0xCD]);
        // allow_vorbis=true: BasicSample returns the compressed bytes
        let result = s.get_raw_data(true);
        assert_eq!(result, vec![0xAB, 0xCD]);
    }

    // --- A-law get_raw_data ---

    #[test]
    fn test_get_raw_data_alaw_encodes_as_s16le() {
        let mut s = make_alaw_sample(&[0xD5, 0x00]);
        let result = s.get_raw_data(false);
        // 2 decoded samples → 4 bytes
        assert_eq!(result.len(), 4);
    }

    // --- Clone ---

    #[test]
    fn test_clone_is_independent() {
        let mut original = make_pcm_s16_sample(&[1000, -1000]);
        let _ = original.get_audio_data(); // populate audio_data
        let mut cloned = original.clone();
        cloned.sample.set_audio_data(vec![0.0], 22_050);
        // original should be unchanged
        assert_eq!(original.sample.audio_data.as_ref().unwrap().len(), 2);
        assert_eq!(original.sample.sample_rate, 44_100);
    }
}
