/// presets.rs
/// purpose: SoundFont preset struct and reader.
/// Ported from: src/soundbank/soundfont/read/presets.ts
///
/// # TypeScript vs Rust design differences
///
/// - `SoundFontPreset` contains a `BasicPreset` by value (composition) instead of inheriting
/// - The `sf2: BasicSoundBank` constructor parameter is dropped (no `parentSoundBank` in Rust)
/// - `\d{3}:\d{3}` name-patch stripping is implemented without a regex crate
/// - Implements `SoundFontPresetZoneSink` (from `preset_zones.rs`) so that `apply_preset_zones`
///   can populate zones without depending on `BasicSoundBank`
use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::basic_preset_zone::BasicPresetZone;
use crate::soundbank::basic_soundbank::basic_zone::BasicZone;
use crate::soundbank::soundfont::read::preset_zones::SoundFontPresetZoneSink;
use crate::utils::little_endian::read_little_endian_indexed;
use crate::utils::riff_chunk::RIFFChunk;
use crate::utils::string::read_binary_string_indexed;

// ---------------------------------------------------------------------------
// strip_patch_number (module-private helper)
// ---------------------------------------------------------------------------

/// Removes the first occurrence of the `DDD:DDD` patch-number pattern from a name.
///
/// Some SF2 files embed e.g. `"000:001"` inside the preset name field.
///
/// Equivalent to: `name.replace(/\d{3}:\d{3}/, "")`  (first occurrence only)
fn strip_patch_number(name: &str) -> String {
    let bytes = name.as_bytes();
    for i in 0..bytes.len().saturating_sub(6) {
        if bytes[i].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3] == b':'
            && bytes[i + 4].is_ascii_digit()
            && bytes[i + 5].is_ascii_digit()
            && bytes[i + 6].is_ascii_digit()
        {
            let mut result = name.to_string();
            result.drain(i..i + 7);
            return result;
        }
    }
    name.to_string()
}

// ---------------------------------------------------------------------------
// SoundFontPreset
// ---------------------------------------------------------------------------

/// A SoundFont preset record, extending BasicPreset with zone-bag indexing.
/// Equivalent to: class SoundFontPreset extends BasicPreset
#[derive(Clone, Debug)]
pub struct SoundFontPreset {
    /// Base preset data (name, program, bank, zones, global zone, …).
    pub preset: BasicPreset,

    /// Index of the first preset bag (zone) entry for this preset in the PBAG chunk.
    /// Equivalent to: public zoneStartIndex: number
    pub zone_start_index: usize,

    /// Number of preset bag entries (zones) for this preset.
    /// Calculated as `next.zone_start_index - self.zone_start_index` in `read_presets`.
    /// Equivalent to: public zonesCount = 0
    pub zones_count: usize,
}

impl SoundFontPreset {
    /// Parses one 38-byte PHDR record from `chunk`:
    ///
    /// | offset | size | field |
    /// |--------|------|-------|
    /// | 0      | 20   | preset name (null-padded ASCII) |
    /// | 20     | 2    | program (WORD) |
    /// | 22     | 2    | wBank: bits 0-6 = bankMSB, bit 7 = drum flag, bits 8-15 = bankLSB |
    /// | 24     | 2    | zone start index (WORD) |
    /// | 26     | 4    | library (DWORD) |
    /// | 30     | 4    | genre (DWORD) |
    /// | 34     | 4    | morphology (DWORD) |
    ///
    /// Equivalent to: `constructor(presetChunk: RIFFChunk, sf2: BasicSoundBank)`
    pub fn new(chunk: &mut RIFFChunk) -> Self {
        let raw_name = read_binary_string_indexed(&mut chunk.data, 20);
        let name = strip_patch_number(&raw_name);

        let program = read_little_endian_indexed(&mut chunk.data, 2) as u8;
        let w_bank = read_little_endian_indexed(&mut chunk.data, 2) as u16;
        let bank_msb = (w_bank & 0x7f) as u8;
        let is_gm_gs_drum = (w_bank & 0x80) > 0;
        let bank_lsb = (w_bank >> 8) as u8;

        let zone_start_index = read_little_endian_indexed(&mut chunk.data, 2) as usize;
        let library = read_little_endian_indexed(&mut chunk.data, 4);
        let genre = read_little_endian_indexed(&mut chunk.data, 4);
        let morphology = read_little_endian_indexed(&mut chunk.data, 4);

        let mut preset = BasicPreset::new();
        preset.name = name;
        preset.program = program;
        preset.bank_msb = bank_msb;
        preset.is_gm_gs_drum = is_gm_gs_drum;
        preset.bank_lsb = bank_lsb;
        preset.library = library;
        preset.genre = genre;
        preset.morphology = morphology;

        SoundFontPreset {
            preset,
            zone_start_index,
            zones_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// SoundFontPresetZoneSink impl
// ---------------------------------------------------------------------------

impl SoundFontPresetZoneSink for SoundFontPreset {
    /// Returns the zone-bag count for this preset.
    /// Equivalent to: `preset.zonesCount`
    fn zones_count(&self) -> usize {
        self.zones_count
    }

    /// Appends a parsed preset zone to the underlying `BasicPreset`.
    /// Equivalent to: `preset.zones.push(zone)`  (via `createSoundFontZone`)
    fn push_zone(&mut self, zone: BasicPresetZone) {
        self.preset.zones.push(zone);
    }

    /// Returns a mutable reference to the preset's global zone.
    /// Equivalent to: `preset.globalZone`
    fn global_zone_mut(&mut self) -> &mut BasicZone {
        &mut self.preset.global_zone
    }
}

// ---------------------------------------------------------------------------
// read_presets
// ---------------------------------------------------------------------------

/// Reads all SoundFont presets from a PHDR sub-chunk.
///
/// The last entry is the EOP (End Of Presets) sentinel and is discarded.
/// `zones_count` for each preset is set to
/// `next.zone_start_index - current.zone_start_index`.
///
/// Equivalent to:
/// `function readPresets(presetChunk: RIFFChunk, parent: BasicSoundBank): SoundFontPreset[]`
///
/// Note: the `parent: BasicSoundBank` parameter of the TypeScript version is not needed in Rust
/// because `BasicPreset` no longer stores a `parentSoundBank` reference.
pub fn read_presets(chunk: &mut RIFFChunk) -> Vec<SoundFontPreset> {
    let mut presets: Vec<SoundFontPreset> = Vec::new();

    while chunk.data.len() > chunk.data.current_index {
        let preset = SoundFontPreset::new(chunk);

        // Set the previous preset's zones_count from the current zone_start_index.
        // Equivalent to: previous.zonesCount = preset.zoneStartIndex - previous.zoneStartIndex
        if let Some(previous) = presets.last_mut() {
            previous.zones_count = preset.zone_start_index - previous.zone_start_index;
        }

        presets.push(preset);
    }

    // Remove EOP (End Of Presets sentinel record)
    presets.pop();

    presets
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::basic_preset_zone::BasicPresetZone;
    use crate::soundbank::basic_soundbank::generator::Generator;
    use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
    use crate::utils::indexed_array::IndexedByteArray;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Builds a raw 38-byte PHDR record.
    fn make_phdr_bytes(
        name: &str,
        program: u16,
        w_bank: u16,
        zone_start: u16,
        library: u32,
        genre: u32,
        morphology: u32,
    ) -> Vec<u8> {
        let mut bytes = vec![0u8; 38];
        let name_bytes = name.as_bytes();
        let copy_len = name_bytes.len().min(20);
        bytes[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
        bytes[20..22].copy_from_slice(&program.to_le_bytes());
        bytes[22..24].copy_from_slice(&w_bank.to_le_bytes());
        bytes[24..26].copy_from_slice(&zone_start.to_le_bytes());
        bytes[26..30].copy_from_slice(&library.to_le_bytes());
        bytes[30..34].copy_from_slice(&genre.to_le_bytes());
        bytes[34..38].copy_from_slice(&morphology.to_le_bytes());
        bytes
    }

    fn make_phdr_simple(name: &str, program: u16, w_bank: u16, zone_start: u16) -> Vec<u8> {
        make_phdr_bytes(name, program, w_bank, zone_start, 0, 0, 0)
    }

    fn make_chunk(data: Vec<u8>) -> RIFFChunk {
        let len = data.len();
        RIFFChunk::new(
            "phdr".to_string(),
            len as u32,
            IndexedByteArray::from_vec(data),
        )
    }

    // -----------------------------------------------------------------------
    // strip_patch_number
    // -----------------------------------------------------------------------

    #[test]
    fn test_strip_no_pattern_unchanged() {
        assert_eq!(strip_patch_number("Grand Piano"), "Grand Piano");
    }

    #[test]
    fn test_strip_prefix_000_001() {
        assert_eq!(strip_patch_number("000:001Grand Piano"), "Grand Piano");
    }

    #[test]
    fn test_strip_embedded_pattern() {
        assert_eq!(strip_patch_number("Grand000:001Piano"), "GrandPiano");
    }

    #[test]
    fn test_strip_trailing_pattern() {
        assert_eq!(strip_patch_number("Grand Piano000:001"), "Grand Piano");
    }

    #[test]
    fn test_strip_first_occurrence_only() {
        // JavaScript replace without /g replaces only the first match
        assert_eq!(strip_patch_number("000:001abc000:002"), "abc000:002");
    }

    #[test]
    fn test_strip_non_digit_before_colon_unchanged() {
        assert_eq!(strip_patch_number("abc:def"), "abc:def");
    }

    #[test]
    fn test_strip_empty_string() {
        assert_eq!(strip_patch_number(""), "");
    }

    #[test]
    fn test_strip_exactly_seven_chars() {
        assert_eq!(strip_patch_number("000:001"), "");
    }

    #[test]
    fn test_strip_partial_pattern_unchanged() {
        assert_eq!(strip_patch_number("00:001"), "00:001"); // only 2 leading digits
    }

    // -----------------------------------------------------------------------
    // SoundFontPreset::new
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_reads_name() {
        let mut chunk = make_chunk(make_phdr_simple("Piano", 0, 0, 0));
        let p = SoundFontPreset::new(&mut chunk);
        assert_eq!(p.preset.name, "Piano");
    }

    #[test]
    fn test_new_strips_patch_number_from_name() {
        let mut chunk = make_chunk(make_phdr_simple("000:001Grand Piano", 0, 0, 0));
        let p = SoundFontPreset::new(&mut chunk);
        assert_eq!(p.preset.name, "Grand Piano");
    }

    #[test]
    fn test_new_reads_program() {
        let mut chunk = make_chunk(make_phdr_simple("P", 42, 0, 0));
        let p = SoundFontPreset::new(&mut chunk);
        assert_eq!(p.preset.program, 42);
    }

    #[test]
    fn test_new_reads_bank_msb_bits_0_6() {
        // wBank = 0x003F → bankMSB = 63, drum=false, bankLSB=0
        let mut chunk = make_chunk(make_phdr_simple("P", 0, 0x003F, 0));
        let p = SoundFontPreset::new(&mut chunk);
        assert_eq!(p.preset.bank_msb, 63);
        assert!(!p.preset.is_gm_gs_drum);
        assert_eq!(p.preset.bank_lsb, 0);
    }

    #[test]
    fn test_new_drum_flag_bit7() {
        // wBank = 0x0080 → bankMSB = 0, drum=true, bankLSB=0
        let mut chunk = make_chunk(make_phdr_simple("Drums", 0, 0x0080, 0));
        let p = SoundFontPreset::new(&mut chunk);
        assert!(p.preset.is_gm_gs_drum);
        assert_eq!(p.preset.bank_msb, 0);
    }

    #[test]
    fn test_new_bank_lsb_from_high_byte() {
        // wBank = 0x0300 → bankLSB = 3, bankMSB = 0, drum=false
        let mut chunk = make_chunk(make_phdr_simple("P", 0, 0x0300, 0));
        let p = SoundFontPreset::new(&mut chunk);
        assert_eq!(p.preset.bank_lsb, 3);
        assert_eq!(p.preset.bank_msb, 0);
    }

    #[test]
    fn test_new_reads_zone_start_index() {
        let mut chunk = make_chunk(make_phdr_simple("P", 0, 0, 17));
        let p = SoundFontPreset::new(&mut chunk);
        assert_eq!(p.zone_start_index, 17);
    }

    #[test]
    fn test_new_zones_count_initial_zero() {
        let mut chunk = make_chunk(make_phdr_simple("P", 0, 0, 0));
        let p = SoundFontPreset::new(&mut chunk);
        assert_eq!(p.zones_count, 0);
    }

    #[test]
    fn test_new_reads_library() {
        let mut chunk = make_chunk(make_phdr_bytes("P", 0, 0, 0, 0xDEAD_BEEF, 0, 0));
        let p = SoundFontPreset::new(&mut chunk);
        assert_eq!(p.preset.library, 0xDEAD_BEEF);
    }

    #[test]
    fn test_new_reads_genre() {
        let mut chunk = make_chunk(make_phdr_bytes("P", 0, 0, 0, 0, 0x1234_5678, 0));
        let p = SoundFontPreset::new(&mut chunk);
        assert_eq!(p.preset.genre, 0x1234_5678);
    }

    #[test]
    fn test_new_reads_morphology() {
        let mut chunk = make_chunk(make_phdr_bytes("P", 0, 0, 0, 0, 0, 0xABCD_EF01));
        let p = SoundFontPreset::new(&mut chunk);
        assert_eq!(p.preset.morphology, 0xABCD_EF01);
    }

    #[test]
    fn test_new_advances_cursor_by_38() {
        let mut chunk = make_chunk(make_phdr_simple("P", 0, 0, 0));
        SoundFontPreset::new(&mut chunk);
        assert_eq!(chunk.data.current_index, 38);
    }

    #[test]
    fn test_new_sequential_reads_advance_cursor() {
        let mut data = make_phdr_simple("Piano", 0, 0, 0);
        data.extend(make_phdr_simple("Guitar", 25, 0, 5));
        let mut chunk = make_chunk(data);
        SoundFontPreset::new(&mut chunk);
        assert_eq!(chunk.data.current_index, 38);
        SoundFontPreset::new(&mut chunk);
        assert_eq!(chunk.data.current_index, 76);
    }

    // -----------------------------------------------------------------------
    // SoundFontPresetZoneSink impl
    // -----------------------------------------------------------------------

    #[test]
    fn test_zones_count_reflects_field() {
        let mut chunk = make_chunk(make_phdr_simple("P", 0, 0, 0));
        let mut p = SoundFontPreset::new(&mut chunk);
        p.zones_count = 5;
        assert_eq!(p.zones_count(), 5);
    }

    #[test]
    fn test_push_zone_adds_to_preset_zones() {
        let mut chunk = make_chunk(make_phdr_simple("P", 0, 0, 0));
        let mut p = SoundFontPreset::new(&mut chunk);
        p.push_zone(BasicPresetZone::new(0, 0));
        assert_eq!(p.preset.zones.len(), 1);
    }

    #[test]
    fn test_push_zone_multiple() {
        let mut chunk = make_chunk(make_phdr_simple("P", 0, 0, 0));
        let mut p = SoundFontPreset::new(&mut chunk);
        p.push_zone(BasicPresetZone::new(0, 0));
        p.push_zone(BasicPresetZone::new(0, 1));
        assert_eq!(p.preset.zones.len(), 2);
    }

    #[test]
    fn test_global_zone_mut_initially_empty() {
        let mut chunk = make_chunk(make_phdr_simple("P", 0, 0, 0));
        let mut p = SoundFontPreset::new(&mut chunk);
        let gz = p.global_zone_mut();
        assert!(gz.generators.is_empty());
        assert!(gz.modulators.is_empty());
    }

    #[test]
    fn test_global_zone_mut_can_add_generators() {
        let mut chunk = make_chunk(make_phdr_simple("P", 0, 0, 0));
        let mut p = SoundFontPreset::new(&mut chunk);
        p.global_zone_mut()
            .add_generators(&[Generator::new_unvalidated(gt::PAN, 64.0)]);
        assert_eq!(p.preset.global_zone.generators.len(), 1);
    }

    // -----------------------------------------------------------------------
    // read_presets
    // -----------------------------------------------------------------------

    #[test]
    fn test_read_presets_empty_chunk_returns_empty() {
        let mut chunk = make_chunk(vec![]);
        assert!(read_presets(&mut chunk).is_empty());
    }

    #[test]
    fn test_read_presets_single_record_is_eop_returns_empty() {
        let mut chunk = make_chunk(make_phdr_simple("EOP", 0, 0, 0));
        assert!(read_presets(&mut chunk).is_empty());
    }

    #[test]
    fn test_read_presets_one_preset_plus_eop() {
        let mut data = make_phdr_simple("Piano", 0, 0, 0);
        data.extend(make_phdr_simple("EOP", 0, 0, 3));
        let mut chunk = make_chunk(data);
        let presets = read_presets(&mut chunk);
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].preset.name, "Piano");
    }

    #[test]
    fn test_read_presets_eop_not_in_result() {
        let mut data = make_phdr_simple("Piano", 0, 0, 0);
        data.extend(make_phdr_simple("EOP", 0, 0, 0));
        let mut chunk = make_chunk(data);
        let presets = read_presets(&mut chunk);
        assert_eq!(presets.len(), 1);
        assert_ne!(presets[0].preset.name, "EOP");
    }

    #[test]
    fn test_read_presets_zones_count_computed() {
        // Piano zone_start=0, Guitar zone_start=4, EOP zone_start=9
        // Piano.zones_count=4, Guitar.zones_count=5
        let mut data = make_phdr_simple("Piano", 0, 0, 0);
        data.extend(make_phdr_simple("Guitar", 25, 0, 4));
        data.extend(make_phdr_simple("EOP", 0, 0, 9));
        let mut chunk = make_chunk(data);
        let presets = read_presets(&mut chunk);
        assert_eq!(presets.len(), 2);
        assert_eq!(presets[0].zones_count, 4);
        assert_eq!(presets[1].zones_count, 5);
    }

    #[test]
    fn test_read_presets_multiple_presets() {
        let mut data = make_phdr_simple("Piano", 0, 0, 0);
        data.extend(make_phdr_simple("Guitar", 25, 0, 3));
        data.extend(make_phdr_simple("Violin", 40, 0, 6));
        data.extend(make_phdr_simple("EOP", 0, 0, 10));
        let mut chunk = make_chunk(data);
        let presets = read_presets(&mut chunk);
        assert_eq!(presets.len(), 3);
        assert_eq!(presets[0].preset.name, "Piano");
        assert_eq!(presets[1].preset.name, "Guitar");
        assert_eq!(presets[2].preset.name, "Violin");
    }

    #[test]
    fn test_read_presets_multiple_zones_counts() {
        let mut data = make_phdr_simple("A", 0, 0, 0);
        data.extend(make_phdr_simple("B", 1, 0, 2));
        data.extend(make_phdr_simple("C", 2, 0, 5));
        data.extend(make_phdr_simple("EOP", 0, 0, 7));
        let mut chunk = make_chunk(data);
        let presets = read_presets(&mut chunk);
        assert_eq!(presets[0].zones_count, 2); // 2-0
        assert_eq!(presets[1].zones_count, 3); // 5-2
        assert_eq!(presets[2].zones_count, 2); // 7-5
    }

    #[test]
    fn test_read_presets_drum_preset_bank() {
        // wBank = 0x0080 → drum preset
        let mut data = make_phdr_simple("Drums", 0, 0x0080, 0);
        data.extend(make_phdr_simple("EOP", 0, 0, 1));
        let mut chunk = make_chunk(data);
        let presets = read_presets(&mut chunk);
        assert!(presets[0].preset.is_gm_gs_drum);
    }

    #[test]
    fn test_read_presets_program_and_bank_preserved() {
        // program=25, wBank=0x0007 (bankMSB=7)
        let mut data = make_phdr_simple("Guitar", 25, 0x0007, 0);
        data.extend(make_phdr_simple("EOP", 0, 0, 1));
        let mut chunk = make_chunk(data);
        let presets = read_presets(&mut chunk);
        assert_eq!(presets[0].preset.program, 25);
        assert_eq!(presets[0].preset.bank_msb, 7);
    }
}
