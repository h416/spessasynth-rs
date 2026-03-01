/// sound_bank_loader.rs
/// purpose: Detects the sound bank format (SF2 or DLS) from the file header
///          and parses it into a BasicSoundBank.
/// Ported from: src/soundbank/sound_bank_loader.ts
use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
use crate::soundbank::downloadable_sounds::downloadable_sounds::DownloadableSounds;
use crate::soundbank::soundfont::read::soundfont::parse_sound_font2;
use crate::utils::string::read_binary_string;

// ---------------------------------------------------------------------------
// load_sound_bank
// ---------------------------------------------------------------------------

/// Detects whether `data` is a DLS or SF2/SF3 file, then parses it.
///
/// Detection reads bytes 8-11 (the RIFF list-type FourCC) and checks whether
/// it equals `"dls "` (case-insensitive).  Any other value is treated as SF2.
///
/// # Panics
/// Panics if the data is too short to contain a type identifier, or if the
/// underlying parser fails (e.g. malformed RIFF structure).
///
/// Equivalent to: SoundBankLoader.fromArrayBuffer(buffer)
pub fn load_sound_bank(data: Vec<u8>) -> BasicSoundBank {
    // Read bytes 8-11 to identify the bank format.
    // Equivalent to:
    //   const check = buffer.slice(8, 12);
    //   const a = new IndexedByteArray(check);
    //   const id = readBinaryStringIndexed(a, 4).toLowerCase();
    if data.len() >= 12 {
        let id = read_binary_string(&data, 4, 8).to_lowercase();
        if id == "dls " {
            return load_dls(&data);
        }
    }
    parse_sound_font2(data)
}

// ---------------------------------------------------------------------------
// load_dls (private)
// ---------------------------------------------------------------------------

/// Parses the byte slice as a DLS file and converts it to a `BasicSoundBank`.
///
/// # Panics
/// Panics if `DownloadableSounds::read` returns an error.
///
/// Equivalent to: private static loadDLS(buffer: ArrayBuffer)
fn load_dls(data: &[u8]) -> BasicSoundBank {
    let dls = DownloadableSounds::read(data).expect("Failed to parse DLS sound bank");
    dls.to_sf()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helper: build a minimal valid SF2 binary (adapted from soundfont.rs tests)
    // -----------------------------------------------------------------------

    fn build_minimal_sf2(bank_name: &str) -> Vec<u8> {
        // INFO content
        let mut info_content: Vec<u8> = Vec::new();
        info_content.extend_from_slice(b"INFO");
        // ifil: version 2.4
        info_content.extend_from_slice(b"ifil");
        info_content.extend_from_slice(&4u32.to_le_bytes());
        info_content.extend_from_slice(&2u16.to_le_bytes());
        info_content.extend_from_slice(&4u16.to_le_bytes());
        // INAM
        let mut name_bytes = bank_name.as_bytes().to_vec();
        if name_bytes.len() % 2 != 0 {
            name_bytes.push(0);
        }
        info_content.extend_from_slice(b"INAM");
        info_content.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
        info_content.extend_from_slice(&name_bytes);
        // isng
        info_content.extend_from_slice(b"isng");
        info_content.extend_from_slice(&8u32.to_le_bytes());
        info_content.extend_from_slice(b"EMU8000\x00");

        // SDTA content
        let mut sdta_content: Vec<u8> = Vec::new();
        sdta_content.extend_from_slice(b"sdta");
        sdta_content.extend_from_slice(b"smpl");
        sdta_content.extend_from_slice(&0u32.to_le_bytes());

        // PDTA content (all sentinels)
        let mut pdta_content: Vec<u8> = Vec::new();
        pdta_content.extend_from_slice(b"pdta");
        for (tag, size) in &[
            (b"phdr", 38u32),
            (b"pbag", 4u32),
            (b"pmod", 10u32),
            (b"pgen", 4u32),
            (b"inst", 22u32),
            (b"ibag", 4u32),
            (b"imod", 10u32),
            (b"igen", 4u32),
            (b"shdr", 46u32),
        ] {
            pdta_content.extend_from_slice(*tag);
            pdta_content.extend_from_slice(&size.to_le_bytes());
            pdta_content.extend_from_slice(&vec![0u8; *size as usize]);
        }

        let mut all_chunks: Vec<u8> = Vec::new();
        for content in &[&info_content, &sdta_content, &pdta_content] {
            all_chunks.extend_from_slice(b"LIST");
            all_chunks.extend_from_slice(&(content.len() as u32).to_le_bytes());
            all_chunks.extend_from_slice(content);
        }

        let content_size = 4 + all_chunks.len();
        let mut riff: Vec<u8> = Vec::new();
        riff.extend_from_slice(b"RIFF");
        riff.extend_from_slice(&(content_size as u32).to_le_bytes());
        riff.extend_from_slice(b"sfbk");
        riff.extend_from_slice(&all_chunks);
        riff
    }

    // -----------------------------------------------------------------------
    // Helper: build a minimal valid DLS binary
    // -----------------------------------------------------------------------

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

    fn build_minimal_dls() -> Vec<u8> {
        let colh = sub_chunk("colh", &0u32.to_le_bytes());
        let wvpl = list_chunk("wvpl", &[]);
        let lins = list_chunk("lins", &[]);

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

    // -----------------------------------------------------------------------
    // load_sound_bank: SF2 detection
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_sf2_bank_name() {
        let data = build_minimal_sf2("SoundBankLoaderTest");
        let bank = load_sound_bank(data);
        assert_eq!(bank.sound_bank_info.name, "SoundBankLoaderTest");
    }

    #[test]
    fn test_load_sf2_empty_presets() {
        let data = build_minimal_sf2("Empty");
        let bank = load_sound_bank(data);
        assert!(bank.presets.is_empty());
    }

    #[test]
    fn test_load_sf2_empty_instruments() {
        let data = build_minimal_sf2("Empty");
        let bank = load_sound_bank(data);
        assert!(bank.instruments.is_empty());
    }

    #[test]
    fn test_load_sf2_empty_samples() {
        let data = build_minimal_sf2("Empty");
        let bank = load_sound_bank(data);
        assert!(bank.samples.is_empty());
    }

    #[test]
    fn test_load_sf2_version() {
        let data = build_minimal_sf2("V");
        let bank = load_sound_bank(data);
        assert_eq!(bank.sound_bank_info.version.major, 2);
        assert_eq!(bank.sound_bank_info.version.minor, 4);
    }

    // -----------------------------------------------------------------------
    // load_sound_bank: DLS detection
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_dls_produces_valid_bank() {
        let data = build_minimal_dls();
        let bank = load_sound_bank(data);
        // A minimal DLS with no instruments converts to an empty SF2 bank.
        assert!(bank.presets.is_empty());
        assert!(bank.instruments.is_empty());
        assert!(bank.samples.is_empty());
    }

    #[test]
    fn test_load_dls_version_2_4() {
        let data = build_minimal_dls();
        let bank = load_sound_bank(data);
        assert_eq!(bank.sound_bank_info.version.major, 2);
        assert_eq!(bank.sound_bank_info.version.minor, 4);
    }

    // -----------------------------------------------------------------------
    // Format detection: bytes 8-11 must equal "dls " (case-insensitive)
    // -----------------------------------------------------------------------

    #[test]
    fn test_dls_bytes_8_to_11_detection() {
        let data = build_minimal_dls();
        // Confirm bytes 8-11 of the DLS binary are b"DLS "
        assert_eq!(&data[8..12], b"DLS ");
    }

    #[test]
    fn test_sf2_bytes_8_to_11_detection() {
        let data = build_minimal_sf2("X");
        // Confirm bytes 8-11 of the SF2 binary are b"sfbk"
        assert_eq!(&data[8..12], b"sfbk");
    }

    // -----------------------------------------------------------------------
    // load_dls (internal)
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_dls_returns_basic_sound_bank() {
        let data = build_minimal_dls();
        let bank = load_dls(&data);
        // Successfully returns a BasicSoundBank from DLS data.
        assert_eq!(bank.sound_bank_info.version.major, 2);
    }

    #[test]
    #[should_panic(expected = "Failed to parse DLS sound bank")]
    fn test_load_dls_panics_on_invalid_data() {
        load_dls(b"not a dls file");
    }

    // -----------------------------------------------------------------------
    // Edge case: data shorter than 12 bytes falls through to SF2 parser
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic]
    fn test_load_short_data_panics_in_sf2_parser() {
        // Fewer than 12 bytes: DLS check is skipped, falls through to SF2 parser which panics.
        let _ = load_sound_bank(vec![0u8; 8]);
    }
}
