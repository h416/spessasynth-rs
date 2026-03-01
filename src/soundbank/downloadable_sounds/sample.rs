/// sample.rs
/// purpose: DLS wave chunk parsing and SF2 sample conversion.
/// Ported from: src/soundbank/downloadable_sounds/sample.ts
///
/// # TypeScript vs Rust design differences
///
/// TypeScript uses `class DownloadableSoundsSample extends DLSVerifier`.
/// Rust has no class inheritance; DLSVerifier's protected static methods are plain
/// module-level functions in `dls_verifier.rs` and called directly here.
///
/// The `dataChunk: RIFFChunk` field is replaced by `data: Vec<u8>` because only the
/// raw audio bytes are needed after parsing (the RIFF framing is reconstructed in `write()`).
///
/// `toSFSample` eagerly decodes audio data before adding the sample to `BasicSoundBank`,
/// since `BasicSoundBank` stores `Vec<BasicSample>` and cannot do polymorphic dispatch
/// the way TypeScript's class inheritance allows.
use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
use crate::soundbank::downloadable_sounds::dls_sample::{DlsSample, w_format_tag};
use crate::soundbank::downloadable_sounds::dls_verifier::verify_and_read_list;
use crate::soundbank::downloadable_sounds::wave_sample::WaveSample;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::{read_little_endian_indexed, write_dword, write_word};
use crate::utils::loggin::spessa_synth_info;
use crate::utils::riff_chunk::{
    RIFFChunk, read_riff_chunk, write_riff_chunk_parts, write_riff_chunk_raw,
};
use crate::utils::string::{get_string_bytes, read_binary_string, read_binary_string_indexed};

// ---------------------------------------------------------------------------
// DownloadableSoundsSample
// ---------------------------------------------------------------------------

/// A DLS wave sample parsed from a `wave` LIST chunk.
///
/// Equivalent to: class DownloadableSoundsSample extends DLSVerifier
#[derive(Debug)]
pub struct DownloadableSoundsSample {
    /// WaveSample (wsmp chunk) metadata.
    /// Equivalent to: public waveSample = new WaveSample()
    pub wave_sample: WaveSample,

    /// Wave format tag (PCM = 0x01, A-law = 0x06).
    /// Equivalent to: public readonly wFormatTag: number
    pub w_format_tag: u16,

    /// Number of bytes per audio sample frame.
    /// Equivalent to: public readonly bytesPerSample: number
    pub bytes_per_sample: u8,

    /// Sample rate in Hz.
    /// Equivalent to: public readonly sampleRate: number
    pub sample_rate: u32,

    /// Raw audio bytes from the `data` sub-chunk.
    /// Equivalent to: public readonly dataChunk: RIFFChunk  (only .data bytes stored)
    pub data: Vec<u8>,

    /// Human-readable name from the INAM sub-chunk.
    /// Equivalent to: public name = "Unnamed sample"
    pub name: String,
}

impl DownloadableSoundsSample {
    /// Creates a new `DownloadableSoundsSample` with default wave sample metadata.
    ///
    /// Equivalent to: constructor(wFormatTag, bytesPerSample, sampleRate, dataChunk)
    pub fn new(w_format_tag: u16, bytes_per_sample: u8, sample_rate: u32, data: Vec<u8>) -> Self {
        Self {
            wave_sample: WaveSample::default(),
            w_format_tag,
            bytes_per_sample,
            sample_rate,
            data,
            name: "Unnamed sample".to_string(),
        }
    }

    /// Parses a `DownloadableSoundsSample` from a `wave` LIST RIFF chunk.
    ///
    /// Reads the `fmt `, `data`, optional `wsmp`, and optional `INFO/INAM` sub-chunks.
    /// Returns `Err` if a required sub-chunk is missing or malformed.
    ///
    /// Equivalent to: static read(waveChunk: RIFFChunk): DownloadableSoundsSample
    pub fn read(wave_chunk: &mut RIFFChunk) -> Result<Self, String> {
        let mut chunks = verify_and_read_list(wave_chunk, &["wave"])?;

        // --- fmt chunk ---
        let fmt_pos = chunks
            .iter()
            .position(|c| c.header == "fmt ")
            .ok_or_else(|| "No fmt chunk in the wave file!".to_string())?;

        let (w_format_tag, sample_rate, bytes_per_sample) = {
            let fmt = &mut chunks[fmt_pos];
            fmt.data.current_index = 0;

            // https://github.com/tpn/winsdk-10/blob/9b69fd26ac0c7d0b83d378dba01080e93349c2ed/Include/10.0.14393.0/shared/mmreg.h#L2108
            let w_format_tag = read_little_endian_indexed(&mut fmt.data, 2) as u16;
            let channels_amount = read_little_endian_indexed(&mut fmt.data, 2);
            if channels_amount != 1 {
                return Err(format!(
                    "Only mono samples are supported. Fmt reports {} channels.",
                    channels_amount
                ));
            }
            let sample_rate = read_little_endian_indexed(&mut fmt.data, 4);
            // Skip avg bytes per sec
            let _ = read_little_endian_indexed(&mut fmt.data, 4);
            // BlockAlign
            let _ = read_little_endian_indexed(&mut fmt.data, 2);
            // Bits per sample (one channel, so bits per sample frame)
            let w_bits_per_sample = read_little_endian_indexed(&mut fmt.data, 2);
            let bytes_per_sample = (w_bits_per_sample / 8) as u8;
            (w_format_tag, sample_rate, bytes_per_sample)
        };

        // --- data chunk ---
        let data_pos = chunks
            .iter()
            .position(|c| c.header == "data")
            .ok_or_else(|| "No data chunk in the WAVE chunk!".to_string())?;
        let data: Vec<u8> = chunks[data_pos].data.to_vec();

        let mut sample =
            DownloadableSoundsSample::new(w_format_tag, bytes_per_sample, sample_rate, data);

        // --- INFO LIST: look for INAM sub-chunk ---
        if let Some(info_pos) = chunks
            .iter()
            .position(|c| c.header == "LIST" && read_binary_string(&c.data, 4, 0) == "INFO")
        {
            let info_chunk = &mut chunks[info_pos];
            info_chunk.data.current_index = 4; // skip "INFO" FourCC
            while info_chunk.data.current_index < info_chunk.data.len() {
                let sub = read_riff_chunk(&mut info_chunk.data, true, false);
                if sub.header == "INAM" {
                    let size = sub.size as usize;
                    let mut sub_data = sub.data;
                    sub_data.current_index = 0;
                    let name = read_binary_string_indexed(&mut sub_data, size);
                    sample.name = name.trim().to_string();
                    break;
                }
            }
        }

        // --- wsmp chunk (optional) ---
        if let Some(wsmp_pos) = chunks.iter().position(|c| c.header == "wsmp") {
            let wsmp = &mut chunks[wsmp_pos];
            sample.wave_sample = WaveSample::read(wsmp)?;
        }

        Ok(sample)
    }

    /// Creates a `DownloadableSoundsSample` from an SF2 `BasicSample`.
    ///
    /// The sample data is encoded as 16-bit PCM (`wFormatTag = 0x01`, `bytesPerSample = 2`).
    ///
    /// Equivalent to: static fromSFSample(sample: BasicSample): DownloadableSoundsSample
    pub fn from_sf_sample(sample: &mut BasicSample) -> Self {
        // Encode audio as s16le
        let raw = sample.get_raw_data(false);
        let mut dls = DownloadableSoundsSample::new(w_format_tag::PCM, 2, sample.sample_rate, raw);
        dls.name = sample.name.clone();
        dls.wave_sample = WaveSample::from_sf_sample(sample);
        dls
    }

    /// Converts this DLS sample to an SF2 `BasicSample` and adds it to `sound_bank`.
    ///
    /// DLS allows `fineTune` to be a 16-bit signed value (max ±32767 cents), while SF2
    /// constrains `pitchCorrection` to ±99 cents.  Excess semitones are folded into
    /// `originalKey` before adding the sample.
    ///
    /// Equivalent to: toSFSample(soundBank: BasicSoundBank): void
    pub fn to_sf_sample(&self, sound_bank: &mut BasicSoundBank) {
        let mut original_key = self.wave_sample.unity_note as i32;
        let mut pitch_correction = self.wave_sample.fine_tune as i32;

        // Fold excess semitones into originalKey (Math.trunc maps to Rust integer division)
        let sample_pitch_semitones = pitch_correction / 100;
        original_key += sample_pitch_semitones;
        pitch_correction -= sample_pitch_semitones * 100;

        let mut loop_start = 0u32;
        let mut loop_end = 0u32;
        if let Some(loop_) = self.wave_sample.loops.first() {
            loop_start = loop_.loop_start;
            loop_end = loop_.loop_start + loop_.loop_length;
        }

        let mut dls_sample = DlsSample::new(
            self.name.clone(),
            self.sample_rate,
            original_key.clamp(0, 127) as u8,
            pitch_correction.clamp(-128, 127) as i8,
            loop_start,
            loop_end,
            self.data.clone(),
            self.w_format_tag,
            self.bytes_per_sample,
        );

        // Decode audio eagerly so BasicSoundBank can store the underlying BasicSample.
        dls_sample.get_audio_data();
        sound_bank.add_sample(dls_sample.sample);
    }

    /// Serializes this sample to a `wave` LIST RIFF chunk.
    ///
    /// Layout: `fmt ` + `wsmp` + `data` + `LIST INFO { INAM }`
    ///
    /// Equivalent to: write(): IndexedByteArray
    pub fn write(&self) -> IndexedByteArray {
        let fmt = self.write_fmt();
        let wsmp = self.wave_sample.write();
        let data_chunk = write_riff_chunk_raw("data", &self.data, false, false);
        let inam_bytes = get_string_bytes(&self.name, true, false);
        let inam = write_riff_chunk_raw("INAM", &inam_bytes, false, false);
        let info = write_riff_chunk_raw("INFO", &inam, false, true);

        spessa_synth_info(&format!("Saved {} successfully!", self.name));

        write_riff_chunk_parts("wave", &[&*fmt, &*wsmp, &*data_chunk, &*info], true)
    }

    /// Serializes the `fmt ` sub-chunk (18 bytes of WAV header fields).
    ///
    /// Equivalent to: private writeFmt(): IndexedByteArray
    fn write_fmt(&self) -> IndexedByteArray {
        let mut fmt_data = IndexedByteArray::new(18);
        write_word(&mut fmt_data, self.w_format_tag as u32); // wFormatTag
        write_word(&mut fmt_data, 1); // wChannels
        write_dword(&mut fmt_data, self.sample_rate); // dwSamplesPerSec
        write_dword(&mut fmt_data, self.sample_rate * 2); // dwAvgBytesPerSec (16-bit assumed)
        write_word(&mut fmt_data, 2); // wBlockAlign
        write_word(&mut fmt_data, self.bytes_per_sample as u32 * 8); // wBitsPerSample
        write_riff_chunk_raw("fmt ", &fmt_data, false, false)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
    use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
    use crate::soundbank::enums::sample_types;
    use crate::soundbank::types::DLSLoop;
    use crate::utils::indexed_array::IndexedByteArray;
    use crate::utils::riff_chunk::RIFFChunk;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Builds the raw bytes for a RIFF sub-chunk: [header 4B][size 4B LE][data].
    fn encode_sub_chunk(header: &str, data: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(header.as_bytes());
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(data);
        if data.len() % 2 != 0 {
            bytes.push(0); // RIFF pad byte
        }
        bytes
    }

    /// Assembles a `wave` LIST RIFFChunk from a list of sub-chunks.
    ///
    /// The resulting chunk has `header = "LIST"` and `data` starting with `"wave"`.
    fn make_wave_list(sub_chunks: &[Vec<u8>]) -> RIFFChunk {
        let mut body = Vec::new();
        body.extend_from_slice(b"wave");
        for sc in sub_chunks {
            body.extend_from_slice(sc);
        }
        let size = body.len() as u32;
        let mut arr = IndexedByteArray::new(body.len());
        for (i, &b) in body.iter().enumerate() {
            arr[i] = b;
        }
        RIFFChunk::new("LIST".to_string(), size, arr)
    }

    /// Returns the 16 bytes of a minimal PCM `fmt ` chunk body.
    fn make_fmt_body(
        w_format_tag: u16,
        channels: u16,
        sample_rate: u32,
        bits_per_sample: u16,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&w_format_tag.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        let avg = sample_rate * (bits_per_sample as u32 / 8) * channels as u32;
        bytes.extend_from_slice(&avg.to_le_bytes());
        let block_align = (channels * bits_per_sample / 8) as u16;
        bytes.extend_from_slice(&block_align.to_le_bytes());
        bytes.extend_from_slice(&bits_per_sample.to_le_bytes());
        bytes
    }

    /// Encodes two i16 values as a minimal `data` chunk body (4 bytes).
    fn make_data_body(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|&s| s.to_le_bytes()).collect()
    }

    /// Creates a BasicSample with provided PCM audio data already set.
    fn make_basic_sample(name: &str, sample_rate: u32, audio: Vec<f32>) -> BasicSample {
        let mut s = BasicSample::new(
            name.to_string(),
            sample_rate,
            60,
            0,
            sample_types::MONO_SAMPLE,
            0,
            0,
        );
        s.set_audio_data(audio, sample_rate);
        s
    }

    // -----------------------------------------------------------------------
    // DownloadableSoundsSample::new
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_stores_format_fields() {
        let s = DownloadableSoundsSample::new(1, 2, 44_100, vec![0u8; 4]);
        assert_eq!(s.w_format_tag, 1);
        assert_eq!(s.bytes_per_sample, 2);
        assert_eq!(s.sample_rate, 44_100);
    }

    #[test]
    fn test_new_default_name() {
        let s = DownloadableSoundsSample::new(1, 2, 44_100, vec![]);
        assert_eq!(s.name, "Unnamed sample");
    }

    #[test]
    fn test_new_stores_data() {
        let data = vec![0x11u8, 0x22, 0x33, 0x44];
        let s = DownloadableSoundsSample::new(1, 2, 44_100, data.clone());
        assert_eq!(s.data, data);
    }

    #[test]
    fn test_new_default_wave_sample() {
        let s = DownloadableSoundsSample::new(1, 2, 44_100, vec![]);
        assert_eq!(s.wave_sample.unity_note, 60);
        assert_eq!(s.wave_sample.fine_tune, 0);
        assert!(s.wave_sample.loops.is_empty());
    }

    // -----------------------------------------------------------------------
    // DownloadableSoundsSample::read
    // -----------------------------------------------------------------------

    fn make_minimal_wave_chunk(sample_rate: u32, audio: &[i16]) -> RIFFChunk {
        let fmt_body = make_fmt_body(1, 1, sample_rate, 16);
        let data_body = make_data_body(audio);
        let fmt_sc = encode_sub_chunk("fmt ", &fmt_body);
        let data_sc = encode_sub_chunk("data", &data_body);
        make_wave_list(&[fmt_sc, data_sc])
    }

    #[test]
    fn test_read_minimal_wave() {
        let mut chunk = make_minimal_wave_chunk(44_100, &[0i16, 1000, -1000]);
        let s = DownloadableSoundsSample::read(&mut chunk).unwrap();
        assert_eq!(s.w_format_tag, 1);
        assert_eq!(s.bytes_per_sample, 2);
        assert_eq!(s.sample_rate, 44_100);
        assert_eq!(s.data.len(), 6); // 3 samples × 2 bytes
    }

    #[test]
    fn test_read_default_name_when_no_info() {
        let mut chunk = make_minimal_wave_chunk(22_050, &[0i16]);
        let s = DownloadableSoundsSample::read(&mut chunk).unwrap();
        assert_eq!(s.name, "Unnamed sample");
    }

    #[test]
    fn test_read_inam_name() {
        // Build INFO LIST sub-chunk containing INAM
        let name_bytes: Vec<u8> = {
            let mut v = b"Piano".to_vec();
            v.push(0); // null terminator (add_zero = true)
            v
        };
        let inam_sc = encode_sub_chunk("INAM", &name_bytes);
        // INFO body = "INFO" + inam_sc
        let mut info_body = b"INFO".to_vec();
        info_body.extend_from_slice(&inam_sc);
        let info_sc = encode_sub_chunk("LIST", &info_body);

        let fmt_sc = encode_sub_chunk("fmt ", &make_fmt_body(1, 1, 44_100, 16));
        let data_sc = encode_sub_chunk("data", &make_data_body(&[0i16]));
        let mut chunk = make_wave_list(&[fmt_sc, data_sc, info_sc]);

        let s = DownloadableSoundsSample::read(&mut chunk).unwrap();
        assert_eq!(s.name, "Piano");
    }

    #[test]
    fn test_read_error_no_fmt() {
        let data_sc = encode_sub_chunk("data", &make_data_body(&[0i16]));
        let mut chunk = make_wave_list(&[data_sc]);
        let result = DownloadableSoundsSample::read(&mut chunk);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No fmt chunk"));
    }

    #[test]
    fn test_read_error_no_data() {
        let fmt_sc = encode_sub_chunk("fmt ", &make_fmt_body(1, 1, 44_100, 16));
        let mut chunk = make_wave_list(&[fmt_sc]);
        let result = DownloadableSoundsSample::read(&mut chunk);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No data chunk"));
    }

    #[test]
    fn test_read_error_stereo() {
        let fmt_sc = encode_sub_chunk("fmt ", &make_fmt_body(1, 2, 44_100, 16)); // 2 channels
        let data_sc = encode_sub_chunk("data", &make_data_body(&[0i16]));
        let mut chunk = make_wave_list(&[fmt_sc, data_sc]);
        let result = DownloadableSoundsSample::read(&mut chunk);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Only mono samples"));
    }

    #[test]
    fn test_read_8bit_sample() {
        let fmt_sc = encode_sub_chunk("fmt ", &make_fmt_body(1, 1, 44_100, 8));
        let data_sc = encode_sub_chunk("data", &[128u8, 200, 50]);
        let mut chunk = make_wave_list(&[fmt_sc, data_sc]);
        let s = DownloadableSoundsSample::read(&mut chunk).unwrap();
        assert_eq!(s.bytes_per_sample, 1);
        assert_eq!(s.data.len(), 3);
    }

    // -----------------------------------------------------------------------
    // DownloadableSoundsSample::from_sf_sample
    // -----------------------------------------------------------------------

    #[test]
    fn test_from_sf_sample_format_tag_is_pcm() {
        let audio: Vec<f32> = vec![0.0, 0.5, -0.5];
        let mut bs = make_basic_sample("Violin", 44_100, audio);
        let dls = DownloadableSoundsSample::from_sf_sample(&mut bs);
        assert_eq!(dls.w_format_tag, w_format_tag::PCM);
        assert_eq!(dls.bytes_per_sample, 2);
    }

    #[test]
    fn test_from_sf_sample_copies_name() {
        let mut bs = make_basic_sample("Flute", 22_050, vec![0.0]);
        let dls = DownloadableSoundsSample::from_sf_sample(&mut bs);
        assert_eq!(dls.name, "Flute");
    }

    #[test]
    fn test_from_sf_sample_copies_sample_rate() {
        let mut bs = make_basic_sample("Drum", 48_000, vec![0.0]);
        let dls = DownloadableSoundsSample::from_sf_sample(&mut bs);
        assert_eq!(dls.sample_rate, 48_000);
    }

    #[test]
    fn test_from_sf_sample_data_is_s16le() {
        // 1.0 → 32767, -1.0 → -32768 (approximately)
        let audio = vec![1.0f32, -1.0f32];
        let mut bs = make_basic_sample("Test", 44_100, audio);
        let dls = DownloadableSoundsSample::from_sf_sample(&mut bs);
        // 2 samples × 2 bytes = 4 bytes
        assert_eq!(dls.data.len(), 4);
    }

    // -----------------------------------------------------------------------
    // DownloadableSoundsSample::to_sf_sample
    // -----------------------------------------------------------------------

    #[test]
    fn test_to_sf_sample_adds_to_bank() {
        let mut dls = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, {
            // Two silent s16le samples
            vec![0u8; 4]
        });
        dls.name = "Bass".to_string();
        let mut bank = BasicSoundBank::default();
        dls.to_sf_sample(&mut bank);
        assert_eq!(bank.samples.len(), 1);
        assert_eq!(bank.samples[0].name, "Bass");
    }

    #[test]
    fn test_to_sf_sample_sample_rate() {
        let dls = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 22_050, vec![0u8; 4]);
        let mut bank = BasicSoundBank::default();
        dls.to_sf_sample(&mut bank);
        assert_eq!(bank.samples[0].sample_rate, 22_050);
    }

    #[test]
    fn test_to_sf_sample_pitch_correction_folding() {
        // fine_tune = 150 cents → 1 semitone folded into original_key
        let mut dls = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![0u8; 4]);
        dls.wave_sample.unity_note = 60;
        dls.wave_sample.fine_tune = 150;
        let mut bank = BasicSoundBank::default();
        dls.to_sf_sample(&mut bank);
        // originalKey = 60 + 1 = 61, pitchCorrection = 150 - 100 = 50
        assert_eq!(bank.samples[0].original_key, 61);
        assert_eq!(bank.samples[0].pitch_correction, 50);
    }

    #[test]
    fn test_to_sf_sample_negative_pitch_correction_folding() {
        // fine_tune = -150 cents → -1 semitone folded (trunc(-1.5) = -1)
        let mut dls = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![0u8; 4]);
        dls.wave_sample.unity_note = 60;
        dls.wave_sample.fine_tune = -150;
        let mut bank = BasicSoundBank::default();
        dls.to_sf_sample(&mut bank);
        // originalKey = 60 + (-1) = 59, pitchCorrection = -150 - (-100) = -50
        assert_eq!(bank.samples[0].original_key, 59);
        assert_eq!(bank.samples[0].pitch_correction, -50);
    }

    #[test]
    fn test_to_sf_sample_loop_points() {
        let mut dls = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![0u8; 40]);
        dls.wave_sample.loops.push(DLSLoop {
            loop_start: 5,
            loop_length: 10,
            loop_type: crate::soundbank::enums::dls_loop_types::FORWARD,
        });
        let mut bank = BasicSoundBank::default();
        dls.to_sf_sample(&mut bank);
        assert_eq!(bank.samples[0].loop_start, 5);
        assert_eq!(bank.samples[0].loop_end, 15); // loop_start + loop_length
    }

    #[test]
    fn test_to_sf_sample_no_loop() {
        let dls = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![0u8; 4]);
        let mut bank = BasicSoundBank::default();
        dls.to_sf_sample(&mut bank);
        assert_eq!(bank.samples[0].loop_start, 0);
        assert_eq!(bank.samples[0].loop_end, 0);
    }

    // -----------------------------------------------------------------------
    // write_fmt
    // -----------------------------------------------------------------------

    // Helper: convert IndexedByteArray to &[u8] for range slicing in assertions.
    fn to_slice(arr: &IndexedByteArray) -> &[u8] {
        arr
    }

    #[test]
    fn test_write_fmt_header() {
        let s = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![]);
        let result = s.write_fmt();
        let b = to_slice(&result);
        // First 4 bytes should be "fmt "
        assert_eq!(&b[0..4], b"fmt ");
    }

    #[test]
    fn test_write_fmt_size_field() {
        let s = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![]);
        let result = s.write_fmt();
        // Size field (bytes 4–7 LE) should be 18
        let size = u32::from_le_bytes([result[4], result[5], result[6], result[7]]);
        assert_eq!(size, 18);
    }

    #[test]
    fn test_write_fmt_format_tag() {
        let s = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![]);
        let result = s.write_fmt();
        // wFormatTag at offset 8
        let tag = u16::from_le_bytes([result[8], result[9]]);
        assert_eq!(tag, w_format_tag::PCM);
    }

    #[test]
    fn test_write_fmt_channels_is_1() {
        let s = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![]);
        let result = s.write_fmt();
        // wChannels at offset 10
        let channels = u16::from_le_bytes([result[10], result[11]]);
        assert_eq!(channels, 1);
    }

    #[test]
    fn test_write_fmt_sample_rate() {
        let s = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![]);
        let result = s.write_fmt();
        // dwSamplesPerSec at offset 12
        let rate = u32::from_le_bytes([result[12], result[13], result[14], result[15]]);
        assert_eq!(rate, 44_100);
    }

    #[test]
    fn test_write_fmt_bits_per_sample() {
        // bytes_per_sample=2 → wBitsPerSample=16
        let s = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![]);
        let result = s.write_fmt();
        // wBitsPerSample layout: RIFF header(4) + size(4) + wFormatTag(2) + wChannels(2)
        //   + dwSamplesPerSec(4) + dwAvgBytesPerSec(4) + wBlockAlign(2) = offset 22
        let bits = u16::from_le_bytes([result[22], result[23]]);
        assert_eq!(bits, 16);
    }

    // -----------------------------------------------------------------------
    // write (round-trip)
    // -----------------------------------------------------------------------

    #[test]
    fn test_write_starts_with_list_header() {
        let s = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![0u8; 4]);
        let result = s.write();
        let b = to_slice(&result);
        // is_list = true → written as "LIST"
        assert_eq!(&b[0..4], b"LIST");
    }

    #[test]
    fn test_write_list_type_is_wave() {
        let s = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![0u8; 4]);
        let result = s.write();
        let b = to_slice(&result);
        // LIST type at offset 8
        assert_eq!(&b[8..12], b"wave");
    }

    #[test]
    fn test_read_write_roundtrip_sample_rate() {
        let mut chunk = make_minimal_wave_chunk(48_000, &[100i16, -200, 0, 32767]);
        let s = DownloadableSoundsSample::read(&mut chunk).unwrap();
        assert_eq!(s.sample_rate, 48_000);

        // Write and confirm it's a valid LIST chunk
        let written = s.write();
        let b = to_slice(&written);
        assert_eq!(&b[0..4], b"LIST");
    }
}
