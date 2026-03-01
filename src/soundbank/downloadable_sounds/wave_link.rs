/// wave_link.rs
/// purpose: DLS WaveLink (wlnk chunk) read/write and SF2 zone conversion.
/// Ported from: src/soundbank/downloadable_sounds/wave_link.ts
use crate::soundbank::basic_soundbank::basic_instrument_zone::BasicInstrumentZone;
use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
use crate::soundbank::enums::sample_types;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::{read_little_endian_indexed, write_dword, write_word};
use crate::utils::riff_chunk::{RIFFChunk, write_riff_chunk_raw};

/// DLS WaveLink parameters parsed from or written to a `wlnk` chunk.
///
/// Equivalent to: class WaveLink
#[derive(Debug, Clone)]
pub struct WaveLink {
    /// Specifies the channel placement of the sample.
    /// Bit 0 = mono sample or left channel of a stereo file.
    /// Equivalent to: public channel = 1
    pub channel: u32,

    /// The 0-based index of the cue entry in the wave pool table.
    /// Equivalent to: public tableIndex: number
    pub table_index: u32,

    /// Flag options for this wave link. All undefined bits must be set to 0.
    /// Equivalent to: public fusOptions = 0
    pub fus_options: u32,

    /// Group number for phase-locked samples. 0 if not a member of a phase-locked group.
    /// Equivalent to: public phaseGroup = 0
    pub phase_group: u32,
}

impl WaveLink {
    /// Creates a new WaveLink with the given table index and default channel/flag values.
    /// Equivalent to: constructor(tableIndex: number)
    pub fn new(table_index: u32) -> Self {
        Self {
            channel: 1,
            table_index,
            fus_options: 0,
            phase_group: 0,
        }
    }

    /// Creates a deep copy of `wave_link`.
    /// Equivalent to: static copyFrom(waveLink: WaveLink): WaveLink
    pub fn copy_from(wave_link: &WaveLink) -> Self {
        Self {
            channel: wave_link.channel,
            table_index: wave_link.table_index,
            fus_options: wave_link.fus_options,
            phase_group: wave_link.phase_group,
        }
    }

    /// Parses a WaveLink from a `wlnk` RIFF chunk, advancing the chunk's data cursor.
    /// Equivalent to: static read(chunk: RIFFChunk): WaveLink
    pub fn read(chunk: &mut RIFFChunk) -> Self {
        // Flags (WORD = 2 bytes)
        let fus_options = read_little_endian_indexed(&mut chunk.data, 2);
        // Phase group (WORD = 2 bytes)
        let phase_group = read_little_endian_indexed(&mut chunk.data, 2);
        // Channel (DWORD = 4 bytes)
        let ul_channel = read_little_endian_indexed(&mut chunk.data, 4);
        // Table index (DWORD = 4 bytes)
        let ul_table_index = read_little_endian_indexed(&mut chunk.data, 4);

        let mut wlnk = WaveLink::new(ul_table_index);
        wlnk.channel = ul_channel;
        wlnk.fus_options = fus_options;
        wlnk.phase_group = phase_group;
        wlnk
    }

    /// Constructs a WaveLink from an SF2 instrument zone and sample list.
    ///
    /// In TypeScript, `samples.indexOf(zone.sample)` is used to find the table index.
    /// In Rust, `zone.sample_idx` already holds the index into the samples slice.
    ///
    /// Returns `Err` if `zone.sample_idx` is out of bounds in `samples`.
    ///
    /// Equivalent to: static fromSFZone(samples: BasicSample[], zone: BasicInstrumentZone): WaveLink
    pub fn from_sf_zone(
        samples: &[BasicSample],
        zone: &BasicInstrumentZone,
    ) -> Result<Self, String> {
        let index = zone.sample_idx;
        if index >= samples.len() {
            return Err(format!(
                "Wave link error: Sample index {} does not exist in the sample list.",
                index
            ));
        }
        let sample = &samples[index];
        let mut wave_link = WaveLink::new(index as u32);

        match sample.sample_type {
            sample_types::RIGHT_SAMPLE => {
                // Right channel
                wave_link.channel = 1 << 1;
            }
            _ => {
                // Left (or mono) — default
                wave_link.channel = 1;
            }
        }

        Ok(wave_link)
    }

    /// Serializes the WaveLink into a `wlnk` RIFF chunk byte array.
    /// Equivalent to: write(): IndexedByteArray
    pub fn write(&self) -> IndexedByteArray {
        let mut wlnk_data = IndexedByteArray::new(12);
        write_word(&mut wlnk_data, self.fus_options); // FusOptions (WORD)
        write_word(&mut wlnk_data, self.phase_group); // UsPhaseGroup (WORD)
        write_dword(&mut wlnk_data, self.channel); // UlChannel (DWORD)
        write_dword(&mut wlnk_data, self.table_index); // UlTableIndex (DWORD)
        write_riff_chunk_raw("wlnk", &wlnk_data, false, false)
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
    use crate::soundbank::enums::sample_types;
    use crate::utils::indexed_array::IndexedByteArray;
    use crate::utils::riff_chunk::{RIFFChunk, read_riff_chunk};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Builds the raw 12-byte body of a wlnk chunk (without the RIFF header/size).
    fn make_wlnk_bytes(
        fus_options: u16,
        phase_group: u16,
        channel: u32,
        table_index: u32,
    ) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&fus_options.to_le_bytes());
        v.extend_from_slice(&phase_group.to_le_bytes());
        v.extend_from_slice(&channel.to_le_bytes());
        v.extend_from_slice(&table_index.to_le_bytes());
        v
    }

    /// Wraps raw data bytes into a RIFFChunk with header "wlnk".
    fn make_wlnk_chunk(data: &[u8]) -> RIFFChunk {
        RIFFChunk::new(
            "wlnk".to_string(),
            data.len() as u32,
            IndexedByteArray::from_vec(data.to_vec()),
        )
    }

    /// Creates a minimal BasicSample for testing.
    fn make_sample(sample_type: u16) -> BasicSample {
        BasicSample::new("Test".to_string(), 44_100, 60, 0, sample_type, 0, 0)
    }

    /// Creates a BasicInstrumentZone pointing to a given sample index.
    fn make_zone(sample_idx: usize) -> BasicInstrumentZone {
        BasicInstrumentZone::new(0, 0, sample_idx)
    }

    // -----------------------------------------------------------------------
    // WaveLink::new
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_table_index() {
        let wl = WaveLink::new(42);
        assert_eq!(wl.table_index, 42);
    }

    #[test]
    fn test_new_default_channel() {
        let wl = WaveLink::new(0);
        assert_eq!(wl.channel, 1);
    }

    #[test]
    fn test_new_default_fus_options() {
        let wl = WaveLink::new(0);
        assert_eq!(wl.fus_options, 0);
    }

    #[test]
    fn test_new_default_phase_group() {
        let wl = WaveLink::new(0);
        assert_eq!(wl.phase_group, 0);
    }

    // -----------------------------------------------------------------------
    // WaveLink::copy_from
    // -----------------------------------------------------------------------

    #[test]
    fn test_copy_from_table_index() {
        let src = WaveLink::new(7);
        let dst = WaveLink::copy_from(&src);
        assert_eq!(dst.table_index, 7);
    }

    #[test]
    fn test_copy_from_channel() {
        let mut src = WaveLink::new(0);
        src.channel = 2;
        let dst = WaveLink::copy_from(&src);
        assert_eq!(dst.channel, 2);
    }

    #[test]
    fn test_copy_from_fus_options() {
        let mut src = WaveLink::new(0);
        src.fus_options = 3;
        let dst = WaveLink::copy_from(&src);
        assert_eq!(dst.fus_options, 3);
    }

    #[test]
    fn test_copy_from_phase_group() {
        let mut src = WaveLink::new(0);
        src.phase_group = 5;
        let dst = WaveLink::copy_from(&src);
        assert_eq!(dst.phase_group, 5);
    }

    #[test]
    fn test_copy_from_independence() {
        let src = WaveLink::new(10);
        let mut dst = WaveLink::copy_from(&src);
        dst.table_index = 99;
        // src must remain unchanged
        assert_eq!(src.table_index, 10);
    }

    // -----------------------------------------------------------------------
    // WaveLink::read
    // -----------------------------------------------------------------------

    #[test]
    fn test_read_fus_options() {
        let data = make_wlnk_bytes(3, 0, 1, 5);
        let mut chunk = make_wlnk_chunk(&data);
        let wl = WaveLink::read(&mut chunk);
        assert_eq!(wl.fus_options, 3);
    }

    #[test]
    fn test_read_phase_group() {
        let data = make_wlnk_bytes(0, 7, 1, 5);
        let mut chunk = make_wlnk_chunk(&data);
        let wl = WaveLink::read(&mut chunk);
        assert_eq!(wl.phase_group, 7);
    }

    #[test]
    fn test_read_channel() {
        let data = make_wlnk_bytes(0, 0, 2, 5);
        let mut chunk = make_wlnk_chunk(&data);
        let wl = WaveLink::read(&mut chunk);
        assert_eq!(wl.channel, 2);
    }

    #[test]
    fn test_read_table_index() {
        let data = make_wlnk_bytes(0, 0, 1, 42);
        let mut chunk = make_wlnk_chunk(&data);
        let wl = WaveLink::read(&mut chunk);
        assert_eq!(wl.table_index, 42);
    }

    #[test]
    fn test_read_all_fields() {
        let data = make_wlnk_bytes(1, 2, 4, 100);
        let mut chunk = make_wlnk_chunk(&data);
        let wl = WaveLink::read(&mut chunk);
        assert_eq!(wl.fus_options, 1);
        assert_eq!(wl.phase_group, 2);
        assert_eq!(wl.channel, 4);
        assert_eq!(wl.table_index, 100);
    }

    // -----------------------------------------------------------------------
    // WaveLink::from_sf_zone
    // -----------------------------------------------------------------------

    #[test]
    fn test_from_sf_zone_mono_channel_is_1() {
        let samples = vec![make_sample(sample_types::MONO_SAMPLE)];
        let zone = make_zone(0);
        let wl = WaveLink::from_sf_zone(&samples, &zone).unwrap();
        assert_eq!(wl.channel, 1);
    }

    #[test]
    fn test_from_sf_zone_left_channel_is_1() {
        let samples = vec![make_sample(sample_types::LEFT_SAMPLE)];
        let zone = make_zone(0);
        let wl = WaveLink::from_sf_zone(&samples, &zone).unwrap();
        assert_eq!(wl.channel, 1);
    }

    #[test]
    fn test_from_sf_zone_right_channel_is_2() {
        let samples = vec![make_sample(sample_types::RIGHT_SAMPLE)];
        let zone = make_zone(0);
        let wl = WaveLink::from_sf_zone(&samples, &zone).unwrap();
        assert_eq!(wl.channel, 2);
    }

    #[test]
    fn test_from_sf_zone_table_index_from_sample_idx() {
        let samples = vec![
            make_sample(sample_types::MONO_SAMPLE),
            make_sample(sample_types::MONO_SAMPLE),
            make_sample(sample_types::RIGHT_SAMPLE),
        ];
        let zone = make_zone(2);
        let wl = WaveLink::from_sf_zone(&samples, &zone).unwrap();
        assert_eq!(wl.table_index, 2);
        assert_eq!(wl.channel, 2); // rightSample
    }

    #[test]
    fn test_from_sf_zone_out_of_bounds_returns_err() {
        let samples = vec![make_sample(sample_types::MONO_SAMPLE)];
        let zone = make_zone(5); // index 5 doesn't exist
        assert!(WaveLink::from_sf_zone(&samples, &zone).is_err());
    }

    #[test]
    fn test_from_sf_zone_empty_samples_returns_err() {
        let samples: Vec<BasicSample> = vec![];
        let zone = make_zone(0);
        assert!(WaveLink::from_sf_zone(&samples, &zone).is_err());
    }

    #[test]
    fn test_from_sf_zone_linked_sample_channel_is_1() {
        // Linked samples fall through to the default case (channel = 1)
        let samples = vec![make_sample(sample_types::LINKED_SAMPLE)];
        let zone = make_zone(0);
        let wl = WaveLink::from_sf_zone(&samples, &zone).unwrap();
        assert_eq!(wl.channel, 1);
    }

    // -----------------------------------------------------------------------
    // WaveLink::write
    // -----------------------------------------------------------------------

    #[test]
    fn test_write_chunk_size() {
        let wl = WaveLink::new(0);
        let out = wl.write();
        // RIFF header (8 bytes) + wlnk body (12 bytes) = 20
        assert_eq!(out.len(), 20);
    }

    #[test]
    fn test_write_header_is_wlnk() {
        let wl = WaveLink::new(0);
        let out = wl.write();
        let s: &[u8] = &out;
        assert_eq!(&s[0..4], b"wlnk");
    }

    #[test]
    fn test_write_size_field() {
        let wl = WaveLink::new(0);
        let out = wl.write();
        let s: &[u8] = &out;
        let size = u32::from_le_bytes([s[4], s[5], s[6], s[7]]);
        assert_eq!(size, 12); // wlnk body is 12 bytes
    }

    #[test]
    fn test_write_fus_options_field() {
        let mut wl = WaveLink::new(0);
        wl.fus_options = 7;
        let out = wl.write();
        let s: &[u8] = &out;
        // fus_options at offset 8 (after 4 header + 4 size), WORD = 2 bytes
        let val = u16::from_le_bytes([s[8], s[9]]);
        assert_eq!(val, 7);
    }

    #[test]
    fn test_write_phase_group_field() {
        let mut wl = WaveLink::new(0);
        wl.phase_group = 3;
        let out = wl.write();
        let s: &[u8] = &out;
        // phase_group at offset 10 (after header 4 + size 4 + fus_options 2), WORD = 2 bytes
        let val = u16::from_le_bytes([s[10], s[11]]);
        assert_eq!(val, 3);
    }

    #[test]
    fn test_write_channel_field() {
        let mut wl = WaveLink::new(0);
        wl.channel = 2;
        let out = wl.write();
        let s: &[u8] = &out;
        // channel at offset 12 (after header 4 + size 4 + fus_options 2 + phase_group 2), DWORD
        let val = u32::from_le_bytes([s[12], s[13], s[14], s[15]]);
        assert_eq!(val, 2);
    }

    #[test]
    fn test_write_table_index_field() {
        let wl = WaveLink::new(99);
        let out = wl.write();
        let s: &[u8] = &out;
        // table_index at offset 16 (after header 4 + size 4 + fus_options 2 + phase_group 2 + channel 4)
        let val = u32::from_le_bytes([s[16], s[17], s[18], s[19]]);
        assert_eq!(val, 99);
    }

    // -----------------------------------------------------------------------
    // write → read roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn test_roundtrip_default_values() {
        let wl_orig = WaveLink::new(5);

        let written = wl_orig.write();
        let mut buf = IndexedByteArray::from_vec(written.to_vec());
        let mut chunk = read_riff_chunk(&mut buf, true, false);
        let wl_read = WaveLink::read(&mut chunk);

        assert_eq!(wl_read.table_index, 5);
        assert_eq!(wl_read.channel, 1);
        assert_eq!(wl_read.fus_options, 0);
        assert_eq!(wl_read.phase_group, 0);
    }

    #[test]
    fn test_roundtrip_custom_values() {
        let mut wl_orig = WaveLink::new(42);
        wl_orig.channel = 2;
        wl_orig.fus_options = 1;
        wl_orig.phase_group = 3;

        let written = wl_orig.write();
        let mut buf = IndexedByteArray::from_vec(written.to_vec());
        let mut chunk = read_riff_chunk(&mut buf, true, false);
        let wl_read = WaveLink::read(&mut chunk);

        assert_eq!(wl_read.table_index, 42);
        assert_eq!(wl_read.channel, 2);
        assert_eq!(wl_read.fus_options, 1);
        assert_eq!(wl_read.phase_group, 3);
    }

    #[test]
    fn test_roundtrip_from_sf_zone_right_sample() {
        let samples = vec![
            make_sample(sample_types::MONO_SAMPLE),
            make_sample(sample_types::RIGHT_SAMPLE),
        ];
        let zone = make_zone(1);
        let wl_orig = WaveLink::from_sf_zone(&samples, &zone).unwrap();

        let written = wl_orig.write();
        let mut buf = IndexedByteArray::from_vec(written.to_vec());
        let mut chunk = read_riff_chunk(&mut buf, true, false);
        let wl_read = WaveLink::read(&mut chunk);

        assert_eq!(wl_read.table_index, 1);
        assert_eq!(wl_read.channel, 2);
    }
}
