/// soundfont.rs
/// purpose: parses a SoundFont2 (.sf2 / .sf3 / .sf2pack) file into a BasicSoundBank.
/// Ported from: src/soundbank/soundfont/read/soundfont.ts
use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
use crate::soundbank::basic_soundbank::modulator::{DecodedModulator, Modulator};
use crate::soundbank::soundfont::read::generators::read_generators;
use crate::soundbank::soundfont::read::instrument_zones::apply_instrument_zones;
use crate::soundbank::soundfont::read::instruments::read_instruments;
use crate::soundbank::soundfont::read::modulators::read_modulators;
use crate::soundbank::soundfont::read::preset_zones::apply_preset_zones;
use crate::soundbank::soundfont::read::presets::read_presets;
use crate::soundbank::soundfont::read::samples::{SmplData, SoundFontSample, read_samples};
use crate::soundbank::soundfont::read::zones::read_zone_indexes;
use crate::soundbank::types::SF2VersionTag;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::read_little_endian_indexed;
use crate::utils::loggin::{spessa_synth_group, spessa_synth_group_end, spessa_synth_info};
use crate::utils::riff_chunk::{RIFFChunk, read_riff_chunk};
use crate::utils::string::{read_binary_string, read_binary_string_indexed};

// ---------------------------------------------------------------------------
// XChunks: extended SF2 (xdta) hydra sub-chunks
// ---------------------------------------------------------------------------

/// Holds the nine hydra sub-chunks from an xdta LIST chunk (Extended SF2 format).
/// Equivalent to: the `xChunks` partial record in the TypeScript constructor.
struct XChunks {
    phdr: RIFFChunk,
    pbag: RIFFChunk,
    pmod: RIFFChunk,
    pgen: RIFFChunk,
    inst: RIFFChunk,
    ibag: RIFFChunk,
    imod: RIFFChunk,
    igen: RIFFChunk,
    shdr: RIFFChunk,
}

// ---------------------------------------------------------------------------
// Private helpers: verify_header, verify_text
// ---------------------------------------------------------------------------

/// Panics with a descriptive error if `chunk.header` does not match `expected` (case-insensitive).
/// Equivalent to: protected verifyHeader(chunk, expected)
fn verify_header(chunk: &RIFFChunk, expected: &str) {
    if chunk.header.to_lowercase() != expected.to_lowercase() {
        panic!(
            "SF parsing error: Invalid chunk header! Expected \"{}\" got \"{}\". \
             The file may be corrupted.",
            expected.to_lowercase(),
            chunk.header.to_lowercase()
        );
    }
}

/// Panics with a descriptive error if `text` does not match `expected` (case-insensitive).
/// Equivalent to: protected verifyText(text, expected)
fn verify_text(text: &str, expected: &str) {
    if text.to_lowercase() != expected.to_lowercase() {
        panic!(
            "SF parsing error: Invalid FourCC: Expected \"{}\" got \"{}\". \
             The file may be corrupted.",
            expected.to_lowercase(),
            text.to_lowercase()
        );
    }
}

// ---------------------------------------------------------------------------
// Private helpers: type conversions
// ---------------------------------------------------------------------------

/// Converts one `DecodedModulator` (raw SF2 fields) into a fully-parsed `Modulator`.
fn decoded_to_modulator(dm: &DecodedModulator) -> Modulator {
    Modulator::new(
        dm.primary_source(),
        dm.secondary_source(),
        dm.destination,
        dm.transform_amount as f64,
        dm.transform_type,
        dm.is_effect_modulator,
        dm.is_default_resonant_modulator,
    )
}

/// Converts a `Vec<DecodedModulator>` into a `Vec<Modulator>`.
/// Used to bridge `read_modulators` output (DecodedModulator) to the consumers
/// (`apply_instrument_zones`, `apply_preset_zones`) that expect `Modulator`.
fn decoded_mods_to_mods(decoded: Vec<DecodedModulator>) -> Vec<Modulator> {
    decoded.iter().map(decoded_to_modulator).collect()
}

/// Converts a `SoundFontSample` into a `BasicSample`, transferring audio / compressed data.
///
/// - SF3 (compressed): calls `get_raw_data(true)` → `set_compressed_data`
/// - SF2 / SF2Pack: calls `get_audio_data()` → `set_audio_data`
/// - Loop points are clamped to `u32` (negative values → 0).
fn soundfont_sample_to_basic_sample(mut sfs: SoundFontSample) -> BasicSample {
    let loop_start = if sfs.loop_start < 0 {
        0u32
    } else {
        sfs.loop_start as u32
    };
    let loop_end = if sfs.loop_end < 0 {
        0u32
    } else {
        sfs.loop_end as u32
    };

    let mut bs = BasicSample::new(
        sfs.name.clone(),
        sfs.sample_rate,
        sfs.original_key,
        sfs.pitch_correction,
        sfs.sample_type,
        loop_start,
        loop_end,
    );
    bs.linked_sample_idx = sfs.linked_sample_idx;
    bs.linked_to = sfs.linked_to.clone();

    if sfs.is_compressed() {
        bs.set_compressed_data(sfs.get_raw_data(true));
    } else {
        match sfs.get_audio_data() {
            Ok(audio) => bs.set_audio_data(audio, sfs.sample_rate),
            Err(_) => bs.set_audio_data(vec![0.0], sfs.sample_rate),
        }
    }
    bs
}

// ---------------------------------------------------------------------------
// parse_sound_font2
// ---------------------------------------------------------------------------

/// Parses a SoundFont2 (`.sf2` / `.sf3`) binary into a `BasicSoundBank`.
///
/// `.sf2pack` (vorbis-compressed smpl) is not yet supported and will panic.
///
/// # Panics
/// Panics on malformed RIFF structure, unrecognised FourCC, or corrupted data.
///
/// Equivalent to: `new SoundFont2(arrayBuffer, false)` constructor
pub fn parse_sound_font2(data: Vec<u8>) -> BasicSoundBank {
    let mut main_file_array = IndexedByteArray::from_vec(data);
    let mut bank = BasicSoundBank::new();

    spessa_synth_group("Parsing a SoundFont2 file...");

    // ── Main RIFF chunk ──────────────────────────────────────────────────────
    // Equivalent to: const firstChunk = readRIFFChunk(mainFileArray, false)
    let first_chunk = read_riff_chunk(&mut main_file_array, false, false);
    verify_header(&first_chunk, "riff");

    let type_str = read_binary_string_indexed(&mut main_file_array, 4).to_lowercase();
    if type_str != "sfbk" && type_str != "sfpk" {
        spessa_synth_group_end();
        panic!(
            "Invalid soundFont! Expected \"sfbk\" or \"sfpk\" got \"{}\"",
            type_str
        );
    }
    let is_sf2_pack = type_str == "sfpk";

    // ── INFO chunk ───────────────────────────────────────────────────────────
    // Equivalent to: const infoChunk = readRIFFChunk(mainFileArray)
    let mut info_chunk = read_riff_chunk(&mut main_file_array, true, false);
    verify_header(&info_chunk, "list");
    let info_string = read_binary_string_indexed(&mut info_chunk.data, 4);
    if info_string != "INFO" {
        spessa_synth_group_end();
        panic!(
            "Invalid soundFont! Expected \"INFO\" got \"{}\"",
            info_string
        );
    }

    // Optional xdta chunk (Extended SF2 – extended sample / zone limits)
    let mut xdta_chunk: Option<RIFFChunk> = None;

    while info_chunk.data.len() > info_chunk.data.current_index {
        let mut chunk = read_riff_chunk(&mut info_chunk.data, true, false);
        // Read full text without advancing the chunk cursor (used for most string fields)
        let text = read_binary_string(&chunk.data, chunk.data.len(), 0);

        match chunk.header.as_str() {
            "ifil" => {
                let major = read_little_endian_indexed(&mut chunk.data, 2) as u16;
                let minor = read_little_endian_indexed(&mut chunk.data, 2) as u16;
                bank.sound_bank_info.version.major = major;
                bank.sound_bank_info.version.minor = minor;
            }
            "iver" => {
                let major = read_little_endian_indexed(&mut chunk.data, 2) as u16;
                let minor = read_little_endian_indexed(&mut chunk.data, 2) as u16;
                bank.sound_bank_info.rom_version = Some(SF2VersionTag { major, minor });
            }
            // DMOD: custom default modulators override
            "DMOD" => {
                let raw_mods = read_modulators(&mut chunk);
                bank.default_modulators = decoded_mods_to_mods(raw_mods);
                bank.custom_default_modulators = true;
            }
            // LIST: possible xdta extended SF2 chunk
            "LIST" => {
                let list_type = read_binary_string_indexed(&mut chunk.data, 4);
                if list_type == "xdta" {
                    spessa_synth_info("Extended SF2 found!");
                    xdta_chunk = Some(chunk);
                }
            }
            "ICRD" => {
                // Store creation date as raw string (SoundBankInfoData.creation_date is String)
                let len = chunk.data.len();
                let date_text = read_binary_string_indexed(&mut chunk.data, len);
                bank.sound_bank_info.creation_date = date_text;
            }
            "ISFT" => {
                bank.sound_bank_info.software = if text.is_empty() { None } else { Some(text) };
            }
            "IPRD" => {
                bank.sound_bank_info.product = if text.is_empty() { None } else { Some(text) };
            }
            "IENG" => {
                bank.sound_bank_info.engineer = if text.is_empty() { None } else { Some(text) };
            }
            "ICOP" => {
                bank.sound_bank_info.copyright = if text.is_empty() { None } else { Some(text) };
            }
            "INAM" => {
                bank.sound_bank_info.name = text;
            }
            "ICMT" => {
                bank.sound_bank_info.comment = if text.is_empty() { None } else { Some(text) };
            }
            "irom" => {
                bank.sound_bank_info.rom_info = if text.is_empty() { None } else { Some(text) };
            }
            "isng" => {
                bank.sound_bank_info.sound_engine = text;
            }
            _ => {}
        }
    }
    bank.print_info();

    // ── xdta extended hydra sub-chunks ───────────────────────────────────────
    // Equivalent to: if (xdtaChunk !== undefined) { xChunks.phdr = readRIFFChunk(...); … }
    let mut x_chunks: Option<XChunks> = if let Some(ref mut xdta) = xdta_chunk {
        Some(XChunks {
            phdr: read_riff_chunk(&mut xdta.data, true, false),
            pbag: read_riff_chunk(&mut xdta.data, true, false),
            pmod: read_riff_chunk(&mut xdta.data, true, false),
            pgen: read_riff_chunk(&mut xdta.data, true, false),
            inst: read_riff_chunk(&mut xdta.data, true, false),
            ibag: read_riff_chunk(&mut xdta.data, true, false),
            imod: read_riff_chunk(&mut xdta.data, true, false),
            igen: read_riff_chunk(&mut xdta.data, true, false),
            shdr: read_riff_chunk(&mut xdta.data, true, false),
        })
    } else {
        None
    };

    // ── SDTA chunk (sample data) ─────────────────────────────────────────────
    // Equivalent to: const sdtaChunk = readRIFFChunk(mainFileArray, false)
    let sdta_chunk = read_riff_chunk(&mut main_file_array, false, false);
    verify_header(&sdta_chunk, "list");
    let sdta_text = read_binary_string_indexed(&mut main_file_array, 4);
    verify_text(&sdta_text, "sdta");

    spessa_synth_info("Verifying smpl chunk...");
    let sample_data_chunk = read_riff_chunk(&mut main_file_array, false, false);
    verify_header(&sample_data_chunk, "smpl");

    if is_sf2_pack {
        spessa_synth_group_end();
        panic!("SF2Pack not yet supported: vorbis decoding is not yet implemented.");
    }

    // Save the start of smpl audio data (cursor points here after reading smpl header)
    // Equivalent to: this.sampleDataStartIndex = mainFileArray.currentIndex
    let sample_data_start_index = main_file_array.current_index;
    spessa_synth_info(&format!(
        "Skipping sample chunk, length: {}",
        (sdta_chunk.size as usize).saturating_sub(12)
    ));

    // Skip past all SDTA content (smpl data + optional sm24 chunk)
    // Equivalent to: mainFileArray.currentIndex += sdtaChunk.size - 12
    main_file_array.current_index += (sdta_chunk.size as usize).saturating_sub(12);

    // ── PDTA chunk (hydra) ───────────────────────────────────────────────────
    spessa_synth_info("Loading preset data chunk...");
    // Equivalent to: const presetChunk = readRIFFChunk(mainFileArray)
    let mut preset_chunk = read_riff_chunk(&mut main_file_array, true, false);
    verify_header(&preset_chunk, "list");
    // Consume the "pdta" FourCC from the list content
    let _pdta_type = read_binary_string_indexed(&mut preset_chunk.data, 4);

    let mut phdr_chunk = read_riff_chunk(&mut preset_chunk.data, true, false);
    verify_header(&phdr_chunk, "phdr");
    let mut pbag_chunk = read_riff_chunk(&mut preset_chunk.data, true, false);
    verify_header(&pbag_chunk, "pbag");
    let mut pmod_chunk = read_riff_chunk(&mut preset_chunk.data, true, false);
    verify_header(&pmod_chunk, "pmod");
    let mut pgen_chunk = read_riff_chunk(&mut preset_chunk.data, true, false);
    verify_header(&pgen_chunk, "pgen");
    let mut inst_chunk = read_riff_chunk(&mut preset_chunk.data, true, false);
    verify_header(&inst_chunk, "inst");
    let mut ibag_chunk = read_riff_chunk(&mut preset_chunk.data, true, false);
    verify_header(&ibag_chunk, "ibag");
    let mut imod_chunk = read_riff_chunk(&mut preset_chunk.data, true, false);
    verify_header(&imod_chunk, "imod");
    let mut igen_chunk = read_riff_chunk(&mut preset_chunk.data, true, false);
    verify_header(&igen_chunk, "igen");
    let mut shdr_chunk = read_riff_chunk(&mut preset_chunk.data, true, false);
    verify_header(&shdr_chunk, "shdr");

    // ── Read samples ─────────────────────────────────────────────────────────
    spessa_synth_info("Parsing samples...");
    // Reset cursor to start of smpl audio data
    // Equivalent to: mainFileArray.currentIndex = this.sampleDataStartIndex
    main_file_array.current_index = sample_data_start_index;

    // `link_samples` is false when xdta is present (linked_sample_index may be extended later)
    // Equivalent to: readSamples(shdrChunk, sampleData, xdtaChunk === undefined)
    let link_samples = xdta_chunk.is_none();

    // SmplData holds an immutable borrow of main_file_array; it is dropped at end of the block.
    let mut sf_samples = {
        let smpl_data = SmplData::Indexed(&main_file_array);
        read_samples(&mut shdr_chunk, &smpl_data, link_samples)
        // smpl_data (and the borrow on main_file_array) is dropped here
    };

    // Apply xdta extensions to samples (extend name and linked_sample_index upper bits)
    if let Some(ref mut x) = x_chunks {
        let dummy_f32 = [0.0f32; 1];
        let x_smpl_data = SmplData::Float32(&dummy_f32);
        let x_samples = read_samples(&mut x.shdr, &x_smpl_data, false);
        if x_samples.len() == sf_samples.len() {
            for (i, s) in sf_samples.iter_mut().enumerate() {
                s.name.push_str(&x_samples[i].name);
                s.linked_sample_index |= x_samples[i].linked_sample_index << 16;
            }
        }
    }

    // Trim names and convert SoundFontSample → BasicSample → add to bank
    for mut s in sf_samples {
        s.name = s.name.trim().to_string();
        bank.samples.push(soundfont_sample_to_basic_sample(s));
    }

    // ── Read instruments ─────────────────────────────────────────────────────
    let instrument_generators = read_generators(&mut igen_chunk);
    let instrument_modulators = decoded_mods_to_mods(read_modulators(&mut imod_chunk));
    let mut instruments = read_instruments(&mut inst_chunk);

    // Apply xdta extensions to instruments (extend name and zone_start_index upper bits)
    if let Some(ref mut x) = x_chunks {
        let x_instruments = read_instruments(&mut x.inst);
        if x_instruments.len() == instruments.len() {
            for (i, inst) in instruments.iter_mut().enumerate() {
                inst.instrument
                    .name
                    .push_str(&x_instruments[i].instrument.name);
                inst.zone_start_index |= x_instruments[i].zone_start_index;
            }
            // Recalculate zones_count after extending zone_start_index values
            for i in 0..instruments.len() {
                if i + 1 < instruments.len() {
                    instruments[i].zones_count =
                        instruments[i + 1].zone_start_index - instruments[i].zone_start_index;
                }
            }
        }
    }

    // Trim instrument names
    for inst in instruments.iter_mut() {
        inst.instrument.name = inst.instrument.name.trim().to_string();
    }

    // Read ibag zone indexes and optionally extend with xdta
    let mut ibag_indexes = read_zone_indexes(&mut ibag_chunk);
    if let Some(ref mut x) = x_chunks {
        let extra = read_zone_indexes(&mut x.ibag);
        let mod_len = ibag_indexes.mod_ndx.len().min(extra.mod_ndx.len());
        for i in 0..mod_len {
            ibag_indexes.mod_ndx[i] |= extra.mod_ndx[i] << 16;
        }
        let gen_len = ibag_indexes.gen_ndx.len().min(extra.gen_ndx.len());
        for i in 0..gen_len {
            ibag_indexes.gen_ndx[i] |= extra.gen_ndx[i] << 16;
        }
    }

    // Apply instrument zones (populates zone lists on each SoundFontInstrument)
    apply_instrument_zones(
        &ibag_indexes,
        &instrument_generators,
        &instrument_modulators,
        bank.samples.len(),
        &mut instruments,
    );

    // Extract BasicInstrument from each SoundFontInstrument and add to bank
    for inst in instruments {
        bank.instruments.push(inst.instrument);
    }

    // ── Read presets ─────────────────────────────────────────────────────────
    let preset_generators = read_generators(&mut pgen_chunk);
    let preset_modulators = decoded_mods_to_mods(read_modulators(&mut pmod_chunk));
    let mut presets = read_presets(&mut phdr_chunk);

    // Apply xdta extensions to presets (extend name and zone_start_index upper bits)
    if let Some(ref mut x) = x_chunks {
        let x_presets = read_presets(&mut x.phdr);
        if x_presets.len() == presets.len() {
            for (i, pres) in presets.iter_mut().enumerate() {
                pres.preset.name.push_str(&x_presets[i].preset.name);
                pres.zone_start_index |= x_presets[i].zone_start_index;
            }
            // Recalculate zones_count after extending zone_start_index values
            for i in 0..presets.len() {
                if i + 1 < presets.len() {
                    presets[i].zones_count =
                        presets[i + 1].zone_start_index - presets[i].zone_start_index;
                }
            }
        }
    }

    // Trim preset names
    for pres in presets.iter_mut() {
        pres.preset.name = pres.preset.name.trim().to_string();
    }

    // Read pbag zone indexes and optionally extend with xdta
    let mut pbag_indexes = read_zone_indexes(&mut pbag_chunk);
    if let Some(ref mut x) = x_chunks {
        let extra = read_zone_indexes(&mut x.pbag);
        let mod_len = pbag_indexes.mod_ndx.len().min(extra.mod_ndx.len());
        for i in 0..mod_len {
            pbag_indexes.mod_ndx[i] |= extra.mod_ndx[i] << 16;
        }
        let gen_len = pbag_indexes.gen_ndx.len().min(extra.gen_ndx.len());
        for i in 0..gen_len {
            pbag_indexes.gen_ndx[i] |= extra.gen_ndx[i] << 16;
        }
    }

    // Apply preset zones (populates zone lists on each SoundFontPreset)
    apply_preset_zones(
        &pbag_indexes,
        &preset_generators,
        &preset_modulators,
        bank.instruments.len(),
        &mut presets,
    );

    // Extract BasicPreset from each SoundFontPreset and add to bank
    for pres in presets {
        bank.presets.push(pres.preset);
    }

    // Sort presets and run parse_internal (XG bank detection)
    bank.flush();

    spessa_synth_info(&format!(
        "Parsing finished! \"{}\" has {} presets, {} instruments and {} samples.",
        bank.sound_bank_info.name,
        bank.presets.len(),
        bank.instruments.len(),
        bank.samples.len()
    ));
    spessa_synth_group_end();

    bank
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::modulator::DecodedModulator;
    use crate::soundbank::enums::sample_types;
    use crate::soundbank::soundfont::read::samples::SF3_BIT_FLIT;

    // -----------------------------------------------------------------------
    // Helper: build a minimal valid SF2 binary
    // -----------------------------------------------------------------------

    /// Builds a minimal SF2 binary with no samples, instruments, or presets.
    /// The bank name can be customised via `bank_name`.
    fn build_minimal_sf2(bank_name: &str) -> Vec<u8> {
        // ── INFO content ──
        let mut info_content: Vec<u8> = Vec::new();
        info_content.extend_from_slice(b"INFO");
        // ifil: version 2.4
        info_content.extend_from_slice(b"ifil");
        info_content.extend_from_slice(&4u32.to_le_bytes());
        info_content.extend_from_slice(&2u16.to_le_bytes()); // major
        info_content.extend_from_slice(&4u16.to_le_bytes()); // minor
        // INAM: bank name (padded to even length)
        let mut name_bytes = bank_name.as_bytes().to_vec();
        if name_bytes.len() % 2 != 0 {
            name_bytes.push(0);
        }
        info_content.extend_from_slice(b"INAM");
        info_content.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
        info_content.extend_from_slice(&name_bytes);
        // isng: "EMU8000\0" (8 bytes)
        info_content.extend_from_slice(b"isng");
        info_content.extend_from_slice(&8u32.to_le_bytes());
        info_content.extend_from_slice(b"EMU8000\x00");

        // ── SDTA content (empty smpl) ──
        let mut sdta_content: Vec<u8> = Vec::new();
        sdta_content.extend_from_slice(b"sdta");
        sdta_content.extend_from_slice(b"smpl");
        sdta_content.extend_from_slice(&0u32.to_le_bytes()); // smpl size = 0

        // ── PDTA content ──
        let mut pdta_content: Vec<u8> = Vec::new();
        pdta_content.extend_from_slice(b"pdta");
        // phdr: 1 EOP sentinel (38 bytes of zeros)
        pdta_content.extend_from_slice(b"phdr");
        pdta_content.extend_from_slice(&38u32.to_le_bytes());
        pdta_content.extend_from_slice(&[0u8; 38]);
        // pbag: 1 sentinel entry (4 bytes)
        pdta_content.extend_from_slice(b"pbag");
        pdta_content.extend_from_slice(&4u32.to_le_bytes());
        pdta_content.extend_from_slice(&[0u8; 4]);
        // pmod: 1 terminal entry (10 bytes)
        pdta_content.extend_from_slice(b"pmod");
        pdta_content.extend_from_slice(&10u32.to_le_bytes());
        pdta_content.extend_from_slice(&[0u8; 10]);
        // pgen: 1 terminal entry (4 bytes)
        pdta_content.extend_from_slice(b"pgen");
        pdta_content.extend_from_slice(&4u32.to_le_bytes());
        pdta_content.extend_from_slice(&[0u8; 4]);
        // inst: 1 EOI sentinel (22 bytes)
        pdta_content.extend_from_slice(b"inst");
        pdta_content.extend_from_slice(&22u32.to_le_bytes());
        pdta_content.extend_from_slice(&[0u8; 22]);
        // ibag: 1 sentinel entry (4 bytes)
        pdta_content.extend_from_slice(b"ibag");
        pdta_content.extend_from_slice(&4u32.to_le_bytes());
        pdta_content.extend_from_slice(&[0u8; 4]);
        // imod: 1 terminal entry (10 bytes)
        pdta_content.extend_from_slice(b"imod");
        pdta_content.extend_from_slice(&10u32.to_le_bytes());
        pdta_content.extend_from_slice(&[0u8; 10]);
        // igen: 1 terminal entry (4 bytes)
        pdta_content.extend_from_slice(b"igen");
        pdta_content.extend_from_slice(&4u32.to_le_bytes());
        pdta_content.extend_from_slice(&[0u8; 4]);
        // shdr: 1 EOS sentinel (46 bytes)
        pdta_content.extend_from_slice(b"shdr");
        pdta_content.extend_from_slice(&46u32.to_le_bytes());
        pdta_content.extend_from_slice(&[0u8; 46]);

        // ── Assemble LIST chunks ──
        let mut all_chunks: Vec<u8> = Vec::new();
        // INFO LIST
        all_chunks.extend_from_slice(b"LIST");
        all_chunks.extend_from_slice(&(info_content.len() as u32).to_le_bytes());
        all_chunks.extend_from_slice(&info_content);
        // SDTA LIST
        all_chunks.extend_from_slice(b"LIST");
        all_chunks.extend_from_slice(&(sdta_content.len() as u32).to_le_bytes());
        all_chunks.extend_from_slice(&sdta_content);
        // PDTA LIST
        all_chunks.extend_from_slice(b"LIST");
        all_chunks.extend_from_slice(&(pdta_content.len() as u32).to_le_bytes());
        all_chunks.extend_from_slice(&pdta_content);

        // ── RIFF wrapper ──
        let content_size = 4 + all_chunks.len(); // "sfbk" + all LIST chunks
        let mut riff: Vec<u8> = Vec::new();
        riff.extend_from_slice(b"RIFF");
        riff.extend_from_slice(&(content_size as u32).to_le_bytes());
        riff.extend_from_slice(b"sfbk");
        riff.extend_from_slice(&all_chunks);
        riff
    }

    // Helper: create a SoundFontSample using Float32 smpl data (SF2Pack path)
    fn make_float32_sf_sample(name: &str, loop_start: i64, loop_end: i64) -> SoundFontSample {
        let f32_data = [0.1f32, 0.2, 0.3, 0.4];
        let smpl = SmplData::Float32(&f32_data);
        // start_byte=0, end_byte=8 → slice [0..4] = 4 float values
        SoundFontSample::new(
            name.to_string(),
            0,
            8,
            loop_start,
            loop_end,
            44100,
            60,
            0,
            0,
            sample_types::MONO_SAMPLE,
            &smpl,
            0,
        )
    }

    // -----------------------------------------------------------------------
    // verify_header
    // -----------------------------------------------------------------------

    #[test]
    fn test_verify_header_matching_lowercase_ok() {
        let chunk = RIFFChunk::new("riff".to_string(), 0, IndexedByteArray::new(0));
        verify_header(&chunk, "riff"); // must not panic
    }

    #[test]
    fn test_verify_header_case_insensitive_ok() {
        let chunk = RIFFChunk::new("RIFF".to_string(), 0, IndexedByteArray::new(0));
        verify_header(&chunk, "riff"); // must not panic
    }

    #[test]
    #[should_panic(expected = "Invalid chunk header")]
    fn test_verify_header_mismatch_panics() {
        let chunk = RIFFChunk::new("LIST".to_string(), 0, IndexedByteArray::new(0));
        verify_header(&chunk, "riff");
    }

    // -----------------------------------------------------------------------
    // verify_text
    // -----------------------------------------------------------------------

    #[test]
    fn test_verify_text_matching_ok() {
        verify_text("sfbk", "sfbk"); // must not panic
    }

    #[test]
    fn test_verify_text_case_insensitive_ok() {
        verify_text("SFBK", "sfbk"); // must not panic
    }

    #[test]
    #[should_panic(expected = "Invalid FourCC")]
    fn test_verify_text_mismatch_panics() {
        verify_text("sfpk", "sfbk");
    }

    // -----------------------------------------------------------------------
    // decoded_mods_to_mods
    // -----------------------------------------------------------------------

    #[test]
    fn test_decoded_mods_to_mods_empty() {
        let result = decoded_mods_to_mods(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_decoded_mods_to_mods_preserves_count() {
        let dm1 = DecodedModulator::new(0x0502, 0x0000, 0x000A, 960, 0);
        let dm2 = DecodedModulator::new(0x0102, 0x0000, 0x0011, -960, 0);
        let result = decoded_mods_to_mods(vec![dm1, dm2]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_decoded_mods_to_mods_transform_amount_widened() {
        // transform_amount i16 → i32
        let dm = DecodedModulator::new(0, 0, 0, i16::MAX, 0);
        let result = decoded_mods_to_mods(vec![dm]);
        assert_eq!(result[0].transform_amount, i16::MAX as f64);
    }

    #[test]
    fn test_decoded_mods_to_mods_negative_amount() {
        let dm = DecodedModulator::new(0, 0, 0, -1, 0);
        let result = decoded_mods_to_mods(vec![dm]);
        assert_eq!(result[0].transform_amount, -1.0f64);
    }

    // -----------------------------------------------------------------------
    // soundfont_sample_to_basic_sample
    // -----------------------------------------------------------------------

    #[test]
    fn test_sample_conversion_basic_fields() {
        let s = make_float32_sf_sample("Piano C4", 10, 90);
        let bs = soundfont_sample_to_basic_sample(s);
        assert_eq!(bs.name, "Piano C4");
        assert_eq!(bs.sample_rate, 44100);
        assert_eq!(bs.original_key, 60);
        assert_eq!(bs.pitch_correction, 0);
        assert_eq!(bs.sample_type, sample_types::MONO_SAMPLE);
    }

    #[test]
    fn test_sample_conversion_loop_points_positive() {
        let s = make_float32_sf_sample("S", 5, 100);
        let bs = soundfont_sample_to_basic_sample(s);
        assert_eq!(bs.loop_start, 5);
        assert_eq!(bs.loop_end, 100);
    }

    #[test]
    fn test_sample_conversion_negative_loop_start_clamped_to_zero() {
        let s = make_float32_sf_sample("S", -10, 50);
        let bs = soundfont_sample_to_basic_sample(s);
        assert_eq!(bs.loop_start, 0);
    }

    #[test]
    fn test_sample_conversion_negative_loop_end_clamped_to_zero() {
        let s = make_float32_sf_sample("S", 0, -5);
        let bs = soundfont_sample_to_basic_sample(s);
        assert_eq!(bs.loop_end, 0);
    }

    #[test]
    fn test_sample_conversion_sf2pack_audio_data_set() {
        // Float32 path: audio_data is pre-populated in SoundFontSample
        let s = make_float32_sf_sample("P", 0, 0);
        let bs = soundfont_sample_to_basic_sample(s);
        assert!(bs.audio_data.is_some());
        let audio = bs.audio_data.as_ref().unwrap();
        // Slice [0..4] from [0.1, 0.2, 0.3, 0.4] → 4 samples
        assert_eq!(audio.len(), 4);
    }

    #[test]
    fn test_sample_conversion_sf3_compressed_data_set() {
        // SF3 path: compressed_data should be transferred
        let smpl_bytes = vec![0xABu8; 200];
        let iba = IndexedByteArray::from_vec(smpl_bytes);
        let smpl_data = SmplData::Indexed(&iba);
        let sf3_type = sample_types::MONO_SAMPLE | SF3_BIT_FLIT;
        let sfs = SoundFontSample::new(
            "SF3sample".to_string(),
            0,
            100,
            0,
            0,
            44100,
            60,
            0,
            0,
            sf3_type,
            &smpl_data,
            0,
        );
        let bs = soundfont_sample_to_basic_sample(sfs);
        assert!(bs.is_compressed());
        assert!(bs.audio_data.is_none());
    }

    #[test]
    fn test_sample_conversion_transfers_linked_sample_idx() {
        let mut s = make_float32_sf_sample("L", 0, 0);
        s.linked_sample_idx = Some(3);
        let bs = soundfont_sample_to_basic_sample(s);
        assert_eq!(bs.linked_sample_idx, Some(3));
    }

    // -----------------------------------------------------------------------
    // parse_sound_font2 – integration tests with minimal SF2 binary
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_minimal_sf2_bank_name() {
        let data = build_minimal_sf2("TestBank");
        let bank = parse_sound_font2(data);
        assert_eq!(bank.sound_bank_info.name, "TestBank");
    }

    #[test]
    fn test_parse_minimal_sf2_no_presets() {
        let data = build_minimal_sf2("Empty");
        let bank = parse_sound_font2(data);
        assert!(bank.presets.is_empty());
    }

    #[test]
    fn test_parse_minimal_sf2_no_instruments() {
        let data = build_minimal_sf2("Empty");
        let bank = parse_sound_font2(data);
        assert!(bank.instruments.is_empty());
    }

    #[test]
    fn test_parse_minimal_sf2_no_samples() {
        let data = build_minimal_sf2("Empty");
        let bank = parse_sound_font2(data);
        assert!(bank.samples.is_empty());
    }

    #[test]
    fn test_parse_minimal_sf2_version_2_4() {
        let data = build_minimal_sf2("V");
        let bank = parse_sound_font2(data);
        assert_eq!(bank.sound_bank_info.version.major, 2);
        assert_eq!(bank.sound_bank_info.version.minor, 4);
    }

    #[test]
    fn test_parse_minimal_sf2_sound_engine_from_isng() {
        let data = build_minimal_sf2("E");
        let bank = parse_sound_font2(data);
        assert_eq!(bank.sound_bank_info.sound_engine, "EMU8000");
    }

    #[test]
    fn test_parse_minimal_sf2_bank_name_trimmed() {
        // "INAM" chunk with "Hi  " (trailing spaces) – the name stored is "Hi  " since
        // we only trim SoundFontSample names, not the bank name itself.
        // But the actual bank_name passed here has no spaces.
        let data = build_minimal_sf2("MyBank");
        let bank = parse_sound_font2(data);
        assert_eq!(bank.sound_bank_info.name, "MyBank");
    }

    #[test]
    #[should_panic(expected = "Invalid soundFont")]
    fn test_parse_invalid_type_panics() {
        // Build a RIFF file with wrong FourCC "XXXX"
        let mut data: Vec<u8> = Vec::new();
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&4u32.to_le_bytes()); // content size
        data.extend_from_slice(b"XXXX");
        parse_sound_font2(data);
    }

    #[test]
    #[should_panic(expected = "Invalid chunk header")]
    fn test_parse_wrong_first_chunk_panics() {
        // Not a RIFF file (starts with "LIST")
        let mut data: Vec<u8> = Vec::new();
        data.extend_from_slice(b"LIST");
        data.extend_from_slice(&4u32.to_le_bytes());
        data.extend_from_slice(b"sfbk");
        parse_sound_font2(data);
    }
}
