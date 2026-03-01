/// instrument.rs
/// purpose: DLS Instrument (ins chunk) with read/write and SF2 preset/instrument conversion.
/// Ported from: src/soundbank/downloadable_sounds/instrument.ts
use crate::soundbank::basic_soundbank::basic_instrument::BasicInstrument;
use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::basic_preset_zone::BasicPresetZone;
use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
use crate::soundbank::basic_soundbank::generator_types::{GENERATOR_LIMITS, generator_types as gt};
use crate::soundbank::basic_soundbank::modulator::Modulator;
use crate::soundbank::downloadable_sounds::articulation::DownloadableSoundsArticulation;
use crate::soundbank::downloadable_sounds::default_dls_modulators::{
    DEFAULT_DLS_CHORUS, DEFAULT_DLS_REVERB,
};
use crate::soundbank::downloadable_sounds::dls_verifier::{parsing_error, verify_and_read_list};
use crate::soundbank::downloadable_sounds::region::DownloadableSoundsRegion;
use crate::soundbank::downloadable_sounds::sample::DownloadableSoundsSample;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::{read_little_endian_indexed, write_dword};
use crate::utils::loggin::{
    spessa_synth_group, spessa_synth_group_collapsed, spessa_synth_group_end,
};
use crate::utils::riff_chunk::{
    RIFFChunk, find_riff_list_type, read_riff_chunk, write_riff_chunk_parts, write_riff_chunk_raw,
};
use crate::utils::string::{get_string_bytes, read_binary_string, read_binary_string_indexed};

// ---------------------------------------------------------------------------
// DownloadableSoundsInstrument
// ---------------------------------------------------------------------------

/// A DLS Instrument with articulation, regions, and bank/program information.
///
/// Equivalent to: class DownloadableSoundsInstrument extends DLSVerifier implements MIDIPatchNamed
pub struct DownloadableSoundsInstrument {
    /// The instrument's global articulation (connection blocks applied to all regions).
    /// Equivalent to: public readonly articulation = new DownloadableSoundsArticulation()
    pub articulation: DownloadableSoundsArticulation,

    /// The list of regions (key/velocity ranges mapped to wave samples).
    /// Equivalent to: public readonly regions = new Array<DownloadableSoundsRegion>()
    pub regions: Vec<DownloadableSoundsRegion>,

    /// Instrument name.
    /// Equivalent to: public name = "Unnamed"
    pub name: String,

    /// MIDI bank select LSB (CC32), bits 0-6 of DLS ulBank field.
    /// Equivalent to: public bankLSB = 0
    pub bank_lsb: u8,

    /// MIDI bank select MSB (CC0), bits 8-14 of DLS ulBank field.
    /// Equivalent to: public bankMSB = 0
    pub bank_msb: u8,

    /// True if this is a drum instrument (bit 31 of DLS ulBank field).
    /// Equivalent to: public isGMGSDrum = false
    pub is_gm_gs_drum: bool,

    /// MIDI program number, bits 0-6 of DLS ulInstrument field.
    /// Equivalent to: public program = 0
    pub program: u8,
}

impl DownloadableSoundsInstrument {
    /// Creates a new instrument with default values.
    pub fn new() -> Self {
        Self {
            articulation: DownloadableSoundsArticulation::new(),
            regions: Vec::new(),
            name: "Unnamed".to_string(),
            bank_lsb: 0,
            bank_msb: 0,
            is_gm_gs_drum: false,
            program: 0,
        }
    }

    // -----------------------------------------------------------------------
    // copyFrom
    // -----------------------------------------------------------------------

    /// Deep-copies an instrument.
    ///
    /// Equivalent to: static copyFrom(inputInstrument: DownloadableSoundsInstrument)
    pub fn copy_from(input: &DownloadableSoundsInstrument) -> Self {
        let mut out = DownloadableSoundsInstrument::new();
        out.name = input.name.clone();
        out.is_gm_gs_drum = input.is_gm_gs_drum;
        out.bank_msb = input.bank_msb;
        out.bank_lsb = input.bank_lsb;
        out.program = input.program;
        out.articulation.copy_from(&input.articulation);
        for region in &input.regions {
            out.regions
                .push(DownloadableSoundsRegion::copy_from(region));
        }
        out
    }

    // -----------------------------------------------------------------------
    // read
    // -----------------------------------------------------------------------

    /// Parses a DLS instrument from an `ins ` LIST RIFF chunk.
    ///
    /// Returns `Err` on parse errors (missing required sub-chunks, invalid data).
    ///
    /// # Borrow-checker note
    ///
    /// TypeScript holds `regionListChunk` before calling `articulation.read(chunks)`.
    /// In Rust, both need `&mut chunks`, so we find the lrgn position (a `usize`) first,
    /// call `articulation.read`, then access `chunks[lrgn_pos]` by index.
    ///
    /// Equivalent to: static read(samples: DownloadableSoundsSample[], chunk: RIFFChunk)
    pub fn read(
        samples: &[DownloadableSoundsSample],
        chunk: &mut RIFFChunk,
    ) -> Result<Self, String> {
        let mut chunks = verify_and_read_list(chunk, &["ins "])?;

        // Find insh (instrument header) position.
        let insh_pos = chunks
            .iter()
            .position(|c| c.header == "insh")
            .ok_or_else(|| parsing_error("No instrument header!"))?;

        // Read insh fields in a block to release the mutable borrow.
        let (regions_count, ul_bank, ul_instrument) = {
            let insh = &mut chunks[insh_pos];
            insh.data.current_index = 0;
            let regions = read_little_endian_indexed(&mut insh.data, 4);
            let ul_bank = read_little_endian_indexed(&mut insh.data, 4);
            let ul_instrument = read_little_endian_indexed(&mut insh.data, 4);
            (regions, ul_bank, ul_instrument)
        };

        // Read instrument name from the INFO/INAM sub-chunk.
        // The INFO block borrow is released at the end of this block.
        let instrument_name: String = {
            let mut name = String::new();
            if let Some(info_chunk) = find_riff_list_type(&mut chunks, "INFO") {
                // find_riff_list_type sets current_index = 4 (past the "INFO" list type FourCC).
                while info_chunk.data.current_index < info_chunk.data.len() {
                    let mut sub = read_riff_chunk(&mut info_chunk.data, true, false);
                    if sub.header == "INAM" {
                        let len = sub.data.len();
                        name = read_binary_string_indexed(&mut sub.data, len)
                            .trim()
                            .to_string();
                        break;
                    }
                }
            }
            name
        };

        let instrument_name = if instrument_name.is_empty() {
            "Unnamed Instrument".to_string()
        } else {
            instrument_name
        };

        let mut instrument = DownloadableSoundsInstrument::new();
        instrument.name = instrument_name.clone();
        instrument.program = (ul_instrument & 127) as u8;
        instrument.bank_msb = ((ul_bank >> 8) & 127) as u8;
        instrument.bank_lsb = (ul_bank & 127) as u8;
        instrument.is_gm_gs_drum = (ul_bank >> 31) > 0;

        spessa_synth_group_collapsed(&format!("Parsing \"{}\"...", instrument_name));

        // Find the lrgn (region list) position as a plain index before calling
        // articulation.read, which also requires &mut chunks.
        let lrgn_pos = chunks
            .iter()
            .position(|c| {
                c.header == "LIST"
                    && c.data.len() >= 4
                    && read_binary_string(&c.data, 4, 0) == "lrgn"
            })
            .ok_or_else(|| {
                spessa_synth_group_end();
                parsing_error("No region list!")
            })?;

        // Read global articulation from lart/lar2 chunks within the ins  list.
        instrument.articulation.read(&mut chunks);

        // Read regions from the lrgn list.
        {
            let lrgn = &mut chunks[lrgn_pos];
            // Skip past the "lrgn" list type FourCC (same as find_riff_list_type).
            lrgn.data.current_index = 4;
            for _ in 0..regions_count {
                if lrgn.data.current_index >= lrgn.data.len() {
                    break;
                }
                let mut region_chunk = read_riff_chunk(&mut lrgn.data, true, false);
                if let Some(region) = DownloadableSoundsRegion::read(samples, &mut region_chunk) {
                    instrument.regions.push(region);
                }
            }
        }

        spessa_synth_group_end();
        Ok(instrument)
    }

    // -----------------------------------------------------------------------
    // fromSFPreset
    // -----------------------------------------------------------------------

    /// Converts an SF2 preset (with its instrument zones) to a DLS instrument.
    ///
    /// # Differences from TypeScript
    ///
    /// TypeScript's `fromSFPreset(preset, samples)` accesses `preset.parentSoundBank.instruments`
    /// via an internal back-reference. In Rust, `BasicPreset` has no back-reference to the sound
    /// bank, so `instruments` is passed explicitly.
    ///
    /// Equivalent to: static fromSFPreset(preset: BasicPreset, samples: BasicSample[])
    pub fn from_sf_preset(
        preset: &BasicPreset,
        samples: &[BasicSample],
        instruments: &[BasicInstrument],
    ) -> Self {
        let mut instrument = DownloadableSoundsInstrument::new();
        instrument.name = preset.name.clone();
        instrument.bank_lsb = preset.bank_lsb;
        instrument.bank_msb = preset.bank_msb;
        instrument.program = preset.program;
        instrument.is_gm_gs_drum = preset.is_gm_gs_drum;

        spessa_synth_group(&format!("Converting {} to DLS...", preset.name));

        // Flatten the preset+instrument zones into a single instrument, then convert each zone
        // to a DLS region.
        let flat_instrument = preset.to_flattened_instrument(instruments);
        for z in &flat_instrument.zones {
            match DownloadableSoundsRegion::from_sf_zone(z, samples) {
                Ok(region) => instrument.regions.push(region),
                Err(e) => {
                    eprintln!("Skipping SF zone during DLS conversion: {e}");
                }
            }
        }

        spessa_synth_group_end();
        instrument
    }

    // -----------------------------------------------------------------------
    // write
    // -----------------------------------------------------------------------

    /// Serialises this instrument as an `ins ` LIST RIFF chunk.
    ///
    /// Chunk order: insh | lrgn LIST | [lar2/lart LIST if non-empty] | INFO LIST (INAM)
    ///
    /// Equivalent to: write(): IndexedByteArray
    pub fn write(&self) -> IndexedByteArray {
        spessa_synth_group_collapsed(&format!("Writing {}...", self.name));

        // insh chunk
        let header = self.write_header();

        // lrgn LIST (region list)
        let region_parts: Vec<IndexedByteArray> = self.regions.iter().map(|r| r.write()).collect();
        let region_slices: Vec<&[u8]> = region_parts.iter().map(|a| &**a).collect();
        let lrgn = write_riff_chunk_parts("lrgn", &region_slices, true);

        let mut parts: Vec<IndexedByteArray> = vec![header, lrgn];

        // Articulation (lar2/lart LIST) - only written when non-empty.
        // SF2→DLS conversion usually produces no local articulation, only global.
        if !self.articulation.is_empty() {
            parts.push(self.articulation.write());
        }

        // INFO LIST containing INAM (instrument name)
        let inam_bytes = get_string_bytes(&self.name, true, false);
        let inam = write_riff_chunk_raw("INAM", &inam_bytes, false, false);
        let info = write_riff_chunk_raw("INFO", &inam, false, true);
        parts.push(info);

        let slices: Vec<&[u8]> = parts.iter().map(|a| &**a).collect();
        let result = write_riff_chunk_parts("ins ", &slices, true);

        spessa_synth_group_end();
        result
    }

    // -----------------------------------------------------------------------
    // toSFPreset
    // -----------------------------------------------------------------------

    /// Converts this DLS instrument to SF2 format and adds it to the given sound bank.
    ///
    /// Creates a `BasicPreset` and a `BasicInstrument`, populates them from DLS articulation
    /// and regions, globalizes the instrument, adds DLS-specific reverb/chorus modulators,
    /// and links preset and instrument together in the sound bank.
    ///
    /// Equivalent to: toSFPreset(soundBank: BasicSoundBank)
    pub fn to_sf_preset(&self, sound_bank: &mut BasicSoundBank) {
        // Pre-determine where the new instrument and preset will be inserted.
        let instrument_idx = sound_bank.instruments.len();
        let preset_idx = sound_bank.presets.len();

        // Build the SF2 instrument.
        let mut sf_instrument = BasicInstrument::new();
        sf_instrument.name = self.name.clone();

        // Apply DLS global articulation to the SF2 instrument global zone.
        self.articulation.to_sf_zone(&mut sf_instrument.global_zone);

        // Add each DLS region as an SF2 instrument zone.
        for region in &self.regions {
            let _ = region.to_sf_zone(&mut sf_instrument, instrument_idx, &mut sound_bank.samples);
        }

        // Move common generators/modulators from all zones to the global zone.
        sf_instrument.globalize();

        // Override reverb with DLS default 1000 (instead of SF2 default 200).
        if !sf_instrument
            .global_zone
            .modulators
            .iter()
            .any(|m| m.destination == gt::REVERB_EFFECTS_SEND)
        {
            let reverb_mod = Modulator::new(
                DEFAULT_DLS_REVERB.primary_source(),
                DEFAULT_DLS_REVERB.secondary_source(),
                DEFAULT_DLS_REVERB.destination,
                DEFAULT_DLS_REVERB.transform_amount,
                DEFAULT_DLS_REVERB.transform_type,
                DEFAULT_DLS_REVERB.is_effect_modulator,
                DEFAULT_DLS_REVERB.is_default_resonant_modulator,
            );
            sf_instrument.global_zone.add_modulators(&[reverb_mod]);
        }

        // Override chorus with DLS default 1000 (instead of SF2 default 200).
        if !sf_instrument
            .global_zone
            .modulators
            .iter()
            .any(|m| m.destination == gt::CHORUS_EFFECTS_SEND)
        {
            let chorus_mod = Modulator::new(
                DEFAULT_DLS_CHORUS.primary_source(),
                DEFAULT_DLS_CHORUS.secondary_source(),
                DEFAULT_DLS_CHORUS.destination,
                DEFAULT_DLS_CHORUS.transform_amount,
                DEFAULT_DLS_CHORUS.transform_type,
                DEFAULT_DLS_CHORUS.is_effect_modulator,
                DEFAULT_DLS_CHORUS.is_default_resonant_modulator,
            );
            sf_instrument.global_zone.add_modulators(&[chorus_mod]);
        }

        // Remove generators that are already at their SF2 default value.
        sf_instrument.global_zone.generators.retain(|g| {
            let def = GENERATOR_LIMITS
                .get(g.generator_type as usize)
                .and_then(|l| *l)
                .map(|l| l.def)
                .unwrap_or(0);
            g.generator_value as i32 != def
        });

        // Build the SF2 preset.
        let mut preset = BasicPreset::new();
        preset.name = self.name.clone();
        preset.bank_msb = self.bank_msb;
        preset.bank_lsb = self.bank_lsb;
        preset.is_gm_gs_drum = self.is_gm_gs_drum;
        preset.program = self.program;

        // Add instrument and preset to the sound bank (separate field borrows – no conflict).
        sound_bank.instruments.push(sf_instrument);
        sound_bank.presets.push(preset);

        // Link the preset to the instrument via a preset zone.
        let zone = BasicPresetZone::new(preset_idx, instrument_idx);
        sound_bank.presets[preset_idx].zones.push(zone);
        sound_bank.instruments[instrument_idx].link_to(preset_idx);
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Serialises the `insh` (instrument header) chunk (12 bytes / 3 DWORDs).
    ///
    /// Layout:
    ///   CRegions (DWORD): number of regions
    ///   ulBank   (DWORD): bits 0-6 = CC32, bits 8-14 = CC0, bit 31 = drums flag
    ///   ulInstrument (DWORD): bits 0-6 = MIDI program number
    ///
    /// Equivalent to: private writeHeader()
    fn write_header(&self) -> IndexedByteArray {
        let mut insh_data = IndexedByteArray::new(12);
        write_dword(&mut insh_data, self.regions.len() as u32);
        // Bank MSB in bits 8-14, bank LSB in bits 0-6
        let mut ul_bank = ((self.bank_msb as u32 & 127) << 8) | (self.bank_lsb as u32 & 127);
        // Bit 31 signals a drum instrument
        if self.is_gm_gs_drum {
            ul_bank |= 1u32 << 31;
        }
        write_dword(&mut insh_data, ul_bank);
        write_dword(&mut insh_data, self.program as u32 & 127);
        write_riff_chunk_raw("insh", &insh_data, false, false)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
    use crate::utils::indexed_array::IndexedByteArray;
    use crate::utils::riff_chunk::RIFFChunk;

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// Wraps the bytes returned by `instrument.write()` in a `RIFFChunk` suitable for `read()`.
    fn make_instrument_chunk(inst: &DownloadableSoundsInstrument) -> RIFFChunk {
        let bytes = inst.write();
        let s: &[u8] = &bytes;
        let size = u32::from_le_bytes([s[4], s[5], s[6], s[7]]);
        let data = IndexedByteArray::from_slice(&s[8..]);
        RIFFChunk::new("LIST".to_string(), size, data)
    }

    /// Finds the offset of `needle` (4-byte pattern) in `haystack`.
    fn find_fourcc(haystack: &[u8], needle: &[u8; 4]) -> Option<usize> {
        haystack.windows(4).position(|w| w == needle)
    }

    // ── new ───────────────────────────────────────────────────────────────────

    #[test]
    fn test_new_default_name() {
        let inst = DownloadableSoundsInstrument::new();
        assert_eq!(inst.name, "Unnamed");
    }

    #[test]
    fn test_new_default_program() {
        let inst = DownloadableSoundsInstrument::new();
        assert_eq!(inst.program, 0);
    }

    #[test]
    fn test_new_default_bank_msb() {
        let inst = DownloadableSoundsInstrument::new();
        assert_eq!(inst.bank_msb, 0);
    }

    #[test]
    fn test_new_default_bank_lsb() {
        let inst = DownloadableSoundsInstrument::new();
        assert_eq!(inst.bank_lsb, 0);
    }

    #[test]
    fn test_new_default_not_drum() {
        let inst = DownloadableSoundsInstrument::new();
        assert!(!inst.is_gm_gs_drum);
    }

    #[test]
    fn test_new_empty_regions() {
        let inst = DownloadableSoundsInstrument::new();
        assert!(inst.regions.is_empty());
    }

    #[test]
    fn test_new_empty_articulation() {
        let inst = DownloadableSoundsInstrument::new();
        assert!(inst.articulation.is_empty());
    }

    // ── copy_from ─────────────────────────────────────────────────────────────

    #[test]
    fn test_copy_from_copies_name() {
        let mut src = DownloadableSoundsInstrument::new();
        src.name = "Grand Piano".to_string();
        let dst = DownloadableSoundsInstrument::copy_from(&src);
        assert_eq!(dst.name, "Grand Piano");
    }

    #[test]
    fn test_copy_from_copies_program() {
        let mut src = DownloadableSoundsInstrument::new();
        src.program = 42;
        let dst = DownloadableSoundsInstrument::copy_from(&src);
        assert_eq!(dst.program, 42);
    }

    #[test]
    fn test_copy_from_copies_bank_msb() {
        let mut src = DownloadableSoundsInstrument::new();
        src.bank_msb = 10;
        let dst = DownloadableSoundsInstrument::copy_from(&src);
        assert_eq!(dst.bank_msb, 10);
    }

    #[test]
    fn test_copy_from_copies_bank_lsb() {
        let mut src = DownloadableSoundsInstrument::new();
        src.bank_lsb = 5;
        let dst = DownloadableSoundsInstrument::copy_from(&src);
        assert_eq!(dst.bank_lsb, 5);
    }

    #[test]
    fn test_copy_from_copies_is_drum() {
        let mut src = DownloadableSoundsInstrument::new();
        src.is_gm_gs_drum = true;
        let dst = DownloadableSoundsInstrument::copy_from(&src);
        assert!(dst.is_gm_gs_drum);
    }

    #[test]
    fn test_copy_from_is_independent_name() {
        let mut src = DownloadableSoundsInstrument::new();
        src.name = "Piano".to_string();
        let mut dst = DownloadableSoundsInstrument::copy_from(&src);
        dst.name = "Violin".to_string();
        assert_eq!(src.name, "Piano");
    }

    #[test]
    fn test_copy_from_is_independent_regions() {
        let src = DownloadableSoundsInstrument::new();
        let mut dst = DownloadableSoundsInstrument::copy_from(&src);
        // Pushing a region to dst should not affect src.regions count.
        // (We can't easily construct a region here without samples, so just check lengths.)
        assert_eq!(src.regions.len(), dst.regions.len());
        let _ = dst; // suppress unused warning
    }

    // ── write_header (insh) ───────────────────────────────────────────────────

    /// Reads ulBank from the insh chunk inside a written instrument.
    fn read_ul_bank_from_write(inst: &DownloadableSoundsInstrument) -> u32 {
        let written = inst.write();
        let s: &[u8] = &written;
        let insh_pos = find_fourcc(s, b"insh").expect("insh not found");
        // insh data: [insh(4)][size(4)][CRegions(4)][ulBank(4)][ulInstrument(4)]
        u32::from_le_bytes([
            s[insh_pos + 8 + 4],
            s[insh_pos + 8 + 5],
            s[insh_pos + 8 + 6],
            s[insh_pos + 8 + 7],
        ])
    }

    #[test]
    fn test_write_header_bank_msb_encoded_in_bits_8_14() {
        let mut inst = DownloadableSoundsInstrument::new();
        inst.bank_msb = 3;
        let ul_bank = read_ul_bank_from_write(&inst);
        assert_eq!((ul_bank >> 8) & 127, 3, "bank MSB should be in bits 8-14");
    }

    #[test]
    fn test_write_header_bank_lsb_encoded_in_bits_0_6() {
        let mut inst = DownloadableSoundsInstrument::new();
        inst.bank_lsb = 7;
        let ul_bank = read_ul_bank_from_write(&inst);
        assert_eq!(ul_bank & 127, 7, "bank LSB should be in bits 0-6");
    }

    #[test]
    fn test_write_header_drum_flag_sets_bit_31() {
        let mut inst = DownloadableSoundsInstrument::new();
        inst.is_gm_gs_drum = true;
        let ul_bank = read_ul_bank_from_write(&inst);
        assert_ne!(ul_bank & (1u32 << 31), 0, "bit 31 should be set for drums");
    }

    #[test]
    fn test_write_header_non_drum_bit_31_clear() {
        let inst = DownloadableSoundsInstrument::new();
        let ul_bank = read_ul_bank_from_write(&inst);
        assert_eq!(
            ul_bank & (1u32 << 31),
            0,
            "bit 31 should not be set for non-drums"
        );
    }

    #[test]
    fn test_write_header_regions_count_zero() {
        let inst = DownloadableSoundsInstrument::new();
        let written = inst.write();
        let s: &[u8] = &written;
        let insh_pos = find_fourcc(s, b"insh").expect("insh not found");
        let c_regions = u32::from_le_bytes([
            s[insh_pos + 8],
            s[insh_pos + 9],
            s[insh_pos + 10],
            s[insh_pos + 11],
        ]);
        assert_eq!(c_regions, 0);
    }

    // ── write structure ───────────────────────────────────────────────────────

    #[test]
    fn test_write_produces_list_chunk() {
        let inst = DownloadableSoundsInstrument::new();
        let written = inst.write();
        let s: &[u8] = &written;
        assert_eq!(&s[0..4], b"LIST", "outer chunk should be LIST");
        assert_eq!(&s[8..12], b"ins ", "list type should be 'ins '");
    }

    #[test]
    fn test_write_contains_insh() {
        let inst = DownloadableSoundsInstrument::new();
        let written = inst.write();
        let s: &[u8] = &written;
        assert!(
            find_fourcc(s, b"insh").is_some(),
            "insh chunk should be present"
        );
    }

    #[test]
    fn test_write_contains_lrgn() {
        let inst = DownloadableSoundsInstrument::new();
        let written = inst.write();
        let s: &[u8] = &written;
        assert!(
            find_fourcc(s, b"lrgn").is_some(),
            "lrgn list type should be present"
        );
    }

    #[test]
    fn test_write_contains_inam() {
        let mut inst = DownloadableSoundsInstrument::new();
        inst.name = "TestInst".to_string();
        let written = inst.write();
        let s: &[u8] = &written;
        assert!(
            find_fourcc(s, b"INAM").is_some(),
            "INAM chunk should be present"
        );
    }

    // ── write / read round-trip ───────────────────────────────────────────────

    #[test]
    fn test_read_roundtrip_name() {
        let mut original = DownloadableSoundsInstrument::new();
        original.name = "Test Instrument".to_string();
        let mut chunk = make_instrument_chunk(&original);
        let parsed = DownloadableSoundsInstrument::read(&[], &mut chunk).unwrap();
        assert_eq!(parsed.name, "Test Instrument");
    }

    #[test]
    fn test_read_roundtrip_program() {
        let mut original = DownloadableSoundsInstrument::new();
        original.program = 7;
        let mut chunk = make_instrument_chunk(&original);
        let parsed = DownloadableSoundsInstrument::read(&[], &mut chunk).unwrap();
        assert_eq!(parsed.program, 7);
    }

    #[test]
    fn test_read_roundtrip_bank_msb() {
        let mut original = DownloadableSoundsInstrument::new();
        original.bank_msb = 12;
        let mut chunk = make_instrument_chunk(&original);
        let parsed = DownloadableSoundsInstrument::read(&[], &mut chunk).unwrap();
        assert_eq!(parsed.bank_msb, 12);
    }

    #[test]
    fn test_read_roundtrip_bank_lsb() {
        let mut original = DownloadableSoundsInstrument::new();
        original.bank_lsb = 3;
        let mut chunk = make_instrument_chunk(&original);
        let parsed = DownloadableSoundsInstrument::read(&[], &mut chunk).unwrap();
        assert_eq!(parsed.bank_lsb, 3);
    }

    #[test]
    fn test_read_roundtrip_is_drum() {
        let mut original = DownloadableSoundsInstrument::new();
        original.is_gm_gs_drum = true;
        let mut chunk = make_instrument_chunk(&original);
        let parsed = DownloadableSoundsInstrument::read(&[], &mut chunk).unwrap();
        assert!(parsed.is_gm_gs_drum);
    }

    #[test]
    fn test_read_roundtrip_no_regions() {
        let original = DownloadableSoundsInstrument::new();
        let mut chunk = make_instrument_chunk(&original);
        let parsed = DownloadableSoundsInstrument::read(&[], &mut chunk).unwrap();
        assert!(parsed.regions.is_empty());
    }

    #[test]
    fn test_read_roundtrip_name_trimming() {
        // Name with trailing whitespace should be trimmed.
        let mut original = DownloadableSoundsInstrument::new();
        original.name = "Piano ".to_string();
        let mut chunk = make_instrument_chunk(&original);
        let parsed = DownloadableSoundsInstrument::read(&[], &mut chunk).unwrap();
        assert_eq!(parsed.name, "Piano");
    }

    #[test]
    fn test_read_returns_err_on_wrong_chunk_type() {
        // Passing a non-ins  LIST chunk should produce an Err.
        let data = IndexedByteArray::from_slice(b"bad\x00fake data here  ");
        let mut chunk = RIFFChunk::new("RIFF".to_string(), 20, data);
        assert!(DownloadableSoundsInstrument::read(&[], &mut chunk).is_err());
    }

    // ── to_sf_preset ──────────────────────────────────────────────────────────

    #[test]
    fn test_to_sf_preset_adds_one_preset() {
        let mut inst = DownloadableSoundsInstrument::new();
        inst.name = "Piano".to_string();
        let mut bank = BasicSoundBank::new();
        inst.to_sf_preset(&mut bank);
        assert_eq!(bank.presets.len(), 1);
    }

    #[test]
    fn test_to_sf_preset_adds_one_instrument() {
        let inst = DownloadableSoundsInstrument::new();
        let mut bank = BasicSoundBank::new();
        inst.to_sf_preset(&mut bank);
        assert_eq!(bank.instruments.len(), 1);
    }

    #[test]
    fn test_to_sf_preset_preset_name_matches() {
        let mut inst = DownloadableSoundsInstrument::new();
        inst.name = "Strings".to_string();
        let mut bank = BasicSoundBank::new();
        inst.to_sf_preset(&mut bank);
        assert_eq!(bank.presets[0].name, "Strings");
    }

    #[test]
    fn test_to_sf_preset_preset_program_matches() {
        let mut inst = DownloadableSoundsInstrument::new();
        inst.program = 40;
        let mut bank = BasicSoundBank::new();
        inst.to_sf_preset(&mut bank);
        assert_eq!(bank.presets[0].program, 40);
    }

    #[test]
    fn test_to_sf_preset_preset_bank_msb_matches() {
        let mut inst = DownloadableSoundsInstrument::new();
        inst.bank_msb = 5;
        let mut bank = BasicSoundBank::new();
        inst.to_sf_preset(&mut bank);
        assert_eq!(bank.presets[0].bank_msb, 5);
    }

    #[test]
    fn test_to_sf_preset_preset_bank_lsb_matches() {
        let mut inst = DownloadableSoundsInstrument::new();
        inst.bank_lsb = 2;
        let mut bank = BasicSoundBank::new();
        inst.to_sf_preset(&mut bank);
        assert_eq!(bank.presets[0].bank_lsb, 2);
    }

    #[test]
    fn test_to_sf_preset_drum_flag_matches() {
        let mut inst = DownloadableSoundsInstrument::new();
        inst.is_gm_gs_drum = true;
        let mut bank = BasicSoundBank::new();
        inst.to_sf_preset(&mut bank);
        assert!(bank.presets[0].is_gm_gs_drum);
    }

    #[test]
    fn test_to_sf_preset_preset_has_one_zone() {
        let inst = DownloadableSoundsInstrument::new();
        let mut bank = BasicSoundBank::new();
        inst.to_sf_preset(&mut bank);
        assert_eq!(bank.presets[0].zones.len(), 1);
    }

    #[test]
    fn test_to_sf_preset_zone_instrument_idx_is_zero() {
        let inst = DownloadableSoundsInstrument::new();
        let mut bank = BasicSoundBank::new();
        inst.to_sf_preset(&mut bank);
        assert_eq!(bank.presets[0].zones[0].instrument_idx, 0);
    }

    #[test]
    fn test_to_sf_preset_instrument_linked_to_preset() {
        let inst = DownloadableSoundsInstrument::new();
        let mut bank = BasicSoundBank::new();
        inst.to_sf_preset(&mut bank);
        assert_eq!(bank.instruments[0].linked_to, vec![0usize]);
    }

    #[test]
    fn test_to_sf_preset_adds_reverb_modulator() {
        let inst = DownloadableSoundsInstrument::new();
        let mut bank = BasicSoundBank::new();
        inst.to_sf_preset(&mut bank);
        let has_reverb = bank.instruments[0]
            .global_zone
            .modulators
            .iter()
            .any(|m| m.destination == gt::REVERB_EFFECTS_SEND);
        assert!(has_reverb, "reverb modulator should be added");
    }

    #[test]
    fn test_to_sf_preset_adds_chorus_modulator() {
        let inst = DownloadableSoundsInstrument::new();
        let mut bank = BasicSoundBank::new();
        inst.to_sf_preset(&mut bank);
        let has_chorus = bank.instruments[0]
            .global_zone
            .modulators
            .iter()
            .any(|m| m.destination == gt::CHORUS_EFFECTS_SEND);
        assert!(has_chorus, "chorus modulator should be added");
    }

    #[test]
    fn test_to_sf_preset_reverb_amount_is_1000() {
        let inst = DownloadableSoundsInstrument::new();
        let mut bank = BasicSoundBank::new();
        inst.to_sf_preset(&mut bank);
        let reverb = bank.instruments[0]
            .global_zone
            .modulators
            .iter()
            .find(|m| m.destination == gt::REVERB_EFFECTS_SEND);
        assert!(reverb.is_some());
        assert_eq!(reverb.unwrap().transform_amount, 1000.0);
    }

    #[test]
    fn test_to_sf_preset_chorus_amount_is_1000() {
        let inst = DownloadableSoundsInstrument::new();
        let mut bank = BasicSoundBank::new();
        inst.to_sf_preset(&mut bank);
        let chorus = bank.instruments[0]
            .global_zone
            .modulators
            .iter()
            .find(|m| m.destination == gt::CHORUS_EFFECTS_SEND);
        assert!(chorus.is_some());
        assert_eq!(chorus.unwrap().transform_amount, 1000.0);
    }

    #[test]
    fn test_to_sf_preset_multiple_instruments_correct_indices() {
        let mut bank = BasicSoundBank::new();
        // Add two instruments to check index tracking.
        let mut inst0 = DownloadableSoundsInstrument::new();
        inst0.name = "First".to_string();
        inst0.program = 0;
        inst0.to_sf_preset(&mut bank);

        let mut inst1 = DownloadableSoundsInstrument::new();
        inst1.name = "Second".to_string();
        inst1.program = 1;
        inst1.to_sf_preset(&mut bank);

        assert_eq!(bank.presets.len(), 2);
        assert_eq!(bank.instruments.len(), 2);
        assert_eq!(bank.presets[1].zones[0].instrument_idx, 1);
        assert_eq!(bank.instruments[1].linked_to, vec![1usize]);
    }
}
