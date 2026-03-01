/// downloadable_sounds.rs
/// purpose: Top-level DLS file container with read/write and SF2 conversion.
/// Ported from: src/soundbank/downloadable_sounds/downloadable_sounds.ts
///
/// # TypeScript vs Rust design differences
///
/// - `DownloadableSounds extends DLSVerifier` → free functions from `dls_verifier.rs` called directly.
/// - `static read(buffer: ArrayBuffer)` → `DownloadableSounds::read(data: &[u8])`.
/// - `async write(options)` → synchronous `write(&self) -> Vec<u8>`.
///   The `progressFunction` callback is not supported (MIDI→WAV scope only).
/// - `Date` for `creationDate` → `String` (matches `SoundBankInfoData.creation_date`).
///   The raw ICRD text is stored as-is and written back verbatim.
/// - `fromSFPreset(preset, samples)` in TypeScript receives only `preset` and `samples`;
///   in Rust `BasicPreset` has no back-reference to the sound bank, so `instruments` is
///   also passed explicitly – handled inside `DownloadableSoundsInstrument::from_sf_preset`.
/// - `soundBank.flush()` is called in `to_sf` to sort presets.
use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
use crate::soundbank::downloadable_sounds::dls_verifier::{
    parsing_error, verify_and_read_list, verify_header, verify_text,
};
use crate::soundbank::downloadable_sounds::instrument::DownloadableSoundsInstrument;
use crate::soundbank::downloadable_sounds::region::DownloadableSoundsRegion;
use crate::soundbank::downloadable_sounds::sample::DownloadableSoundsSample;
use crate::soundbank::types::{SF2VersionTag, SoundBankInfoData};
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::{read_little_endian_indexed, write_dword};
use crate::utils::loggin::{
    spessa_synth_group, spessa_synth_group_collapsed, spessa_synth_group_end, spessa_synth_info,
    spessa_synth_warn,
};
use crate::utils::midi_hacks::BankSelectHacks;
use crate::utils::riff_chunk::{
    RIFFChunk, find_riff_list_type, read_riff_chunk, write_riff_chunk_parts, write_riff_chunk_raw,
};
use crate::utils::string::{get_string_bytes, read_binary_string, read_binary_string_indexed};

// ---------------------------------------------------------------------------
// DownloadableSounds
// ---------------------------------------------------------------------------

/// Top-level DLS sound bank container.
///
/// Equivalent to: class DownloadableSounds extends DLSVerifier
pub struct DownloadableSounds {
    /// Wave samples stored in the wvpl chunk.
    /// Equivalent to: public readonly samples = new Array<DownloadableSoundsSample>()
    pub samples: Vec<DownloadableSoundsSample>,

    /// Instruments stored in the lins chunk.
    /// Equivalent to: public readonly instruments = new Array<DownloadableSoundsInstrument>()
    pub instruments: Vec<DownloadableSoundsInstrument>,

    /// Sound bank metadata (name, date, engine, etc.).
    /// Equivalent to: public soundBankInfo: SoundBankInfoData
    pub sound_bank_info: SoundBankInfoData,
}

impl DownloadableSounds {
    /// Creates a new `DownloadableSounds` with default metadata.
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
            instruments: Vec::new(),
            sound_bank_info: SoundBankInfoData {
                name: "Unnamed".to_string(),
                version: SF2VersionTag { major: 2, minor: 4 },
                creation_date: String::new(),
                sound_engine: "DLS Level 2.2".to_string(),
                engineer: None,
                product: None,
                copyright: None,
                comment: None,
                subject: None,
                rom_info: None,
                software: Some("SpessaSynth".to_string()),
                rom_version: None,
            },
        }
    }

    // -----------------------------------------------------------------------
    // read
    // -----------------------------------------------------------------------

    /// Parses a DLS file from a raw byte slice.
    ///
    /// Returns `Err(String)` if the file is malformed or missing required chunks.
    ///
    /// Equivalent to: static read(buffer: ArrayBuffer): DownloadableSounds
    pub fn read(data: &[u8]) -> Result<Self, String> {
        if data.is_empty() {
            return Err("No data provided!".to_string());
        }
        let mut data_array = IndexedByteArray::from_slice(data);
        spessa_synth_group("Parsing DLS file...");

        // Read the main RIFF chunk header.
        let first_chunk = read_riff_chunk(&mut data_array, false, false);
        verify_header(&first_chunk, &["RIFF"])?;
        verify_text(
            &read_binary_string_indexed(&mut data_array, 4).to_lowercase(),
            &["dls "],
        )?;

        // Collect all top-level chunks.
        let mut chunks: Vec<RIFFChunk> = Vec::new();
        while data_array.current_index < data_array.len() {
            chunks.push(read_riff_chunk(&mut data_array, true, false));
        }

        let mut dls = DownloadableSounds::new();

        // Set defaults before reading INFO.
        dls.sound_bank_info.name = "Unnamed DLS".to_string();
        dls.sound_bank_info.product = Some("SpessaSynth DLS".to_string());
        dls.sound_bank_info.comment = Some("(no description)".to_string());

        // Parse the INFO LIST chunk.
        if let Some(info_chunk) = find_riff_list_type(&mut chunks, "INFO") {
            while info_chunk.data.current_index < info_chunk.data.len() {
                let mut info_part = read_riff_chunk(&mut info_chunk.data, true, false);
                let size = info_part.size as usize;
                let text = read_binary_string_indexed(&mut info_part.data, size);
                match info_part.header.as_str() {
                    "INAM" => dls.sound_bank_info.name = text,
                    "ICRD" => dls.sound_bank_info.creation_date = text,
                    "ICMT" => dls.sound_bank_info.comment = Some(text),
                    "ISBJ" => dls.sound_bank_info.subject = Some(text),
                    "ICOP" => dls.sound_bank_info.copyright = Some(text),
                    "IENG" => dls.sound_bank_info.engineer = Some(text),
                    "IPRD" => dls.sound_bank_info.product = Some(text),
                    "ISFT" => dls.sound_bank_info.software = Some(text),
                    _ => {}
                }
            }
        }

        Self::print_info(&dls);

        // Read "colh" chunk (instrument count).
        let instrument_amount = {
            let colh_pos = chunks
                .iter()
                .position(|c| c.header == "colh")
                .ok_or_else(|| {
                    spessa_synth_group_end();
                    parsing_error("No colh chunk!")
                })?;
            let colh = &mut chunks[colh_pos];
            colh.data.current_index = 0;
            read_little_endian_indexed(&mut colh.data, 4)
        };
        spessa_synth_info(&format!("Instruments amount: {instrument_amount}"));

        // Read the wvpl (wave pool) LIST chunk.
        let wave_chunks: Vec<RIFFChunk> = {
            let wvpl_chunk = chunks.iter_mut().find(|c| {
                c.header == "LIST"
                    && c.data.len() >= 4
                    && read_binary_string(&c.data, 4, 0) == "wvpl"
            });
            match wvpl_chunk {
                None => {
                    spessa_synth_group_end();
                    return Err(parsing_error("No wvpl chunk!"));
                }
                Some(wvpl) => verify_and_read_list(wvpl, &["wvpl"])?,
            }
        };
        for mut wave in wave_chunks {
            match DownloadableSoundsSample::read(&mut wave) {
                Ok(sample) => dls.samples.push(sample),
                Err(e) => spessa_synth_warn(&format!("Skipping wave sample: {e}")),
            }
        }

        // Read the lins (instrument list) LIST chunk.
        let instrument_chunks: Vec<RIFFChunk> = {
            let lins_chunk = chunks.iter_mut().find(|c| {
                c.header == "LIST"
                    && c.data.len() >= 4
                    && read_binary_string(&c.data, 4, 0) == "lins"
            });
            match lins_chunk {
                None => {
                    spessa_synth_group_end();
                    return Err(parsing_error("No lins chunk!"));
                }
                Some(lins) => verify_and_read_list(lins, &["lins"])?,
            }
        };
        spessa_synth_group_collapsed("Loading instruments...");
        if instrument_chunks.len() as u32 != instrument_amount {
            spessa_synth_warn(&format!(
                "Colh reported invalid amount of instruments. Detected {}, expected {instrument_amount}",
                instrument_chunks.len()
            ));
        }
        for mut ins_chunk in instrument_chunks {
            match DownloadableSoundsInstrument::read(&dls.samples, &mut ins_chunk) {
                Ok(instrument) => dls.instruments.push(instrument),
                Err(e) => spessa_synth_warn(&format!("Skipping instrument: {e}")),
            }
        }
        spessa_synth_group_end();

        // MobileBAE instrument aliasing (pgal chunk).
        // https://github.com/spessasus/spessasynth_core/issues/14
        // https://lpcwiki.miraheze.org/wiki/MobileBAE#Proprietary_instrument_aliasing_chunk
        if let Some(pgal_pos) = chunks.iter().position(|c| c.header == "pgal") {
            spessa_synth_info("Found the instrument aliasing chunk!");
            // Copy the data to avoid borrow-checker issues with dls.instruments.
            let pgal_len = chunks[pgal_pos].data.len();
            let mut pgal_data = chunks[pgal_pos].data.slice(0, pgal_len);
            pgal_data.current_index = 0;

            // If the bank doesn't start with 00 01 02 03, skip the first 4 bytes.
            if pgal_data.len() < 4
                || pgal_data[0] != 0
                || pgal_data[1] != 1
                || pgal_data[2] != 2
                || pgal_data[3] != 3
            {
                pgal_data.current_index += 4;
            }

            // Find the drum instrument index.
            let drum_idx = dls
                .instruments
                .iter()
                .position(|i| BankSelectHacks::is_xg_drums(i.bank_msb) || i.is_gm_gs_drum);
            if drum_idx.is_none() {
                spessa_synth_warn("MobileBAE aliasing chunk without a drum preset. Aborting!");
                spessa_synth_group_end();
                return Ok(dls);
            }
            let drum_idx = drum_idx.unwrap();

            // Read 128-byte drum alias table.
            if pgal_data.current_index + 128 > pgal_data.len() {
                spessa_synth_warn("MobileBAE aliasing chunk too short for drum table. Aborting!");
                spessa_synth_group_end();
                return Ok(dls);
            }
            let drum_aliases_start = pgal_data.current_index;
            let mut new_drum_regions: Vec<DownloadableSoundsRegion> = Vec::new();
            for key_num in 0u8..128 {
                let alias = pgal_data[drum_aliases_start + key_num as usize];
                if alias == key_num {
                    continue;
                }
                // Find the matching region.
                let region_opt = dls.instruments[drum_idx]
                    .regions
                    .iter()
                    .find(|r| r.key_range.max as u8 == alias && r.key_range.min as u8 == alias);
                match region_opt {
                    None => {
                        spessa_synth_warn(&format!(
                            "Invalid drum alias {key_num} to {alias}: region does not exist."
                        ));
                    }
                    Some(region) => {
                        let mut copied = DownloadableSoundsRegion::copy_from(region);
                        copied.key_range.max = key_num as f64;
                        copied.key_range.min = key_num as f64;
                        new_drum_regions.push(copied);
                    }
                }
            }
            dls.instruments[drum_idx].regions.extend(new_drum_regions);
            pgal_data.current_index = drum_aliases_start + 128;

            // Skip 4-byte footer.
            pgal_data.current_index += 4;

            // Read program alias entries.
            while pgal_data.current_index + 8 <= pgal_data.len() {
                // Alias target
                let alias_bank_num = read_little_endian_indexed(&mut pgal_data, 2) as u16;
                let alias_bank_lsb = (alias_bank_num & 0x7f) as u8;
                let alias_bank_msb = ((alias_bank_num >> 7) & 0x7f) as u8;
                let alias_program = pgal_data[pgal_data.current_index];
                pgal_data.current_index += 1;
                let null_byte = pgal_data[pgal_data.current_index];
                pgal_data.current_index += 1;
                if null_byte != 0 {
                    spessa_synth_warn(&format!("Invalid alias byte. Expected 0, got {null_byte}"));
                }

                // Input source
                let input_bank_num = read_little_endian_indexed(&mut pgal_data, 2) as u16;
                let input_bank_lsb = (input_bank_num & 0x7f) as u8;
                let input_bank_msb = ((input_bank_num >> 7) & 0x7f) as u8;
                let input_program = pgal_data[pgal_data.current_index];
                pgal_data.current_index += 1;
                let null_byte2 = pgal_data[pgal_data.current_index];
                pgal_data.current_index += 1;
                if null_byte2 != 0 {
                    spessa_synth_warn(&format!(
                        "Invalid alias header. Expected 0, got {null_byte2}"
                    ));
                }

                // Find the source instrument.
                let input_opt = dls.instruments.iter().position(|inst| {
                    inst.bank_lsb == input_bank_lsb
                        && inst.bank_msb == input_bank_msb
                        && inst.program == input_program
                        && !inst.is_gm_gs_drum
                });
                match input_opt {
                    None => {
                        spessa_synth_warn(&format!(
                            "Invalid alias. Missing instrument: {input_bank_lsb}:{input_bank_msb}:{input_program}"
                        ));
                    }
                    Some(idx) => {
                        let mut alias_inst =
                            DownloadableSoundsInstrument::copy_from(&dls.instruments[idx]);
                        alias_inst.bank_msb = alias_bank_msb;
                        alias_inst.bank_lsb = alias_bank_lsb;
                        alias_inst.program = alias_program;
                        dls.instruments.push(alias_inst);
                    }
                }
            }
        }

        spessa_synth_info(&format!(
            "Parsing finished! \"{}\" has {} instruments and {} samples.",
            if dls.sound_bank_info.name.is_empty() {
                "UNNAMED"
            } else {
                &dls.sound_bank_info.name
            },
            dls.instruments.len(),
            dls.samples.len()
        ));
        spessa_synth_group_end();
        Ok(dls)
    }

    // -----------------------------------------------------------------------
    // from_sf
    // -----------------------------------------------------------------------

    /// Converts a `BasicSoundBank` (SF2) to a `DownloadableSounds` (DLS) container.
    ///
    /// Equivalent to: static fromSF(bank: BasicSoundBank): DownloadableSounds
    pub fn from_sf(bank: &mut BasicSoundBank) -> Self {
        spessa_synth_group_collapsed("Saving SF2 to DLS level 2...");
        let mut dls = DownloadableSounds::new();
        dls.sound_bank_info = bank.sound_bank_info.clone();
        let original_comment = dls
            .sound_bank_info
            .comment
            .clone()
            .unwrap_or_else(|| "(No description)".to_string());
        dls.sound_bank_info.comment = Some(format!(
            "{original_comment}\nConverted from SF2 to DLS with SpessaSynth"
        ));

        for sample in bank.samples.iter_mut() {
            dls.samples
                .push(DownloadableSoundsSample::from_sf_sample(sample));
        }
        for preset in &bank.presets {
            dls.instruments
                .push(DownloadableSoundsInstrument::from_sf_preset(
                    preset,
                    &bank.samples,
                    &bank.instruments,
                ));
        }

        spessa_synth_info("Conversion complete!");
        spessa_synth_group_end();
        dls
    }

    // -----------------------------------------------------------------------
    // write
    // -----------------------------------------------------------------------

    /// Serialises this DLS bank to raw bytes.
    ///
    /// Layout: RIFF("DLS ") { colh | lins LIST | ptbl | wvpl LIST | INFO LIST }
    ///
    /// Note: TypeScript's `write()` is `async` and accepts a `progressFunction`.
    /// Rust's version is synchronous and no progress callback is supported.
    ///
    /// Equivalent to: async write(options: DLSWriteOptions): Promise<ArrayBuffer>
    pub fn write(&self) -> Vec<u8> {
        spessa_synth_group_collapsed("Saving DLS...");

        // colh chunk: instrument count.
        let mut colh_data = IndexedByteArray::new(4);
        write_dword(&mut colh_data, self.instruments.len() as u32);
        let colh = write_riff_chunk_raw("colh", &colh_data, false, false);

        // lins LIST: one ins  chunk per instrument.
        spessa_synth_group_collapsed("Writing instruments...");
        let instrument_parts: Vec<IndexedByteArray> =
            self.instruments.iter().map(|i| i.write()).collect();
        let instrument_slices: Vec<&[u8]> = instrument_parts.iter().map(|a| &**a).collect();
        let lins = write_riff_chunk_parts("lins", &instrument_slices, true);
        spessa_synth_info("Success!");
        spessa_synth_group_end();

        // wvpl LIST: one wave chunk per sample.
        spessa_synth_group_collapsed("Writing WAVE samples...");
        let mut current_index: u32 = 0;
        let mut ptbl_offsets: Vec<u32> = Vec::new();
        let mut sample_parts: Vec<IndexedByteArray> = Vec::new();
        for s in &self.samples {
            let out = s.write();
            ptbl_offsets.push(current_index);
            current_index += out.len() as u32;
            sample_parts.push(out);
        }
        let sample_slices: Vec<&[u8]> = sample_parts.iter().map(|a| &**a).collect();
        let wvpl = write_riff_chunk_parts("wvpl", &sample_slices, true);
        spessa_synth_info("Succeeded!");
        spessa_synth_group_end();

        // ptbl chunk: pool table with per-sample offsets.
        let ptbl_size = 8 + 4 * ptbl_offsets.len();
        let mut ptbl_data = IndexedByteArray::new(ptbl_size);
        write_dword(&mut ptbl_data, 8); // cbSize (size of the ptbl header)
        write_dword(&mut ptbl_data, ptbl_offsets.len() as u32);
        for &offset in &ptbl_offsets {
            write_dword(&mut ptbl_data, offset);
        }
        let ptbl = write_riff_chunk_raw("ptbl", &ptbl_data, false, false);

        // INFO LIST: metadata.
        let mut info_chunks: Vec<IndexedByteArray> = Vec::new();
        let mut write_dls_info = |type_: &str, text: &str| {
            let bytes = get_string_bytes(text, true, false);
            info_chunks.push(write_riff_chunk_raw(type_, &bytes, false, false));
        };

        write_dls_info("INAM", &self.sound_bank_info.name);
        if !self.sound_bank_info.creation_date.is_empty() {
            write_dls_info("ICRD", &self.sound_bank_info.creation_date);
        }
        if let Some(ref comment) = self.sound_bank_info.comment {
            write_dls_info("ICMT", comment);
        }
        if let Some(ref copyright) = self.sound_bank_info.copyright {
            write_dls_info("ICOP", copyright);
        }
        if let Some(ref engineer) = self.sound_bank_info.engineer {
            write_dls_info("IENG", engineer);
        }
        if let Some(ref product) = self.sound_bank_info.product {
            write_dls_info("IPRD", product);
        }
        // Always write ISFT as "SpessaSynth" (matching TypeScript behaviour).
        write_dls_info("ISFT", "SpessaSynth");
        if let Some(ref subject) = self.sound_bank_info.subject {
            write_dls_info("ISBJ", subject);
        }

        let info_slices: Vec<&[u8]> = info_chunks.iter().map(|a| &**a).collect();
        let info = write_riff_chunk_parts("INFO", &info_slices, true);

        // Combine: RIFF("DLS ") { DLS  | colh | lins | ptbl | wvpl | INFO }
        spessa_synth_info("Combining everything...");
        let dls_str_bytes = get_string_bytes("DLS ", false, false);
        let parts: Vec<&[u8]> = vec![&dls_str_bytes, &colh, &lins, &ptbl, &wvpl, &info];
        let out = write_riff_chunk_parts("RIFF", &parts, false);

        spessa_synth_info("Saved successfully!");
        spessa_synth_group_end();
        out.to_vec()
    }

    // -----------------------------------------------------------------------
    // to_sf
    // -----------------------------------------------------------------------

    /// Converts this DLS bank to an SF2 `BasicSoundBank`.
    ///
    /// Equivalent to: toSF(): BasicSoundBank
    pub fn to_sf(&self) -> BasicSoundBank {
        spessa_synth_group("Converting DLS to SF2...");
        let mut sound_bank = BasicSoundBank::new();

        sound_bank.sound_bank_info = self.sound_bank_info.clone();
        let original_comment = sound_bank
            .sound_bank_info
            .comment
            .clone()
            .unwrap_or_else(|| "(No description)".to_string());
        sound_bank.sound_bank_info.comment = Some(format!(
            "{original_comment}\nConverted from DLS to SF2 with SpessaSynth"
        ));
        sound_bank.sound_bank_info.version = SF2VersionTag { major: 2, minor: 4 };

        for sample in &self.samples {
            sample.to_sf_sample(&mut sound_bank);
        }
        for instrument in &self.instruments {
            instrument.to_sf_preset(&mut sound_bank);
        }
        sound_bank.flush();

        spessa_synth_info("Conversion complete!");
        spessa_synth_group_end();
        sound_bank
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Logs metadata fields from `dls.sound_bank_info`.
    ///
    /// Equivalent to: private static printInfo(dls: DownloadableSounds)
    fn print_info(dls: &DownloadableSounds) {
        let info = &dls.sound_bank_info;
        spessa_synth_info(&format!("name: \"{}\"", info.name));
        spessa_synth_info(&format!(
            "version: \"{}.{}\"",
            info.version.major, info.version.minor
        ));
        if !info.creation_date.is_empty() {
            spessa_synth_info(&format!("creation_date: \"{}\"", info.creation_date));
        }
        spessa_synth_info(&format!("sound_engine: \"{}\"", info.sound_engine));
        if let Some(ref v) = info.engineer {
            spessa_synth_info(&format!("engineer: \"{v}\""));
        }
        if let Some(ref v) = info.product {
            spessa_synth_info(&format!("product: \"{v}\""));
        }
        if let Some(ref v) = info.copyright {
            spessa_synth_info(&format!("copyright: \"{v}\""));
        }
        if let Some(ref v) = info.comment {
            spessa_synth_info(&format!("comment: \"{v}\""));
        }
        if let Some(ref v) = info.subject {
            spessa_synth_info(&format!("subject: \"{v}\""));
        }
        if let Some(ref v) = info.software {
            spessa_synth_info(&format!("software: \"{v}\""));
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
    use crate::soundbank::downloadable_sounds::dls_sample::w_format_tag;
    use crate::soundbank::downloadable_sounds::sample::DownloadableSoundsSample;

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// Builds the raw bytes for a RIFF sub-chunk: [header 4B][size 4B LE][data].
    fn sub_chunk(header: &str, data: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(header.as_bytes());
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(data);
        if data.len() % 2 != 0 {
            b.push(0);
        }
        b
    }

    /// Builds a LIST chunk: [LIST 4B][total_size 4B LE][type 4B][sub_chunks bytes]
    fn list_chunk(type_: &str, sub_chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut body: Vec<u8> = Vec::new();
        body.extend_from_slice(type_.as_bytes());
        for sc in sub_chunks {
            body.extend_from_slice(sc);
        }
        let mut b = Vec::new();
        b.extend_from_slice(b"LIST");
        b.extend_from_slice(&(body.len() as u32).to_le_bytes());
        b.extend_from_slice(&body);
        b
    }

    /// Creates a minimal 16-byte `fmt ` chunk body for PCM mono 16-bit.
    fn fmt_body(sample_rate: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&1u16.to_le_bytes()); // wFormatTag = PCM
        b.extend_from_slice(&1u16.to_le_bytes()); // wChannels = 1
        b.extend_from_slice(&sample_rate.to_le_bytes());
        b.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // avg bytes/sec
        b.extend_from_slice(&2u16.to_le_bytes()); // block align
        b.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        b
    }

    /// Builds a minimal `wave` LIST sub-chunk with given PCM data.
    fn wave_chunk(sample_rate: u32, audio: &[i16]) -> Vec<u8> {
        let data_bytes: Vec<u8> = audio.iter().flat_map(|&s| s.to_le_bytes()).collect();
        list_chunk(
            "wave",
            &[
                sub_chunk("fmt ", &fmt_body(sample_rate)),
                sub_chunk("data", &data_bytes),
            ],
        )
    }

    /// Builds a minimal DLS file as a byte vector.
    ///
    /// Includes the minimum required chunks: colh, wvpl, lins.
    fn make_minimal_dls(instrument_count: u32, samples: &[Vec<u8>]) -> Vec<u8> {
        // colh
        let colh = sub_chunk("colh", &(instrument_count.to_le_bytes()));

        // wvpl LIST
        let wvpl = list_chunk("wvpl", samples);

        // lins LIST (empty)
        let lins = list_chunk("lins", &[]);

        // RIFF("DLS ") body
        let mut riff_body = Vec::new();
        riff_body.extend_from_slice(b"DLS ");
        riff_body.extend_from_slice(&colh);
        riff_body.extend_from_slice(&wvpl);
        riff_body.extend_from_slice(&lins);

        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
        out.extend_from_slice(&riff_body);
        out
    }

    // ── new ───────────────────────────────────────────────────────────────────

    #[test]
    fn test_new_default_name() {
        let dls = DownloadableSounds::new();
        assert_eq!(dls.sound_bank_info.name, "Unnamed");
    }

    #[test]
    fn test_new_empty_samples() {
        let dls = DownloadableSounds::new();
        assert!(dls.samples.is_empty());
    }

    #[test]
    fn test_new_empty_instruments() {
        let dls = DownloadableSounds::new();
        assert!(dls.instruments.is_empty());
    }

    #[test]
    fn test_new_default_sound_engine() {
        let dls = DownloadableSounds::new();
        assert_eq!(dls.sound_bank_info.sound_engine, "DLS Level 2.2");
    }

    #[test]
    fn test_new_default_software() {
        let dls = DownloadableSounds::new();
        assert_eq!(dls.sound_bank_info.software.as_deref(), Some("SpessaSynth"));
    }

    // ── read: error cases ──────────────────────────────────────────────────

    #[test]
    fn test_read_empty_data_returns_err() {
        assert!(DownloadableSounds::read(&[]).is_err());
    }

    #[test]
    fn test_read_wrong_header_returns_err() {
        // 'WAVE' instead of 'RIFF'
        let mut bad_data = b"WAVE".to_vec();
        bad_data.extend_from_slice(&100u32.to_le_bytes());
        bad_data.extend_from_slice(b"DLS ");
        assert!(DownloadableSounds::read(&bad_data).is_err());
    }

    #[test]
    fn test_read_wrong_type_returns_err() {
        // RIFF with 'sfbk' instead of 'DLS '
        let mut riff_body = b"sfbk".to_vec();
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
        out.append(&mut riff_body);
        assert!(DownloadableSounds::read(&out).is_err());
    }

    #[test]
    fn test_read_missing_colh_returns_err() {
        // RIFF with DLS  but no colh
        let wvpl = list_chunk("wvpl", &[]);
        let lins = list_chunk("lins", &[]);
        let mut riff_body = b"DLS ".to_vec();
        riff_body.extend_from_slice(&wvpl);
        riff_body.extend_from_slice(&lins);
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
        out.extend_from_slice(&riff_body);
        assert!(DownloadableSounds::read(&out).is_err());
    }

    #[test]
    fn test_read_missing_wvpl_returns_err() {
        let colh = sub_chunk("colh", &0u32.to_le_bytes());
        let lins = list_chunk("lins", &[]);
        let mut riff_body = b"DLS ".to_vec();
        riff_body.extend_from_slice(&colh);
        riff_body.extend_from_slice(&lins);
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
        out.extend_from_slice(&riff_body);
        assert!(DownloadableSounds::read(&out).is_err());
    }

    #[test]
    fn test_read_missing_lins_returns_err() {
        let colh = sub_chunk("colh", &0u32.to_le_bytes());
        let wvpl = list_chunk("wvpl", &[]);
        let mut riff_body = b"DLS ".to_vec();
        riff_body.extend_from_slice(&colh);
        riff_body.extend_from_slice(&wvpl);
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
        out.extend_from_slice(&riff_body);
        assert!(DownloadableSounds::read(&out).is_err());
    }

    // ── read: valid DLS ────────────────────────────────────────────────────

    #[test]
    fn test_read_minimal_dls_ok() {
        let data = make_minimal_dls(0, &[]);
        assert!(DownloadableSounds::read(&data).is_ok());
    }

    #[test]
    fn test_read_sets_default_name() {
        let data = make_minimal_dls(0, &[]);
        let dls = DownloadableSounds::read(&data).unwrap();
        assert_eq!(dls.sound_bank_info.name, "Unnamed DLS");
    }

    #[test]
    fn test_read_parses_inam() {
        // Add INFO LIST with INAM.
        let name_bytes = b"TestBank\x00".to_vec();
        let inam = sub_chunk("INAM", &name_bytes);
        let info = list_chunk("INFO", &[inam]);

        let colh = sub_chunk("colh", &0u32.to_le_bytes());
        let wvpl = list_chunk("wvpl", &[]);
        let lins = list_chunk("lins", &[]);

        let mut riff_body = b"DLS ".to_vec();
        riff_body.extend_from_slice(&colh);
        riff_body.extend_from_slice(&wvpl);
        riff_body.extend_from_slice(&lins);
        riff_body.extend_from_slice(&info);

        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
        out.extend_from_slice(&riff_body);

        let dls = DownloadableSounds::read(&out).unwrap();
        assert_eq!(dls.sound_bank_info.name, "TestBank");
    }

    #[test]
    fn test_read_parses_icrd() {
        let date_bytes = b"2024-01-01\x00".to_vec();
        let icrd = sub_chunk("ICRD", &date_bytes);
        let info = list_chunk("INFO", &[icrd]);

        let colh = sub_chunk("colh", &0u32.to_le_bytes());
        let wvpl = list_chunk("wvpl", &[]);
        let lins = list_chunk("lins", &[]);

        let mut riff_body = b"DLS ".to_vec();
        riff_body.extend_from_slice(&colh);
        riff_body.extend_from_slice(&wvpl);
        riff_body.extend_from_slice(&lins);
        riff_body.extend_from_slice(&info);

        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
        out.extend_from_slice(&riff_body);

        let dls = DownloadableSounds::read(&out).unwrap();
        assert_eq!(dls.sound_bank_info.creation_date, "2024-01-01");
    }

    #[test]
    fn test_read_parses_icmt() {
        let comment_bytes = b"A comment\x00".to_vec();
        let icmt = sub_chunk("ICMT", &comment_bytes);
        let info = list_chunk("INFO", &[icmt]);

        let colh = sub_chunk("colh", &0u32.to_le_bytes());
        let wvpl = list_chunk("wvpl", &[]);
        let lins = list_chunk("lins", &[]);

        let mut riff_body = b"DLS ".to_vec();
        riff_body.extend_from_slice(&colh);
        riff_body.extend_from_slice(&wvpl);
        riff_body.extend_from_slice(&lins);
        riff_body.extend_from_slice(&info);

        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
        out.extend_from_slice(&riff_body);

        let dls = DownloadableSounds::read(&out).unwrap();
        assert_eq!(dls.sound_bank_info.comment.as_deref(), Some("A comment"));
    }

    #[test]
    fn test_read_with_one_sample() {
        let wave = wave_chunk(44_100, &[0i16, 1000]);
        let data = make_minimal_dls(0, &[wave]);
        let dls = DownloadableSounds::read(&data).unwrap();
        assert_eq!(dls.samples.len(), 1);
    }

    #[test]
    fn test_read_with_two_samples() {
        let wave1 = wave_chunk(44_100, &[0i16]);
        let wave2 = wave_chunk(22_050, &[0i16]);
        let data = make_minimal_dls(0, &[wave1, wave2]);
        let dls = DownloadableSounds::read(&data).unwrap();
        assert_eq!(dls.samples.len(), 2);
    }

    #[test]
    fn test_read_sample_rate_preserved() {
        let wave = wave_chunk(48_000, &[100i16]);
        let data = make_minimal_dls(0, &[wave]);
        let dls = DownloadableSounds::read(&data).unwrap();
        assert_eq!(dls.samples[0].sample_rate, 48_000);
    }

    #[test]
    fn test_read_no_instruments_when_lins_empty() {
        let data = make_minimal_dls(0, &[]);
        let dls = DownloadableSounds::read(&data).unwrap();
        assert!(dls.instruments.is_empty());
    }

    // ── write ─────────────────────────────────────────────────────────────

    #[test]
    fn test_write_starts_with_riff() {
        let dls = DownloadableSounds::new();
        let out = dls.write();
        assert_eq!(&out[0..4], b"RIFF");
    }

    #[test]
    fn test_write_contains_dls_fourcc() {
        let dls = DownloadableSounds::new();
        let out = dls.write();
        // After RIFF(4) + size(4) = index 8 to 12
        assert_eq!(&out[8..12], b"DLS ");
    }

    #[test]
    fn test_write_contains_colh() {
        let dls = DownloadableSounds::new();
        let out = dls.write();
        assert!(out.windows(4).any(|w| w == b"colh"), "colh not found");
    }

    #[test]
    fn test_write_contains_lins() {
        let dls = DownloadableSounds::new();
        let out = dls.write();
        assert!(out.windows(4).any(|w| w == b"lins"), "lins not found");
    }

    #[test]
    fn test_write_contains_ptbl() {
        let dls = DownloadableSounds::new();
        let out = dls.write();
        assert!(out.windows(4).any(|w| w == b"ptbl"), "ptbl not found");
    }

    #[test]
    fn test_write_contains_wvpl() {
        let dls = DownloadableSounds::new();
        let out = dls.write();
        assert!(out.windows(4).any(|w| w == b"wvpl"), "wvpl not found");
    }

    #[test]
    fn test_write_contains_info() {
        let dls = DownloadableSounds::new();
        let out = dls.write();
        assert!(out.windows(4).any(|w| w == b"INFO"), "INFO not found");
    }

    #[test]
    fn test_write_contains_inam() {
        let mut dls = DownloadableSounds::new();
        dls.sound_bank_info.name = "TestDLS".to_string();
        let out = dls.write();
        assert!(out.windows(4).any(|w| w == b"INAM"), "INAM not found");
    }

    #[test]
    fn test_write_colh_count_zero_instruments() {
        let dls = DownloadableSounds::new();
        let out = dls.write();
        // Find colh and read the 4-byte count field.
        let colh_pos = out.windows(4).position(|w| w == b"colh").unwrap();
        // colh layout: [colh(4)][size(4)][count(4)]
        let count = u32::from_le_bytes([
            out[colh_pos + 8],
            out[colh_pos + 9],
            out[colh_pos + 10],
            out[colh_pos + 11],
        ]);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_write_software_is_spessasynth() {
        let dls = DownloadableSounds::new();
        let out = dls.write();
        let out_str = String::from_utf8_lossy(&out);
        assert!(out_str.contains("SpessaSynth"), "ISFT value not found");
    }

    // ── write / read round-trip ────────────────────────────────────────────

    #[test]
    fn test_write_read_roundtrip_name() {
        let mut dls = DownloadableSounds::new();
        dls.sound_bank_info.name = "RoundTrip".to_string();
        let bytes = dls.write();
        let dls2 = DownloadableSounds::read(&bytes).unwrap();
        assert_eq!(dls2.sound_bank_info.name, "RoundTrip");
    }

    #[test]
    fn test_write_read_roundtrip_comment() {
        let mut dls = DownloadableSounds::new();
        dls.sound_bank_info.comment = Some("Hello DLS".to_string());
        let bytes = dls.write();
        let dls2 = DownloadableSounds::read(&bytes).unwrap();
        assert_eq!(dls2.sound_bank_info.comment.as_deref(), Some("Hello DLS"));
    }

    #[test]
    fn test_write_read_roundtrip_no_instruments() {
        let dls = DownloadableSounds::new();
        let bytes = dls.write();
        let dls2 = DownloadableSounds::read(&bytes).unwrap();
        assert!(dls2.instruments.is_empty());
    }

    #[test]
    fn test_write_read_roundtrip_one_sample() {
        let mut dls = DownloadableSounds::new();
        let s = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![0u8; 4]);
        dls.samples.push(s);
        let bytes = dls.write();
        let dls2 = DownloadableSounds::read(&bytes).unwrap();
        assert_eq!(dls2.samples.len(), 1);
        assert_eq!(dls2.samples[0].sample_rate, 44_100);
    }

    // ── to_sf ─────────────────────────────────────────────────────────────

    #[test]
    fn test_to_sf_empty_produces_valid_bank() {
        let dls = DownloadableSounds::new();
        let bank = dls.to_sf();
        assert!(bank.presets.is_empty());
        assert!(bank.instruments.is_empty());
        assert!(bank.samples.is_empty());
    }

    #[test]
    fn test_to_sf_copies_name() {
        let mut dls = DownloadableSounds::new();
        dls.sound_bank_info.name = "MyDLS".to_string();
        let bank = dls.to_sf();
        assert_eq!(bank.sound_bank_info.name, "MyDLS");
    }

    #[test]
    fn test_to_sf_comment_appended() {
        let mut dls = DownloadableSounds::new();
        dls.sound_bank_info.comment = Some("Original".to_string());
        let bank = dls.to_sf();
        let comment = bank.sound_bank_info.comment.unwrap();
        assert!(
            comment.contains("Original"),
            "original comment should be preserved"
        );
        assert!(
            comment.contains("DLS to SF2"),
            "conversion note should be appended"
        );
    }

    #[test]
    fn test_to_sf_version_set_to_2_4() {
        let dls = DownloadableSounds::new();
        let bank = dls.to_sf();
        assert_eq!(bank.sound_bank_info.version.major, 2);
        assert_eq!(bank.sound_bank_info.version.minor, 4);
    }

    #[test]
    fn test_to_sf_one_sample_added() {
        let mut dls = DownloadableSounds::new();
        let s = DownloadableSoundsSample::new(w_format_tag::PCM, 2, 44_100, vec![0u8; 4]);
        dls.samples.push(s);
        let bank = dls.to_sf();
        assert_eq!(bank.samples.len(), 1);
    }

    #[test]
    fn test_to_sf_one_instrument_added() {
        let mut dls = DownloadableSounds::new();
        dls.instruments.push(DownloadableSoundsInstrument::new());
        let bank = dls.to_sf();
        assert_eq!(bank.presets.len(), 1);
        assert_eq!(bank.instruments.len(), 1);
    }

    // ── from_sf ───────────────────────────────────────────────────────────

    #[test]
    fn test_from_sf_empty_bank_produces_empty_dls() {
        let mut bank = BasicSoundBank::new();
        let dls = DownloadableSounds::from_sf(&mut bank);
        assert!(dls.samples.is_empty());
        assert!(dls.instruments.is_empty());
    }

    #[test]
    fn test_from_sf_copies_name() {
        let mut bank = BasicSoundBank::new();
        bank.sound_bank_info.name = "SF2Bank".to_string();
        let dls = DownloadableSounds::from_sf(&mut bank);
        assert_eq!(dls.sound_bank_info.name, "SF2Bank");
    }

    #[test]
    fn test_from_sf_comment_appended() {
        let mut bank = BasicSoundBank::new();
        bank.sound_bank_info.comment = Some("Original SF2".to_string());
        let dls = DownloadableSounds::from_sf(&mut bank);
        let comment = dls.sound_bank_info.comment.unwrap();
        assert!(comment.contains("Original SF2"));
        assert!(comment.contains("SF2 to DLS"));
    }
}
