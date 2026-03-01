/// write.rs
/// purpose: Write a BasicSoundBank as an SF2 binary file.
/// Ported from: src/soundbank/soundfont/write/write.ts
///
/// # Differences from TypeScript
/// - Async is removed: the function is synchronous.
/// - `compress` / `compressionFunction` / `progressFunction` are omitted
///   (Vorbis encoding is outside the MIDI→WAV scope of this port).
/// - TypeScript's `for (const [t, d] of Object.entries(bank.soundBankInfo))` loop
///   is replaced with explicit per-field handling (Rust structs are not iterable).
/// - `creationDate` is stored as a `String` (not a `Date`); no `.toISOString()` call.
use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
use crate::soundbank::basic_soundbank::modulator::{
    MOD_BYTE_SIZE, Modulator, SPESSASYNTH_DEFAULT_MODULATORS,
};
use crate::soundbank::soundfont::write::sdta::get_sdta;
use crate::soundbank::soundfont::write::shdr::get_shdr;
use crate::soundbank::soundfont::write::write_sf2_elements::write_sf2_elements;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::write_word;
use crate::utils::loggin::{
    spessa_synth_group, spessa_synth_group_collapsed, spessa_synth_group_end, spessa_synth_info,
};
use crate::utils::riff_chunk::{write_riff_chunk_parts, write_riff_chunk_raw};
use crate::utils::string::get_string_bytes;

// ---------------------------------------------------------------------------
// SoundFont2WriteOptions
// ---------------------------------------------------------------------------

/// Options for writing a sound bank as an SF2 file.
///
/// Compression-related options (`compress`, `compressionFunction`, `progressFunction`)
/// are omitted because Vorbis encoding is outside the MIDI→WAV scope of this port.
///
/// Equivalent to: `SoundFont2WriteOptions` (partial) in TypeScript.
#[derive(Debug, Clone)]
pub struct SoundFont2WriteOptions {
    /// When `true`, Vorbis-compressed samples are re-encoded as PCM (SF2.4) before writing.
    /// Equivalent to: `decompress`
    pub decompress: bool,
    /// When `true`, write the `DMOD` chunk if the bank has custom default modulators.
    /// Equivalent to: `writeDefaultModulators`
    pub write_default_modulators: bool,
    /// When `true`, write the `xdta` chunk if any index exceeds 0xFFFF or any name exceeds
    /// 20 characters.
    /// Equivalent to: `writeExtendedLimits`
    pub write_extended_limits: bool,
}

impl Default for SoundFont2WriteOptions {
    /// Equivalent to: `DEFAULT_SF2_WRITE_OPTIONS`
    fn default() -> Self {
        Self {
            decompress: false,
            write_default_modulators: true,
            write_extended_limits: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helper
// ---------------------------------------------------------------------------

/// Wraps a string in a RIFF INFO sub-chunk (null-terminated, even-padded).
/// Equivalent to: `const writeSF2Info = (type, data) => writeRIFFChunkRaw(type, getStringBytes(data, true, true))`
fn make_info_chunk(fourcc: &str, data: &str) -> IndexedByteArray {
    let bytes = get_string_bytes(data, true, true); // null-terminate + ensure even length
    write_riff_chunk_raw(fourcc, &bytes, false, false)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Writes the sound bank as an SF2 binary file and returns the raw bytes.
///
/// The bank's `sound_bank_info` is mutated in place (software tag and version are updated
/// to reflect the format written) before serialisation.
///
/// Equivalent to: `export async function writeSF2Internal(bank, writeOptions)`
///
/// # Parameters
/// - `bank`    – sound bank to serialise.
/// - `options` – write options; use [`SoundFont2WriteOptions::default()`] for standard defaults.
pub fn write_sf2_internal(bank: &mut BasicSoundBank, options: &SoundFont2WriteOptions) -> Vec<u8> {
    spessa_synth_group_collapsed("Saving soundbank...");

    // -----------------------------------------------------------------------
    // Mutate bank.soundBankInfo to reflect the format that will be written.
    // Equivalent to: bank.soundBankInfo.software = "SpessaSynth"; ...
    // -----------------------------------------------------------------------
    bank.sound_bank_info.software = Some("SpessaSynth".to_string()); // ( ͡° ͜ʖ ͡°)

    let any_compressed = bank.samples.iter().any(|s| s.is_compressed());
    if any_compressed {
        // Upgrade version tag to SF3
        bank.sound_bank_info.version.major = 3;
        bank.sound_bank_info.version.minor = 0;
    }
    if options.decompress {
        // Restore to SF2.4 (decompress takes precedence over SF3 detection)
        bank.sound_bank_info.version.major = 2;
        bank.sound_bank_info.version.minor = 4;
    }

    // -----------------------------------------------------------------------
    // Build INFO sub-chunks.
    // Equivalent to: const infoArrays: IndexedByteArray[] = []; ... for-of loop
    // -----------------------------------------------------------------------
    spessa_synth_group("Writing INFO...");
    let mut info_arrays: Vec<IndexedByteArray> = Vec::new();

    // ifil – SF2 version tag (major.minor)
    {
        let mut ifil_data = IndexedByteArray::new(4);
        write_word(&mut ifil_data, bank.sound_bank_info.version.major as u32);
        write_word(&mut ifil_data, bank.sound_bank_info.version.minor as u32);
        info_arrays.push(write_riff_chunk_raw("ifil", &ifil_data, false, false));
    }

    // iver – ROM version tag (optional)
    if let Some(rom_ver) = bank.sound_bank_info.rom_version.clone() {
        let mut iver_data = IndexedByteArray::new(4);
        write_word(&mut iver_data, rom_ver.major as u32);
        write_word(&mut iver_data, rom_ver.minor as u32);
        info_arrays.push(write_riff_chunk_raw("iver", &iver_data, false, false));
    }

    // Merge comment + subject.
    // Equivalent to: const commentText = (comment ?? "") + (subject ? `\n${subject}` : "")
    let comment_text = {
        let base = bank
            .sound_bank_info
            .comment
            .as_deref()
            .unwrap_or("")
            .to_string();
        match bank.sound_bank_info.subject.as_deref() {
            Some(subj) if !subj.is_empty() => format!("{}\n{}", base, subj),
            _ => base,
        }
    };

    // Per-field INFO chunk writes, following TypeScript property-declaration order.
    // Equivalent to: switch (type) cases inside for-of Object.entries loop.

    // INAM – bank name (always written)
    info_arrays.push(make_info_chunk("INAM", &bank.sound_bank_info.name.clone()));

    // isng – sound engine
    if !bank.sound_bank_info.sound_engine.is_empty() {
        info_arrays.push(make_info_chunk(
            "isng",
            &bank.sound_bank_info.sound_engine.clone(),
        ));
    }

    // ICRD – creation date
    if !bank.sound_bank_info.creation_date.is_empty() {
        info_arrays.push(make_info_chunk(
            "ICRD",
            &bank.sound_bank_info.creation_date.clone(),
        ));
    }

    // IENG – engineer
    if let Some(engineer) = bank.sound_bank_info.engineer.clone() {
        info_arrays.push(make_info_chunk("IENG", &engineer));
    }

    // IPRD – product
    if let Some(product) = bank.sound_bank_info.product.clone() {
        info_arrays.push(make_info_chunk("IPRD", &product));
    }

    // ICOP – copyright
    if let Some(copyright) = bank.sound_bank_info.copyright.clone() {
        info_arrays.push(make_info_chunk("ICOP", &copyright));
    }

    // ICMT – comment (merged with subject; only written when non-empty)
    if !comment_text.is_empty() {
        info_arrays.push(make_info_chunk("ICMT", &comment_text));
    }

    // irom – ROM info
    if let Some(rom_info) = bank.sound_bank_info.rom_info.clone() {
        info_arrays.push(make_info_chunk("irom", &rom_info));
    }

    // ISFT – software (set to "SpessaSynth" above)
    if let Some(software) = bank.sound_bank_info.software.clone() {
        info_arrays.push(make_info_chunk("ISFT", &software));
    }

    // DMOD – write custom default modulators if any modulator in the bank is not in
    // SPESSASYNTH_DEFAULT_MODULATORS (comparison includes transform amount).
    // Equivalent to: const unchangedDefaultModulators = bank.defaultModulators.some(mod => !SPESSASYNTH_DEFAULT_MODULATORS.some(m => isIdentical(m, mod, true)))
    let has_custom_mods = bank.default_modulators.iter().any(|mod_| {
        !SPESSASYNTH_DEFAULT_MODULATORS
            .iter()
            .any(|m| Modulator::is_identical(m, mod_, true))
    });

    if has_custom_mods && options.write_default_modulators {
        let mods: Vec<Modulator> = bank.default_modulators.clone();
        spessa_synth_info(&format!("Writing {} default modulators...", mods.len()));
        // dmodSize = MOD_BYTE_SIZE + mods.length * MOD_BYTE_SIZE = (mods.length + 1) * MOD_BYTE_SIZE
        // The extra MOD_BYTE_SIZE is for the terminal (all-zero) record.
        let dmod_size = MOD_BYTE_SIZE + mods.len() * MOD_BYTE_SIZE;
        let mut dmod_data = IndexedByteArray::new(dmod_size);
        for mod_ in &mods {
            mod_.write(&mut dmod_data, None);
        }
        // Terminal modulator: all-zero record.
        // The buffer was zero-initialised by IndexedByteArray::new(), so the
        // trailing MOD_BYTE_SIZE bytes are already 0.  We only need to advance
        // current_index so the cursor stays consistent.
        dmod_data.current_index += MOD_BYTE_SIZE;
        info_arrays.push(write_riff_chunk_raw("DMOD", &dmod_data, false, false));
    }

    spessa_synth_group_end();

    // -----------------------------------------------------------------------
    // Write sdta (sample data) chunk.
    // -----------------------------------------------------------------------
    spessa_synth_info("Writing SDTA...");
    let mut smpl_start_offsets: Vec<u64> = Vec::new();
    let mut smpl_end_offsets: Vec<u64> = Vec::new();
    let sdta_chunk: Vec<u8> = get_sdta(
        bank,
        &mut smpl_start_offsets,
        &mut smpl_end_offsets,
        options.decompress,
    );

    // -----------------------------------------------------------------------
    // Write pdta chunk (SHDR + instrument section + preset section).
    // -----------------------------------------------------------------------
    spessa_synth_info("Writing PDTA...");

    spessa_synth_info("Writing SHDR...");
    let shdr_chunk = get_shdr(bank, &smpl_start_offsets, &smpl_end_offsets);

    spessa_synth_group("Writing instruments...");
    let inst_data = write_sf2_elements(bank, false);
    spessa_synth_group_end();

    spessa_synth_group("Writing presets...");
    let pres_data = write_sf2_elements(bank, true);
    spessa_synth_group_end();

    // Assemble pdta LIST chunk in SF2 spec order: phdr pbag pmod pgen inst ibag imod igen shdr
    let pdta_chunk = {
        let chunks: [&[u8]; 9] = [
            &pres_data.hdr.pdta,
            &pres_data.bag.pdta,
            &pres_data.r#mod.pdta,
            &pres_data.r#gen.pdta,
            &inst_data.hdr.pdta,
            &inst_data.bag.pdta,
            &inst_data.r#mod.pdta,
            &inst_data.r#gen.pdta,
            &shdr_chunk.pdta,
        ];
        write_riff_chunk_parts("pdta", &chunks, true)
    };

    // -----------------------------------------------------------------------
    // Optionally append the xdta chunk to the INFO arrays.
    // https://github.com/spessasus/soundfont-proposals/blob/main/extended_limits.md
    // -----------------------------------------------------------------------
    let write_xdta = options.write_extended_limits
        && (inst_data.write_xdta
            || pres_data.write_xdta
            || bank.presets.iter().any(|p| p.name.len() > 20)
            || bank.instruments.iter().any(|i| i.name.len() > 20)
            || bank.samples.iter().any(|s| s.name.len() > 20));

    if write_xdta {
        spessa_synth_info(
            "Writing the xdta chunk as writeExtendedLimits is enabled \
             and at least one condition was met.",
        );
        // xdta follows the same order as pdta, but uses the xdta sub-chunks.
        let xdta_chunk = {
            let chunks: [&[u8]; 9] = [
                &pres_data.hdr.xdta,
                &pres_data.bag.xdta,
                &pres_data.r#mod.xdta,
                &pres_data.r#gen.xdta,
                &inst_data.hdr.xdta,
                &inst_data.bag.xdta,
                &inst_data.r#mod.xdta,
                &inst_data.r#gen.xdta,
                &shdr_chunk.xdta,
            ];
            write_riff_chunk_parts("xdta", &chunks, true)
        };
        // The xdta LIST chunk is embedded inside the INFO LIST chunk.
        info_arrays.push(xdta_chunk);
    }

    // -----------------------------------------------------------------------
    // Build INFO LIST chunk from accumulated sub-chunks.
    // -----------------------------------------------------------------------
    let info_chunk = {
        let refs: Vec<&[u8]> = info_arrays.iter().map(|a| a.as_ref()).collect();
        write_riff_chunk_parts("INFO", &refs, true)
    };

    // -----------------------------------------------------------------------
    // Assemble the final RIFF/sfbk file.
    //   RIFF | <size> | "sfbk" | <INFO LIST> | <sdta LIST> | <pdta LIST>
    // -----------------------------------------------------------------------
    spessa_synth_info("Writing the output file...");
    let sfbk_tag = get_string_bytes("sfbk", false, false);
    let main = {
        let final_chunks: [&[u8]; 4] = [&sfbk_tag, &info_chunk, &sdta_chunk, &pdta_chunk];
        write_riff_chunk_parts("RIFF", &final_chunks, false)
    };

    spessa_synth_info(&format!(
        "Saved successfully! Final file size: {}",
        main.len()
    ));
    spessa_synth_group_end();

    main.to_vec()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::basic_instrument::BasicInstrument;
    use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
    use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
    use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
    use crate::soundbank::basic_soundbank::modulator::{Modulator, SPESSASYNTH_DEFAULT_MODULATORS};
    use crate::soundbank::enums::sample_types;
    use crate::utils::indexed_array::IndexedByteArray;
    use crate::utils::little_endian::read_little_endian;
    use crate::utils::riff_chunk::read_riff_chunk;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn default_opts() -> SoundFont2WriteOptions {
        SoundFont2WriteOptions::default()
    }

    /// Write an empty bank and return the raw bytes.
    fn write_empty() -> Vec<u8> {
        let mut bank = BasicSoundBank::default();
        write_sf2_internal(&mut bank, &default_opts())
    }

    /// Makes a minimal PCM sample.
    fn make_pcm_sample(name: &str) -> BasicSample {
        let mut s = BasicSample::new(
            name.to_string(),
            44_100,
            60,
            0,
            sample_types::MONO_SAMPLE,
            0,
            9,
        );
        s.set_audio_data(vec![0.0f32; 10], 44_100);
        s
    }

    /// Makes a Vorbis-compressed sample.
    fn make_compressed_sample(name: &str) -> BasicSample {
        let mut s = BasicSample::new(
            name.to_string(),
            44_100,
            60,
            0,
            sample_types::MONO_SAMPLE,
            0,
            0,
        );
        s.set_compressed_data(vec![0xABu8; 4]);
        s
    }

    // -----------------------------------------------------------------------
    // SoundFont2WriteOptions::default
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_opts_decompress_false() {
        assert!(!default_opts().decompress);
    }

    #[test]
    fn test_default_opts_write_default_modulators_true() {
        assert!(default_opts().write_default_modulators);
    }

    #[test]
    fn test_default_opts_write_extended_limits_true() {
        assert!(default_opts().write_extended_limits);
    }

    // -----------------------------------------------------------------------
    // Top-level RIFF structure
    // -----------------------------------------------------------------------

    #[test]
    fn test_output_starts_with_riff() {
        let out = write_empty();
        assert_eq!(&out[0..4], b"RIFF");
    }

    #[test]
    fn test_output_contains_sfbk_tag() {
        let out = write_empty();
        // "sfbk" is at offset 8 (after "RIFF" + 4-byte size field)
        assert_eq!(&out[8..12], b"sfbk");
    }

    #[test]
    fn test_riff_size_field_matches_output_size() {
        let out = write_empty();
        let size = read_little_endian(&out, 4, 4) as usize;
        // RIFF size = total length − 8 (header + size field)
        assert_eq!(size, out.len() - 8);
    }

    // -----------------------------------------------------------------------
    // INFO LIST chunk
    // -----------------------------------------------------------------------

    /// Finds the INFO LIST chunk in the output and returns its data.
    fn find_info_chunk(data: &[u8]) -> Vec<u8> {
        // The RIFF body starts at offset 12 (past "RIFF" + size + "sfbk").
        let mut pos = 12usize;
        while pos + 8 <= data.len() {
            let tag = &data[pos..pos + 4];
            let size = read_little_endian(data, 4, pos + 4) as usize;
            if tag == b"LIST" && pos + 12 <= data.len() && &data[pos + 8..pos + 12] == b"INFO" {
                return data[pos..pos + 8 + size].to_vec();
            }
            pos += 8 + size;
            if size % 2 != 0 {
                pos += 1;
            }
        }
        vec![]
    }

    #[test]
    fn test_output_contains_info_list_chunk() {
        let out = write_empty();
        let info = find_info_chunk(&out);
        assert!(!info.is_empty(), "INFO LIST chunk not found");
        assert_eq!(&info[0..4], b"LIST");
        assert_eq!(&info[8..12], b"INFO");
    }

    #[test]
    fn test_info_chunk_contains_ifil() {
        let out = write_empty();
        let info = find_info_chunk(&out);
        // ifil is the first sub-chunk (at offset 12 in the INFO body)
        assert_eq!(&info[12..16], b"ifil");
    }

    #[test]
    fn test_info_ifil_size_is_4() {
        let out = write_empty();
        let info = find_info_chunk(&out);
        let ifil_size = read_little_endian(&info, 4, 16) as usize;
        assert_eq!(ifil_size, 4);
    }

    // -----------------------------------------------------------------------
    // Version tag logic
    // -----------------------------------------------------------------------

    #[test]
    fn test_version_sf2_4_for_uncompressed_bank() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("A"));
        let out = write_sf2_internal(&mut bank, &default_opts());
        let info = find_info_chunk(&out);
        // ifil data starts at offset 20 in info block (12 for list header + 8 for chunk header)
        let major = read_little_endian(&info, 2, 20) as u16;
        let minor = read_little_endian(&info, 2, 22) as u16;
        assert_eq!(major, 2);
        assert_eq!(minor, 4);
    }

    #[test]
    fn test_version_sf3_for_compressed_bank() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_compressed_sample("V"));
        let out = write_sf2_internal(&mut bank, &default_opts());
        let info = find_info_chunk(&out);
        let major = read_little_endian(&info, 2, 20) as u16;
        let minor = read_little_endian(&info, 2, 22) as u16;
        assert_eq!(major, 3);
        assert_eq!(minor, 0);
    }

    #[test]
    fn test_version_sf2_4_when_decompress_overrides_compressed() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_compressed_sample("V"));
        let opts = SoundFont2WriteOptions {
            decompress: true,
            ..default_opts()
        };
        let out = write_sf2_internal(&mut bank, &opts);
        let info = find_info_chunk(&out);
        let major = read_little_endian(&info, 2, 20) as u16;
        let minor = read_little_endian(&info, 2, 22) as u16;
        assert_eq!(major, 2);
        assert_eq!(minor, 4);
    }

    // -----------------------------------------------------------------------
    // INFO sub-chunk: ISFT (software)
    // -----------------------------------------------------------------------

    #[test]
    fn test_info_contains_isft_spessasynth() {
        let out = write_empty();
        // Search for "ISFT" anywhere in the output
        let found = out.windows(4).any(|w| w == b"ISFT");
        assert!(found, "ISFT chunk not found in output");
    }

    #[test]
    fn test_isft_contains_spessasynth_string() {
        let out = write_empty();
        // Find the ISFT chunk and verify its content contains "SpessaSynth"
        let pos = out.windows(4).position(|w| w == b"ISFT");
        assert!(pos.is_some(), "ISFT not found");
        let p = pos.unwrap();
        let size = read_little_endian(&out, 4, p + 4) as usize;
        let text_bytes = &out[p + 8..p + 8 + size];
        let text = std::str::from_utf8(text_bytes)
            .unwrap_or("")
            .trim_matches('\0');
        assert_eq!(text, "SpessaSynth");
    }

    // -----------------------------------------------------------------------
    // INFO sub-chunk: INAM (name)
    // -----------------------------------------------------------------------

    #[test]
    fn test_info_contains_inam() {
        let out = write_empty();
        let found = out.windows(4).any(|w| w == b"INAM");
        assert!(found, "INAM chunk not found");
    }

    // -----------------------------------------------------------------------
    // INFO sub-chunk: ICMT (comment + subject merged)
    // -----------------------------------------------------------------------

    #[test]
    fn test_icmt_not_written_when_no_comment() {
        let out = write_empty(); // default bank has no comment
        let found = out.windows(4).any(|w| w == b"ICMT");
        assert!(
            !found,
            "ICMT should not be written when there is no comment"
        );
    }

    #[test]
    fn test_icmt_written_when_comment_set() {
        let mut bank = BasicSoundBank::default();
        bank.sound_bank_info.comment = Some("Hello".to_string());
        let out = write_sf2_internal(&mut bank, &default_opts());
        let found = out.windows(4).any(|w| w == b"ICMT");
        assert!(found, "ICMT should be written when comment is set");
    }

    #[test]
    fn test_icmt_merges_subject_with_comment() {
        let mut bank = BasicSoundBank::default();
        bank.sound_bank_info.comment = Some("MyComment".to_string());
        bank.sound_bank_info.subject = Some("MySubject".to_string());
        let out = write_sf2_internal(&mut bank, &default_opts());
        let pos = out.windows(4).position(|w| w == b"ICMT").unwrap();
        let size = read_little_endian(&out, 4, pos + 4) as usize;
        let text_bytes = &out[pos + 8..pos + 8 + size];
        let text = std::str::from_utf8(text_bytes)
            .unwrap_or("")
            .trim_matches('\0');
        assert!(
            text.contains("MyComment"),
            "merged text must contain comment"
        );
        assert!(
            text.contains("MySubject"),
            "merged text must contain subject"
        );
    }

    // -----------------------------------------------------------------------
    // DMOD chunk
    // -----------------------------------------------------------------------

    #[test]
    fn test_no_dmod_when_bank_has_standard_default_modulators() {
        // Default bank uses SPESSASYNTH_DEFAULT_MODULATORS, so no DMOD should be written.
        let out = write_empty();
        let found = out.windows(4).any(|w| w == b"DMOD");
        assert!(!found, "DMOD should not be written for default modulators");
    }

    #[test]
    fn test_dmod_written_when_bank_has_custom_default_modulators() {
        let mut bank = BasicSoundBank::default();
        // Add a unique modulator not in SPESSASYNTH_DEFAULT_MODULATORS
        let custom_mod = Modulator {
            transform_amount: 99_999.0,
            ..Modulator::default()
        };
        bank.default_modulators.push(custom_mod);
        let out = write_sf2_internal(&mut bank, &default_opts());
        let found = out.windows(4).any(|w| w == b"DMOD");
        assert!(
            found,
            "DMOD should be written when custom default modulators exist"
        );
    }

    #[test]
    fn test_no_dmod_when_write_default_modulators_false() {
        let mut bank = BasicSoundBank::default();
        let custom_mod = Modulator {
            transform_amount: 99_999.0,
            ..Modulator::default()
        };
        bank.default_modulators.push(custom_mod);
        let opts = SoundFont2WriteOptions {
            write_default_modulators: false,
            ..default_opts()
        };
        let out = write_sf2_internal(&mut bank, &opts);
        let found = out.windows(4).any(|w| w == b"DMOD");
        assert!(
            !found,
            "DMOD must not be written when write_default_modulators is false"
        );
    }

    // -----------------------------------------------------------------------
    // sdta chunk presence
    // -----------------------------------------------------------------------

    #[test]
    fn test_output_contains_sdta_chunk() {
        let out = write_empty();
        // sdta LIST chunk has "LIST" then size then "sdta"
        let found = out
            .windows(4)
            .enumerate()
            .any(|(i, w)| w == b"LIST" && i + 12 <= out.len() && &out[i + 8..i + 12] == b"sdta");
        assert!(found, "sdta LIST chunk not found");
    }

    // -----------------------------------------------------------------------
    // pdta chunk presence
    // -----------------------------------------------------------------------

    #[test]
    fn test_output_contains_pdta_chunk() {
        let out = write_empty();
        let found = out
            .windows(4)
            .enumerate()
            .any(|(i, w)| w == b"LIST" && i + 12 <= out.len() && &out[i + 8..i + 12] == b"pdta");
        assert!(found, "pdta LIST chunk not found");
    }

    #[test]
    fn test_pdta_contains_phdr() {
        let out = write_empty();
        let found = out.windows(4).any(|w| w == b"phdr");
        assert!(found, "phdr chunk not found in pdta");
    }

    #[test]
    fn test_pdta_contains_inst() {
        let out = write_empty();
        let found = out.windows(4).any(|w| w == b"inst");
        assert!(found, "inst chunk not found in pdta");
    }

    #[test]
    fn test_pdta_contains_shdr() {
        let out = write_empty();
        let found = out.windows(4).any(|w| w == b"shdr");
        assert!(found, "shdr chunk not found in pdta");
    }

    // -----------------------------------------------------------------------
    // xdta chunk (extended limits)
    // -----------------------------------------------------------------------

    #[test]
    fn test_no_xdta_for_empty_bank() {
        let out = write_empty();
        let found = out
            .windows(4)
            .enumerate()
            .any(|(i, w)| w == b"LIST" && i + 12 <= out.len() && &out[i + 8..i + 12] == b"xdta");
        assert!(!found, "xdta should not be written for an empty bank");
    }

    #[test]
    fn test_xdta_written_when_sample_name_exceeds_20_chars() {
        let mut bank = BasicSoundBank::default();
        let long_name = "ABCDEFGHIJKLMNOPQRSTU"; // 21 chars
        bank.samples.push(make_pcm_sample(long_name));
        let out = write_sf2_internal(&mut bank, &default_opts());
        let found = out
            .windows(4)
            .enumerate()
            .any(|(i, w)| w == b"LIST" && i + 12 <= out.len() && &out[i + 8..i + 12] == b"xdta");
        assert!(
            found,
            "xdta should be written when a sample name > 20 chars"
        );
    }

    #[test]
    fn test_xdta_written_when_preset_name_exceeds_20_chars() {
        let mut bank = BasicSoundBank::default();
        bank.presets
            .push(BasicPreset::with_name("ABCDEFGHIJKLMNOPQRSTU")); // 21 chars
        let out = write_sf2_internal(&mut bank, &default_opts());
        let found = out
            .windows(4)
            .enumerate()
            .any(|(i, w)| w == b"LIST" && i + 12 <= out.len() && &out[i + 8..i + 12] == b"xdta");
        assert!(
            found,
            "xdta should be written when a preset name > 20 chars"
        );
    }

    #[test]
    fn test_xdta_written_when_instrument_name_exceeds_20_chars() {
        let mut bank = BasicSoundBank::default();
        bank.instruments
            .push(BasicInstrument::with_name("ABCDEFGHIJKLMNOPQRSTU")); // 21 chars
        let out = write_sf2_internal(&mut bank, &default_opts());
        let found = out
            .windows(4)
            .enumerate()
            .any(|(i, w)| w == b"LIST" && i + 12 <= out.len() && &out[i + 8..i + 12] == b"xdta");
        assert!(
            found,
            "xdta should be written when an instrument name > 20 chars"
        );
    }

    #[test]
    fn test_no_xdta_when_write_extended_limits_false() {
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("ABCDEFGHIJKLMNOPQRSTU")); // 21 chars
        let opts = SoundFont2WriteOptions {
            write_extended_limits: false,
            ..default_opts()
        };
        let out = write_sf2_internal(&mut bank, &opts);
        let found = out
            .windows(4)
            .enumerate()
            .any(|(i, w)| w == b"LIST" && i + 12 <= out.len() && &out[i + 8..i + 12] == b"xdta");
        assert!(
            !found,
            "xdta must not be written when write_extended_limits is false"
        );
    }

    // -----------------------------------------------------------------------
    // Round-trip read with read_riff_chunk
    // -----------------------------------------------------------------------

    #[test]
    fn test_main_riff_chunk_readable() {
        let out = write_empty();
        let mut arr = IndexedByteArray::from_vec(out.clone());
        let chunk = read_riff_chunk(&mut arr, false, false);
        assert_eq!(chunk.header, "RIFF");
        assert_eq!(chunk.size as usize, out.len() - 8);
    }

    // -----------------------------------------------------------------------
    // make_info_chunk helper
    // -----------------------------------------------------------------------

    #[test]
    fn test_make_info_chunk_has_correct_fourcc() {
        let chunk = make_info_chunk("INAM", "Piano");
        assert_eq!(&(*chunk)[0..4], b"INAM");
    }

    #[test]
    fn test_make_info_chunk_size_is_even() {
        // "A" → 1 char + null = 2 bytes (even) → size = 2
        let chunk = make_info_chunk("TEST", "A");
        let size = read_little_endian(&chunk, 4, 4) as usize;
        assert_eq!(size % 2, 0);
    }

    #[test]
    fn test_make_info_chunk_contains_null_terminator() {
        let chunk = make_info_chunk("TEST", "Hi");
        let size = read_little_endian(&chunk, 4, 4) as usize;
        // At least one null byte within the data
        let has_null = (*chunk)[8..8 + size].iter().any(|&b| b == 0);
        assert!(has_null, "INFO chunk data must contain null terminator");
    }

    // -----------------------------------------------------------------------
    // Bank with actual content
    // -----------------------------------------------------------------------

    #[test]
    fn test_bank_with_sample_output_larger_than_empty() {
        let empty_out = write_empty();
        let mut bank = BasicSoundBank::default();
        bank.samples.push(make_pcm_sample("Piano"));
        let out = write_sf2_internal(&mut bank, &default_opts());
        assert!(
            out.len() > empty_out.len(),
            "output with samples must be larger than empty output"
        );
    }

    #[test]
    fn test_optional_info_fields_written() {
        let mut bank = BasicSoundBank::default();
        bank.sound_bank_info.engineer = Some("Composer".to_string());
        bank.sound_bank_info.copyright = Some("(c) 2024".to_string());
        bank.sound_bank_info.product = Some("MySoundFont".to_string());
        let out = write_sf2_internal(&mut bank, &default_opts());
        assert!(out.windows(4).any(|w| w == b"IENG"), "IENG not found");
        assert!(out.windows(4).any(|w| w == b"ICOP"), "ICOP not found");
        assert!(out.windows(4).any(|w| w == b"IPRD"), "IPRD not found");
    }
}
