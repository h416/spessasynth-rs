/// articulation.rs
/// purpose: DLS Articulation (collection of connection blocks) with read/write and SF zone conversion.
/// Ported from: src/soundbank/downloadable_sounds/articulation.ts
use crate::soundbank::basic_soundbank::basic_zone::BasicZone;
use crate::soundbank::basic_soundbank::generator_types::{GeneratorType, generator_types as gt};
use crate::soundbank::basic_soundbank::modulator::Modulator;
use crate::soundbank::downloadable_sounds::connection_block::ConnectionBlock;
use crate::soundbank::downloadable_sounds::default_dls_modulators::{
    DLS_1_NO_VIBRATO_MOD, DLS_1_NO_VIBRATO_PRESSURE,
};
use crate::soundbank::downloadable_sounds::dls_verifier::verify_header;
use crate::soundbank::enums::{dls_destinations, dls_sources};
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::{read_little_endian_indexed, write_dword};
use crate::utils::loggin::spessa_synth_warn;
use crate::utils::riff_chunk::{
    RIFFChunk, find_riff_list_type, read_riff_chunk, write_riff_chunk_parts, write_riff_chunk_raw,
};

// ---------------------------------------------------------------------------
// DlsMode
// ---------------------------------------------------------------------------

/// DLS articulation mode: level 1 or level 2.
/// Equivalent to: "dls1" | "dls2"
#[derive(Clone, Debug, PartialEq)]
pub enum DlsMode {
    Dls1,
    Dls2,
}

// ---------------------------------------------------------------------------
// DownloadableSoundsArticulation
// ---------------------------------------------------------------------------

/// A DLS articulation: a list of connection blocks and their parse mode.
/// Equivalent to: class DownloadableSoundsArticulation extends DLSVerifier
pub struct DownloadableSoundsArticulation {
    /// The connection blocks in this articulation.
    /// Equivalent to: readonly connectionBlocks: ConnectionBlock[]
    pub connection_blocks: Vec<ConnectionBlock>,
    /// DLS level (1 or 2), determines chunk names in read/write.
    /// Equivalent to: mode: "dls1" | "dls2"
    pub mode: DlsMode,
}

impl DownloadableSoundsArticulation {
    /// Creates a new, empty articulation in DLS2 mode.
    pub fn new() -> Self {
        Self {
            connection_blocks: Vec::new(),
            mode: DlsMode::Dls2,
        }
    }

    /// Returns the number of connection blocks.
    /// Equivalent to: get length(): number
    pub fn len(&self) -> usize {
        self.connection_blocks.len()
    }

    /// Returns true if the articulation has no connection blocks.
    pub fn is_empty(&self) -> bool {
        self.connection_blocks.is_empty()
    }

    /// Deep-copies an existing articulation.
    /// Equivalent to: copyFrom(inputArticulation: DownloadableSoundsArticulation)
    pub fn copy_from(&mut self, input: &DownloadableSoundsArticulation) {
        self.mode = input.mode.clone();
        for block in &input.connection_blocks {
            self.connection_blocks
                .push(ConnectionBlock::copy_from(block));
        }
    }

    /// Converts an SF BasicZone into DLS connection blocks.
    /// Equivalent to: fromSFZone(z: BasicInstrumentZone)
    ///
    /// TypeScript takes `BasicInstrumentZone` (which extends `BasicZone`).
    /// In Rust, `BasicInstrumentZone` wraps a `zone: BasicZone`; callers pass `&z.zone`.
    #[allow(clippy::wrong_self_convention)]
    pub fn from_sf_zone(&mut self, z: &BasicZone) {
        self.mode = DlsMode::Dls2;

        // Copy to avoid modifying the input zone.
        let mut zone = BasicZone::new();
        zone.copy_from(z);

        // Read_articulation.ts:
        // According to viena and another strange (with modulators) rendition of gm.dls in sf2,
        // It shall be divided by -128,
        // And a strange correction needs to be applied to the real value:
        // Real + (60 / 128) * scale
        // We do this here.
        let generators_snapshot: Vec<_> = zone.generators.clone();
        for rel_gen in &generators_snapshot {
            let absolute_counterpart: GeneratorType = match rel_gen.generator_type {
                gt::KEY_NUM_TO_VOL_ENV_DECAY => gt::DECAY_VOL_ENV,
                gt::KEY_NUM_TO_VOL_ENV_HOLD => gt::HOLD_VOL_ENV,
                gt::KEY_NUM_TO_MOD_ENV_DECAY => gt::DECAY_MOD_ENV,
                gt::KEY_NUM_TO_MOD_ENV_HOLD => gt::HOLD_MOD_ENV,
                _ => continue,
            };

            let absolute_value_opt = zone
                .generators
                .iter()
                .find(|g| g.generator_type == absolute_counterpart)
                .map(|g| g.generator_value as f64);

            let dls_relative = (rel_gen.generator_value as i32) * -128;

            let absolute_value = match absolute_value_opt {
                Some(v) => v,
                None => continue, // No absolute generator here
            };

            let subtraction = (60.0 / 128.0) * dls_relative as f64;
            let new_absolute = absolute_value - subtraction;

            zone.set_generator(rel_gen.generator_type, Some(dls_relative as f64), false);
            zone.set_generator(absolute_counterpart, Some(new_absolute), false);
        }

        let generators = zone.generators.clone();
        for generator in &generators {
            ConnectionBlock::from_sf_generator(generator, self);
        }
        let modulators = zone.modulators.clone();
        for modulator in &modulators {
            ConnectionBlock::from_sf_modulator(modulator, self);
        }
    }

    /// Reads DLS articulation data from a chunk list (lart or lar2 LIST).
    /// Equivalent to: read(chunks: RIFFChunk[])
    pub fn read(&mut self, chunks: &mut [RIFFChunk]) {
        if let Some(lart) = find_riff_list_type(chunks, "lart") {
            self.mode = DlsMode::Dls1;
            while lart.data.current_index < lart.data.len() {
                let art1 = read_riff_chunk(&mut lart.data, true, false);
                // Note: DLS spec says lart should only have art1, but DirectMusic Producer
                // "FarmGame.dls" has art1 in lar2. We allow both.
                if verify_header(&art1, &["art1", "art2"]).is_err() {
                    spessa_synth_warn(&format!(
                        "Unexpected chunk header in lart: \"{}\"",
                        art1.header
                    ));
                    break;
                }
                let mut art_data = art1.data;
                let cb_size = read_little_endian_indexed(&mut art_data, 4);
                if cb_size != 8 {
                    spessa_synth_warn(&format!(
                        "CbSize in articulation mismatch. Expected 8, got {cb_size}"
                    ));
                }
                let connections_amount = read_little_endian_indexed(&mut art_data, 4);
                for _ in 0..connections_amount {
                    self.connection_blocks
                        .push(ConnectionBlock::read(&mut art_data));
                }
            }
        } else if let Some(lar2) = find_riff_list_type(chunks, "lar2") {
            self.mode = DlsMode::Dls2;
            while lar2.data.current_index < lar2.data.len() {
                let art2 = read_riff_chunk(&mut lar2.data, true, false);
                // Note: same as above – allow both art2 and art1 in lar2.
                if verify_header(&art2, &["art2", "art1"]).is_err() {
                    spessa_synth_warn(&format!(
                        "Unexpected chunk header in lar2: \"{}\"",
                        art2.header
                    ));
                    break;
                }
                let mut art_data = art2.data;
                let cb_size = read_little_endian_indexed(&mut art_data, 4);
                if cb_size != 8 {
                    spessa_synth_warn(&format!(
                        "CbSize in articulation mismatch. Expected 8, got {cb_size}"
                    ));
                }
                let connections_amount = read_little_endian_indexed(&mut art_data, 4);
                for _ in 0..connections_amount {
                    self.connection_blocks
                        .push(ConnectionBlock::read(&mut art_data));
                }
            }
        }
    }

    /// Serialises this articulation as a lar2/lart LIST chunk (containing art2/art1).
    /// Note: this writes "lar2" or "lart", not just "art2" / "art1".
    /// Equivalent to: write()
    pub fn write(&self) -> IndexedByteArray {
        let mut art_header_data = IndexedByteArray::new(8);
        write_dword(&mut art_header_data, 8); // CbSize
        write_dword(&mut art_header_data, self.connection_blocks.len() as u32); // CConnectionBlocks

        let block_arrays: Vec<IndexedByteArray> =
            self.connection_blocks.iter().map(|b| b.write()).collect();

        let (chunk_name, list_name) = if self.mode == DlsMode::Dls2 {
            ("art2", "lar2")
        } else {
            ("art1", "lart")
        };

        // Build a slice of &[u8] references: [header, block0, block1, ...]
        let header_slice: &[u8] = &art_header_data;
        let mut parts: Vec<&[u8]> = vec![header_slice];
        for arr in &block_arrays {
            parts.push(arr);
        }

        let art2 = write_riff_chunk_parts(chunk_name, &parts, false);
        write_riff_chunk_raw(list_name, &art2, false, true)
    }

    /// Converts DLS articulation into an SF zone (applying generators and modulators).
    /// Equivalent to: toSFZone(zone: BasicZone)
    pub fn to_sf_zone(&self, zone: &mut BasicZone) {
        for connection in &self.connection_blocks {
            let amount = connection.short_scale();
            let source = connection.source.source;
            let control = connection.control.source;

            // If source and control are both zero (none), it's a static generator
            if connection.is_static_parameter() {
                connection.to_sf_generator(zone);
                continue;
            }

            // A few special cases which are generators
            if control == dls_sources::NONE {
                if source == dls_sources::KEY_NUM {
                    // Scale tuning: keyNum → pitch
                    if connection.destination == dls_destinations::PITCH {
                        zone.set_generator(gt::SCALE_TUNING, Some(amount as f64 / 128.0), true);
                        continue;
                    }
                    // Key-to-envelope targets will be handled after this loop
                    if connection.destination == dls_destinations::MOD_ENV_HOLD
                        || connection.destination == dls_destinations::MOD_ENV_DECAY
                        || connection.destination == dls_destinations::VOL_ENV_HOLD
                        || connection.destination == dls_destinations::VOL_ENV_DECAY
                    {
                        continue;
                    }
                } else {
                    // Check for compound SF destination (e.g. modLfoToPitch)
                    if let Some(sf_gen) = connection.to_combined_sf_destination() {
                        zone.set_generator(sf_gen, Some(amount as f64), true);
                        continue;
                    }
                }
            }

            // General modulator
            connection.to_sf_modulator(zone);
        }

        // DLS 1 does not have vibrato LFO: disable it with zero-amount modulators
        if self.mode == DlsMode::Dls1 {
            let no_vib_mod = Modulator::new(
                DLS_1_NO_VIBRATO_MOD.primary_source(),
                DLS_1_NO_VIBRATO_MOD.secondary_source(),
                DLS_1_NO_VIBRATO_MOD.destination,
                DLS_1_NO_VIBRATO_MOD.transform_amount,
                DLS_1_NO_VIBRATO_MOD.transform_type,
                DLS_1_NO_VIBRATO_MOD.is_effect_modulator,
                DLS_1_NO_VIBRATO_MOD.is_default_resonant_modulator,
            );
            let no_vib_pressure = Modulator::new(
                DLS_1_NO_VIBRATO_PRESSURE.primary_source(),
                DLS_1_NO_VIBRATO_PRESSURE.secondary_source(),
                DLS_1_NO_VIBRATO_PRESSURE.destination,
                DLS_1_NO_VIBRATO_PRESSURE.transform_amount,
                DLS_1_NO_VIBRATO_PRESSURE.transform_type,
                DLS_1_NO_VIBRATO_PRESSURE.is_effect_modulator,
                DLS_1_NO_VIBRATO_PRESSURE.is_default_resonant_modulator,
            );
            zone.add_modulators(&[no_vib_mod, no_vib_pressure]);
        }

        // Perform correction for key-to-envelope generators.
        //
        // According to viena and another strange (with modulators) rendition of gm.dls in sf2,
        // It shall be divided by -128
        // And a strange correction needs to be applied to the real (generator) value:
        //   Real + (60 / 128) * scale
        // Where real means the actual generator (e.g. decayVolEnv)
        // And scale means the keyNumToVolEnvDecay
        for connection in &self.connection_blocks {
            if connection.source.source != dls_sources::KEY_NUM {
                continue;
            }
            let value = connection.short_scale();
            let (key_to_gen, real_gen, dls_dest) = match connection.destination {
                dls_destinations::VOL_ENV_HOLD => (
                    gt::KEY_NUM_TO_VOL_ENV_HOLD,
                    gt::HOLD_VOL_ENV,
                    dls_destinations::VOL_ENV_HOLD,
                ),
                dls_destinations::VOL_ENV_DECAY => (
                    gt::KEY_NUM_TO_VOL_ENV_DECAY,
                    gt::DECAY_VOL_ENV,
                    dls_destinations::VOL_ENV_DECAY,
                ),
                dls_destinations::MOD_ENV_HOLD => (
                    gt::KEY_NUM_TO_MOD_ENV_HOLD,
                    gt::HOLD_MOD_ENV,
                    dls_destinations::MOD_ENV_HOLD,
                ),
                dls_destinations::MOD_ENV_DECAY => (
                    gt::KEY_NUM_TO_MOD_ENV_DECAY,
                    gt::DECAY_MOD_ENV,
                    dls_destinations::MOD_ENV_DECAY,
                ),
                _ => continue,
            };

            let key_to_gen_value = value as f64 / -128.0;
            zone.set_generator(key_to_gen, Some(key_to_gen_value), true);

            // Airfont 340 fix: only apply correction when keyToGenValue <= 120
            if key_to_gen_value <= 120.0 {
                let correction = ((60.0 / 128.0) * value as f64).round() as i32;

                // Find the static connection block with this DLS destination
                let real_scale = self
                    .connection_blocks
                    .iter()
                    .find(|b| b.is_static_parameter() && b.destination == dls_dest)
                    .map(|b| b.short_scale());

                if let Some(real_val) = real_scale {
                    zone.set_generator(real_gen, Some((correction + real_val) as f64), true);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator::Generator;
    use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
    use crate::soundbank::downloadable_sounds::connection_source::ConnectionSource;
    use crate::soundbank::enums::{dls_destinations as dd, dls_sources as ds};
    use crate::utils::little_endian::write_word;
    use crate::utils::riff_chunk::write_riff_chunk_raw;
    use crate::utils::string::write_binary_string_indexed;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn make_art_bytes(connections: &[ConnectionBlock]) -> Vec<u8> {
        // 8-byte art header: cbSize=8, nConnections=len
        let mut art_header = IndexedByteArray::new(8);
        write_dword(&mut art_header, 8);
        write_dword(&mut art_header, connections.len() as u32);

        let mut bytes = art_header.to_vec();
        for block in connections {
            bytes.extend_from_slice(&block.write().to_vec());
        }
        bytes
    }

    fn make_list_chunk(list_type: &str, content: &[u8]) -> RIFFChunk {
        // LIST [ size (4B LE) ] [ list_type (4B) ] [ content ]
        let mut data = IndexedByteArray::new(4 + content.len());
        write_binary_string_indexed(&mut data, list_type, 0);
        for (i, &b) in content.iter().enumerate() {
            data[4 + i] = b;
        }
        RIFFChunk::new("LIST".to_string(), (4 + content.len()) as u32, data)
    }

    fn art_chunk_as_list(art_type: &str, connections: &[ConnectionBlock]) -> RIFFChunk {
        // art2/art1 chunk bytes wrapped in a lar2/lart LIST
        let art_body = make_art_bytes(connections);
        let list_name = if art_type == "art2" { "lar2" } else { "lart" };

        // Build inner art chunk: [art_type(4)][size(4)][body]
        let inner_chunk = write_riff_chunk_raw(art_type, &art_body, false, false);
        make_list_chunk(list_name, &inner_chunk)
    }

    fn static_block(destination: u16, scale: i32) -> ConnectionBlock {
        ConnectionBlock::new(
            ConnectionSource::default(),
            ConnectionSource::default(),
            destination,
            0,
            scale,
        )
    }

    // ── new / len / is_empty ──────────────────────────────────────────────────

    #[test]
    fn test_new_starts_empty() {
        let art = DownloadableSoundsArticulation::new();
        assert!(art.connection_blocks.is_empty());
        assert_eq!(art.len(), 0);
        assert!(art.is_empty());
    }

    #[test]
    fn test_new_default_mode_dls2() {
        let art = DownloadableSoundsArticulation::new();
        assert_eq!(art.mode, DlsMode::Dls2);
    }

    #[test]
    fn test_len_after_push() {
        let mut art = DownloadableSoundsArticulation::new();
        art.connection_blocks.push(static_block(dd::PAN, 0));
        assert_eq!(art.len(), 1);
        assert!(!art.is_empty());
    }

    // ── copy_from ─────────────────────────────────────────────────────────────

    #[test]
    fn test_copy_from_copies_mode() {
        let mut src = DownloadableSoundsArticulation::new();
        src.mode = DlsMode::Dls1;
        let mut dst = DownloadableSoundsArticulation::new();
        dst.copy_from(&src);
        assert_eq!(dst.mode, DlsMode::Dls1);
    }

    #[test]
    fn test_copy_from_copies_blocks() {
        let mut src = DownloadableSoundsArticulation::new();
        src.connection_blocks.push(static_block(dd::PAN, 100 << 16));
        let mut dst = DownloadableSoundsArticulation::new();
        dst.copy_from(&src);
        assert_eq!(dst.connection_blocks.len(), 1);
        assert_eq!(dst.connection_blocks[0].destination, dd::PAN);
    }

    #[test]
    fn test_copy_from_is_independent() {
        let mut src = DownloadableSoundsArticulation::new();
        src.connection_blocks.push(static_block(dd::PAN, 0));
        let mut dst = DownloadableSoundsArticulation::new();
        dst.copy_from(&src);
        // Mutating dst does not affect src
        dst.connection_blocks.push(static_block(dd::GAIN, 0));
        assert_eq!(src.connection_blocks.len(), 1);
    }

    // ── write / read round-trip ──────────────────────────────────────────────

    #[test]
    fn test_write_empty_dls2_produces_lar2() {
        let art = DownloadableSoundsArticulation::new();
        let out = art.write();
        // Convert to &[u8] for slice comparisons (IndexedByteArray has no Range<usize> index)
        let s: &[u8] = &out;
        // Output starts with "LIST" (is_list=true in write_riff_chunk_raw)
        assert_eq!(&s[0..4], b"LIST");
        // LIST type should be "lar2"
        assert_eq!(&s[8..12], b"lar2");
    }

    #[test]
    fn test_write_empty_dls1_produces_lart() {
        let mut art = DownloadableSoundsArticulation::new();
        art.mode = DlsMode::Dls1;
        let out = art.write();
        let s: &[u8] = &out;
        assert_eq!(&s[0..4], b"LIST");
        assert_eq!(&s[8..12], b"lart");
    }

    #[test]
    fn test_write_read_roundtrip_dls2() {
        let mut art = DownloadableSoundsArticulation::new();
        art.connection_blocks.push(static_block(dd::PAN, 50 << 16));
        art.connection_blocks
            .push(static_block(dd::GAIN, -100 << 16));

        let written = art.write();
        // `written` is a complete serialised LIST chunk.
        // Build a RIFFChunk for find_riff_list_type: header="LIST", data=written[8..]
        let written_len = written.len();
        let s: &[u8] = &written;
        let size = u32::from_le_bytes([s[4], s[5], s[6], s[7]]);
        let data = IndexedByteArray::from_slice(&s[8..]);
        let list_chunk = RIFFChunk::new("LIST".to_string(), size, data);
        let mut chunks = vec![list_chunk];

        let mut recovered = DownloadableSoundsArticulation::new();
        recovered.read(&mut chunks);

        assert_eq!(recovered.mode, DlsMode::Dls2);
        assert_eq!(recovered.connection_blocks.len(), 2);
        assert_eq!(recovered.connection_blocks[0].destination, dd::PAN);
        assert_eq!(recovered.connection_blocks[0].short_scale(), 50);
        assert_eq!(recovered.connection_blocks[1].destination, dd::GAIN);
        assert_eq!(recovered.connection_blocks[1].short_scale(), -100);
        let _ = written_len; // suppress unused warning
    }

    // ── read ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_read_from_lar2_sets_dls2_mode() {
        let block = static_block(dd::PAN, 0);
        let list_chunk = art_chunk_as_list("art2", &[block]);
        let mut chunks = vec![list_chunk];

        let mut art = DownloadableSoundsArticulation::new();
        art.read(&mut chunks);

        assert_eq!(art.mode, DlsMode::Dls2);
    }

    #[test]
    fn test_read_from_lart_sets_dls1_mode() {
        let block = static_block(dd::PAN, 0);
        let list_chunk = art_chunk_as_list("art1", &[block]);
        let mut chunks = vec![list_chunk];

        let mut art = DownloadableSoundsArticulation::new();
        art.read(&mut chunks);

        assert_eq!(art.mode, DlsMode::Dls1);
    }

    #[test]
    fn test_read_correct_number_of_blocks() {
        let blocks = vec![
            static_block(dd::PAN, 10 << 16),
            static_block(dd::GAIN, 20 << 16),
            static_block(dd::PITCH, 30 << 16),
        ];
        let list_chunk = art_chunk_as_list("art2", &blocks);
        let mut chunks = vec![list_chunk];

        let mut art = DownloadableSoundsArticulation::new();
        art.read(&mut chunks);

        assert_eq!(art.connection_blocks.len(), 3);
    }

    #[test]
    fn test_read_empty_chunks_leaves_empty() {
        let mut chunks: Vec<RIFFChunk> = Vec::new();
        let mut art = DownloadableSoundsArticulation::new();
        art.read(&mut chunks);
        assert!(art.connection_blocks.is_empty());
    }

    // ── to_sf_zone ────────────────────────────────────────────────────────────

    #[test]
    fn test_to_sf_zone_static_pan() {
        let mut art = DownloadableSoundsArticulation::new();
        art.connection_blocks.push(static_block(dd::PAN, 200 << 16));
        let mut zone = BasicZone::new();
        art.to_sf_zone(&mut zone);
        assert_eq!(zone.get_generator(gt::PAN, -999), 200);
    }

    #[test]
    fn test_to_sf_zone_scale_tuning() {
        // keyNum → pitch with scale=100<<16: scaleTuning = 100/128 ≈ 0 (rounded to int)
        let mut art = DownloadableSoundsArticulation::new();
        let block = ConnectionBlock::new(
            ConnectionSource::new(ds::KEY_NUM, 0, false, false),
            ConnectionSource::default(),
            dd::PITCH,
            0,
            100 << 16,
        );
        art.connection_blocks.push(block);
        let mut zone = BasicZone::new();
        art.to_sf_zone(&mut zone);
        // scaleTuning = 100 / 128 = 0.78125, f64::round() → 1
        let scale_tuning = zone.get_generator(gt::SCALE_TUNING, -9999);
        assert_eq!(scale_tuning, 1);
    }

    #[test]
    fn test_to_sf_zone_dls1_adds_vibrato_modulators() {
        let mut art = DownloadableSoundsArticulation::new();
        art.mode = DlsMode::Dls1;
        let mut zone = BasicZone::new();
        art.to_sf_zone(&mut zone);
        // Two no-vibrato modulators should have been added
        assert_eq!(zone.modulators.len(), 2);
    }

    #[test]
    fn test_to_sf_zone_dls2_no_vibrato_modulators() {
        let mut art = DownloadableSoundsArticulation::new();
        art.mode = DlsMode::Dls2;
        let mut zone = BasicZone::new();
        art.to_sf_zone(&mut zone);
        assert_eq!(zone.modulators.len(), 0);
    }

    #[test]
    fn test_to_sf_zone_key_to_vol_env_hold_correction() {
        // keyNum → volEnvHold  value = -128 → keyNumToVolEnvHold = 1
        let mut art = DownloadableSoundsArticulation::new();
        let key_block = ConnectionBlock::new(
            ConnectionSource::new(ds::KEY_NUM, 0, false, false),
            ConnectionSource::default(),
            dd::VOL_ENV_HOLD,
            0,
            (-128_i32) << 16,
        );
        // static hold block for correction
        let hold_block = static_block(dd::VOL_ENV_HOLD, 500 << 16);
        art.connection_blocks.push(key_block);
        art.connection_blocks.push(hold_block);
        let mut zone = BasicZone::new();
        art.to_sf_zone(&mut zone);
        // keyNumToVolEnvHold = -128 / -128 = 1
        assert_eq!(zone.get_generator(gt::KEY_NUM_TO_VOL_ENV_HOLD, -999), 1);
    }

    // ── from_sf_zone ──────────────────────────────────────────────────────────

    #[test]
    fn test_from_sf_zone_sets_dls2_mode() {
        let mut art = DownloadableSoundsArticulation::new();
        art.mode = DlsMode::Dls1;
        let zone = BasicZone::new();
        art.from_sf_zone(&zone);
        assert_eq!(art.mode, DlsMode::Dls2);
    }

    #[test]
    fn test_from_sf_zone_pan_generator_creates_block() {
        let mut art = DownloadableSoundsArticulation::new();
        let mut zone = BasicZone::new();
        zone.set_generator(gt::PAN, Some(150.0), true);
        art.from_sf_zone(&zone);
        let pan_block = art
            .connection_blocks
            .iter()
            .find(|b| b.is_static_parameter() && b.destination == dd::PAN);
        assert!(pan_block.is_some(), "Expected a pan connection block");
        assert_eq!(pan_block.unwrap().short_scale(), 150);
    }

    #[test]
    fn test_from_sf_zone_empty_zone_creates_no_blocks() {
        let mut art = DownloadableSoundsArticulation::new();
        let zone = BasicZone::new();
        art.from_sf_zone(&zone);
        assert!(art.connection_blocks.is_empty());
    }

    #[test]
    fn test_from_sf_zone_does_not_modify_original_zone() {
        let mut art = DownloadableSoundsArticulation::new();
        let mut zone = BasicZone::new();
        zone.set_generator(gt::PAN, Some(50.0), true);
        art.from_sf_zone(&zone);
        // Original zone should still have the pan generator unchanged
        assert_eq!(zone.get_generator(gt::PAN, -999), 50);
    }

    // ── write content correctness ─────────────────────────────────────────────

    #[test]
    fn test_write_contains_correct_connection_count() {
        let mut art = DownloadableSoundsArticulation::new();
        art.connection_blocks.push(static_block(dd::PAN, 0));
        art.connection_blocks.push(static_block(dd::GAIN, 0));
        let out = art.write();
        // Layout: LIST(4) + size(4) + "lar2"(4) + "art2"(4) + art2_size(4) + cbSize(4) + nConns(4)
        // nConns is at bytes 24..28
        let n_conns = u32::from_le_bytes([out[24], out[25], out[26], out[27]]);
        assert_eq!(n_conns, 2);
    }

    #[test]
    fn test_write_cb_size_is_8() {
        let art = DownloadableSoundsArticulation::new();
        let out = art.write();
        // cbSize at bytes 16..20
        let cb_size = u32::from_le_bytes([out[16], out[17], out[18], out[19]]);
        assert_eq!(cb_size, 8);
    }
}
