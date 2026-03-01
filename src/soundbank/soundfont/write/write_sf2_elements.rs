/// write_sf2_elements.rs
/// purpose: Build SF2 pgen/pmod/pbag/phdr (or igen/imod/ibag/inst) chunks from a BasicSoundBank.
/// Ported from: src/soundbank/soundfont/write/write_sf2_elements.ts
use crate::soundbank::basic_soundbank::basic_instrument::INST_BYTE_SIZE;
use crate::soundbank::basic_soundbank::basic_preset::PHDR_BYTE_SIZE;
use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
use crate::soundbank::basic_soundbank::basic_zone::BAG_BYTE_SIZE;
use crate::soundbank::basic_soundbank::generator::{GEN_BYTE_SIZE, Generator};
use crate::soundbank::basic_soundbank::modulator::{MOD_BYTE_SIZE, Modulator};
use crate::soundbank::basic_soundbank::modulator_source::ModulatorSource;
use crate::soundbank::soundfont::write::types::ExtendedSF2Chunks;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::write_word;
use crate::utils::riff_chunk::write_riff_chunk_raw;
use crate::utils::string::write_binary_string_indexed;

// ---------------------------------------------------------------------------
// Public output type
// ---------------------------------------------------------------------------

/// Return value of `write_sf2_elements`.
/// Equivalent to: return type of `writeSF2Elements` in TypeScript.
pub struct SF2ElementsOutput {
    /// Generator chunk pair (pgen/igen in pdta, terminal-only mod chunk in xdta).
    pub r#gen: ExtendedSF2Chunks,

    /// Modulator chunk pair (pmod/imod).
    pub r#mod: ExtendedSF2Chunks,

    /// Bag (zone index) chunk pair (pbag/ibag).
    pub bag: ExtendedSF2Chunks,

    /// Header chunk pair (phdr/inst).
    pub hdr: ExtendedSF2Chunks,

    /// Whether the index values exceeded 0xFFFF, requiring the extended xdta sub-chunks.
    /// Equivalent to: `writeXdta`
    pub write_xdta: bool,
}

// ---------------------------------------------------------------------------
// Internal zone-accumulation helper
// ---------------------------------------------------------------------------

/// Appends the generators and modulators from one zone to the running collections,
/// and pushes the current index values onto the respective index lists.
///
/// This mirrors the inner `writeZone` closure in the TypeScript source.
#[allow(clippy::too_many_arguments)]
fn accumulate_zone(
    zone_gens: Vec<Generator>,
    zone_mods: &[Modulator],
    generators: &mut Vec<Generator>,
    modulators: &mut Vec<Modulator>,
    generator_indexes: &mut Vec<usize>,
    modulator_indexes: &mut Vec<usize>,
    current_gen_index: &mut usize,
    current_mod_index: &mut usize,
) {
    generator_indexes.push(*current_gen_index);
    *current_gen_index += zone_gens.len();
    generators.extend(zone_gens);

    modulator_indexes.push(*current_mod_index);
    *current_mod_index += zone_mods.len();
    modulators.extend_from_slice(zone_mods);
}

// ---------------------------------------------------------------------------
// Main function
// ---------------------------------------------------------------------------

/// Builds all SF2 sub-chunks needed for either the preset or the instrument section.
///
/// # Parameters
/// - `bank` -- sound bank to read presets/instruments from.
/// - `is_preset` -- `true` = write preset section (phdr/pbag/pmod/pgen);
///   `false` = write instrument section (inst/ibag/imod/igen).
///
/// # Notes
/// Reference for the extended-limits design:
/// <https://github.com/spessasus/soundfont-proposals/blob/main/extended_limits.md>
///
/// Equivalent to: `writeSF2Elements(bank, isPreset = false)`
pub fn write_sf2_elements(bank: &BasicSoundBank, is_preset: bool) -> SF2ElementsOutput {
    // --- Select chunk FourCC codes and header byte size ---
    let gen_header = if is_preset { "pgen" } else { "igen" };
    let mod_header = if is_preset { "pmod" } else { "imod" };
    let bag_header = if is_preset { "pbag" } else { "ibag" };
    let hdr_header = if is_preset { "phdr" } else { "inst" };
    let hdr_byte_size = if is_preset {
        PHDR_BYTE_SIZE
    } else {
        INST_BYTE_SIZE
    };

    // --- Running index state ---
    let mut current_gen_index: usize = 0;
    let mut generator_indexes: Vec<usize> = Vec::new();
    let mut current_mod_index: usize = 0;
    let mut modulator_indexes: Vec<usize> = Vec::new();

    let mut generators: Vec<Generator> = Vec::new();
    let mut modulators: Vec<Modulator> = Vec::new();

    let mut zone_index: usize = 0;
    let mut zone_indexes: Vec<usize> = Vec::new();

    // --- Iterate over elements and accumulate zone data ---
    if is_preset {
        for preset in &bank.presets {
            zone_indexes.push(zone_index);

            // Global zone
            accumulate_zone(
                preset.global_zone.get_write_generators(&()),
                &preset.global_zone.modulators,
                &mut generators,
                &mut modulators,
                &mut generator_indexes,
                &mut modulator_indexes,
                &mut current_gen_index,
                &mut current_mod_index,
            );

            // Non-global zones
            for zone in &preset.zones {
                accumulate_zone(
                    zone.get_write_generators(&()),
                    &zone.zone.modulators,
                    &mut generators,
                    &mut modulators,
                    &mut generator_indexes,
                    &mut modulator_indexes,
                    &mut current_gen_index,
                    &mut current_mod_index,
                );
            }

            zone_index += preset.zones.len() + 1; // +1 for terminal record
        }
    } else {
        for inst in &bank.instruments {
            zone_indexes.push(zone_index);

            // Global zone
            accumulate_zone(
                inst.global_zone.get_write_generators(&()),
                &inst.global_zone.modulators,
                &mut generators,
                &mut modulators,
                &mut generator_indexes,
                &mut modulator_indexes,
                &mut current_gen_index,
                &mut current_mod_index,
            );

            // Non-global zones
            for zone in &inst.zones {
                accumulate_zone(
                    zone.get_write_generators(&()),
                    &zone.zone.modulators,
                    &mut generators,
                    &mut modulators,
                    &mut generator_indexes,
                    &mut modulator_indexes,
                    &mut current_gen_index,
                    &mut current_mod_index,
                );
            }

            zone_index += inst.zones.len() + 1; // +1 for terminal record
        }
    }

    // --- Terminal records ---
    // Equivalent to: generators.push(new Generator(0, 0, false))
    generators.push(Generator::new_unvalidated(0, 0.0));

    // Equivalent to: modulators.push(new DecodedModulator(0, 0, 0, 0, 0))
    // (DecodedModulator extends Modulator in TypeScript; here we construct the equivalent Modulator
    //  with all-zero fields so that write() produces 10 zero bytes.)
    modulators.push(Modulator {
        destination: 0,
        transform_amount: 0.0,
        transform_type: 0,
        is_effect_modulator: false,
        is_default_resonant_modulator: false,
        primary_source: ModulatorSource::default(),
        secondary_source: ModulatorSource::default(),
    });

    generator_indexes.push(current_gen_index);
    modulator_indexes.push(current_mod_index);
    zone_indexes.push(zone_index);

    // --- Serialize generators ---
    let gen_size = generators.len() * GEN_BYTE_SIZE;
    let mut gen_data = IndexedByteArray::new(gen_size);
    for g in &generators {
        g.write(&mut gen_data);
    }

    // --- Serialize modulators ---
    let mod_size = modulators.len() * MOD_BYTE_SIZE;
    let mut mod_data = IndexedByteArray::new(mod_size);
    for m in &modulators {
        m.write(&mut mod_data, None);
    }

    // --- Serialize bag (zone index) records ---
    let bag_size = modulator_indexes.len() * BAG_BYTE_SIZE;
    let mut bag_data = ExtendedSF2Chunks {
        pdta: IndexedByteArray::new(bag_size),
        xdta: IndexedByteArray::new(bag_size),
    };
    for (i, &mod_idx) in modulator_indexes.iter().enumerate() {
        let gen_idx = generator_indexes[i];
        // Bottom WORD: regular ibag/pbag
        write_word(&mut bag_data.pdta, (gen_idx & 0xFFFF) as u32);
        write_word(&mut bag_data.pdta, (mod_idx & 0xFFFF) as u32);
        // Top WORD: extended ibag/pbag
        write_word(&mut bag_data.xdta, (gen_idx >> 16) as u32);
        write_word(&mut bag_data.xdta, (mod_idx >> 16) as u32);
    }

    // --- Serialize header (phdr/inst) records ---
    let num_elements = if is_preset {
        bank.presets.len()
    } else {
        bank.instruments.len()
    };
    let hdr_size = (num_elements + 1) * hdr_byte_size;
    let mut hdr_data = ExtendedSF2Chunks {
        pdta: IndexedByteArray::new(hdr_size),
        xdta: IndexedByteArray::new(hdr_size),
    };

    if is_preset {
        for (i, preset) in bank.presets.iter().enumerate() {
            preset.write(&mut hdr_data, zone_indexes[i]);
        }
    } else {
        for (i, inst) in bank.instruments.iter().enumerate() {
            inst.write(&mut hdr_data, zone_indexes[i]);
        }
    }

    // Write terminal header record
    if is_preset {
        // Preset terminal: "EOP\0" × 20 bytes, skip program+bank (4 bytes), write low-word zone index,
        // skip library+genre+morphology (12 bytes).
        write_binary_string_indexed(&mut hdr_data.pdta, "EOP", 20);
        hdr_data.pdta.current_index += 4; // Program, bank
        write_word(&mut hdr_data.pdta, (zone_index & 0xFFFF) as u32);
        hdr_data.pdta.current_index += 12; // Library, genre, morphology

        write_binary_string_indexed(&mut hdr_data.xdta, "", 20);
        hdr_data.xdta.current_index += 4; // Program, bank
        write_word(&mut hdr_data.xdta, (zone_index >> 16) as u32);
        hdr_data.xdta.current_index += 12; // Library, genre, morphology
    } else {
        // Instrument terminal: "EOI\0" × 20 bytes, then write low-word zone index.
        write_binary_string_indexed(&mut hdr_data.pdta, "EOI", 20);
        write_word(&mut hdr_data.pdta, (zone_index & 0xFFFF) as u32);

        write_binary_string_indexed(&mut hdr_data.xdta, "", 20);
        write_word(&mut hdr_data.xdta, (zone_index >> 16) as u32);
    }

    // --- Determine whether extended xdta chunks are needed ---
    let max_index = current_gen_index.max(current_mod_index).max(zone_index);
    let write_xdta = max_index > 0xFFFF;

    // --- Build output RIFF chunks ---
    SF2ElementsOutput {
        write_xdta,
        r#gen: ExtendedSF2Chunks {
            pdta: write_riff_chunk_raw(gen_header, &gen_data, false, false),
            // Same as the mod header: contains only the terminal generator record to allow
            // reuse of the pdta parser.
            xdta: write_riff_chunk_raw(
                mod_header,
                &IndexedByteArray::new(GEN_BYTE_SIZE),
                false,
                false,
            ),
        },
        r#mod: ExtendedSF2Chunks {
            pdta: write_riff_chunk_raw(mod_header, &mod_data, false, false),
            // This chunk exists solely to preserve parser compatibility and contains only the
            // terminal modulator record.
            xdta: write_riff_chunk_raw(
                mod_header,
                &IndexedByteArray::new(MOD_BYTE_SIZE),
                false,
                false,
            ),
        },
        bag: ExtendedSF2Chunks {
            pdta: write_riff_chunk_raw(bag_header, &bag_data.pdta, false, false),
            xdta: write_riff_chunk_raw(bag_header, &bag_data.xdta, false, false),
        },
        hdr: ExtendedSF2Chunks {
            pdta: write_riff_chunk_raw(hdr_header, &hdr_data.pdta, false, false),
            xdta: write_riff_chunk_raw(hdr_header, &hdr_data.xdta, false, false),
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::basic_instrument::BasicInstrument;
    use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
    use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
    use crate::soundbank::basic_soundbank::basic_zone::BasicZone;
    use crate::soundbank::basic_soundbank::generator::GEN_BYTE_SIZE;
    use crate::soundbank::basic_soundbank::modulator::MOD_BYTE_SIZE;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Returns a minimal BasicSoundBank with no presets or instruments.
    fn empty_bank() -> BasicSoundBank {
        BasicSoundBank::default()
    }

    /// Reads a FourCC string from the first 4 bytes of a chunk.
    fn read_fourcc(chunk: &IndexedByteArray) -> String {
        let s: &[u8] = chunk;
        std::str::from_utf8(&s[..4]).unwrap_or("????").to_string()
    }

    /// Reads a u32 little-endian from bytes `start..start+4` of a chunk.
    fn read_u32_le(chunk: &IndexedByteArray, start: usize) -> u32 {
        let s: &[u8] = chunk;
        u32::from_le_bytes([s[start], s[start + 1], s[start + 2], s[start + 3]])
    }

    /// Reads a u16 little-endian from bytes `start..start+2` of a chunk.
    fn read_u16_le(chunk: &IndexedByteArray, start: usize) -> u16 {
        let s: &[u8] = chunk;
        u16::from_le_bytes([s[start], s[start + 1]])
    }

    // -----------------------------------------------------------------------
    // Basic sanity: empty bank
    // -----------------------------------------------------------------------

    #[test]
    fn test_empty_bank_instrument_does_not_panic() {
        let bank = empty_bank();
        let _ = write_sf2_elements(&bank, false);
    }

    #[test]
    fn test_empty_bank_preset_does_not_panic() {
        let bank = empty_bank();
        let _ = write_sf2_elements(&bank, true);
    }

    // -----------------------------------------------------------------------
    // Chunk FourCC verification
    // -----------------------------------------------------------------------

    #[test]
    fn test_instrument_gen_chunk_fourcc_is_igen() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, false);
        assert_eq!(read_fourcc(&out.r#gen.pdta), "igen");
    }

    #[test]
    fn test_preset_gen_chunk_fourcc_is_pgen() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, true);
        assert_eq!(read_fourcc(&out.r#gen.pdta), "pgen");
    }

    #[test]
    fn test_instrument_mod_chunk_fourcc_is_imod() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, false);
        assert_eq!(read_fourcc(&out.r#mod.pdta), "imod");
    }

    #[test]
    fn test_preset_mod_chunk_fourcc_is_pmod() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, true);
        assert_eq!(read_fourcc(&out.r#mod.pdta), "pmod");
    }

    #[test]
    fn test_instrument_bag_chunk_fourcc_is_ibag() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, false);
        assert_eq!(read_fourcc(&out.bag.pdta), "ibag");
    }

    #[test]
    fn test_preset_bag_chunk_fourcc_is_pbag() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, true);
        assert_eq!(read_fourcc(&out.bag.pdta), "pbag");
    }

    #[test]
    fn test_instrument_hdr_chunk_fourcc_is_inst() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, false);
        assert_eq!(read_fourcc(&out.hdr.pdta), "inst");
    }

    #[test]
    fn test_preset_hdr_chunk_fourcc_is_phdr() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, true);
        assert_eq!(read_fourcc(&out.hdr.pdta), "phdr");
    }

    // -----------------------------------------------------------------------
    // Terminal-record content
    // -----------------------------------------------------------------------

    // gen chunk: 8-byte RIFF header + 1 terminal gen record (4 bytes) = 12 bytes
    #[test]
    fn test_empty_bank_igen_pdta_size_is_one_terminal_gen() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, false);
        // RIFF chunk: 4 (FourCC) + 4 (size) + GEN_BYTE_SIZE data
        let expected = 8 + GEN_BYTE_SIZE; // 12
        assert_eq!(out.r#gen.pdta.len(), expected);
    }

    // mod chunk: 8-byte RIFF header + 1 terminal mod record (10 bytes) = 18 bytes
    #[test]
    fn test_empty_bank_imod_pdta_size_is_one_terminal_mod() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, false);
        let expected = 8 + MOD_BYTE_SIZE; // 18
        assert_eq!(out.r#mod.pdta.len(), expected);
    }

    // Terminal gen data (bytes 8-11) should be all zeros.
    #[test]
    fn test_empty_bank_igen_terminal_gen_is_all_zeros() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, false);
        let s: &[u8] = &out.r#gen.pdta;
        assert_eq!(&s[8..12], &[0u8, 0, 0, 0]);
    }

    // Terminal mod data (bytes 8-17) should be all zeros.
    #[test]
    fn test_empty_bank_imod_terminal_mod_is_all_zeros() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, false);
        let s: &[u8] = &out.r#mod.pdta;
        assert_eq!(&s[8..18], &[0u8; 10]);
    }

    // -----------------------------------------------------------------------
    // EOI / EOP terminal header records
    // -----------------------------------------------------------------------

    #[test]
    fn test_empty_bank_inst_terminal_starts_with_eoi() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, false);
        // hdr.pdta: 8-byte RIFF header, then INST_BYTE_SIZE data (no elements), then terminal.
        // With zero instruments, the first record IS the terminal.
        let s: &[u8] = &out.hdr.pdta;
        // offset 8 = start of inst data
        assert_eq!(&s[8..11], b"EOI");
    }

    #[test]
    fn test_empty_bank_preset_terminal_starts_with_eop() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, true);
        let s: &[u8] = &out.hdr.pdta;
        assert_eq!(&s[8..11], b"EOP");
    }

    // -----------------------------------------------------------------------
    // write_xdta flag
    // -----------------------------------------------------------------------

    #[test]
    fn test_empty_bank_write_xdta_is_false() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, false);
        assert!(!out.write_xdta);
    }

    // -----------------------------------------------------------------------
    // Bag record count
    // -----------------------------------------------------------------------

    // For an empty bank, only the terminal bag entry is written (1 entry × 4 bytes).
    // RIFF: 8-byte header + 1 × BAG_BYTE_SIZE data.
    #[test]
    fn test_empty_bank_ibag_pdta_size_is_one_terminal_bag() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, false);
        let expected = 8 + BAG_BYTE_SIZE; // 8 + 4 = 12
        assert_eq!(out.bag.pdta.len(), expected);
    }

    // -----------------------------------------------------------------------
    // Single instrument with no zones
    // -----------------------------------------------------------------------

    #[test]
    fn test_single_inst_no_zones_bag_count() {
        // One instrument with no zones: 1 global zone bag + 1 terminal = 2 bag entries.
        let mut bank = empty_bank();
        bank.instruments
            .push(BasicInstrument::with_name("TestInst"));

        let out = write_sf2_elements(&bank, false);
        // RIFF header (8) + 2 bag entries × 4 bytes = 16
        let expected = 8 + 2 * BAG_BYTE_SIZE;
        assert_eq!(out.bag.pdta.len(), expected);
    }

    #[test]
    fn test_single_inst_hdr_size() {
        // (1 instrument + 1 terminal) × INST_BYTE_SIZE
        let mut bank = empty_bank();
        bank.instruments
            .push(BasicInstrument::with_name("TestInst"));

        let out = write_sf2_elements(&bank, false);
        let expected = 8 + 2 * INST_BYTE_SIZE;
        assert_eq!(out.hdr.pdta.len(), expected);
    }

    #[test]
    fn test_single_inst_name_in_hdr() {
        let mut bank = empty_bank();
        bank.instruments.push(BasicInstrument::with_name("Piano"));

        let out = write_sf2_elements(&bank, false);
        let s: &[u8] = &out.hdr.pdta;
        // First inst record starts at offset 8 (after RIFF header).
        assert_eq!(&s[8..13], b"Piano");
    }

    // -----------------------------------------------------------------------
    // Single preset with no zones
    // -----------------------------------------------------------------------

    #[test]
    fn test_single_preset_no_zones_bag_count() {
        let mut bank = empty_bank();
        bank.presets.push(BasicPreset::with_name("TestPreset"));

        let out = write_sf2_elements(&bank, true);
        // 1 global zone bag + 1 terminal = 2 bag entries
        let expected = 8 + 2 * BAG_BYTE_SIZE;
        assert_eq!(out.bag.pdta.len(), expected);
    }

    #[test]
    fn test_single_preset_hdr_size() {
        let mut bank = empty_bank();
        bank.presets.push(BasicPreset::with_name("TestPreset"));

        let out = write_sf2_elements(&bank, true);
        let expected = 8 + 2 * PHDR_BYTE_SIZE;
        assert_eq!(out.hdr.pdta.len(), expected);
    }

    // -----------------------------------------------------------------------
    // Gen / mod chunk xdta terminal-only contents
    // -----------------------------------------------------------------------

    // The gen xdta chunk uses the mod_header FourCC and contains only GEN_BYTE_SIZE bytes.
    #[test]
    fn test_instrument_gen_xdta_uses_imod_fourcc() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, false);
        assert_eq!(read_fourcc(&out.r#gen.xdta), "imod");
    }

    #[test]
    fn test_preset_gen_xdta_uses_pmod_fourcc() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, true);
        assert_eq!(read_fourcc(&out.r#gen.xdta), "pmod");
    }

    #[test]
    fn test_instrument_mod_xdta_uses_imod_fourcc() {
        let bank = empty_bank();
        let out = write_sf2_elements(&bank, false);
        assert_eq!(read_fourcc(&out.r#mod.xdta), "imod");
    }

    // -----------------------------------------------------------------------
    // Bag record encoding (gen/mod index split)
    // -----------------------------------------------------------------------

    // With no generators or modulators, the first bag entry should have
    // gen_index = 0 (low word) and mod_index = 0 (low word).
    #[test]
    fn test_bag_first_entry_is_all_zeros_when_no_generators() {
        let mut bank = empty_bank();
        bank.instruments.push(BasicInstrument::with_name("Test"));

        let out = write_sf2_elements(&bank, false);
        // First bag entry starts at offset 8 (after RIFF header): gen_idx_low, mod_idx_low
        assert_eq!(read_u16_le(&out.bag.pdta, 8), 0); // gen_index & 0xFFFF
        assert_eq!(read_u16_le(&out.bag.pdta, 10), 0); // mod_index & 0xFFFF
    }

    // -----------------------------------------------------------------------
    // Two instruments: zone_index advances correctly
    // -----------------------------------------------------------------------

    #[test]
    fn test_two_instruments_bag_count() {
        let mut bank = empty_bank();
        bank.instruments.push(BasicInstrument::with_name("Inst0"));
        bank.instruments.push(BasicInstrument::with_name("Inst1"));

        let out = write_sf2_elements(&bank, false);
        // Each instrument: 1 global zone bag entry → 2 instruments = 2 global bags
        // + 1 terminal bag = 3 entries total
        let expected = 8 + 3 * BAG_BYTE_SIZE;
        assert_eq!(out.bag.pdta.len(), expected);
    }

    #[test]
    fn test_two_presets_bag_count() {
        let mut bank = empty_bank();
        bank.presets.push(BasicPreset::with_name("P0"));
        bank.presets.push(BasicPreset::with_name("P1"));

        let out = write_sf2_elements(&bank, true);
        let expected = 8 + 3 * BAG_BYTE_SIZE;
        assert_eq!(out.bag.pdta.len(), expected);
    }

    // -----------------------------------------------------------------------
    // Generator with a zone that has generators
    // -----------------------------------------------------------------------

    #[test]
    fn test_instrument_with_global_zone_generators_included_in_gen_chunk() {
        use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;

        let mut bank = empty_bank();
        let mut inst = BasicInstrument::with_name("TestInst");
        inst.global_zone.set_generator(gt::PAN, Some(100.0), false);
        bank.instruments.push(inst);

        let out = write_sf2_elements(&bank, false);

        // gen chunk: RIFF header (8) + (1 PAN gen + 1 terminal) × GEN_BYTE_SIZE
        let expected = 8 + 2 * GEN_BYTE_SIZE;
        assert_eq!(out.r#gen.pdta.len(), expected);
    }

    // -----------------------------------------------------------------------
    // Modulator in a zone is included in mod chunk
    // -----------------------------------------------------------------------

    #[test]
    fn test_instrument_with_modulator_included_in_mod_chunk() {
        let mut bank = empty_bank();
        let mut inst = BasicInstrument::with_name("TestInst");
        inst.global_zone.add_modulators(&[Modulator::default()]);
        bank.instruments.push(inst);

        let out = write_sf2_elements(&bank, false);

        // mod chunk: RIFF header (8) + (1 explicit mod + 1 terminal) × MOD_BYTE_SIZE
        let expected = 8 + 2 * MOD_BYTE_SIZE;
        assert_eq!(out.r#mod.pdta.len(), expected);
    }

    // -----------------------------------------------------------------------
    // xdta gen/mod index high words are zero for small banks
    // -----------------------------------------------------------------------

    #[test]
    fn test_bag_xdta_high_words_are_zero_for_small_bank() {
        let mut bank = empty_bank();
        bank.instruments.push(BasicInstrument::with_name("Test"));

        let out = write_sf2_elements(&bank, false);
        // xdta first entry high words
        assert_eq!(read_u16_le(&out.bag.xdta, 8), 0);
        assert_eq!(read_u16_le(&out.bag.xdta, 10), 0);
    }

    // -----------------------------------------------------------------------
    // INST_BYTE_SIZE and PHDR_BYTE_SIZE sanity
    // -----------------------------------------------------------------------

    #[test]
    fn test_inst_byte_size_constant() {
        assert_eq!(INST_BYTE_SIZE, 22);
    }

    #[test]
    fn test_phdr_byte_size_constant() {
        assert_eq!(PHDR_BYTE_SIZE, 38);
    }
}
