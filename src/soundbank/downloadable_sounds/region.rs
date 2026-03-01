/// region.rs
/// purpose: DLS Region (rgn/rgn2 chunk) read/write and SF2 instrument zone conversion.
/// Ported from: src/soundbank/downloadable_sounds/region.ts
use crate::soundbank::basic_soundbank::basic_instrument::BasicInstrument;
use crate::soundbank::basic_soundbank::basic_instrument_zone::BasicInstrumentZone;
use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
use crate::soundbank::basic_soundbank::generator_types::{GENERATOR_LIMITS, generator_types as gt};
use crate::soundbank::downloadable_sounds::articulation::DownloadableSoundsArticulation;
use crate::soundbank::downloadable_sounds::dls_verifier::{parsing_error, verify_and_read_list};
use crate::soundbank::downloadable_sounds::sample::DownloadableSoundsSample;
use crate::soundbank::downloadable_sounds::wave_link::WaveLink;
use crate::soundbank::downloadable_sounds::wave_sample::WaveSample;
use crate::soundbank::types::GenericRange;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::{read_little_endian_indexed, write_word};
use crate::utils::loggin::spessa_synth_warn;
use crate::utils::riff_chunk::{RIFFChunk, write_riff_chunk_parts, write_riff_chunk_raw};

// ---------------------------------------------------------------------------
// DownloadableSoundsRegion
// ---------------------------------------------------------------------------

/// A DLS Region: maps a key/velocity range to a wave sample with articulation.
///
/// Equivalent to: class DownloadableSoundsRegion extends DLSVerifier
pub struct DownloadableSoundsRegion {
    /// The articulation (connection blocks) for this region.
    /// Equivalent to: public readonly articulation = new DownloadableSoundsArticulation()
    pub articulation: DownloadableSoundsArticulation,

    /// Specifies the key range for this region.
    /// Equivalent to: public keyRange: GenericRange = { min: 0, max: 127 }
    pub key_range: GenericRange,

    /// Specifies the velocity range for this region.
    /// Equivalent to: public velRange: GenericRange = { min: 0, max: 127 }
    pub vel_range: GenericRange,

    /// Specifies the key group (exclusive class) for drum instruments.
    /// Regions with the same non-zero key_group cut each other off.
    /// Equivalent to: public keyGroup = 0
    pub key_group: u16,

    /// Synthesis flag options for this region.
    /// Equivalent to: public fusOptions = 0
    pub fus_options: u16,

    /// Layer index for editor display purposes.
    /// Equivalent to: public usLayer = 0
    pub us_layer: u16,

    /// WaveSample (wsmp chunk) metadata.
    /// Equivalent to: public readonly waveSample: WaveSample
    pub wave_sample: WaveSample,

    /// WaveLink (wlnk chunk) data linking this region to a wave pool entry.
    /// Equivalent to: public readonly waveLink: WaveLink
    pub wave_link: WaveLink,
}

impl DownloadableSoundsRegion {
    /// Creates a new region with default ranges (0-127 for both key and velocity).
    /// Equivalent to: constructor(waveLink: WaveLink, waveSample: WaveSample)
    pub fn new(wave_link: WaveLink, wave_sample: WaveSample) -> Self {
        Self {
            articulation: DownloadableSoundsArticulation::new(),
            key_range: GenericRange {
                min: 0.0,
                max: 127.0,
            },
            vel_range: GenericRange {
                min: 0.0,
                max: 127.0,
            },
            key_group: 0,
            fus_options: 0,
            us_layer: 0,
            wave_sample,
            wave_link,
        }
    }

    /// Deep-copies a region.
    /// Equivalent to: static copyFrom(inputRegion: DownloadableSoundsRegion)
    pub fn copy_from(input: &DownloadableSoundsRegion) -> Self {
        let mut region = DownloadableSoundsRegion::new(
            WaveLink::copy_from(&input.wave_link),
            WaveSample::copy_from(&input.wave_sample),
        );
        region.key_group = input.key_group;
        region.key_range = input.key_range.clone();
        region.vel_range = input.vel_range.clone();
        region.us_layer = input.us_layer;
        region.fus_options = input.fus_options;
        region.articulation.copy_from(&input.articulation);
        region
    }

    /// Parses a DLS region from a `rgn ` or `rgn2` LIST RIFF chunk.
    ///
    /// Returns `None` when a required sub-chunk (wlnk or rgnh) is missing;
    /// the caller should skip such regions silently after logging a warning.
    ///
    /// Equivalent to: static read(samples: DownloadableSoundsSample[], chunk: RIFFChunk)
    pub fn read(samples: &[DownloadableSoundsSample], chunk: &mut RIFFChunk) -> Option<Self> {
        let mut region_chunks = match verify_and_read_list(chunk, &["rgn ", "rgn2"]) {
            Ok(chunks) => chunks,
            Err(e) => {
                spessa_synth_warn(&format!("Failed to read DLS region chunk: {e}"));
                return None;
            }
        };

        // wsmp: wave sample chunk (optional – falls back to the wave pool sample's wsmp)
        let wsmp_pos = region_chunks.iter().position(|c| c.header == "wsmp");
        let wave_sample_opt: Option<WaveSample> =
            wsmp_pos.and_then(|pos| WaveSample::read(&mut region_chunks[pos]).ok());

        // wlnk: wave link chunk (required)
        let wlnk_pos = region_chunks.iter().position(|c| c.header == "wlnk");
        let wlnk_pos = match wlnk_pos {
            Some(pos) => pos,
            None => {
                // No wave link means no sample – nothing useful in this region.
                spessa_synth_warn("Invalid DLS region: missing 'wlnk' chunk! Discarding...");
                return None;
            }
        };
        let wave_link = WaveLink::read(&mut region_chunks[wlnk_pos]);

        // rgnh: region header chunk (required)
        let rgnh_pos = region_chunks.iter().position(|c| c.header == "rgnh");
        let rgnh_pos = match rgnh_pos {
            Some(pos) => pos,
            None => {
                spessa_synth_warn("Invalid DLS region: missing 'rgnh' chunk! Discarding...");
                return None;
            }
        };

        // Validate sample index
        let sample_idx = wave_link.table_index as usize;
        let sample = match samples.get(sample_idx) {
            Some(s) => s,
            None => {
                spessa_synth_warn(&parsing_error(&format!(
                    "Invalid sample index: {}. Samples available: {}",
                    wave_link.table_index,
                    samples.len()
                )));
                return None;
            }
        };

        // If no wsmp chunk in region, fall back to wave pool sample's waveSample
        let wave_sample =
            wave_sample_opt.unwrap_or_else(|| WaveSample::copy_from(&sample.wave_sample));

        let mut region = DownloadableSoundsRegion::new(wave_link, wave_sample);

        // Parse rgnh header fields
        {
            let rgnh = &mut region_chunks[rgnh_pos];
            rgnh.data.current_index = 0;

            let key_min = read_little_endian_indexed(&mut rgnh.data, 2) as f64;
            let key_max = read_little_endian_indexed(&mut rgnh.data, 2) as f64;
            let vel_min = read_little_endian_indexed(&mut rgnh.data, 2) as f64;
            let mut vel_max = read_little_endian_indexed(&mut rgnh.data, 2) as f64;

            // Fix for files that write zeros for both velocity min and max.
            if vel_min == 0.0 && vel_max == 0.0 {
                vel_max = 127.0;
            }

            region.key_range = GenericRange {
                min: key_min,
                max: key_max,
            };
            region.vel_range = GenericRange {
                min: vel_min,
                max: vel_max,
            };

            region.fus_options = read_little_endian_indexed(&mut rgnh.data, 2) as u16;
            region.key_group = read_little_endian_indexed(&mut rgnh.data, 2) as u16;

            // usLayer is an optional extension field (present only in DLS2 regions)
            if rgnh.data.len() - rgnh.data.current_index >= 2 {
                region.us_layer = read_little_endian_indexed(&mut rgnh.data, 2) as u16;
            }
        }

        region.articulation.read(&mut region_chunks);
        Some(region)
    }

    /// Constructs a region from an SF2 instrument zone.
    ///
    /// # Differences from TypeScript
    ///
    /// TypeScript accesses `zone.sample` directly; Rust uses `zone.sample_idx` to look up
    /// the sample in `samples`.  `WaveSample::from_sf_zone` also requires the sample
    /// explicitly (no hidden back-reference in Rust).
    ///
    /// Returns `Err` if `zone.sample_idx` is out of bounds in `samples`.
    ///
    /// Equivalent to: static fromSFZone(zone: BasicInstrumentZone, samples: BasicSample[])
    pub fn from_sf_zone(
        zone: &BasicInstrumentZone,
        samples: &[BasicSample],
    ) -> Result<Self, String> {
        let sample = samples
            .get(zone.sample_idx)
            .ok_or_else(|| parsing_error(&format!("Invalid sample index: {}", zone.sample_idx)))?;

        let wave_sample = WaveSample::from_sf_zone(zone, sample);
        let wave_link = WaveLink::from_sf_zone(samples, zone)?;

        let mut region = DownloadableSoundsRegion::new(wave_link, wave_sample);

        // Assign ranges, clamping min to ≥ 0 (SF2 key/vel range min is -1 when unset)
        region.key_range = GenericRange {
            min: zone.zone.key_range.min.max(0.0),
            max: zone.zone.key_range.max,
        };
        region.vel_range = GenericRange {
            min: zone.zone.vel_range.min.max(0.0),
            max: zone.zone.vel_range.max,
        };

        // KeyGroup maps to SF2 exclusiveClass generator
        region.key_group = zone.zone.get_generator(gt::EXCLUSIVE_CLASS, 0) as u16;
        region.articulation.from_sf_zone(&zone.zone);

        Ok(region)
    }

    /// Serialises this region as a `rgn2` LIST RIFF chunk.
    ///
    /// Order: rgnh header → wsmp → wlnk → articulation (lar2/lart).
    ///
    /// Equivalent to: write(): IndexedByteArray
    pub fn write(&self) -> IndexedByteArray {
        let header = self.write_header();
        let wsmp = self.wave_sample.write();
        let wlnk = self.wave_link.write();
        let art = self.articulation.write();
        write_riff_chunk_parts("rgn2", &[&*header, &*wsmp, &*wlnk, &*art], true)
    }

    /// Converts this DLS region into an SF2 instrument zone appended to `instrument`.
    ///
    /// # Differences from TypeScript
    ///
    /// TypeScript's `instrument.createZone(sample)` returns the zone object directly.
    /// In Rust, `BasicInstrument::create_zone` returns the zone *index*; callers access the
    /// zone via `instrument.zones[zone_idx]`.
    ///
    /// Returns `Err` if `self.wave_link.table_index` is out of bounds in `samples`.
    ///
    /// Equivalent to: toSFZone(instrument: BasicInstrument, samples: BasicSample[])
    pub fn to_sf_zone(
        &self,
        instrument: &mut BasicInstrument,
        instrument_idx: usize,
        samples: &mut [BasicSample],
    ) -> Result<usize, String> {
        let sample_idx = self.wave_link.table_index as usize;
        if sample_idx >= samples.len() {
            return Err(parsing_error(&format!(
                "Invalid sample index: {}",
                self.wave_link.table_index
            )));
        }

        let zone_idx = instrument.create_zone(instrument_idx, sample_idx, samples);

        // Set key/velocity ranges on the new zone
        {
            let zone = &mut instrument.zones[zone_idx];
            zone.zone.key_range = self.key_range.clone();
            zone.zone.vel_range = self.vel_range.clone();

            // Default (full) range → mark as "not set" with min = -1
            if self.key_range.max == 127.0 && self.key_range.min == 0.0 {
                zone.zone.key_range.min = -1.0;
            }
            if self.vel_range.max == 127.0 && self.vel_range.min == 0.0 {
                zone.zone.vel_range.min = -1.0;
            }

            // KeyGroup → exclusiveClass generator
            if self.key_group != 0 {
                zone.zone
                    .set_generator(gt::EXCLUSIVE_CLASS, Some(self.key_group as f64), true);
            }
        }

        // Apply wave-sample tuning/loop parameters and articulation connection blocks.
        // Borrow `samples[sample_idx]` immutably while `instrument.zones[zone_idx]` is
        // borrowed mutably – safe because they are separate allocations.
        self.wave_sample
            .to_sf_zone(&mut instrument.zones[zone_idx].zone, &samples[sample_idx]);
        self.articulation
            .to_sf_zone(&mut instrument.zones[zone_idx].zone);

        // Remove generators whose value equals the SF2 default
        instrument.zones[zone_idx].zone.generators.retain(|g| {
            let def = GENERATOR_LIMITS
                .get(g.generator_type as usize)
                .and_then(|l| *l)
                .map(|l| l.def)
                .unwrap_or(0);
            g.generator_value as i32 != def
        });

        Ok(zone_idx)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Serialises the `rgnh` chunk (region header).
    ///
    /// Layout (14 bytes / 7 WORDs):
    ///   keyRangeMin · keyRangeMax · velRangeMin · velRangeMax · fusOptions · keyGroup · usLayer
    ///
    /// Equivalent to: private writeHeader()
    fn write_header(&self) -> IndexedByteArray {
        // 7 WORD fields × 2 bytes = 14 bytes
        let mut rgnh_data = IndexedByteArray::new(14);
        write_word(&mut rgnh_data, self.key_range.min.max(0.0) as u32);
        write_word(&mut rgnh_data, self.key_range.max as u32);
        write_word(&mut rgnh_data, self.vel_range.min.max(0.0) as u32);
        write_word(&mut rgnh_data, self.vel_range.max as u32);
        write_word(&mut rgnh_data, self.fus_options as u32);
        write_word(&mut rgnh_data, self.key_group as u32);
        write_word(&mut rgnh_data, self.us_layer as u32);
        write_riff_chunk_raw("rgnh", &rgnh_data, false, false)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::basic_instrument::BasicInstrument;
    use crate::soundbank::basic_soundbank::basic_instrument_zone::BasicInstrumentZone;
    use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
    use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
    use crate::soundbank::downloadable_sounds::dls_sample::w_format_tag;
    use crate::soundbank::downloadable_sounds::sample::DownloadableSoundsSample;
    use crate::soundbank::downloadable_sounds::wave_link::WaveLink;
    use crate::soundbank::downloadable_sounds::wave_sample::WaveSample;
    use crate::soundbank::enums::sample_types;
    use crate::soundbank::types::GenericRange;
    use crate::utils::indexed_array::IndexedByteArray;
    use crate::utils::riff_chunk::RIFFChunk;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Creates a minimal mono BasicSample for testing.
    fn make_basic_sample(original_key: u8) -> BasicSample {
        BasicSample::new(
            "TestSample".to_string(),
            44_100,
            original_key,
            0,
            sample_types::MONO_SAMPLE,
            0,
            0,
        )
    }

    /// Creates a BasicInstrumentZone with the given sample index.
    fn make_instrument_zone(sample_idx: usize) -> BasicInstrumentZone {
        BasicInstrumentZone::new(0, 0, sample_idx)
    }

    /// Creates a minimal DownloadableSoundsSample (silent, 4 bytes of PCM).
    fn make_dls_sample() -> DownloadableSoundsSample {
        DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![0u8; 4])
    }

    /// Builds the raw bytes for a minimal rgnh sub-chunk body (without RIFF header/size).
    fn make_rgnh_body(
        key_min: u16,
        key_max: u16,
        vel_min: u16,
        vel_max: u16,
        fus_options: u16,
        key_group: u16,
        us_layer: Option<u16>,
    ) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&key_min.to_le_bytes());
        v.extend_from_slice(&key_max.to_le_bytes());
        v.extend_from_slice(&vel_min.to_le_bytes());
        v.extend_from_slice(&vel_max.to_le_bytes());
        v.extend_from_slice(&fus_options.to_le_bytes());
        v.extend_from_slice(&key_group.to_le_bytes());
        if let Some(layer) = us_layer {
            v.extend_from_slice(&layer.to_le_bytes());
        }
        v
    }

    /// Encodes a RIFF sub-chunk byte sequence: [header 4B][size 4B LE][data][pad?]
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

    /// Builds a `rgn2` LIST RIFFChunk from sub-chunk byte vectors.
    fn make_region_list(list_type: &str, sub_chunks: &[Vec<u8>]) -> RIFFChunk {
        // LIST body = list_type (4B) + sub_chunks
        let mut body: Vec<u8> = list_type.as_bytes().to_vec();
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

    /// Builds minimal wlnk bytes for a given table index (mono sample → channel = 1).
    fn make_wlnk_body(table_index: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&0u16.to_le_bytes()); // fusOptions
        v.extend_from_slice(&0u16.to_le_bytes()); // phaseGroup
        v.extend_from_slice(&1u32.to_le_bytes()); // channel (1 = mono/left)
        v.extend_from_slice(&table_index.to_le_bytes());
        v
    }

    /// Constructs a complete rgn2 LIST chunk suitable for `DownloadableSoundsRegion::read`.
    fn make_region_chunk(
        table_index: u32,
        key_min: u16,
        key_max: u16,
        vel_min: u16,
        vel_max: u16,
    ) -> RIFFChunk {
        let rgnh = encode_sub_chunk(
            "rgnh",
            &make_rgnh_body(key_min, key_max, vel_min, vel_max, 0, 0, Some(0)),
        );
        let wlnk = encode_sub_chunk("wlnk", &make_wlnk_body(table_index));
        make_region_list("rgn2", &[rgnh, wlnk])
    }

    // -----------------------------------------------------------------------
    // new / default values
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_default_key_range() {
        let wl = WaveLink::new(0);
        let ws = WaveSample::new();
        let region = DownloadableSoundsRegion::new(wl, ws);
        assert_eq!(region.key_range.min, 0.0);
        assert_eq!(region.key_range.max, 127.0);
    }

    #[test]
    fn test_new_default_vel_range() {
        let wl = WaveLink::new(0);
        let ws = WaveSample::new();
        let region = DownloadableSoundsRegion::new(wl, ws);
        assert_eq!(region.vel_range.min, 0.0);
        assert_eq!(region.vel_range.max, 127.0);
    }

    #[test]
    fn test_new_default_key_group_is_zero() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        assert_eq!(region.key_group, 0);
    }

    #[test]
    fn test_new_default_fus_options_is_zero() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        assert_eq!(region.fus_options, 0);
    }

    #[test]
    fn test_new_default_us_layer_is_zero() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        assert_eq!(region.us_layer, 0);
    }

    #[test]
    fn test_new_articulation_is_empty() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        assert!(region.articulation.is_empty());
    }

    // -----------------------------------------------------------------------
    // copy_from
    // -----------------------------------------------------------------------

    #[test]
    fn test_copy_from_copies_key_range() {
        let mut src = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        src.key_range = GenericRange {
            min: 36.0,
            max: 60.0,
        };
        let dst = DownloadableSoundsRegion::copy_from(&src);
        assert_eq!(dst.key_range.min, 36.0);
        assert_eq!(dst.key_range.max, 60.0);
    }

    #[test]
    fn test_copy_from_copies_vel_range() {
        let mut src = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        src.vel_range = GenericRange {
            min: 64.0,
            max: 100.0,
        };
        let dst = DownloadableSoundsRegion::copy_from(&src);
        assert_eq!(dst.vel_range.min, 64.0);
        assert_eq!(dst.vel_range.max, 100.0);
    }

    #[test]
    fn test_copy_from_copies_key_group() {
        let mut src = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        src.key_group = 3;
        let dst = DownloadableSoundsRegion::copy_from(&src);
        assert_eq!(dst.key_group, 3);
    }

    #[test]
    fn test_copy_from_copies_fus_options() {
        let mut src = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        src.fus_options = 7;
        let dst = DownloadableSoundsRegion::copy_from(&src);
        assert_eq!(dst.fus_options, 7);
    }

    #[test]
    fn test_copy_from_copies_us_layer() {
        let mut src = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        src.us_layer = 2;
        let dst = DownloadableSoundsRegion::copy_from(&src);
        assert_eq!(dst.us_layer, 2);
    }

    #[test]
    fn test_copy_from_copies_wave_link_table_index() {
        let src = DownloadableSoundsRegion::new(WaveLink::new(5), WaveSample::new());
        let dst = DownloadableSoundsRegion::copy_from(&src);
        assert_eq!(dst.wave_link.table_index, 5);
    }

    #[test]
    fn test_copy_from_independence_key_range() {
        let mut src = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        src.key_range = GenericRange {
            min: 10.0,
            max: 50.0,
        };
        let mut dst = DownloadableSoundsRegion::copy_from(&src);
        dst.key_range.min = 99.0;
        // src must be unchanged
        assert_eq!(src.key_range.min, 10.0);
    }

    // -----------------------------------------------------------------------
    // write_header (private, tested via write)
    // -----------------------------------------------------------------------

    #[test]
    fn test_write_header_chunk_header_is_rgnh() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        let hdr = region.write_header();
        let s: &[u8] = &hdr;
        assert_eq!(&s[0..4], b"rgnh");
    }

    #[test]
    fn test_write_header_size_is_14() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        let hdr = region.write_header();
        let s: &[u8] = &hdr;
        let size = u32::from_le_bytes([s[4], s[5], s[6], s[7]]);
        assert_eq!(size, 14);
    }

    #[test]
    fn test_write_header_key_range_min_clamped() {
        // key_range.min < 0 should be clamped to 0 in the output
        let mut region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        region.key_range.min = -1.0; // SF2 "not-set" sentinel
        let hdr = region.write_header();
        let s: &[u8] = &hdr;
        // RIFF header is 8 bytes, first WORD starts at offset 8
        let key_min = u16::from_le_bytes([s[8], s[9]]);
        assert_eq!(key_min, 0);
    }

    #[test]
    fn test_write_header_key_range_max() {
        let mut region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        region.key_range.max = 72.0;
        let hdr = region.write_header();
        let s: &[u8] = &hdr;
        let key_max = u16::from_le_bytes([s[10], s[11]]);
        assert_eq!(key_max, 72);
    }

    #[test]
    fn test_write_header_vel_range_fields() {
        let mut region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        region.vel_range = GenericRange {
            min: 64.0,
            max: 100.0,
        };
        let hdr = region.write_header();
        let s: &[u8] = &hdr;
        let vel_min = u16::from_le_bytes([s[12], s[13]]);
        let vel_max = u16::from_le_bytes([s[14], s[15]]);
        assert_eq!(vel_min, 64);
        assert_eq!(vel_max, 100);
    }

    #[test]
    fn test_write_header_key_group() {
        let mut region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        region.key_group = 2;
        let hdr = region.write_header();
        let s: &[u8] = &hdr;
        // fusOptions at offset 16, keyGroup at offset 18
        let key_group = u16::from_le_bytes([s[18], s[19]]);
        assert_eq!(key_group, 2);
    }

    #[test]
    fn test_write_header_us_layer() {
        let mut region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        region.us_layer = 3;
        let hdr = region.write_header();
        let s: &[u8] = &hdr;
        let us_layer = u16::from_le_bytes([s[20], s[21]]);
        assert_eq!(us_layer, 3);
    }

    // -----------------------------------------------------------------------
    // write (top-level structure)
    // -----------------------------------------------------------------------

    #[test]
    fn test_write_is_list_chunk() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        let out = region.write();
        let s: &[u8] = &out;
        assert_eq!(&s[0..4], b"LIST");
    }

    #[test]
    fn test_write_list_type_is_rgn2() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        let out = region.write();
        let s: &[u8] = &out;
        assert_eq!(&s[8..12], b"rgn2");
    }

    #[test]
    fn test_write_contains_rgnh() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        let out = region.write();
        let bytes = out.to_vec();
        // Find "rgnh" anywhere in the serialised bytes
        assert!(
            bytes.windows(4).any(|w| w == b"rgnh"),
            "Expected 'rgnh' in output"
        );
    }

    #[test]
    fn test_write_contains_wlnk() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        let out = region.write();
        let bytes = out.to_vec();
        assert!(
            bytes.windows(4).any(|w| w == b"wlnk"),
            "Expected 'wlnk' in output"
        );
    }

    #[test]
    fn test_write_contains_wsmp() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        let out = region.write();
        let bytes = out.to_vec();
        assert!(
            bytes.windows(4).any(|w| w == b"wsmp"),
            "Expected 'wsmp' in output"
        );
    }

    // -----------------------------------------------------------------------
    // read – soft error cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_read_missing_wlnk_returns_none() {
        // Only rgnh, no wlnk → should be discarded
        let rgnh = encode_sub_chunk("rgnh", &make_rgnh_body(0, 127, 0, 127, 0, 0, None));
        let samples = vec![make_dls_sample()];
        let mut chunk = make_region_list("rgn2", &[rgnh]);
        let result = DownloadableSoundsRegion::read(&samples, &mut chunk);
        assert!(result.is_none());
    }

    #[test]
    fn test_read_missing_rgnh_returns_none() {
        // Only wlnk, no rgnh → should be discarded
        let wlnk = encode_sub_chunk("wlnk", &make_wlnk_body(0));
        let samples = vec![make_dls_sample()];
        let mut chunk = make_region_list("rgn2", &[wlnk]);
        let result = DownloadableSoundsRegion::read(&samples, &mut chunk);
        assert!(result.is_none());
    }

    #[test]
    fn test_read_invalid_sample_index_returns_none() {
        // table_index = 5 but samples has only 1 element → should be discarded
        let rgnh = encode_sub_chunk("rgnh", &make_rgnh_body(0, 127, 0, 127, 0, 0, None));
        let wlnk = encode_sub_chunk("wlnk", &make_wlnk_body(5));
        let samples = vec![make_dls_sample()];
        let mut chunk = make_region_list("rgn2", &[rgnh, wlnk]);
        let result = DownloadableSoundsRegion::read(&samples, &mut chunk);
        assert!(result.is_none());
    }

    #[test]
    fn test_read_invalid_list_type_returns_none() {
        // Chunk is "wvpl" list, not "rgn " or "rgn2" → fail
        let rgnh = encode_sub_chunk("rgnh", &make_rgnh_body(0, 127, 0, 127, 0, 0, None));
        let wlnk = encode_sub_chunk("wlnk", &make_wlnk_body(0));
        let samples = vec![make_dls_sample()];
        let mut chunk = make_region_list("wvpl", &[rgnh, wlnk]);
        let result = DownloadableSoundsRegion::read(&samples, &mut chunk);
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // read – success cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_read_success_key_range() {
        let samples = vec![make_dls_sample()];
        let mut chunk = make_region_chunk(0, 36, 60, 0, 127);
        let region = DownloadableSoundsRegion::read(&samples, &mut chunk).unwrap();
        assert_eq!(region.key_range.min, 36.0);
        assert_eq!(region.key_range.max, 60.0);
    }

    #[test]
    fn test_read_success_vel_range() {
        let samples = vec![make_dls_sample()];
        let mut chunk = make_region_chunk(0, 0, 127, 64, 127);
        let region = DownloadableSoundsRegion::read(&samples, &mut chunk).unwrap();
        assert_eq!(region.vel_range.min, 64.0);
        assert_eq!(region.vel_range.max, 127.0);
    }

    #[test]
    fn test_read_vel_range_zero_zero_fixed_to_0_127() {
        // velMin=0, velMax=0 → should be fixed to 0-127
        let samples = vec![make_dls_sample()];
        let mut chunk = make_region_chunk(0, 0, 127, 0, 0);
        let region = DownloadableSoundsRegion::read(&samples, &mut chunk).unwrap();
        assert_eq!(region.vel_range.min, 0.0);
        assert_eq!(region.vel_range.max, 127.0);
    }

    #[test]
    fn test_read_success_wave_link_table_index() {
        let samples = vec![make_dls_sample(), make_dls_sample()];
        let mut chunk = make_region_chunk(1, 0, 127, 0, 127);
        let region = DownloadableSoundsRegion::read(&samples, &mut chunk).unwrap();
        assert_eq!(region.wave_link.table_index, 1);
    }

    #[test]
    fn test_read_success_key_group_zero() {
        let samples = vec![make_dls_sample()];
        let mut chunk = make_region_chunk(0, 0, 127, 0, 127);
        let region = DownloadableSoundsRegion::read(&samples, &mut chunk).unwrap();
        assert_eq!(region.key_group, 0);
    }

    #[test]
    fn test_read_key_group_from_rgnh() {
        // key_group = 5, encoded in rgnh body
        let rgnh = encode_sub_chunk("rgnh", &make_rgnh_body(0, 127, 0, 127, 0, 5, Some(0)));
        let wlnk = encode_sub_chunk("wlnk", &make_wlnk_body(0));
        let samples = vec![make_dls_sample()];
        let mut chunk = make_region_list("rgn2", &[rgnh, wlnk]);
        let region = DownloadableSoundsRegion::read(&samples, &mut chunk).unwrap();
        assert_eq!(region.key_group, 5);
    }

    #[test]
    fn test_read_us_layer_from_rgnh() {
        let rgnh = encode_sub_chunk("rgnh", &make_rgnh_body(0, 127, 0, 127, 0, 0, Some(2)));
        let wlnk = encode_sub_chunk("wlnk", &make_wlnk_body(0));
        let samples = vec![make_dls_sample()];
        let mut chunk = make_region_list("rgn2", &[rgnh, wlnk]);
        let region = DownloadableSoundsRegion::read(&samples, &mut chunk).unwrap();
        assert_eq!(region.us_layer, 2);
    }

    #[test]
    fn test_read_us_layer_absent_is_zero() {
        // rgnh without usLayer field (only 12 bytes)
        let rgnh = encode_sub_chunk("rgnh", &make_rgnh_body(0, 127, 0, 127, 0, 0, None));
        let wlnk = encode_sub_chunk("wlnk", &make_wlnk_body(0));
        let samples = vec![make_dls_sample()];
        let mut chunk = make_region_list("rgn2", &[rgnh, wlnk]);
        let region = DownloadableSoundsRegion::read(&samples, &mut chunk).unwrap();
        assert_eq!(region.us_layer, 0);
    }

    #[test]
    fn test_read_rgn1_list_type_accepted() {
        // "rgn " (with trailing space) is also a valid list type
        let rgnh = encode_sub_chunk("rgnh", &make_rgnh_body(0, 127, 0, 127, 0, 0, None));
        let wlnk = encode_sub_chunk("wlnk", &make_wlnk_body(0));
        let samples = vec![make_dls_sample()];
        let mut chunk = make_region_list("rgn ", &[rgnh, wlnk]);
        let region = DownloadableSoundsRegion::read(&samples, &mut chunk);
        assert!(region.is_some());
    }

    // -----------------------------------------------------------------------
    // from_sf_zone
    // -----------------------------------------------------------------------

    #[test]
    fn test_from_sf_zone_invalid_sample_idx_returns_err() {
        let zone = make_instrument_zone(5); // out of bounds
        let samples: Vec<BasicSample> = vec![];
        let result = DownloadableSoundsRegion::from_sf_zone(&zone, &samples);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_sf_zone_key_range_from_zone() {
        let mut zone = make_instrument_zone(0);
        // key_range min=48, max=72 encoded as a KEY_RANGE generator: (72<<8)|48 = 18480
        use crate::soundbank::basic_soundbank::generator::Generator;
        zone.zone.add_generators(&[Generator::new_unvalidated(
            gt::KEY_RANGE,
            ((72_i32 << 8) | 48) as f64,
        )]);
        let samples = vec![make_basic_sample(60)];
        let region = DownloadableSoundsRegion::from_sf_zone(&zone, &samples).unwrap();
        assert_eq!(region.key_range.min, 48.0);
        assert_eq!(region.key_range.max, 72.0);
    }

    #[test]
    fn test_from_sf_zone_vel_range_min_clamped() {
        // SF2 "not set" sentinel (-1) should become 0 in DLS
        let mut zone = make_instrument_zone(0);
        zone.zone.vel_range = GenericRange {
            min: -1.0,
            max: 127.0,
        };
        let samples = vec![make_basic_sample(60)];
        let region = DownloadableSoundsRegion::from_sf_zone(&zone, &samples).unwrap();
        assert_eq!(region.vel_range.min, 0.0);
    }

    #[test]
    fn test_from_sf_zone_key_group_from_exclusive_class() {
        let mut zone = make_instrument_zone(0);
        zone.zone
            .set_generator(gt::EXCLUSIVE_CLASS, Some(3.0), true);
        let samples = vec![make_basic_sample(60)];
        let region = DownloadableSoundsRegion::from_sf_zone(&zone, &samples).unwrap();
        assert_eq!(region.key_group, 3);
    }

    #[test]
    fn test_from_sf_zone_wave_link_table_index_from_sample_idx() {
        let zone = make_instrument_zone(0);
        let samples = vec![make_basic_sample(60)];
        let region = DownloadableSoundsRegion::from_sf_zone(&zone, &samples).unwrap();
        assert_eq!(region.wave_link.table_index, 0);
    }

    // -----------------------------------------------------------------------
    // to_sf_zone
    // -----------------------------------------------------------------------

    #[test]
    fn test_to_sf_zone_invalid_sample_idx_returns_err() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(5), WaveSample::new());
        let mut instrument = BasicInstrument::new();
        let mut samples: Vec<BasicSample> = vec![];
        let result = region.to_sf_zone(&mut instrument, 0, &mut samples);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_sf_zone_creates_zone_in_instrument() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        let mut instrument = BasicInstrument::new();
        let mut samples = vec![make_basic_sample(60)];
        let zone_idx = region.to_sf_zone(&mut instrument, 0, &mut samples).unwrap();
        assert_eq!(zone_idx, 0);
        assert_eq!(instrument.zones.len(), 1);
    }

    #[test]
    fn test_to_sf_zone_key_range_set() {
        let mut region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        region.key_range = GenericRange {
            min: 36.0,
            max: 60.0,
        };
        let mut instrument = BasicInstrument::new();
        let mut samples = vec![make_basic_sample(60)];
        let zone_idx = region.to_sf_zone(&mut instrument, 0, &mut samples).unwrap();
        assert_eq!(instrument.zones[zone_idx].zone.key_range.min, 36.0);
        assert_eq!(instrument.zones[zone_idx].zone.key_range.max, 60.0);
    }

    #[test]
    fn test_to_sf_zone_full_key_range_sets_min_to_minus1() {
        // Default DLS range (0-127) → SF2 "not set" sentinel -1 for min
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        // key_range is already 0-127 by default
        let mut instrument = BasicInstrument::new();
        let mut samples = vec![make_basic_sample(60)];
        let zone_idx = region.to_sf_zone(&mut instrument, 0, &mut samples).unwrap();
        assert_eq!(instrument.zones[zone_idx].zone.key_range.min, -1.0);
    }

    #[test]
    fn test_to_sf_zone_full_vel_range_sets_min_to_minus1() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        let mut instrument = BasicInstrument::new();
        let mut samples = vec![make_basic_sample(60)];
        let zone_idx = region.to_sf_zone(&mut instrument, 0, &mut samples).unwrap();
        assert_eq!(instrument.zones[zone_idx].zone.vel_range.min, -1.0);
    }

    #[test]
    fn test_to_sf_zone_non_full_key_range_not_overridden() {
        let mut region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        region.key_range = GenericRange {
            min: 0.0,
            max: 60.0,
        }; // max ≠ 127
        let mut instrument = BasicInstrument::new();
        let mut samples = vec![make_basic_sample(60)];
        let zone_idx = region.to_sf_zone(&mut instrument, 0, &mut samples).unwrap();
        // Should keep min = 0, not set to -1
        assert_eq!(instrument.zones[zone_idx].zone.key_range.min, 0.0);
    }

    #[test]
    fn test_to_sf_zone_key_group_sets_exclusive_class() {
        let mut region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        region.key_group = 2;
        let mut instrument = BasicInstrument::new();
        let mut samples = vec![make_basic_sample(60)];
        let zone_idx = region.to_sf_zone(&mut instrument, 0, &mut samples).unwrap();
        let excl_class = instrument.zones[zone_idx]
            .zone
            .get_generator(gt::EXCLUSIVE_CLASS, -1);
        assert_eq!(excl_class, 2);
    }

    #[test]
    fn test_to_sf_zone_zero_key_group_no_exclusive_class() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        let mut instrument = BasicInstrument::new();
        let mut samples = vec![make_basic_sample(60)];
        let zone_idx = region.to_sf_zone(&mut instrument, 0, &mut samples).unwrap();
        // keyGroup=0 → should NOT set exclusiveClass
        let excl_class = instrument.zones[zone_idx]
            .zone
            .get_generator(gt::EXCLUSIVE_CLASS, -999);
        assert_eq!(excl_class, -999);
    }

    #[test]
    fn test_to_sf_zone_links_sample_to_instrument() {
        let region = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        let mut instrument = BasicInstrument::new();
        let mut samples = vec![make_basic_sample(60)];
        region.to_sf_zone(&mut instrument, 0, &mut samples).unwrap();
        // BasicSample should be linked to instrument idx 0
        assert!(samples[0].linked_to.contains(&0));
    }

    // -----------------------------------------------------------------------
    // write → read round-trip
    // -----------------------------------------------------------------------

    /// Builds a RIFFChunk from the raw bytes returned by `write()`.
    /// `write()` produces a LIST chunk: [LIST 4B][size 4B LE][body...].
    fn chunk_from_written(written: IndexedByteArray) -> RIFFChunk {
        let bytes = written.to_vec();
        let size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let data = IndexedByteArray::from_slice(&bytes[8..]);
        RIFFChunk::new("LIST".to_string(), size, data)
    }

    #[test]
    fn test_roundtrip_key_range() {
        let mut original = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        original.key_range = GenericRange {
            min: 48.0,
            max: 72.0,
        };

        let mut chunk = chunk_from_written(original.write());
        let samples = vec![make_dls_sample()];
        let recovered = DownloadableSoundsRegion::read(&samples, &mut chunk).unwrap();
        assert_eq!(recovered.key_range.min, 48.0);
        assert_eq!(recovered.key_range.max, 72.0);
    }

    #[test]
    fn test_roundtrip_vel_range() {
        let mut original = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        original.vel_range = GenericRange {
            min: 64.0,
            max: 100.0,
        };

        let mut chunk = chunk_from_written(original.write());
        let samples = vec![make_dls_sample()];
        let recovered = DownloadableSoundsRegion::read(&samples, &mut chunk).unwrap();
        assert_eq!(recovered.vel_range.min, 64.0);
        assert_eq!(recovered.vel_range.max, 100.0);
    }

    #[test]
    fn test_roundtrip_key_group() {
        let mut original = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());
        original.key_group = 3;

        let mut chunk = chunk_from_written(original.write());
        let samples = vec![make_dls_sample()];
        let recovered = DownloadableSoundsRegion::read(&samples, &mut chunk).unwrap();
        assert_eq!(recovered.key_group, 3);
    }

    #[test]
    fn test_roundtrip_wave_link_table_index() {
        let original = DownloadableSoundsRegion::new(WaveLink::new(0), WaveSample::new());

        let mut chunk = chunk_from_written(original.write());
        let samples = vec![make_dls_sample()];
        let recovered = DownloadableSoundsRegion::read(&samples, &mut chunk).unwrap();
        assert_eq!(recovered.wave_link.table_index, 0);
    }
}
