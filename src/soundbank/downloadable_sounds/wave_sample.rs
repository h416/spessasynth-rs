/// wave_sample.rs
/// purpose: DLS WaveSample (wsmp chunk) read/write and SF2 zone conversion.
/// Ported from: src/soundbank/downloadable_sounds/wave_sample.ts
use crate::soundbank::basic_soundbank::basic_instrument_zone::BasicInstrumentZone;
use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
use crate::soundbank::basic_soundbank::basic_zone::BasicZone;
use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
use crate::soundbank::downloadable_sounds::dls_verifier::verify_header;
use crate::soundbank::enums::{DLSLoopType, dls_loop_types};
use crate::soundbank::types::DLSLoop;
use crate::synthesizer::types::SampleLoopingMode;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::{
    read_little_endian_indexed, signed_int16, write_dword, write_word,
};
use crate::utils::loggin::spessa_synth_warn;
use crate::utils::riff_chunk::{RIFFChunk, write_riff_chunk_raw};

/// Fixed size of the wsmp header field (without loop records).
/// Equivalent to: const WSMP_SIZE = 20
const WSMP_SIZE: usize = 20;

/// Fixed size of one wavesample-loop record.
/// Equivalent to: const WSMP_LOOP_SIZE = 16
const WSMP_LOOP_SIZE: usize = 16;

/// DLS WaveSample parameters parsed from a wsmp chunk.
///
/// TypeScript uses a class extending DLSVerifier; Rust uses module-level verify_header()
/// (see dls_verifier.rs).
///
/// Equivalent to: class WaveSample extends DLSVerifier
#[derive(Debug, Clone)]
pub struct WaveSample {
    /// Gain to apply to this sample in 32-bit relative gain units (1 unit = 1/655360 dB).
    /// Equivalent to: public gain = 0
    pub gain: i32,

    /// MIDI note that replays the sample at its original pitch (0-127; 60 = Middle C).
    /// Equivalent to: public unityNote = 60
    pub unity_note: u16,

    /// Tuning offset from unity_note in cents (16-bit signed).
    /// Equivalent to: public fineTune = 0
    pub fine_tune: i16,

    /// Loop records contained in the wsmp chunk (0 or 1 in practice).
    /// Equivalent to: public loops = new Array<DLSLoop>()
    pub loops: Vec<DLSLoop>,

    /// Sample compression / option flags (default F_WSMP_NO_COMPRESSION = 2).
    /// Equivalent to: public fulOptions = 2
    pub ful_options: u32,
}

impl Default for WaveSample {
    fn default() -> Self {
        Self {
            gain: 0,
            unity_note: 60,
            fine_tune: 0,
            loops: Vec::new(),
            ful_options: 2,
        }
    }
}

impl WaveSample {
    /// Creates a new WaveSample with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a deep copy of `input`.
    /// Equivalent to: static copyFrom(inputWaveSample: WaveSample): WaveSample
    pub fn copy_from(input: &WaveSample) -> Self {
        Self {
            gain: input.gain,
            unity_note: input.unity_note,
            fine_tune: input.fine_tune,
            loops: input.loops.clone(),
            ful_options: input.ful_options,
        }
    }

    /// Parses a WaveSample from a `wsmp` RIFF chunk.
    /// Returns `Err` if the chunk header is invalid.
    /// Equivalent to: static read(chunk: RIFFChunk): WaveSample
    pub fn read(chunk: &mut RIFFChunk) -> Result<Self, String> {
        verify_header(chunk, &["wsmp"])?;

        let mut ws = WaveSample::new();
        chunk.data.current_index = 0;

        // CbSize: expected to be WSMP_SIZE (20)
        let cb_size = read_little_endian_indexed(&mut chunk.data, 4);
        if cb_size != WSMP_SIZE as u32 {
            spessa_synth_warn(&format!(
                "Wsmp cbSize mismatch: got {}, expected {}.",
                cb_size, WSMP_SIZE
            ));
        }

        // usUnityNote (WORD)
        ws.unity_note = read_little_endian_indexed(&mut chunk.data, 2) as u16;

        // sFineTune (signed 16-bit)
        let b1 = chunk.data[chunk.data.current_index];
        chunk.data.current_index += 1;
        let b2 = chunk.data[chunk.data.current_index];
        chunk.data.current_index += 1;
        ws.fine_tune = signed_int16(b1, b2);

        // lGain: each unit represents 1/655360 dB; treat as signed 32-bit (mirrors `| 0` in JS)
        ws.gain = read_little_endian_indexed(&mut chunk.data, 4) as i32;

        ws.ful_options = read_little_endian_indexed(&mut chunk.data, 4);

        // cSampleLoops: one shot = 0, looped = 1
        let loops_amount = read_little_endian_indexed(&mut chunk.data, 4);
        if loops_amount > 0 {
            let loop_cb_size = read_little_endian_indexed(&mut chunk.data, 4);
            if loop_cb_size != WSMP_LOOP_SIZE as u32 {
                // Note: the expected value in the message intentionally uses WSMP_SIZE,
                // faithfully matching the TypeScript source.
                spessa_synth_warn(&format!(
                    "CbSize for loop in wsmp mismatch. Expected {}, got {}.",
                    WSMP_SIZE, loop_cb_size
                ));
            }
            // ulLoopType stored as DWORD (4 bytes) even though DLSLoopType is u16
            let loop_type = read_little_endian_indexed(&mut chunk.data, 4) as DLSLoopType;
            let loop_start = read_little_endian_indexed(&mut chunk.data, 4);
            let loop_length = read_little_endian_indexed(&mut chunk.data, 4);
            ws.loops.push(DLSLoop {
                loop_type,
                loop_start,
                loop_length,
            });
        }

        Ok(ws)
    }

    /// Constructs a WaveSample from a BasicSample (SF2 sample metadata).
    /// Equivalent to: static fromSFSample(sample: BasicSample): WaveSample
    pub fn from_sf_sample(sample: &BasicSample) -> Self {
        let mut ws = WaveSample::new();
        ws.unity_note = sample.original_key as u16;
        ws.fine_tune = sample.pitch_correction as i16;
        if sample.loop_end != 0 || sample.loop_start != 0 {
            ws.loops.push(DLSLoop {
                loop_start: sample.loop_start,
                loop_length: sample.loop_end.wrapping_sub(sample.loop_start),
                loop_type: dls_loop_types::FORWARD,
            });
        }
        ws
    }

    /// Constructs a WaveSample from an SF2 instrument zone and its associated sample.
    ///
    /// In TypeScript, `zone.sample` is accessed directly; Rust requires the sample to be
    /// passed explicitly because the zone only stores an index.
    ///
    /// Equivalent to: static fromSFZone(zone: BasicInstrumentZone): WaveSample
    pub fn from_sf_zone(zone: &BasicInstrumentZone, sample: &BasicSample) -> Self {
        let mut ws = WaveSample::new();

        ws.unity_note = zone
            .zone
            .get_generator(gt::OVERRIDING_ROOT_KEY, sample.original_key as i32)
            as u16;

        // Many drum banks set scale tuning to 0 and keep the root key at 60.
        // We implement scale tuning via a DLS articulator that fluid doesn't support,
        // so adjust the root key here instead.
        if zone.zone.get_generator(gt::SCALE_TUNING, 100) == 0
            && (zone.zone.key_range.max - zone.zone.key_range.min) == 0.0
        {
            ws.unity_note = zone.zone.key_range.min as u16;
        }

        /*
         Note: this may slightly change the generators when doing SF -> DLS -> SF, but the
         tuning remains correct.
         Testcase: Helicopter from GeneralUser-GS v2.0.1
         It sets coarse -13 fine 2 (total -1298 cents).
         This converts to coarse -12 fine -98 which is still -1298 cents.
        */
        ws.fine_tune = (zone.zone.fine_tuning() + sample.pitch_correction as i32) as i16;

        // E-mu attenuation correction
        let attenuation_cb = zone.zone.get_generator(gt::INITIAL_ATTENUATION, 0) as f64 * 0.4;
        // Gain is stored as a 32-bit value; JS `<<` converts to int32 first, then shifts
        ws.gain = ((-attenuation_cb) as i32) << 16;

        let looping_mode = zone.zone.get_generator(gt::SAMPLE_MODES, 0) as SampleLoopingMode;

        // Don't add loops unless needed
        if looping_mode != 0 {
            // Make sure to apply startloop / endloop address offsets
            let loop_start = (sample.loop_start as i32)
                + zone.zone.get_generator(gt::STARTLOOP_ADDRS_OFFSET, 0)
                + zone
                    .zone
                    .get_generator(gt::STARTLOOP_ADDRS_COARSE_OFFSET, 0)
                    * 32_768;
            let loop_end = (sample.loop_end as i32)
                + zone.zone.get_generator(gt::ENDLOOP_ADDRS_OFFSET, 0)
                + zone.zone.get_generator(gt::ENDLOOP_ADDRS_COARSE_OFFSET, 0) * 32_768;

            let dls_loop_type = match looping_mode {
                3 => dls_loop_types::LOOP_AND_RELEASE,
                _ => dls_loop_types::FORWARD,
            };

            ws.loops.push(DLSLoop {
                loop_type: dls_loop_type,
                loop_start: loop_start.max(0) as u32,
                loop_length: (loop_end - loop_start).max(0) as u32,
            });
        }

        ws
    }

    /// Converts this WaveSample's data into an SF2 zone, adjusting generators and tuning.
    /// Equivalent to: toSFZone(zone: BasicZone, sample: BasicSample)
    pub fn to_sf_zone(&self, zone: &mut BasicZone, sample: &BasicSample) {
        let loop_opt = self.loops.first();

        let mut looping_mode: SampleLoopingMode = 0;
        if let Some(loop_data) = loop_opt {
            looping_mode = if loop_data.loop_type == dls_loop_types::LOOP_AND_RELEASE {
                3
            } else {
                1
            };
        }
        if looping_mode != 0 {
            zone.set_generator(gt::SAMPLE_MODES, Some(looping_mode as f64), true);
        }

        // Convert gain to attenuation and apply E-MU correction
        let wsmp_gain16 = self.gain >> 16;
        let wsmp_attenuation = -wsmp_gain16;
        let wsmp_attenuation_corrected = wsmp_attenuation as f64 / 0.4;

        if wsmp_attenuation_corrected != 0.0 {
            zone.set_generator(
                gt::INITIAL_ATTENUATION,
                Some(wsmp_attenuation_corrected),
                true,
            );
        }

        // Correct tuning: remove the sample's built-in pitch correction offset
        zone.set_fine_tuning(self.fine_tune as i32 - sample.pitch_correction as i32);

        // Correct the root key if it differs from the sample's original key
        if self.unity_note != sample.original_key as u16 {
            zone.set_generator(gt::OVERRIDING_ROOT_KEY, Some(self.unity_note as f64), true);
        }

        // Correct loop offsets if needed
        if let Some(loop_data) = loop_opt {
            let diff_start = loop_data.loop_start as i32 - sample.loop_start as i32;
            let loop_end_abs = loop_data.loop_start as i32 + loop_data.loop_length as i32;
            let diff_end = loop_end_abs - sample.loop_end as i32;

            if diff_start != 0 {
                let fine = diff_start % 32_768;
                zone.set_generator(gt::STARTLOOP_ADDRS_OFFSET, Some(fine as f64), true);
                // Coarse generator uses 32768 samples per step
                let coarse = diff_start / 32_768;
                if coarse != 0 {
                    zone.set_generator(
                        gt::STARTLOOP_ADDRS_COARSE_OFFSET,
                        Some(coarse as f64),
                        true,
                    );
                }
            }

            if diff_end != 0 {
                let fine = diff_end % 32_768;
                zone.set_generator(gt::ENDLOOP_ADDRS_OFFSET, Some(fine as f64), true);
                // Coarse generator uses 32768 samples per step
                let coarse = diff_end / 32_768;
                if coarse != 0 {
                    zone.set_generator(gt::ENDLOOP_ADDRS_COARSE_OFFSET, Some(coarse as f64), true);
                }
            }
        }
    }

    /// Serializes the WaveSample into a wsmp RIFF chunk byte array.
    /// Equivalent to: write(): IndexedByteArray
    pub fn write(&self) -> IndexedByteArray {
        let total_size = WSMP_SIZE + self.loops.len() * WSMP_LOOP_SIZE;
        let mut wsmp_data = IndexedByteArray::new(total_size);

        // CbSize (DWORD)
        write_dword(&mut wsmp_data, WSMP_SIZE as u32);
        // usUnityNote (WORD)
        write_word(&mut wsmp_data, self.unity_note as u32);
        // sFineTune (WORD, signed bits preserved via u16 reinterpretation)
        write_word(&mut wsmp_data, self.fine_tune as u16 as u32);
        // lGain (DWORD, bit-pattern of i32 preserved via u32 cast)
        write_dword(&mut wsmp_data, self.gain as u32);
        // fulOptions (DWORD)
        write_dword(&mut wsmp_data, self.ful_options);
        // cSampleLoops (DWORD)
        write_dword(&mut wsmp_data, self.loops.len() as u32);

        for loop_data in &self.loops {
            write_dword(&mut wsmp_data, WSMP_LOOP_SIZE as u32);
            write_dword(&mut wsmp_data, loop_data.loop_type as u32);
            write_dword(&mut wsmp_data, loop_data.loop_start);
            write_dword(&mut wsmp_data, loop_data.loop_length);
        }

        write_riff_chunk_raw("wsmp", &wsmp_data, false, false)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::basic_instrument_zone::BasicInstrumentZone;
    use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
    use crate::soundbank::basic_soundbank::basic_zone::BasicZone;
    use crate::soundbank::basic_soundbank::generator::Generator;
    use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
    use crate::soundbank::enums::{dls_loop_types, sample_types};
    use crate::utils::indexed_array::IndexedByteArray;
    use crate::utils::riff_chunk::{RIFFChunk, read_riff_chunk};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Builds the raw data bytes of a wsmp chunk (without the RIFF header/size).
    fn make_wsmp_bytes(
        unity_note: u16,
        fine_tune: i16,
        gain: i32,
        ful_options: u32,
        loops: &[(u32, u32, u32)], // (loop_type, loop_start, loop_length)
    ) -> Vec<u8> {
        let mut v = Vec::new();
        // cbSize
        v.extend_from_slice(&(WSMP_SIZE as u32).to_le_bytes());
        // usUnityNote
        v.extend_from_slice(&unity_note.to_le_bytes());
        // sFineTune
        v.extend_from_slice(&fine_tune.to_le_bytes());
        // lGain (bit-pattern)
        v.extend_from_slice(&(gain as u32).to_le_bytes());
        // fulOptions
        v.extend_from_slice(&ful_options.to_le_bytes());
        // cSampleLoops
        v.extend_from_slice(&(loops.len() as u32).to_le_bytes());
        for &(ltype, lstart, llen) in loops {
            v.extend_from_slice(&(WSMP_LOOP_SIZE as u32).to_le_bytes());
            v.extend_from_slice(&ltype.to_le_bytes());
            v.extend_from_slice(&lstart.to_le_bytes());
            v.extend_from_slice(&llen.to_le_bytes());
        }
        v
    }

    /// Wraps raw data bytes into a RIFFChunk with header "wsmp".
    fn make_wsmp_chunk(data: &[u8]) -> RIFFChunk {
        RIFFChunk::new(
            "wsmp".to_string(),
            data.len() as u32,
            IndexedByteArray::from_vec(data.to_vec()),
        )
    }

    /// Creates a minimal BasicSample for testing.
    fn make_sample(
        original_key: u8,
        pitch_correction: i8,
        loop_start: u32,
        loop_end: u32,
    ) -> BasicSample {
        BasicSample::new(
            "Test".to_string(),
            44_100,
            original_key,
            pitch_correction,
            sample_types::MONO_SAMPLE,
            loop_start,
            loop_end,
        )
    }

    /// Creates a BasicInstrumentZone with the given sample index, optionally setting generators.
    fn make_zone(sample_idx: usize) -> BasicInstrumentZone {
        BasicInstrumentZone::new(0, 0, sample_idx)
    }

    // -----------------------------------------------------------------------
    // WaveSample::new / Default
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_unity_note_is_60() {
        let ws = WaveSample::new();
        assert_eq!(ws.unity_note, 60);
    }

    #[test]
    fn test_default_gain_is_0() {
        let ws = WaveSample::new();
        assert_eq!(ws.gain, 0);
    }

    #[test]
    fn test_default_fine_tune_is_0() {
        let ws = WaveSample::new();
        assert_eq!(ws.fine_tune, 0);
    }

    #[test]
    fn test_default_ful_options_is_2() {
        let ws = WaveSample::new();
        assert_eq!(ws.ful_options, 2);
    }

    #[test]
    fn test_default_loops_empty() {
        let ws = WaveSample::new();
        assert!(ws.loops.is_empty());
    }

    // -----------------------------------------------------------------------
    // copy_from
    // -----------------------------------------------------------------------

    #[test]
    fn test_copy_from_copies_unity_note() {
        let mut src = WaveSample::new();
        src.unity_note = 72;
        let dst = WaveSample::copy_from(&src);
        assert_eq!(dst.unity_note, 72);
    }

    #[test]
    fn test_copy_from_copies_gain() {
        let mut src = WaveSample::new();
        src.gain = -65536;
        let dst = WaveSample::copy_from(&src);
        assert_eq!(dst.gain, -65536);
    }

    #[test]
    fn test_copy_from_copies_fine_tune() {
        let mut src = WaveSample::new();
        src.fine_tune = -50;
        let dst = WaveSample::copy_from(&src);
        assert_eq!(dst.fine_tune, -50);
    }

    #[test]
    fn test_copy_from_copies_ful_options() {
        let mut src = WaveSample::new();
        src.ful_options = 0;
        let dst = WaveSample::copy_from(&src);
        assert_eq!(dst.ful_options, 0);
    }

    #[test]
    fn test_copy_from_deep_copies_loops() {
        let mut src = WaveSample::new();
        src.loops.push(DLSLoop {
            loop_type: dls_loop_types::FORWARD,
            loop_start: 100,
            loop_length: 50,
        });
        let mut dst = WaveSample::copy_from(&src);
        // Mutate dst loops; src must remain unchanged
        dst.loops[0].loop_start = 999;
        assert_eq!(src.loops[0].loop_start, 100);
    }

    #[test]
    fn test_copy_from_empty_loops() {
        let src = WaveSample::new();
        let dst = WaveSample::copy_from(&src);
        assert!(dst.loops.is_empty());
    }

    // -----------------------------------------------------------------------
    // read
    // -----------------------------------------------------------------------

    #[test]
    fn test_read_no_loop_unity_note() {
        let data = make_wsmp_bytes(60, 0, 0, 2, &[]);
        let mut chunk = make_wsmp_chunk(&data);
        let ws = WaveSample::read(&mut chunk).unwrap();
        assert_eq!(ws.unity_note, 60);
    }

    #[test]
    fn test_read_no_loop_fine_tune_positive() {
        let data = make_wsmp_bytes(60, 25, 0, 2, &[]);
        let mut chunk = make_wsmp_chunk(&data);
        let ws = WaveSample::read(&mut chunk).unwrap();
        assert_eq!(ws.fine_tune, 25);
    }

    #[test]
    fn test_read_no_loop_fine_tune_negative() {
        let data = make_wsmp_bytes(60, -50, 0, 2, &[]);
        let mut chunk = make_wsmp_chunk(&data);
        let ws = WaveSample::read(&mut chunk).unwrap();
        assert_eq!(ws.fine_tune, -50);
    }

    #[test]
    fn test_read_no_loop_gain_zero() {
        let data = make_wsmp_bytes(60, 0, 0, 2, &[]);
        let mut chunk = make_wsmp_chunk(&data);
        let ws = WaveSample::read(&mut chunk).unwrap();
        assert_eq!(ws.gain, 0);
    }

    #[test]
    fn test_read_no_loop_gain_negative() {
        let data = make_wsmp_bytes(60, 0, -65536, 2, &[]);
        let mut chunk = make_wsmp_chunk(&data);
        let ws = WaveSample::read(&mut chunk).unwrap();
        assert_eq!(ws.gain, -65536);
    }

    #[test]
    fn test_read_no_loop_ful_options() {
        let data = make_wsmp_bytes(60, 0, 0, 3, &[]);
        let mut chunk = make_wsmp_chunk(&data);
        let ws = WaveSample::read(&mut chunk).unwrap();
        assert_eq!(ws.ful_options, 3);
    }

    #[test]
    fn test_read_no_loop_loops_empty() {
        let data = make_wsmp_bytes(60, 0, 0, 2, &[]);
        let mut chunk = make_wsmp_chunk(&data);
        let ws = WaveSample::read(&mut chunk).unwrap();
        assert!(ws.loops.is_empty());
    }

    #[test]
    fn test_read_with_loop_count_one() {
        let data = make_wsmp_bytes(60, 0, 0, 2, &[(0, 1000, 500)]);
        let mut chunk = make_wsmp_chunk(&data);
        let ws = WaveSample::read(&mut chunk).unwrap();
        assert_eq!(ws.loops.len(), 1);
    }

    #[test]
    fn test_read_with_loop_type() {
        let data = make_wsmp_bytes(
            60,
            0,
            0,
            2,
            &[(dls_loop_types::LOOP_AND_RELEASE as u32, 0, 0)],
        );
        let mut chunk = make_wsmp_chunk(&data);
        let ws = WaveSample::read(&mut chunk).unwrap();
        assert_eq!(ws.loops[0].loop_type, dls_loop_types::LOOP_AND_RELEASE);
    }

    #[test]
    fn test_read_with_loop_start() {
        let data = make_wsmp_bytes(60, 0, 0, 2, &[(0, 1000, 500)]);
        let mut chunk = make_wsmp_chunk(&data);
        let ws = WaveSample::read(&mut chunk).unwrap();
        assert_eq!(ws.loops[0].loop_start, 1000);
    }

    #[test]
    fn test_read_with_loop_length() {
        let data = make_wsmp_bytes(60, 0, 0, 2, &[(0, 1000, 500)]);
        let mut chunk = make_wsmp_chunk(&data);
        let ws = WaveSample::read(&mut chunk).unwrap();
        assert_eq!(ws.loops[0].loop_length, 500);
    }

    #[test]
    fn test_read_invalid_header_returns_err() {
        let mut chunk = RIFFChunk::new(
            "xxxx".to_string(),
            4,
            IndexedByteArray::from_vec(vec![0u8; 4]),
        );
        assert!(WaveSample::read(&mut chunk).is_err());
    }

    #[test]
    fn test_read_custom_unity_note() {
        let data = make_wsmp_bytes(72, 0, 0, 2, &[]);
        let mut chunk = make_wsmp_chunk(&data);
        let ws = WaveSample::read(&mut chunk).unwrap();
        assert_eq!(ws.unity_note, 72);
    }

    // -----------------------------------------------------------------------
    // from_sf_sample
    // -----------------------------------------------------------------------

    #[test]
    fn test_from_sf_sample_unity_note() {
        let s = make_sample(72, 0, 0, 0);
        let ws = WaveSample::from_sf_sample(&s);
        assert_eq!(ws.unity_note, 72);
    }

    #[test]
    fn test_from_sf_sample_fine_tune() {
        let s = make_sample(60, -10, 0, 0);
        let ws = WaveSample::from_sf_sample(&s);
        assert_eq!(ws.fine_tune, -10);
    }

    #[test]
    fn test_from_sf_sample_no_loop_when_points_zero() {
        let s = make_sample(60, 0, 0, 0);
        let ws = WaveSample::from_sf_sample(&s);
        assert!(ws.loops.is_empty());
    }

    #[test]
    fn test_from_sf_sample_loop_added_when_loop_end_nonzero() {
        let s = make_sample(60, 0, 100, 600);
        let ws = WaveSample::from_sf_sample(&s);
        assert_eq!(ws.loops.len(), 1);
        assert_eq!(ws.loops[0].loop_start, 100);
        assert_eq!(ws.loops[0].loop_length, 500);
        assert_eq!(ws.loops[0].loop_type, dls_loop_types::FORWARD);
    }

    #[test]
    fn test_from_sf_sample_loop_added_when_loop_start_nonzero() {
        let s = make_sample(60, 0, 50, 0);
        let ws = WaveSample::from_sf_sample(&s);
        assert_eq!(ws.loops.len(), 1);
    }

    // -----------------------------------------------------------------------
    // from_sf_zone
    // -----------------------------------------------------------------------

    #[test]
    fn test_from_sf_zone_default_unity_note_from_sample() {
        let zone = make_zone(0);
        let sample = make_sample(65, 0, 0, 0);
        let ws = WaveSample::from_sf_zone(&zone, &sample);
        assert_eq!(ws.unity_note, 65);
    }

    #[test]
    fn test_from_sf_zone_overriding_root_key_used() {
        let mut zone = make_zone(0);
        zone.zone
            .set_generator(gt::OVERRIDING_ROOT_KEY, Some(48.0), true);
        let sample = make_sample(60, 0, 0, 0);
        let ws = WaveSample::from_sf_zone(&zone, &sample);
        assert_eq!(ws.unity_note, 48);
    }

    #[test]
    fn test_from_sf_zone_scale_tuning_zero_uses_key_range_min() {
        let mut zone = make_zone(0);
        zone.zone.set_generator(gt::SCALE_TUNING, Some(0.0), true);
        // key_range: min=max=36 → (36 << 8) | 36 = 9252
        zone.zone
            .add_generators(&[Generator::new_unvalidated(gt::KEY_RANGE, 9252.0)]);
        let sample = make_sample(60, 0, 0, 0);
        let ws = WaveSample::from_sf_zone(&zone, &sample);
        assert_eq!(ws.unity_note, 36);
    }

    #[test]
    fn test_from_sf_zone_fine_tune_from_zone_and_sample() {
        let mut zone = make_zone(0);
        zone.zone.set_generator(gt::FINE_TUNE, Some(30.0), true);
        let sample = make_sample(60, 5, 0, 0);
        let ws = WaveSample::from_sf_zone(&zone, &sample);
        // zone.fine_tuning() = 30, sample.pitch_correction = 5 → 35
        assert_eq!(ws.fine_tune, 35);
    }

    #[test]
    fn test_from_sf_zone_no_loop_when_sample_modes_0() {
        let zone = make_zone(0);
        let sample = make_sample(60, 0, 100, 600);
        let ws = WaveSample::from_sf_zone(&zone, &sample);
        // sampleModes defaults to 0 → no loop
        assert!(ws.loops.is_empty());
    }

    #[test]
    fn test_from_sf_zone_forward_loop_when_sample_modes_1() {
        let mut zone = make_zone(0);
        zone.zone.set_generator(gt::SAMPLE_MODES, Some(1.0), true);
        let sample = make_sample(60, 0, 100, 600);
        let ws = WaveSample::from_sf_zone(&zone, &sample);
        assert_eq!(ws.loops.len(), 1);
        assert_eq!(ws.loops[0].loop_type, dls_loop_types::FORWARD);
        assert_eq!(ws.loops[0].loop_start, 100);
        assert_eq!(ws.loops[0].loop_length, 500);
    }

    #[test]
    fn test_from_sf_zone_loop_and_release_when_sample_modes_3() {
        let mut zone = make_zone(0);
        zone.zone.set_generator(gt::SAMPLE_MODES, Some(3.0), true);
        let sample = make_sample(60, 0, 200, 800);
        let ws = WaveSample::from_sf_zone(&zone, &sample);
        assert_eq!(ws.loops.len(), 1);
        assert_eq!(ws.loops[0].loop_type, dls_loop_types::LOOP_AND_RELEASE);
    }

    #[test]
    fn test_from_sf_zone_gain_from_initial_attenuation() {
        let mut zone = make_zone(0);
        // initialAttenuation = 100 → attenuationCb = 40.0 → gain = (-40 as i32) << 16 = -2621440
        zone.zone
            .set_generator(gt::INITIAL_ATTENUATION, Some(100.0), true);
        let sample = make_sample(60, 0, 0, 0);
        let ws = WaveSample::from_sf_zone(&zone, &sample);
        assert_eq!(ws.gain, (-40_i32) << 16);
    }

    #[test]
    fn test_from_sf_zone_zero_attenuation_gives_zero_gain() {
        let zone = make_zone(0);
        let sample = make_sample(60, 0, 0, 0);
        let ws = WaveSample::from_sf_zone(&zone, &sample);
        assert_eq!(ws.gain, 0);
    }

    #[test]
    fn test_from_sf_zone_loop_with_startloop_offset() {
        let mut zone = make_zone(0);
        zone.zone.set_generator(gt::SAMPLE_MODES, Some(1.0), true);
        zone.zone
            .set_generator(gt::STARTLOOP_ADDRS_OFFSET, Some(10.0), true);
        let sample = make_sample(60, 0, 100, 600);
        let ws = WaveSample::from_sf_zone(&zone, &sample);
        // loop_start = 100 + 10 = 110
        assert_eq!(ws.loops[0].loop_start, 110);
    }

    #[test]
    fn test_from_sf_zone_loop_with_endloop_offset() {
        let mut zone = make_zone(0);
        zone.zone.set_generator(gt::SAMPLE_MODES, Some(1.0), true);
        zone.zone
            .set_generator(gt::ENDLOOP_ADDRS_OFFSET, Some(-5.0), true);
        let sample = make_sample(60, 0, 100, 600);
        let ws = WaveSample::from_sf_zone(&zone, &sample);
        // loop_end = 600 - 5 = 595, loop_length = 595 - 100 = 495
        assert_eq!(ws.loops[0].loop_length, 495);
    }

    // -----------------------------------------------------------------------
    // to_sf_zone
    // -----------------------------------------------------------------------

    #[test]
    fn test_to_sf_zone_no_loop_sample_modes_not_set() {
        let ws = WaveSample::new(); // no loops
        let mut zone = BasicZone::new();
        let sample = make_sample(60, 0, 100, 600);
        ws.to_sf_zone(&mut zone, &sample);
        // SAMPLE_MODES should not be set
        assert_eq!(zone.get_generator(gt::SAMPLE_MODES, -1), -1);
    }

    #[test]
    fn test_to_sf_zone_forward_loop_sets_sample_modes_1() {
        let mut ws = WaveSample::new();
        ws.loops.push(DLSLoop {
            loop_type: dls_loop_types::FORWARD,
            loop_start: 100,
            loop_length: 500,
        });
        let mut zone = BasicZone::new();
        let sample = make_sample(60, 0, 100, 600);
        ws.to_sf_zone(&mut zone, &sample);
        assert_eq!(zone.get_generator(gt::SAMPLE_MODES, -1), 1);
    }

    #[test]
    fn test_to_sf_zone_loop_and_release_sets_sample_modes_3() {
        let mut ws = WaveSample::new();
        ws.loops.push(DLSLoop {
            loop_type: dls_loop_types::LOOP_AND_RELEASE,
            loop_start: 100,
            loop_length: 500,
        });
        let mut zone = BasicZone::new();
        let sample = make_sample(60, 0, 100, 600);
        ws.to_sf_zone(&mut zone, &sample);
        assert_eq!(zone.get_generator(gt::SAMPLE_MODES, -1), 3);
    }

    #[test]
    fn test_to_sf_zone_zero_gain_no_attenuation() {
        let ws = WaveSample::new(); // gain = 0
        let mut zone = BasicZone::new();
        let sample = make_sample(60, 0, 0, 0);
        ws.to_sf_zone(&mut zone, &sample);
        // INITIAL_ATTENUATION should not be set
        assert_eq!(zone.get_generator(gt::INITIAL_ATTENUATION, -1), -1);
    }

    #[test]
    fn test_to_sf_zone_negative_gain_sets_attenuation() {
        let mut ws = WaveSample::new();
        // gain = (-40 as i32) << 16  (represents -40 units at bit16 = 40 cB attenuation)
        ws.gain = (-40_i32) << 16;
        let mut zone = BasicZone::new();
        let sample = make_sample(60, 0, 0, 0);
        ws.to_sf_zone(&mut zone, &sample);
        // wsmpGain16 = -40, wsmpAttenuation = 40, corrected = 40 / 0.4 = 100
        // After clamping by set_generator(validate=true), max for INITIAL_ATTENUATION is 1440
        assert_eq!(zone.get_generator(gt::INITIAL_ATTENUATION, -1), 100);
    }

    #[test]
    fn test_to_sf_zone_tuning_set() {
        let mut ws = WaveSample::new();
        ws.fine_tune = 50; // 50 cents
        let mut zone = BasicZone::new();
        let sample = make_sample(60, 0, 0, 0); // pitch_correction = 0
        ws.to_sf_zone(&mut zone, &sample);
        // zone.fine_tuning() should be 50 cents
        assert_eq!(zone.fine_tuning(), 50);
    }

    #[test]
    fn test_to_sf_zone_tuning_subtracts_pitch_correction() {
        let mut ws = WaveSample::new();
        ws.fine_tune = 80;
        let mut zone = BasicZone::new();
        let sample = make_sample(60, 30, 0, 0); // pitch_correction = 30
        ws.to_sf_zone(&mut zone, &sample);
        // zone.fine_tuning() = 80 - 30 = 50 cents
        assert_eq!(zone.fine_tuning(), 50);
    }

    #[test]
    fn test_to_sf_zone_same_unity_note_no_overriding_root_key() {
        let ws = WaveSample::new(); // unity_note = 60
        let mut zone = BasicZone::new();
        let sample = make_sample(60, 0, 0, 0); // original_key = 60
        ws.to_sf_zone(&mut zone, &sample);
        assert_eq!(zone.get_generator(gt::OVERRIDING_ROOT_KEY, -999), -999);
    }

    #[test]
    fn test_to_sf_zone_different_unity_note_sets_overriding_root_key() {
        let mut ws = WaveSample::new();
        ws.unity_note = 48;
        let mut zone = BasicZone::new();
        let sample = make_sample(60, 0, 0, 0);
        ws.to_sf_zone(&mut zone, &sample);
        assert_eq!(zone.get_generator(gt::OVERRIDING_ROOT_KEY, -999), 48);
    }

    #[test]
    fn test_to_sf_zone_loop_same_points_no_offset_generators() {
        let mut ws = WaveSample::new();
        ws.loops.push(DLSLoop {
            loop_type: dls_loop_types::FORWARD,
            loop_start: 100,
            loop_length: 500,
        });
        let mut zone = BasicZone::new();
        let sample = make_sample(60, 0, 100, 600); // loop matches
        ws.to_sf_zone(&mut zone, &sample);
        // diff_start = 0 and diff_end = 0 → no offset generators
        assert_eq!(zone.get_generator(gt::STARTLOOP_ADDRS_OFFSET, -999), -999);
        assert_eq!(zone.get_generator(gt::ENDLOOP_ADDRS_OFFSET, -999), -999);
    }

    #[test]
    fn test_to_sf_zone_loop_start_diff_sets_fine_offset() {
        let mut ws = WaveSample::new();
        ws.loops.push(DLSLoop {
            loop_type: dls_loop_types::FORWARD,
            loop_start: 110,
            loop_length: 490,
        });
        let mut zone = BasicZone::new();
        let sample = make_sample(60, 0, 100, 600); // sample loop_start = 100
        ws.to_sf_zone(&mut zone, &sample);
        // diff_start = 110 - 100 = 10 → STARTLOOP_ADDRS_OFFSET = 10
        assert_eq!(zone.get_generator(gt::STARTLOOP_ADDRS_OFFSET, -999), 10);
    }

    #[test]
    fn test_to_sf_zone_loop_end_diff_sets_fine_offset() {
        let mut ws = WaveSample::new();
        ws.loops.push(DLSLoop {
            loop_type: dls_loop_types::FORWARD,
            loop_start: 100,
            loop_length: 495,
        });
        let mut zone = BasicZone::new();
        let sample = make_sample(60, 0, 100, 600);
        ws.to_sf_zone(&mut zone, &sample);
        // loop_end_abs = 100 + 495 = 595, sample.loop_end = 600
        // diff_end = 595 - 600 = -5 → ENDLOOP_ADDRS_OFFSET = -5
        assert_eq!(zone.get_generator(gt::ENDLOOP_ADDRS_OFFSET, -999), -5);
    }

    // -----------------------------------------------------------------------
    // write
    // -----------------------------------------------------------------------

    #[test]
    fn test_write_no_loop_chunk_size() {
        let ws = WaveSample::new();
        let out = ws.write();
        // RIFF header (8 bytes) + WSMP_SIZE (20) = 28
        assert_eq!(out.len(), 28);
    }

    #[test]
    fn test_write_with_loop_chunk_size() {
        let mut ws = WaveSample::new();
        ws.loops.push(DLSLoop {
            loop_type: dls_loop_types::FORWARD,
            loop_start: 0,
            loop_length: 0,
        });
        let out = ws.write();
        // 8 + 20 + 16 = 44
        assert_eq!(out.len(), 44);
    }

    #[test]
    fn test_write_header_is_wsmp() {
        let ws = WaveSample::new();
        let out = ws.write();
        let s: &[u8] = &out;
        assert_eq!(&s[0..4], b"wsmp");
    }

    #[test]
    fn test_write_size_field() {
        let ws = WaveSample::new();
        let out = ws.write();
        let s: &[u8] = &out;
        let size = u32::from_le_bytes([s[4], s[5], s[6], s[7]]);
        assert_eq!(size, WSMP_SIZE as u32);
    }

    #[test]
    fn test_write_negative_gain_preserved() {
        let mut ws = WaveSample::new();
        ws.gain = -65536;
        let out = ws.write();
        let s: &[u8] = &out;
        // lGain is at offset 8 (cbSize=4, unityNote=2, fineTune=2) + RIFF header 8
        let offset = 8 + 4 + 2 + 2; // = 16
        let gain_raw = u32::from_le_bytes([s[offset], s[offset + 1], s[offset + 2], s[offset + 3]]);
        assert_eq!(gain_raw as i32, -65536);
    }

    #[test]
    fn test_write_negative_fine_tune_preserved() {
        let mut ws = WaveSample::new();
        ws.fine_tune = -50;
        let out = ws.write();
        let s: &[u8] = &out;
        // sFineTune is at offset 8 + 4 + 2 = 14
        let offset = 8 + 4 + 2;
        let ft_raw = i16::from_le_bytes([s[offset], s[offset + 1]]);
        assert_eq!(ft_raw, -50);
    }

    // -----------------------------------------------------------------------
    // write → read roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn test_roundtrip_no_loop() {
        let mut ws_orig = WaveSample::new();
        ws_orig.unity_note = 48;
        ws_orig.fine_tune = -20;
        ws_orig.gain = 0;
        ws_orig.ful_options = 2;

        let written = ws_orig.write();
        let written_bytes = written.to_vec();
        let mut buf = IndexedByteArray::from_vec(written_bytes);
        let mut chunk = read_riff_chunk(&mut buf, true, false);
        let ws_read = WaveSample::read(&mut chunk).unwrap();

        assert_eq!(ws_read.unity_note, 48);
        assert_eq!(ws_read.fine_tune, -20);
        assert_eq!(ws_read.gain, 0);
        assert_eq!(ws_read.ful_options, 2);
        assert!(ws_read.loops.is_empty());
    }

    #[test]
    fn test_roundtrip_with_loop() {
        let mut ws_orig = WaveSample::new();
        ws_orig.unity_note = 60;
        ws_orig.fine_tune = 10;
        ws_orig.gain = -131072; // -2 << 16
        ws_orig.loops.push(DLSLoop {
            loop_type: dls_loop_types::LOOP_AND_RELEASE,
            loop_start: 500,
            loop_length: 1000,
        });

        let written = ws_orig.write();
        let written_bytes = written.to_vec();
        let mut buf = IndexedByteArray::from_vec(written_bytes);
        let mut chunk = read_riff_chunk(&mut buf, true, false);
        let ws_read = WaveSample::read(&mut chunk).unwrap();

        assert_eq!(ws_read.unity_note, 60);
        assert_eq!(ws_read.fine_tune, 10);
        assert_eq!(ws_read.gain, -131072);
        assert_eq!(ws_read.loops.len(), 1);
        assert_eq!(ws_read.loops[0].loop_type, dls_loop_types::LOOP_AND_RELEASE);
        assert_eq!(ws_read.loops[0].loop_start, 500);
        assert_eq!(ws_read.loops[0].loop_length, 1000);
    }

    #[test]
    fn test_roundtrip_from_sf_sample() {
        let sample = make_sample(72, -5, 200, 800);
        let ws_orig = WaveSample::from_sf_sample(&sample);

        let written = ws_orig.write();
        let written_bytes = written.to_vec();
        let mut buf = IndexedByteArray::from_vec(written_bytes);
        let mut chunk = read_riff_chunk(&mut buf, true, false);
        let ws_read = WaveSample::read(&mut chunk).unwrap();

        assert_eq!(ws_read.unity_note, 72);
        assert_eq!(ws_read.fine_tune, -5);
        assert_eq!(ws_read.loops.len(), 1);
        assert_eq!(ws_read.loops[0].loop_start, 200);
        assert_eq!(ws_read.loops[0].loop_length, 600);
    }
}
