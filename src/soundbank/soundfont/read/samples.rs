/// samples.rs
/// purpose: Parses SoundFont sample headers and audio data.
/// Ported from: src/soundbank/soundfont/read/samples.ts
use crate::soundbank::enums::{SampleType, sample_types};
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::{read_little_endian_indexed, signed_int8};
use crate::utils::loggin::{spessa_synth_info, spessa_synth_warn};
use crate::utils::riff_chunk::RIFFChunk;
use crate::utils::string::read_binary_string_indexed;

/// SF3 compression bit flag in sampleType field.
/// Equivalent to: SF3_BIT_FLIT = 0x10
pub const SF3_BIT_FLIT: u16 = 0x10;

/// Audio data source for SoundFontSample construction.
/// Equivalent to: `IndexedByteArray | Float32Array` union type in TypeScript.
pub enum SmplData<'a> {
    /// SF2 or SF3: an indexed byte array; `current_index` is the smpl chunk data start offset.
    Indexed(&'a IndexedByteArray),
    /// SF2Pack: pre-decoded float32 PCM data (entire smpl as float).
    Float32(&'a [f32]),
}

/// Represents a SoundFont sample, combining both `BasicSample` and `SoundFontSample` fields.
/// Equivalent to: `class SoundFontSample extends BasicSample` in TypeScript.
///
/// Note: `BasicSample` (dep 4) is not yet ported; its fields are inlined here.
/// `linkTo`/`unlinkFrom` back-references are tracked as `Vec<usize>` instrument indices.
#[derive(Debug)]
pub struct SoundFontSample {
    // === Inlined BasicSample fields ===
    /// The sample's name.
    pub name: String,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Original pitch as a MIDI note number (0–127).
    pub original_key: u8,
    /// Pitch correction in cents, can be negative.
    pub pitch_correction: i8,
    /// SF2 sample type (mono, left, right, linked, ROM variants).
    pub sample_type: SampleType,
    /// Loop start relative to sample start, in sample points.
    pub loop_start: i64,
    /// Loop end relative to sample start, in sample points.
    pub loop_end: i64,
    /// Instrument indices that use this sample (TODO: linkTo/unlinkFrom when BasicSample ported).
    pub linked_to: Vec<usize>,
    /// True when audio data was set externally (SF2Pack), preventing raw copy.
    data_overridden: bool,
    /// Compressed (SF3 vorbis) data, if applicable.
    compressed_data: Option<Vec<u8>>,
    /// Decoded float32 PCM cache.
    audio_data: Option<Vec<f32>>,

    // === SoundFontSample-specific fields ===
    /// Raw SF2 linked-sample index from the shdr record.
    pub linked_sample_index: usize,
    /// Resolved index of the stereo partner in the samples `Vec` after linking.
    pub linked_sample_idx: Option<usize>,
    /// Raw s16le bytes sliced from the smpl chunk (SF2 only; None for SF3/SF2Pack).
    s16le_data: Option<Vec<u8>>,
    /// Start byte offset within the smpl chunk.
    pub start_byte_offset: usize,
    /// End byte offset within the smpl chunk.
    pub end_byte_offset: usize,
    /// Index of this record in the shdr list (i.e., sample ID).
    pub sample_id: usize,
}

impl SoundFontSample {
    /// Creates a new `SoundFontSample` from parsed shdr fields and smpl data.
    /// Equivalent to: `new SoundFontSample(...)` constructor in TypeScript.
    ///
    /// * `sample_start_index` – byte offset into smpl (raw shdr u32 × 2).
    /// * `sample_end_index`   – byte offset into smpl (raw shdr u32 × 2).
    /// * `sample_loop_start_index` – absolute sample-point index from shdr.
    /// * `sample_loop_end_index`   – absolute sample-point index from shdr.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sample_name: String,
        sample_start_index: usize,
        sample_end_index: usize,
        sample_loop_start_index: i64,
        sample_loop_end_index: i64,
        sample_rate: u32,
        sample_pitch: u8,
        sample_pitch_correction: i8,
        linked_sample_index: usize,
        mut sample_type: SampleType,
        smpl_data: &SmplData,
        sample_index: usize,
    ) -> Self {
        // Check SF3 compression flag.
        // https://github.com/FluidSynth/fluidsynth/wiki/SoundFont3Format
        let compressed = (sample_type & SF3_BIT_FLIT) > 0;
        // Remove the compression flag from sampleType.
        sample_type &= !SF3_BIT_FLIT;

        // Compute loop points relative to the sample start, in sample points.
        // Equivalent to: sampleLoopStartIndex - sampleStartIndex / 2
        let mut loop_start = sample_loop_start_index - (sample_start_index as i64 / 2);
        let mut loop_end = sample_loop_end_index - (sample_start_index as i64 / 2);

        let mut s16le_data: Option<Vec<u8>> = None;
        let mut audio_data: Option<Vec<f32>> = None;
        let mut compressed_data: Option<Vec<u8>> = None;
        let data_overridden: bool;

        match smpl_data {
            SmplData::Indexed(iba) => {
                // Capture smpl chunk start offset (equivalent to: smplStart = sampleDataArray.currentIndex)
                let smpl_start = iba.current_index;

                if compressed {
                    // SF3: correct loop points back to absolute indices, then slice vorbis bytes.
                    loop_start += sample_start_index as i64 / 2;
                    loop_end += sample_start_index as i64 / 2;
                    let start = (sample_start_index / 2 + smpl_start).min(iba.len());
                    let end = (sample_end_index / 2 + smpl_start).min(iba.len());
                    // Deref IndexedByteArray → &[u8] for range slicing
                    let raw: &[u8] = iba;
                    compressed_data = Some(raw[start..end].to_vec());
                    data_overridden = false;
                } else {
                    // Regular SF2: slice s16le bytes.
                    let start = (smpl_start + sample_start_index).min(iba.len());
                    let end = (smpl_start + sample_end_index).min(iba.len());
                    // Deref IndexedByteArray → &[u8] for range slicing
                    let raw: &[u8] = iba;
                    s16le_data = Some(raw[start..end].to_vec());
                    data_overridden = false;
                }
            }
            SmplData::Float32(f32_data) => {
                // SF2Pack: float32 array decoded from vorbis, copy the relevant slice.
                // smplStart = 0 for Float32.
                let start = (sample_start_index / 2).min(f32_data.len());
                let end = (sample_end_index / 2).min(f32_data.len());
                audio_data = Some(f32_data[start..end].to_vec());
                data_overridden = true;
            }
        }

        Self {
            name: sample_name,
            sample_rate,
            original_key: sample_pitch,
            pitch_correction: sample_pitch_correction,
            sample_type,
            loop_start,
            loop_end,
            linked_to: Vec::new(),
            data_overridden,
            compressed_data,
            audio_data,
            linked_sample_index,
            linked_sample_idx: None,
            s16le_data,
            start_byte_offset: sample_start_index,
            end_byte_offset: sample_end_index,
            sample_id: sample_index,
        }
    }

    /// Whether the sample is compressed with SF3 (vorbis).
    /// Equivalent to: `get isCompressed(): boolean`
    pub fn is_compressed(&self) -> bool {
        self.compressed_data.is_some()
    }

    /// Whether this sample is part of a stereo pair (left, right, or linked).
    /// Equivalent to: `get isLinked(): boolean`
    pub fn is_linked(&self) -> bool {
        matches!(
            self.sample_type,
            sample_types::RIGHT_SAMPLE | sample_types::LEFT_SAMPLE | sample_types::LINKED_SAMPLE
        )
    }

    /// Sets the sample type to mono and clears the stereo partner reference.
    /// Equivalent to: `unlinkSample()`
    pub fn unlink_sample(&mut self) {
        self.sample_type = sample_types::MONO_SAMPLE;
        self.linked_sample_idx = None;
    }

    /// Loads and caches the float32 audio data.
    ///
    /// - SF2Pack: returns the pre-decoded float32 directly.
    /// - SF2: converts s16le bytes to float32.
    /// - SF3 (vorbis): not yet implemented; returns `Err`.
    ///
    /// Equivalent to: `getAudioData(): Float32Array`
    pub fn get_audio_data(&mut self) -> Result<Vec<f32>, String> {
        // Return cached data if already decoded.
        if let Some(ref data) = self.audio_data {
            return Ok(data.clone());
        }

        // SF3 vorbis decoding (equivalent to super.getAudioData() in TypeScript)
        if self.is_compressed() {
            // TODO: SF3 vorbis decoding via an external crate (lewton or similar)
            return Err(format!(
                "SF3 vorbis decoding not yet implemented for sample \"{}\"",
                self.name
            ));
        }

        let s16le = match &self.s16le_data {
            Some(data) => data,
            None => {
                return Err(format!(
                    "Unexpected lack of audio data for sample \"{}\"",
                    self.name
                ));
            }
        };

        // Validate byte length.
        let byte_length = self.end_byte_offset.saturating_sub(self.start_byte_offset);
        if byte_length < 1 {
            spessa_synth_warn(&format!(
                "Invalid sample {}! Invalid length: {}",
                self.name, byte_length
            ));
            return Ok(vec![0.0]);
        }

        // Convert s16le PCM → float32.
        // Equivalent to: audioData[i] = element / 32_768
        let samples_count = s16le.len() / 2;
        let mut decoded = Vec::with_capacity(samples_count);
        for i in 0..samples_count {
            let lo = s16le[i * 2] as i16;
            let hi = (s16le[i * 2 + 1] as i16) << 8;
            let pcm = lo | hi;
            decoded.push(pcm as f32 / 32_768.0);
        }

        self.audio_data = Some(decoded.clone());
        Ok(decoded)
    }

    /// Returns the raw byte data suitable for writing a SoundFont file.
    ///
    /// - SF3 (vorbis): returns the compressed bytes if `allow_vorbis` is true.
    /// - SF2: returns the s16le bytes directly.
    /// - SF2Pack / overridden: returns empty (TODO: encode from float32).
    ///
    /// Equivalent to: `getRawData(allowVorbis: boolean): Uint8Array`
    pub fn get_raw_data(&self, allow_vorbis: bool) -> Vec<u8> {
        if self.data_overridden || self.compressed_data.is_some() {
            if let Some(ref cd) = self.compressed_data
                && allow_vorbis && !self.data_overridden
            {
                return cd.clone();
            }
            // TODO: encode s16le from audio_data (BasicSample::encodeS16LE)
            return Vec::new();
        }
        // Return the raw s16le bytes sliced from the smpl chunk.
        self.s16le_data.clone().unwrap_or_default()
    }
}

/// Resolves stereo sample pair links after all samples are loaded.
///
/// For each sample whose `sample_type` indicates it is part of a stereo pair,
/// this finds the linked sample by `linked_sample_index` and sets mutual
/// `linked_sample_idx` references, adjusting `sample_type` for the partner.
///
/// Equivalent to: `for (const s of samples) s.getLinkedSample(samples)`
pub fn link_soundfont_samples(samples: &mut [SoundFontSample]) {
    let count = samples.len();
    for i in 0..count {
        // Skip if already resolved or not a stereo type.
        if !samples[i].is_linked() || samples[i].linked_sample_idx.is_some() {
            continue;
        }
        let linked_idx = samples[i].linked_sample_index;
        if linked_idx >= count {
            spessa_synth_info(&format!(
                "Invalid linked sample for {}. Setting to mono.",
                samples[i].name
            ));
            samples[i].unlink_sample();
            continue;
        }
        // Check for corrupted files: the target is already linked to someone else.
        if samples[linked_idx].linked_sample_idx.is_some() {
            spessa_synth_info(&format!(
                "Invalid linked sample for {}: {} is already linked to another sample.",
                samples[i].name, samples[linked_idx].name
            ));
            samples[i].unlink_sample();
        } else {
            // Set mutual link and adjust partner's sample type.
            let i_type = samples[i].sample_type;
            samples[i].linked_sample_idx = Some(linked_idx);
            samples[linked_idx].linked_sample_idx = Some(i);
            match i_type {
                sample_types::LEFT_SAMPLE => {
                    samples[linked_idx].sample_type = sample_types::RIGHT_SAMPLE;
                }
                sample_types::RIGHT_SAMPLE => {
                    samples[linked_idx].sample_type = sample_types::LEFT_SAMPLE;
                }
                sample_types::LINKED_SAMPLE => {
                    samples[linked_idx].sample_type = sample_types::LINKED_SAMPLE;
                }
                _ => {}
            }
        }
    }
}

/// Reads all sample headers from a shdr RIFF chunk, slices audio from `smpl_chunk_data`,
/// and optionally links stereo pairs.
///
/// Equivalent to: `export function readSamples(sampleHeadersChunk, smplChunkData, linkSamples)`
pub fn read_samples(
    sample_headers_chunk: &mut RIFFChunk,
    smpl_chunk_data: &SmplData,
    link_samples: bool,
) -> Vec<SoundFontSample> {
    let mut samples = Vec::new();
    let mut index = 0usize;

    while sample_headers_chunk.data.len() > sample_headers_chunk.data.current_index {
        let sample = read_sample(index, &mut sample_headers_chunk.data, smpl_chunk_data);
        samples.push(sample);
        index += 1;
    }

    // Remove the EOS (End-of-Samples) sentinel record.
    samples.pop();

    if link_samples {
        link_soundfont_samples(&mut samples);
    }

    samples
}

/// Reads one 46-byte shdr record and constructs a `SoundFontSample`.
///
/// shdr record layout (SF2 spec, section 7.10):
/// - 20 bytes: sample name (null-padded ASCII)
/// -  4 bytes: sample start (u32 LE, in sample points)
/// -  4 bytes: sample end   (u32 LE, in sample points)
/// -  4 bytes: loop start   (u32 LE, in sample points)
/// -  4 bytes: loop end     (u32 LE, in sample points)
/// -  4 bytes: sample rate  (u32 LE, Hz)
/// -  1 byte:  original pitch (0–127, or 255 → default 60)
/// -  1 byte:  pitch correction (signed)
/// -  2 bytes: sample link (u16 LE)
/// -  2 bytes: sample type (u16 LE)
///
/// Equivalent to: `function readSample(index, sampleHeaderData, smplArrayData)`
fn read_sample(
    index: usize,
    sample_header_data: &mut IndexedByteArray,
    smpl_array_data: &SmplData,
) -> SoundFontSample {
    let sample_name = read_binary_string_indexed(sample_header_data, 20);

    // Start/end: raw u32 sample-point indices × 2 = byte offsets.
    let sample_start_index = read_little_endian_indexed(sample_header_data, 4) as usize * 2;
    let sample_end_index = read_little_endian_indexed(sample_header_data, 4) as usize * 2;

    // Loop points: raw u32 sample-point indices (absolute from smpl start).
    let sample_loop_start_index = read_little_endian_indexed(sample_header_data, 4) as i64;
    let sample_loop_end_index = read_little_endian_indexed(sample_header_data, 4) as i64;

    let sample_rate = read_little_endian_indexed(sample_header_data, 4);

    // Original pitch: clamp out-of-range values to 60.
    let raw_pitch = sample_header_data[sample_header_data.current_index];
    sample_header_data.current_index += 1;
    let sample_pitch = if raw_pitch > 127 { 60 } else { raw_pitch };

    // Pitch correction: signed byte.
    let raw_correction = sample_header_data[sample_header_data.current_index];
    sample_header_data.current_index += 1;
    let sample_pitch_correction = signed_int8(raw_correction);

    let sample_link = read_little_endian_indexed(sample_header_data, 2) as usize;
    let sample_type = read_little_endian_indexed(sample_header_data, 2) as SampleType;

    SoundFontSample::new(
        sample_name,
        sample_start_index,
        sample_end_index,
        sample_loop_start_index,
        sample_loop_end_index,
        sample_rate,
        sample_pitch,
        sample_pitch_correction,
        sample_link,
        sample_type,
        smpl_array_data,
        index,
    )
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::enums::sample_types as st;
    use crate::utils::indexed_array::IndexedByteArray;
    use crate::utils::riff_chunk::RIFFChunk;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Builds a raw 46-byte shdr record.
    fn make_shdr_record(
        name: &str,
        start_pts: u32,
        end_pts: u32,
        loop_start: u32,
        loop_end: u32,
        sample_rate: u32,
        pitch: u8,
        pitch_corr: i8,
        link: u16,
        sample_type: u16,
    ) -> Vec<u8> {
        let mut rec = vec![0u8; 46];
        // Name: copy up to 20 bytes, rest stay 0
        let name_bytes = name.as_bytes();
        let len = name_bytes.len().min(20);
        rec[..len].copy_from_slice(&name_bytes[..len]);
        // Offsets in the record
        rec[20..24].copy_from_slice(&start_pts.to_le_bytes());
        rec[24..28].copy_from_slice(&end_pts.to_le_bytes());
        rec[28..32].copy_from_slice(&loop_start.to_le_bytes());
        rec[32..36].copy_from_slice(&loop_end.to_le_bytes());
        rec[36..40].copy_from_slice(&sample_rate.to_le_bytes());
        rec[40] = pitch;
        rec[41] = pitch_corr as u8;
        rec[42..44].copy_from_slice(&link.to_le_bytes());
        rec[44..46].copy_from_slice(&sample_type.to_le_bytes());
        rec
    }

    /// Builds a shdr RIFFChunk from a list of 46-byte records.
    fn make_shdr_chunk(records: &[Vec<u8>]) -> RIFFChunk {
        let data: Vec<u8> = records.iter().flat_map(|r| r.iter().copied()).collect();
        RIFFChunk::new(
            "shdr".to_string(),
            data.len() as u32,
            IndexedByteArray::from_vec(data),
        )
    }

    /// Builds an EOS record (all zeros with name "EOS").
    fn eos_record() -> Vec<u8> {
        make_shdr_record("EOS", 0, 0, 0, 0, 44100, 60, 0, 0, 1)
    }

    // -----------------------------------------------------------------------
    // SF3_BIT_FLIT constant
    // -----------------------------------------------------------------------

    #[test]
    fn test_sf3_bit_flit_value() {
        assert_eq!(SF3_BIT_FLIT, 0x10);
    }

    // -----------------------------------------------------------------------
    // SoundFontSample::new – SF2 path (IndexedByteArray, uncompressed)
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_sf2_basic_fields() {
        // 200 bytes of s16le smpl data (100 samples)
        let smpl_bytes: Vec<u8> = (0u16..100).flat_map(|v| v.to_le_bytes()).collect();
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);

        let s = SoundFontSample::new(
            "Piano".to_string(),
            0,   // start_byte_offset
            200, // end_byte_offset
            10,  // loop_start abs
            90,  // loop_end abs
            44100,
            60,
            -5,
            0,
            st::MONO_SAMPLE,
            &smpl,
            0,
        );

        assert_eq!(s.name, "Piano");
        assert_eq!(s.sample_rate, 44100);
        assert_eq!(s.original_key, 60);
        assert_eq!(s.pitch_correction, -5);
        assert_eq!(s.sample_type, st::MONO_SAMPLE);
        assert_eq!(s.sample_id, 0);
        assert_eq!(s.start_byte_offset, 0);
        assert_eq!(s.end_byte_offset, 200);
    }

    #[test]
    fn test_new_sf2_loop_points_relative() {
        // start=200 bytes → 100 sample points. loop_start=150, loop_end=180.
        // Relative: loop_start = 150 - 100 = 50, loop_end = 180 - 100 = 80.
        let smpl_bytes = vec![0u8; 400];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);

        let s = SoundFontSample::new(
            "Test".to_string(),
            200, // start_byte_offset
            400, // end_byte_offset
            150, // loop_start (absolute sample points)
            180, // loop_end (absolute sample points)
            44100,
            60,
            0,
            0,
            st::MONO_SAMPLE,
            &smpl,
            0,
        );

        assert_eq!(s.loop_start, 50);
        assert_eq!(s.loop_end, 80);
    }

    #[test]
    fn test_new_sf2_stores_s16le_slice() {
        // smpl = 0,1,2,...,9 (10 bytes = 5 s16le samples)
        let smpl_bytes: Vec<u8> = (0..10u8).collect();
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);

        let s = SoundFontSample::new(
            "S".to_string(),
            2, // start: skip first byte → start at byte 2
            8, // end: byte 8
            0,
            0,
            44100,
            60,
            0,
            0,
            st::MONO_SAMPLE,
            &smpl,
            0,
        );

        // s16le_data should be bytes [2..8]
        assert!(s.s16le_data.is_some());
        let raw = s.s16le_data.as_ref().unwrap();
        assert_eq!(raw.len(), 6);
        assert_eq!(raw[0], 2);
        assert_eq!(raw[5], 7);
    }

    #[test]
    fn test_new_sf2_smpl_start_offset_applied() {
        // smpl data has cursor at 4 (smpl_start = 4).
        // sample start_byte_offset = 0, end_byte_offset = 4.
        // Should slice iba[4..8].
        let smpl_bytes: Vec<u8> = (0..12u8).collect();
        let mut iba = IndexedByteArray::from_vec(smpl_bytes);
        iba.current_index = 4; // smpl_start = 4
        let smpl = SmplData::Indexed(&iba);

        let s = SoundFontSample::new(
            "S".to_string(),
            0, // start_byte_offset
            4, // end_byte_offset
            0,
            0,
            44100,
            60,
            0,
            0,
            st::MONO_SAMPLE,
            &smpl,
            0,
        );

        let raw = s.s16le_data.as_ref().unwrap();
        assert_eq!(raw.len(), 4);
        // iba[4..8] = bytes 4,5,6,7
        assert_eq!(raw[0], 4);
        assert_eq!(raw[3], 7);
    }

    #[test]
    fn test_new_sf2_not_compressed() {
        let smpl_bytes = vec![0u8; 100];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);
        let s = SoundFontSample::new(
            "S".to_string(),
            0,
            100,
            0,
            0,
            44100,
            60,
            0,
            0,
            st::MONO_SAMPLE,
            &smpl,
            0,
        );
        assert!(!s.is_compressed());
    }

    // -----------------------------------------------------------------------
    // SoundFontSample::new – SF3 path (compressed)
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_sf3_strips_compression_flag() {
        let smpl_bytes = vec![0xABu8; 200];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);
        // SF3 mono = MONO_SAMPLE | SF3_BIT_FLIT = 1 | 0x10 = 0x11
        let sf3_type = st::MONO_SAMPLE | SF3_BIT_FLIT;

        let s = SoundFontSample::new(
            "SF3".to_string(),
            0,   // start_byte_offset
            200, // end_byte_offset
            10,
            90,
            44100,
            60,
            0,
            0,
            sf3_type,
            &smpl,
            0,
        );

        // Compression flag must be stripped from sample_type
        assert_eq!(s.sample_type, st::MONO_SAMPLE);
        assert!(s.is_compressed());
        assert!(s.s16le_data.is_none());
    }

    #[test]
    fn test_new_sf3_stores_compressed_slice() {
        // smpl = 0xAB×200 bytes. SF3 slice is by sample-point indices /2.
        // start_byte_offset=4, end_byte_offset=20 → slice from 4/2=2 to 20/2=10 → 8 bytes.
        let smpl_bytes = vec![0xABu8; 200];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);
        let sf3_type = st::MONO_SAMPLE | SF3_BIT_FLIT;

        let s = SoundFontSample::new(
            "SF3".to_string(),
            4,
            20,
            0,
            0,
            44100,
            60,
            0,
            0,
            sf3_type,
            &smpl,
            0,
        );

        let cd = s.compressed_data.as_ref().unwrap();
        assert_eq!(cd.len(), 8); // (20-4)/2 = 8
        assert!(cd.iter().all(|&b| b == 0xAB));
    }

    #[test]
    fn test_new_sf3_loop_points_corrected() {
        // start=100 bytes, loop_start=60 abs, loop_end=80 abs.
        // Initial relative: 60 - 100/2 = 10, 80 - 50 = 30.
        // SF3 correction: += start/2 = 50 → loop_start=60, loop_end=80 (back to absolute).
        let smpl_bytes = vec![0u8; 200];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);
        let sf3_type = st::MONO_SAMPLE | SF3_BIT_FLIT;

        let s = SoundFontSample::new(
            "SF3".to_string(),
            100, // start_byte_offset
            200, // end_byte_offset
            60,  // loop_start (absolute)
            80,  // loop_end (absolute)
            44100,
            60,
            0,
            0,
            sf3_type,
            &smpl,
            0,
        );

        assert_eq!(s.loop_start, 60);
        assert_eq!(s.loop_end, 80);
    }

    // -----------------------------------------------------------------------
    // SoundFontSample::new – SF2Pack path (Float32)
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_sf2pack_stores_audio_data() {
        let f32_data: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        let smpl = SmplData::Float32(&f32_data);

        // start_byte_offset=4, end_byte_offset=20 → float slice [2..10]
        let s = SoundFontSample::new(
            "SF2Pack".to_string(),
            4,
            20,
            0,
            0,
            44100,
            60,
            0,
            0,
            st::MONO_SAMPLE,
            &smpl,
            0,
        );

        assert!(s.audio_data.is_some());
        let data = s.audio_data.as_ref().unwrap();
        assert_eq!(data.len(), 8); // (20-4)/2 = 8
        assert!((data[0] - f32_data[2]).abs() < 1e-6);
    }

    #[test]
    fn test_new_sf2pack_data_overridden_true() {
        let f32_data: Vec<f32> = vec![0.0; 100];
        let smpl = SmplData::Float32(&f32_data);
        let s = SoundFontSample::new(
            "P".to_string(),
            0,
            100,
            0,
            0,
            44100,
            60,
            0,
            0,
            st::MONO_SAMPLE,
            &smpl,
            0,
        );
        assert!(s.data_overridden);
    }

    #[test]
    fn test_new_sf2pack_no_s16le_data() {
        let f32_data: Vec<f32> = vec![0.0; 100];
        let smpl = SmplData::Float32(&f32_data);
        let s = SoundFontSample::new(
            "P".to_string(),
            0,
            100,
            0,
            0,
            44100,
            60,
            0,
            0,
            st::MONO_SAMPLE,
            &smpl,
            0,
        );
        assert!(s.s16le_data.is_none());
    }

    // -----------------------------------------------------------------------
    // is_linked / is_compressed / unlink_sample
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_linked_left_sample() {
        let f = vec![0.0f32; 10];
        let smpl = SmplData::Float32(&f);
        let s = SoundFontSample::new(
            "L".to_string(),
            0,
            0,
            0,
            0,
            44100,
            60,
            0,
            1,
            st::LEFT_SAMPLE,
            &smpl,
            0,
        );
        assert!(s.is_linked());
    }

    #[test]
    fn test_is_linked_right_sample() {
        let f = vec![0.0f32; 10];
        let smpl = SmplData::Float32(&f);
        let s = SoundFontSample::new(
            "R".to_string(),
            0,
            0,
            0,
            0,
            44100,
            60,
            0,
            0,
            st::RIGHT_SAMPLE,
            &smpl,
            0,
        );
        assert!(s.is_linked());
    }

    #[test]
    fn test_is_linked_mono_not_linked() {
        let f = vec![0.0f32; 10];
        let smpl = SmplData::Float32(&f);
        let s = SoundFontSample::new(
            "M".to_string(),
            0,
            0,
            0,
            0,
            44100,
            60,
            0,
            0,
            st::MONO_SAMPLE,
            &smpl,
            0,
        );
        assert!(!s.is_linked());
    }

    #[test]
    fn test_unlink_sample_sets_mono() {
        let f = vec![0.0f32; 10];
        let smpl = SmplData::Float32(&f);
        let mut s = SoundFontSample::new(
            "L".to_string(),
            0,
            0,
            0,
            0,
            44100,
            60,
            0,
            1,
            st::LEFT_SAMPLE,
            &smpl,
            0,
        );
        s.linked_sample_idx = Some(1);
        s.unlink_sample();
        assert_eq!(s.sample_type, st::MONO_SAMPLE);
        assert!(s.linked_sample_idx.is_none());
        assert!(!s.is_linked());
    }

    // -----------------------------------------------------------------------
    // get_audio_data
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_audio_data_sf2_basic_conversion() {
        // 4 bytes: two s16le samples: 32768 → 1.0, -32768 → -1.0
        // But 32768 as u16 = 0x8000, as i16 = -32768...
        // Let's use: sample = 16384 (0x4000) → 16384/32768 = 0.5
        //             sample = -16384 (0xC000) → -16384/32768 = -0.5
        let smpl_bytes: Vec<u8> = vec![
            0x00, 0x40, // 16384 LE
            0x00, 0xC0, // -16384 LE (as i16)
        ];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);

        let mut s = SoundFontSample::new(
            "T".to_string(),
            0,
            4,
            0,
            0,
            44100,
            60,
            0,
            0,
            st::MONO_SAMPLE,
            &smpl,
            0,
        );

        let data = s.get_audio_data().unwrap();
        assert_eq!(data.len(), 2);
        assert!(
            (data[0] - 0.5).abs() < 1e-5,
            "Expected 0.5, got {}",
            data[0]
        );
        assert!(
            (data[1] - (-0.5)).abs() < 1e-5,
            "Expected -0.5, got {}",
            data[1]
        );
    }

    #[test]
    fn test_get_audio_data_cached_on_second_call() {
        let smpl_bytes = vec![0x00, 0x40]; // one sample: 16384
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);
        let mut s = SoundFontSample::new(
            "T".to_string(),
            0,
            2,
            0,
            0,
            44100,
            60,
            0,
            0,
            st::MONO_SAMPLE,
            &smpl,
            0,
        );
        let first = s.get_audio_data().unwrap();
        let second = s.get_audio_data().unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn test_get_audio_data_sf2pack_returns_directly() {
        let f32_data: Vec<f32> = vec![0.1, 0.2, 0.3];
        let smpl = SmplData::Float32(&f32_data);
        let mut s = SoundFontSample::new(
            "P".to_string(),
            0,
            6,
            0,
            0,
            44100,
            60,
            0,
            0,
            st::MONO_SAMPLE,
            &smpl,
            0,
        );
        // audio_data was set during construction for SF2Pack
        let data = s.get_audio_data().unwrap();
        assert_eq!(data.len(), 3);
    }

    #[test]
    fn test_get_audio_data_sf3_returns_error() {
        let smpl_bytes = vec![0u8; 100];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);
        let sf3_type = st::MONO_SAMPLE | SF3_BIT_FLIT;
        let mut s = SoundFontSample::new(
            "SF3".to_string(),
            0,
            100,
            0,
            0,
            44100,
            60,
            0,
            0,
            sf3_type,
            &smpl,
            0,
        );
        assert!(s.get_audio_data().is_err());
    }

    // -----------------------------------------------------------------------
    // get_raw_data
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_raw_data_sf2_returns_s16le() {
        let smpl_bytes: Vec<u8> = (0..10u8).collect();
        let iba = IndexedByteArray::from_vec(smpl_bytes.clone());
        let smpl = SmplData::Indexed(&iba);
        let s = SoundFontSample::new(
            "T".to_string(),
            2,
            8,
            0,
            0,
            44100,
            60,
            0,
            0,
            st::MONO_SAMPLE,
            &smpl,
            0,
        );
        let raw = s.get_raw_data(false);
        assert_eq!(raw, smpl_bytes[2..8].to_vec());
    }

    #[test]
    fn test_get_raw_data_sf3_allow_vorbis() {
        let smpl_bytes = vec![0xABu8; 100];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);
        let sf3_type = st::MONO_SAMPLE | SF3_BIT_FLIT;
        let s = SoundFontSample::new(
            "SF3".to_string(),
            0,
            100,
            0,
            0,
            44100,
            60,
            0,
            0,
            sf3_type,
            &smpl,
            0,
        );
        let raw = s.get_raw_data(true);
        assert_eq!(raw.len(), 50); // 100/2 bytes
        assert!(raw.iter().all(|&b| b == 0xAB));
    }

    #[test]
    fn test_get_raw_data_sf3_disallow_vorbis_returns_empty() {
        let smpl_bytes = vec![0xABu8; 100];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);
        let sf3_type = st::MONO_SAMPLE | SF3_BIT_FLIT;
        let s = SoundFontSample::new(
            "SF3".to_string(),
            0,
            100,
            0,
            0,
            44100,
            60,
            0,
            0,
            sf3_type,
            &smpl,
            0,
        );
        let raw = s.get_raw_data(false);
        assert!(raw.is_empty());
    }

    // -----------------------------------------------------------------------
    // read_sample
    // -----------------------------------------------------------------------

    #[test]
    fn test_read_sample_basic() {
        // start=0 pts → 0 bytes, end=100 pts → 200 bytes, loop_start=10, loop_end=90
        let rec = make_shdr_record("Piano", 0, 100, 10, 90, 44100, 60, 0, 0, st::MONO_SAMPLE);
        let smpl_bytes = vec![0u8; 200];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);
        let mut hdr = IndexedByteArray::from_vec(rec);

        let s = read_sample(3, &mut hdr, &smpl);

        assert_eq!(s.name, "Piano");
        assert_eq!(s.start_byte_offset, 0);
        assert_eq!(s.end_byte_offset, 200);
        assert_eq!(s.loop_start, 10); // 10 - 0 = 10
        assert_eq!(s.loop_end, 90);
        assert_eq!(s.sample_rate, 44100);
        assert_eq!(s.original_key, 60);
        assert_eq!(s.pitch_correction, 0);
        assert_eq!(s.linked_sample_index, 0);
        assert_eq!(s.sample_type, st::MONO_SAMPLE);
        assert_eq!(s.sample_id, 3);
    }

    #[test]
    fn test_read_sample_pitch_out_of_range_defaults_to_60() {
        let rec = make_shdr_record("S", 0, 0, 0, 0, 44100, 200, 0, 0, st::MONO_SAMPLE);
        let smpl_bytes = vec![0u8; 10];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);
        let mut hdr = IndexedByteArray::from_vec(rec);
        let s = read_sample(0, &mut hdr, &smpl);
        assert_eq!(s.original_key, 60);
    }

    #[test]
    fn test_read_sample_pitch_correction_signed() {
        // pitch_correction = -10 as u8 = 0xF6
        let rec = make_shdr_record("S", 0, 0, 0, 0, 44100, 60, -10, 0, st::MONO_SAMPLE);
        let smpl_bytes = vec![0u8; 10];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);
        let mut hdr = IndexedByteArray::from_vec(rec);
        let s = read_sample(0, &mut hdr, &smpl);
        assert_eq!(s.pitch_correction, -10);
    }

    #[test]
    fn test_read_sample_advances_cursor_46_bytes() {
        let rec = make_shdr_record("S", 0, 0, 0, 0, 44100, 60, 0, 0, st::MONO_SAMPLE);
        let smpl_bytes = vec![0u8; 10];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);
        let mut hdr = IndexedByteArray::from_vec(rec);
        read_sample(0, &mut hdr, &smpl);
        assert_eq!(hdr.current_index, 46);
    }

    // -----------------------------------------------------------------------
    // read_samples (full function)
    // -----------------------------------------------------------------------

    #[test]
    fn test_read_samples_removes_eos() {
        // One normal sample + one EOS
        let rec = make_shdr_record("Piano", 0, 10, 0, 0, 44100, 60, 0, 0, st::MONO_SAMPLE);
        let mut chunk = make_shdr_chunk(&[rec, eos_record()]);
        let smpl_bytes = vec![0u8; 100];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);

        let samples = read_samples(&mut chunk, &smpl, false);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].name, "Piano");
    }

    #[test]
    fn test_read_samples_multiple_samples() {
        let r1 = make_shdr_record("Violin", 0, 10, 0, 0, 44100, 60, 0, 0, st::MONO_SAMPLE);
        let r2 = make_shdr_record("Flute", 10, 20, 0, 0, 44100, 65, 0, 0, st::MONO_SAMPLE);
        let mut chunk = make_shdr_chunk(&[r1, r2, eos_record()]);
        let smpl_bytes = vec![0u8; 100];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);

        let samples = read_samples(&mut chunk, &smpl, false);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].name, "Violin");
        assert_eq!(samples[1].name, "Flute");
        assert_eq!(samples[1].original_key, 65);
    }

    #[test]
    fn test_read_samples_indices_correct() {
        let r1 = make_shdr_record("A", 0, 10, 0, 0, 44100, 60, 0, 0, st::MONO_SAMPLE);
        let r2 = make_shdr_record("B", 10, 20, 0, 0, 44100, 60, 0, 0, st::MONO_SAMPLE);
        let mut chunk = make_shdr_chunk(&[r1, r2, eos_record()]);
        let smpl_bytes = vec![0u8; 100];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl = SmplData::Indexed(&iba);

        let samples = read_samples(&mut chunk, &smpl, false);
        assert_eq!(samples[0].sample_id, 0);
        assert_eq!(samples[1].sample_id, 1);
    }

    // -----------------------------------------------------------------------
    // link_soundfont_samples
    // -----------------------------------------------------------------------

    fn make_sample(name: &str, sample_type: u16, link_idx: usize) -> SoundFontSample {
        let f = vec![0.0f32; 1];
        let smpl = SmplData::Float32(&f);
        SoundFontSample::new(
            name.to_string(),
            0,
            0,
            0,
            0,
            44100,
            60,
            0,
            link_idx,
            sample_type,
            &smpl,
            0,
        )
    }

    #[test]
    fn test_link_left_right_pair() {
        // sample[0]: LEFT, links to sample[1]
        // sample[1]: RIGHT, links to sample[0]
        // Link function processes i=0 first: sample[0] is LEFT → sets sample[1] to RIGHT_SAMPLE.
        let mut samples = vec![
            make_sample("L", st::LEFT_SAMPLE, 1),
            make_sample("R", st::RIGHT_SAMPLE, 0),
        ];
        link_soundfont_samples(&mut samples);

        assert_eq!(samples[0].linked_sample_idx, Some(1));
        assert_eq!(samples[1].linked_sample_idx, Some(0));
        // When sample[0] is LEFT, its partner sample[1] is explicitly set to RIGHT by the link fn
        assert_eq!(samples[1].sample_type, st::RIGHT_SAMPLE);
    }

    #[test]
    fn test_link_invalid_index_sets_mono() {
        let mut samples = vec![make_sample("L", st::LEFT_SAMPLE, 99)];
        link_soundfont_samples(&mut samples);
        assert_eq!(samples[0].sample_type, st::MONO_SAMPLE);
        assert!(samples[0].linked_sample_idx.is_none());
    }

    #[test]
    fn test_link_already_linked_target_sets_mono() {
        // Loop processes i=0 first:
        //   i=0: sample[0]=LEFT, links to sample[1] (unlinked) → both get linked
        //   i=1: sample[1] already has linked_sample_idx → skipped
        //   i=2: sample[2]=RIGHT, links to sample[1] (already linked!) → sample[2] unlinked
        let mut samples = vec![
            make_sample("L0", st::LEFT_SAMPLE, 1),
            make_sample("L1", st::LEFT_SAMPLE, 2),
            make_sample("R2", st::RIGHT_SAMPLE, 1),
        ];
        link_soundfont_samples(&mut samples);
        // sample[0] linked to sample[1] successfully
        assert_eq!(samples[0].linked_sample_idx, Some(1));
        // sample[2] tried to link to already-linked sample[1] → unlinked to mono
        assert_eq!(samples[2].sample_type, st::MONO_SAMPLE);
    }

    #[test]
    fn test_link_mono_samples_unchanged() {
        let mut samples = vec![
            make_sample("M1", st::MONO_SAMPLE, 0),
            make_sample("M2", st::MONO_SAMPLE, 0),
        ];
        link_soundfont_samples(&mut samples);
        assert!(samples[0].linked_sample_idx.is_none());
        assert!(samples[1].linked_sample_idx.is_none());
    }

    #[test]
    fn test_read_samples_with_linking() {
        // sample 0: LEFT_SAMPLE, links to index 1
        // sample 1: RIGHT_SAMPLE, links to index 0
        let r0 = make_shdr_record("L", 0, 0, 0, 0, 44100, 60, 0, 1, st::LEFT_SAMPLE);
        let r1 = make_shdr_record("R", 0, 0, 0, 0, 44100, 60, 0, 0, st::RIGHT_SAMPLE);
        let mut chunk = make_shdr_chunk(&[r0, r1, eos_record()]);
        let f = vec![0.0f32; 10];
        let smpl = SmplData::Float32(&f);

        let samples = read_samples(&mut chunk, &smpl, true);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].linked_sample_idx, Some(1));
        assert_eq!(samples[1].linked_sample_idx, Some(0));
    }
}
