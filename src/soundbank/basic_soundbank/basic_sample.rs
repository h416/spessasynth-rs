/// basic_sample.rs
/// purpose: Representation of a single audio sample within a sound bank.
/// Ported from: src/soundbank/basic_soundbank/basic_sample.ts
///
/// # TypeScript vs Rust design differences
///
/// In TypeScript, `linkedTo: BasicInstrument[]` holds direct object references and
/// `linkedSample?: BasicSample` represents the stereo link.
///
/// In Rust, to avoid circular ownership:
/// - `linked_to: Vec<usize>`           : indices into `BasicSoundBank::instruments`
/// - `linked_sample_idx: Option<usize>` : index into `BasicSoundBank::samples`
///
/// TypeScript's `setLinkedSample()` mutates two samples simultaneously, which cannot
/// be expressed with a single `&mut self`. Implemented at the `BasicSoundBank` level.
///
/// Vorbis compression (`compressSample`) is for SF3 writing and not needed for
/// midi-to-wav, so it is omitted (TODO).
/// Vorbis decoding (`decode_vorbis_into_audio_data`) is needed for SF3 reading but
/// will be implemented after adding the lewton / symphonia crate (TODO).
use crate::soundbank::enums::{SampleType, sample_types};
use crate::utils::loggin::spessa_synth_warn;

/// Default resample target rate used when the sample rate is out of range.
/// Equivalent to: const RESAMPLE_RATE = 48_000
const RESAMPLE_RATE: u32 = 48_000;

/// A single audio sample within a sound bank.
/// Equivalent to: class BasicSample
#[derive(Clone, Debug)]
pub struct BasicSample {
    /// The sample's name.
    /// Equivalent to: public name: string
    pub name: String,

    /// Sample rate in Hz.
    /// Equivalent to: public sampleRate: number
    pub sample_rate: u32,

    /// Original pitch of the sample as a MIDI note number (0–127).
    /// Equivalent to: public originalKey: number
    pub original_key: u8,

    /// Pitch correction in cents, can be negative.
    /// Equivalent to: public pitchCorrection: number
    pub pitch_correction: i8,

    /// The type of the sample (mono, left, right, linked, ROM variants).
    /// Equivalent to: public sampleType: SampleType
    pub sample_type: SampleType,

    /// Loop start relative to the sample start, in sample points.
    /// Equivalent to: public loopStart: number
    pub loop_start: u32,

    /// Loop end relative to the sample start, in sample points.
    /// Equivalent to: public loopEnd: number
    pub loop_end: u32,

    /// Index of the stereo partner in `BasicSoundBank::samples`.
    /// `None` if this sample is not part of a stereo pair.
    /// Equivalent to: public linkedSample?: BasicSample
    pub linked_sample_idx: Option<usize>,

    /// Indices of instruments that reference this sample in `BasicSoundBank::instruments`.
    /// Duplicates are allowed (one instrument can reference the same sample multiple times).
    /// Equivalent to: public linkedTo: BasicInstrument[] = []
    pub linked_to: Vec<usize>,

    /// Whether the audio data was set externally and cannot be copied back unchanged.
    /// Equivalent to: protected dataOverridden = true
    pub data_overridden: bool,

    /// Vorbis-compressed audio data (SF3 format).
    /// `None` if the sample is not compressed.
    /// Equivalent to: protected compressedData?: Uint8Array
    pub compressed_data: Option<Vec<u8>>,

    /// Decoded float32 PCM audio data.
    /// Lazily populated on the first call to `get_audio_data()`.
    /// Equivalent to: protected audioData?: Float32Array
    pub audio_data: Option<Vec<f32>>,
}

impl BasicSample {
    /// Creates a new `BasicSample`.
    ///
    /// Equivalent to:
    /// ```ts
    /// constructor(sampleName, sampleRate, originalKey, pitchCorrection,
    ///             sampleType, loopStart, loopEnd)
    /// ```
    pub fn new(
        name: String,
        sample_rate: u32,
        original_key: u8,
        pitch_correction: i8,
        sample_type: SampleType,
        loop_start: u32,
        loop_end: u32,
    ) -> Self {
        Self {
            name,
            sample_rate,
            original_key,
            pitch_correction,
            sample_type,
            loop_start,
            loop_end,
            linked_sample_idx: None,
            linked_to: Vec::new(),
            data_overridden: true,
            compressed_data: None,
            audio_data: None,
        }
    }

    /// Creates an empty sample with default values.
    /// Equivalent to: `new EmptySample()`
    pub fn empty() -> Self {
        Self::new(
            String::new(),
            44_100,
            60,
            0,
            sample_types::MONO_SAMPLE,
            0,
            0,
        )
    }

    // -----------------------------------------------------------------------
    // Getters
    // -----------------------------------------------------------------------

    /// Returns `true` if the sample is compressed using vorbis (SF3 format).
    /// Equivalent to: `public get isCompressed(): boolean`
    #[inline]
    pub fn is_compressed(&self) -> bool {
        self.compressed_data.is_some()
    }

    /// Returns `true` if this sample is part of a stereo pair (left, right, or linked).
    /// Equivalent to: `public get isLinked(): boolean`
    #[inline]
    pub fn is_linked(&self) -> bool {
        matches!(
            self.sample_type,
            sample_types::RIGHT_SAMPLE | sample_types::LEFT_SAMPLE | sample_types::LINKED_SAMPLE
        )
    }

    /// Returns the number of instruments currently referencing this sample.
    /// Equivalent to: `public get useCount()`
    #[inline]
    pub fn use_count(&self) -> usize {
        self.linked_to.len()
    }

    // -----------------------------------------------------------------------
    // Audio data access
    // -----------------------------------------------------------------------

    /// Returns a reference to the decoded float32 PCM audio data.
    ///
    /// Decodes vorbis-compressed data on first access and caches the result.
    /// Panics if neither audio data nor compressed data is available.
    ///
    /// Equivalent to: `public getAudioData(): Float32Array`
    pub fn get_audio_data(&mut self) -> &[f32] {
        if self.audio_data.is_none() {
            if self.is_compressed() {
                self.decode_vorbis_into_audio_data();
            } else {
                panic!(
                    "Sample data is undefined for BasicSample '{}'. \
                     The file may be corrupted.",
                    self.name
                );
            }
        }
        self.audio_data.as_deref().unwrap()
    }

    /// Replaces the audio data in-place with new float32 PCM samples.
    /// Clears any compressed data and marks the sample as overridden.
    ///
    /// Equivalent to:
    /// ```ts
    /// public setAudioData(audioData: Float32Array, sampleRate: number)
    /// ```
    pub fn set_audio_data(&mut self, audio_data: Vec<f32>, sample_rate: u32) {
        self.audio_data = Some(audio_data);
        self.sample_rate = sample_rate;
        self.data_overridden = true;
        self.compressed_data = None;
    }

    /// Sets compressed vorbis data and flags the sample as compressed.
    /// Clears any decoded audio data cache.
    ///
    /// Equivalent to:
    /// ```ts
    /// public setCompressedData(data: Uint8Array)
    /// ```
    pub fn set_compressed_data(&mut self, data: Vec<u8>) {
        self.audio_data = None;
        self.compressed_data = Some(data);
        self.data_overridden = false;
    }

    /// Returns the raw bytes for writing: vorbis bytes if allowed and present,
    /// otherwise s16le PCM.
    ///
    /// Ensures audio data is decoded before encoding when vorbis is not used.
    ///
    /// Equivalent to:
    /// ```ts
    /// public getRawData(allowVorbis: boolean): Uint8Array
    /// ```
    pub fn get_raw_data(&mut self, allow_vorbis: bool) -> Vec<u8> {
        if let Some(ref cd) = self.compressed_data
            && allow_vorbis && !self.data_overridden
        {
            return cd.clone();
        }
        // Ensure audio data is decoded so encode_s16le can use it.
        if self.audio_data.is_none() {
            if self.is_compressed() {
                self.decode_vorbis_into_audio_data();
            } else {
                panic!(
                    "get_raw_data: no audio data available for sample '{}'",
                    self.name
                );
            }
        }
        self.encode_s16le()
    }

    // -----------------------------------------------------------------------
    // Audio manipulation
    // -----------------------------------------------------------------------

    /// Resamples the audio data to a new sample rate using nearest-neighbour interpolation.
    /// Adjusts `loop_start`, `loop_end`, and `sample_rate` accordingly.
    ///
    /// Equivalent to:
    /// ```ts
    /// public resampleData(newSampleRate: number)
    /// ```
    pub fn resample_data(&mut self, new_sample_rate: u32) {
        // Clone the data first so we can mutate self afterward.
        let audio_data = self.get_audio_data().to_vec();
        let ratio = new_sample_rate as f64 / self.sample_rate as f64;
        let new_len = (audio_data.len() as f64 * ratio) as usize;
        let inv_ratio = 1.0 / ratio;
        let resampled: Vec<f32> = (0..new_len)
            .map(|i| {
                let src_idx =
                    ((i as f64 * inv_ratio) as usize).min(audio_data.len().saturating_sub(1));
                audio_data[src_idx]
            })
            .collect();
        self.loop_start = (self.loop_start as f64 * ratio) as u32;
        self.loop_end = (self.loop_end as f64 * ratio) as u32;
        self.sample_rate = new_sample_rate;
        self.audio_data = Some(resampled);
    }

    // -----------------------------------------------------------------------
    // Sample type / stereo link management
    // -----------------------------------------------------------------------

    /// Sets the sample type.
    /// If the new type is not a stereo link type, clears `linked_sample_idx`.
    ///
    /// Note: Mutually updating the stereo partner's type must be handled at
    /// the `BasicSoundBank` level (same as TypeScript's `setSampleType` side effect).
    ///
    /// Equivalent to: `public setSampleType(type: SampleType)`
    pub fn set_sample_type(&mut self, sample_type: SampleType) {
        if (sample_type & 0x80_00) != 0 {
            panic!("ROM samples are not supported.");
        }
        self.sample_type = sample_type;
        if !self.is_linked() {
            self.linked_sample_idx = None;
        }
    }

    /// Unlinks the sample from its stereo pair, setting it to mono.
    /// Equivalent to: `public unlinkSample()`
    pub fn unlink_sample(&mut self) {
        self.set_sample_type(sample_types::MONO_SAMPLE);
    }

    // -----------------------------------------------------------------------
    // Instrument back-reference management
    // -----------------------------------------------------------------------

    /// Registers an instrument (by index) as a user of this sample.
    /// Equivalent to: `public linkTo(instrument: BasicInstrument)`
    pub fn link_to(&mut self, instrument_idx: usize) {
        self.linked_to.push(instrument_idx);
    }

    /// Removes the registration of an instrument (by index) from this sample.
    /// Logs a warning if the instrument was not registered.
    /// Equivalent to: `public unlinkFrom(instrument: BasicInstrument)`
    pub fn unlink_from(&mut self, instrument_idx: usize) {
        if let Some(pos) = self.linked_to.iter().position(|&i| i == instrument_idx) {
            self.linked_to.remove(pos);
        } else {
            spessa_synth_warn(&format!(
                "Cannot unlink instrument {} from '{}': not linked.",
                instrument_idx, self.name
            ));
        }
    }

    // -----------------------------------------------------------------------
    // Encoding
    // -----------------------------------------------------------------------

    /// Encodes the cached audio data as signed 16-bit little-endian PCM bytes.
    /// Panics if `audio_data` is `None` (call `get_audio_data()` first).
    ///
    /// Equivalent to: `protected encodeS16LE(): IndexedByteArray`
    pub fn encode_s16le(&self) -> Vec<u8> {
        let data = self
            .audio_data
            .as_deref()
            .expect("encode_s16le: audio_data must be set before calling this method");
        let mut out = Vec::with_capacity(data.len() * 2);
        for &sample in data {
            let scaled = sample * 32_768.0;
            let clamped: i16 = if scaled > 32_767.0 {
                32_767
            } else if scaled < -32_768.0 {
                -32_768
            } else {
                scaled as i16
            };
            out.extend_from_slice(&clamped.to_le_bytes());
        }
        out
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Decodes vorbis-compressed data into `audio_data`.
    ///
    /// TODO: implement with lewton / symphonia after adding to Cargo.toml.
    /// Currently fills with silence to avoid panics during non-SF3 use cases.
    ///
    /// Equivalent to: `protected decodeVorbis(): Float32Array`
    fn decode_vorbis_into_audio_data(&mut self) {
        // TODO: Add vorbis decoding crate (lewton or symphonia) to Cargo.toml.
        // Equivalent to the stbvorbis.decode() call in TypeScript.
        spessa_synth_warn(&format!(
            "decode_vorbis not implemented for sample '{}'. Filling with silence.",
            self.name
        ));
        let silence_len = (self.loop_end as usize).saturating_add(1);
        self.audio_data = Some(vec![0.0f32; silence_len]);
    }

    /// Returns the `RESAMPLE_RATE` constant (exposed for tests / utilities).
    /// Equivalent to: `const RESAMPLE_RATE = 48_000`
    #[inline]
    pub fn resample_rate() -> u32 {
        RESAMPLE_RATE
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::enums::sample_types as st;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_pcm_sample(loop_end: u32) -> BasicSample {
        let mut s = BasicSample::new(
            "Test".to_string(),
            44_100,
            60,
            0,
            st::MONO_SAMPLE,
            0,
            loop_end,
        );
        let data: Vec<f32> = (0..=loop_end).map(|i| i as f32 / loop_end as f32).collect();
        s.set_audio_data(data, 44_100);
        s
    }

    // -----------------------------------------------------------------------
    // BasicSample::new
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_stores_name() {
        let s = BasicSample::new("Piano".to_string(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 100);
        assert_eq!(s.name, "Piano");
    }

    #[test]
    fn test_new_stores_sample_rate() {
        let s = BasicSample::new(String::new(), 22_050, 60, 0, st::MONO_SAMPLE, 0, 0);
        assert_eq!(s.sample_rate, 22_050);
    }

    #[test]
    fn test_new_stores_original_key() {
        let s = BasicSample::new(String::new(), 44_100, 72, 0, st::MONO_SAMPLE, 0, 0);
        assert_eq!(s.original_key, 72);
    }

    #[test]
    fn test_new_stores_pitch_correction_negative() {
        let s = BasicSample::new(String::new(), 44_100, 60, -10, st::MONO_SAMPLE, 0, 0);
        assert_eq!(s.pitch_correction, -10);
    }

    #[test]
    fn test_new_stores_sample_type() {
        let s = BasicSample::new(String::new(), 44_100, 60, 0, st::LEFT_SAMPLE, 0, 0);
        assert_eq!(s.sample_type, st::LEFT_SAMPLE);
    }

    #[test]
    fn test_new_stores_loop_start_and_end() {
        let s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 10, 200);
        assert_eq!(s.loop_start, 10);
        assert_eq!(s.loop_end, 200);
    }

    #[test]
    fn test_new_linked_sample_idx_is_none() {
        let s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        assert!(s.linked_sample_idx.is_none());
    }

    #[test]
    fn test_new_linked_to_is_empty() {
        let s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        assert!(s.linked_to.is_empty());
    }

    #[test]
    fn test_new_data_overridden_is_true() {
        let s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        assert!(s.data_overridden);
    }

    #[test]
    fn test_new_compressed_data_is_none() {
        let s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        assert!(s.compressed_data.is_none());
    }

    #[test]
    fn test_new_audio_data_is_none() {
        let s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        assert!(s.audio_data.is_none());
    }

    // -----------------------------------------------------------------------
    // BasicSample::empty
    // -----------------------------------------------------------------------

    #[test]
    fn test_empty_name_is_empty_string() {
        let s = BasicSample::empty();
        assert_eq!(s.name, "");
    }

    #[test]
    fn test_empty_sample_rate_is_44100() {
        let s = BasicSample::empty();
        assert_eq!(s.sample_rate, 44_100);
    }

    #[test]
    fn test_empty_original_key_is_60() {
        let s = BasicSample::empty();
        assert_eq!(s.original_key, 60);
    }

    #[test]
    fn test_empty_sample_type_is_mono() {
        let s = BasicSample::empty();
        assert_eq!(s.sample_type, st::MONO_SAMPLE);
    }

    #[test]
    fn test_empty_loop_points_are_zero() {
        let s = BasicSample::empty();
        assert_eq!(s.loop_start, 0);
        assert_eq!(s.loop_end, 0);
    }

    // -----------------------------------------------------------------------
    // is_compressed
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_compressed_false_by_default() {
        let s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        assert!(!s.is_compressed());
    }

    #[test]
    fn test_is_compressed_true_after_set_compressed_data() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.set_compressed_data(vec![1, 2, 3]);
        assert!(s.is_compressed());
    }

    // -----------------------------------------------------------------------
    // is_linked
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_linked_false_for_mono() {
        let s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        assert!(!s.is_linked());
    }

    #[test]
    fn test_is_linked_true_for_left() {
        let s = BasicSample::new(String::new(), 44_100, 60, 0, st::LEFT_SAMPLE, 0, 0);
        assert!(s.is_linked());
    }

    #[test]
    fn test_is_linked_true_for_right() {
        let s = BasicSample::new(String::new(), 44_100, 60, 0, st::RIGHT_SAMPLE, 0, 0);
        assert!(s.is_linked());
    }

    #[test]
    fn test_is_linked_true_for_linked() {
        let s = BasicSample::new(String::new(), 44_100, 60, 0, st::LINKED_SAMPLE, 0, 0);
        assert!(s.is_linked());
    }

    // -----------------------------------------------------------------------
    // use_count
    // -----------------------------------------------------------------------

    #[test]
    fn test_use_count_zero_initially() {
        let s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        assert_eq!(s.use_count(), 0);
    }

    #[test]
    fn test_use_count_increments_after_link_to() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.link_to(0);
        s.link_to(1);
        assert_eq!(s.use_count(), 2);
    }

    // -----------------------------------------------------------------------
    // link_to / unlink_from
    // -----------------------------------------------------------------------

    #[test]
    fn test_link_to_adds_instrument_idx() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.link_to(5);
        assert_eq!(s.linked_to, vec![5]);
    }

    #[test]
    fn test_link_to_allows_duplicates() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.link_to(3);
        s.link_to(3);
        assert_eq!(s.linked_to, vec![3, 3]);
    }

    #[test]
    fn test_unlink_from_removes_first_occurrence() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.link_to(2);
        s.link_to(2);
        s.unlink_from(2);
        assert_eq!(s.linked_to.len(), 1);
        assert_eq!(s.linked_to[0], 2);
    }

    #[test]
    fn test_unlink_from_nonexistent_does_not_panic() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.unlink_from(99); // Should warn but not panic
    }

    #[test]
    fn test_link_to_then_unlink_from_empties_list() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.link_to(7);
        s.unlink_from(7);
        assert!(s.linked_to.is_empty());
    }

    // -----------------------------------------------------------------------
    // set_audio_data
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_audio_data_stores_data() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.set_audio_data(vec![0.1, 0.2], 22_050);
        assert_eq!(s.audio_data.as_ref().unwrap(), &[0.1, 0.2]);
    }

    #[test]
    fn test_set_audio_data_updates_sample_rate() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.set_audio_data(vec![], 22_050);
        assert_eq!(s.sample_rate, 22_050);
    }

    #[test]
    fn test_set_audio_data_sets_data_overridden() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.data_overridden = false;
        s.set_audio_data(vec![], 44_100);
        assert!(s.data_overridden);
    }

    #[test]
    fn test_set_audio_data_clears_compressed_data() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.compressed_data = Some(vec![1, 2, 3]);
        s.set_audio_data(vec![], 44_100);
        assert!(s.compressed_data.is_none());
    }

    // -----------------------------------------------------------------------
    // set_compressed_data
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_compressed_data_stores_data() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.set_compressed_data(vec![0xAB, 0xCD]);
        assert_eq!(s.compressed_data.as_ref().unwrap(), &[0xAB, 0xCD]);
    }

    #[test]
    fn test_set_compressed_data_clears_audio_data() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.audio_data = Some(vec![0.5]);
        s.set_compressed_data(vec![1]);
        assert!(s.audio_data.is_none());
    }

    #[test]
    fn test_set_compressed_data_clears_data_overridden() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        assert!(s.data_overridden);
        s.set_compressed_data(vec![1]);
        assert!(!s.data_overridden);
    }

    // -----------------------------------------------------------------------
    // get_audio_data
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_audio_data_returns_stored_data() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 3);
        s.set_audio_data(vec![0.1, 0.2, 0.3], 44_100);
        let data = s.get_audio_data();
        assert_eq!(data, &[0.1, 0.2, 0.3]);
    }

    #[test]
    fn test_get_audio_data_returns_same_on_second_call() {
        let mut s = make_pcm_sample(99);
        let first: Vec<f32> = s.get_audio_data().to_vec();
        let second: Vec<f32> = s.get_audio_data().to_vec();
        assert_eq!(first, second);
    }

    #[test]
    #[should_panic(expected = "Sample data is undefined")]
    fn test_get_audio_data_panics_when_no_data() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        // Neither audio_data nor compressed_data set
        s.get_audio_data();
    }

    // -----------------------------------------------------------------------
    // get_raw_data
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_raw_data_returns_compressed_when_allowed() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.set_compressed_data(vec![0x01, 0x02, 0x03]);
        let raw = s.get_raw_data(true);
        assert_eq!(raw, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_get_raw_data_encodes_pcm_when_vorbis_not_allowed() {
        let mut s = make_pcm_sample(3);
        // Override with known data: all zeros → all zero bytes in s16le
        s.set_audio_data(vec![0.0, 0.0, 0.0, 0.0], 44_100);
        s.set_compressed_data(vec![0xFF]); // present but not allowed
        // data_overridden is false after set_compressed_data, re-set:
        s.set_audio_data(vec![0.0, 0.0], 44_100); // resets data_overridden=true
        let raw = s.get_raw_data(true);
        // vorbis not returned because data_overridden = true
        assert_eq!(raw.len(), 4); // 2 f32 samples → 4 bytes s16le
    }

    #[test]
    fn test_get_raw_data_not_overridden_not_allowed_returns_pcm() {
        // compressed present, allow_vorbis=false → encodes PCM
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.set_compressed_data(vec![0xFF]); // data_overridden = false
        // Manually set audio_data so encode_s16le works
        s.audio_data = Some(vec![0.0]);
        let raw = s.get_raw_data(false);
        // 1 sample → 2 bytes (both 0)
        assert_eq!(raw, vec![0, 0]);
    }

    // -----------------------------------------------------------------------
    // encode_s16le
    // -----------------------------------------------------------------------

    #[test]
    fn test_encode_s16le_zero_sample() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.set_audio_data(vec![0.0], 44_100);
        assert_eq!(s.encode_s16le(), vec![0x00, 0x00]);
    }

    #[test]
    fn test_encode_s16le_positive_half() {
        // 0.5 * 32768 = 16384 = 0x4000
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.set_audio_data(vec![0.5], 44_100);
        let raw = s.encode_s16le();
        let val = i16::from_le_bytes([raw[0], raw[1]]);
        assert_eq!(val, 16384);
    }

    #[test]
    fn test_encode_s16le_negative_half() {
        // -0.5 * 32768 = -16384
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.set_audio_data(vec![-0.5], 44_100);
        let raw = s.encode_s16le();
        let val = i16::from_le_bytes([raw[0], raw[1]]);
        assert_eq!(val, -16384);
    }

    #[test]
    fn test_encode_s16le_clamps_positive_overflow() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.set_audio_data(vec![2.0], 44_100); // > 1.0
        let raw = s.encode_s16le();
        let val = i16::from_le_bytes([raw[0], raw[1]]);
        assert_eq!(val, 32_767);
    }

    #[test]
    fn test_encode_s16le_clamps_negative_overflow() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.set_audio_data(vec![-2.0], 44_100); // < -1.0
        let raw = s.encode_s16le();
        let val = i16::from_le_bytes([raw[0], raw[1]]);
        assert_eq!(val, -32_768);
    }

    #[test]
    fn test_encode_s16le_output_length() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.set_audio_data(vec![0.0; 10], 44_100);
        assert_eq!(s.encode_s16le().len(), 20); // 10 samples × 2 bytes
    }

    #[test]
    fn test_encode_s16le_little_endian_order() {
        // 256 = 0x0100: LE = [0x00, 0x01]
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        let val = 256.0 / 32_768.0;
        s.set_audio_data(vec![val], 44_100);
        let raw = s.encode_s16le();
        assert_eq!(raw, vec![0x00, 0x01]);
    }

    #[test]
    #[should_panic(expected = "audio_data must be set")]
    fn test_encode_s16le_panics_without_audio_data() {
        let s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.encode_s16le();
    }

    // -----------------------------------------------------------------------
    // resample_data
    // -----------------------------------------------------------------------

    #[test]
    fn test_resample_data_updates_sample_rate() {
        let mut s = make_pcm_sample(99);
        s.resample_data(22_050);
        assert_eq!(s.sample_rate, 22_050);
    }

    #[test]
    fn test_resample_data_output_length_halved() {
        let mut s = make_pcm_sample(99);
        let original_len = s.audio_data.as_ref().unwrap().len();
        s.resample_data(22_050); // half of 44_100
        let new_len = s.audio_data.as_ref().unwrap().len();
        assert_eq!(new_len, original_len / 2);
    }

    #[test]
    fn test_resample_data_output_length_doubled() {
        let mut s = make_pcm_sample(99);
        let original_len = s.audio_data.as_ref().unwrap().len();
        s.resample_data(88_200); // double of 44_100
        let new_len = s.audio_data.as_ref().unwrap().len();
        assert_eq!(new_len, original_len * 2);
    }

    #[test]
    fn test_resample_data_adjusts_loop_start() {
        let mut s = make_pcm_sample(99);
        s.loop_start = 10;
        s.resample_data(88_200); // ×2
        assert_eq!(s.loop_start, 20);
    }

    #[test]
    fn test_resample_data_adjusts_loop_end() {
        let mut s = make_pcm_sample(99);
        s.loop_end = 80;
        s.resample_data(88_200); // ×2
        assert_eq!(s.loop_end, 160);
    }

    // -----------------------------------------------------------------------
    // set_sample_type / unlink_sample
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_sample_type_updates_type() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.set_sample_type(st::LEFT_SAMPLE);
        assert_eq!(s.sample_type, st::LEFT_SAMPLE);
    }

    #[test]
    fn test_set_sample_type_clears_linked_sample_idx_when_non_linked_type() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::LEFT_SAMPLE, 0, 0);
        s.linked_sample_idx = Some(3);
        s.set_sample_type(st::MONO_SAMPLE);
        assert!(s.linked_sample_idx.is_none());
    }

    #[test]
    fn test_set_sample_type_keeps_linked_sample_idx_for_linked_types() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::LEFT_SAMPLE, 0, 0);
        s.linked_sample_idx = Some(3);
        s.set_sample_type(st::RIGHT_SAMPLE); // still linked
        assert_eq!(s.linked_sample_idx, Some(3));
    }

    #[test]
    #[should_panic(expected = "ROM samples are not supported")]
    fn test_set_sample_type_panics_for_rom_type() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::MONO_SAMPLE, 0, 0);
        s.set_sample_type(st::ROM_MONO_SAMPLE);
    }

    #[test]
    fn test_unlink_sample_sets_mono() {
        let mut s = BasicSample::new(String::new(), 44_100, 60, 0, st::LEFT_SAMPLE, 0, 0);
        s.linked_sample_idx = Some(1);
        s.unlink_sample();
        assert_eq!(s.sample_type, st::MONO_SAMPLE);
        assert!(s.linked_sample_idx.is_none());
    }

    // -----------------------------------------------------------------------
    // resample_rate constant
    // -----------------------------------------------------------------------

    #[test]
    fn test_resample_rate_is_48000() {
        assert_eq!(BasicSample::resample_rate(), 48_000);
    }

    // -----------------------------------------------------------------------
    // Clone
    // -----------------------------------------------------------------------

    #[test]
    fn test_clone_produces_independent_copy() {
        let mut s = make_pcm_sample(9);
        let mut cloned = s.clone();
        cloned.set_audio_data(vec![99.0], 22_050);
        // original should be unchanged
        assert_eq!(s.get_audio_data().len(), 10);
        assert_eq!(s.sample_rate, 44_100);
    }
}
