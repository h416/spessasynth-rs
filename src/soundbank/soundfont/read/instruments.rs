/// instruments.rs
/// purpose: SoundFont instrument struct and reader.
/// Ported from: src/soundbank/soundfont/read/instruments.ts
use crate::soundbank::basic_soundbank::basic_instrument::BasicInstrument;
use crate::soundbank::basic_soundbank::basic_instrument_zone::BasicInstrumentZone;
use crate::soundbank::basic_soundbank::basic_zone::BasicZone;
use crate::soundbank::soundfont::read::instrument_zones::SoundFontInstrumentZoneSink;
use crate::utils::little_endian::read_little_endian_indexed;
use crate::utils::riff_chunk::RIFFChunk;
use crate::utils::string::read_binary_string_indexed;

// ---------------------------------------------------------------------------
// SoundFontInstrument
// ---------------------------------------------------------------------------

/// A SoundFont instrument record, extending BasicInstrument with zone bag indexing.
/// Equivalent to: class SoundFontInstrument extends BasicInstrument
#[derive(Clone, Debug)]
pub struct SoundFontInstrument {
    /// Base instrument data (name, zones, global zone, linked_to).
    pub instrument: BasicInstrument,

    /// Index of the first instrument bag (zone) entry for this instrument in the IBAG chunk.
    /// Equivalent to: public zoneStartIndex: number
    pub zone_start_index: usize,

    /// Number of instrument bag entries (zones) for this instrument.
    /// Computed as the difference between the next instrument's zoneStartIndex and this one's.
    /// Equivalent to: public zonesCount = 0
    pub zones_count: usize,
}

impl SoundFontInstrument {
    /// Creates a SoundFontInstrument by reading 22 bytes from a chunk:
    /// - 20 bytes: instrument name (null-padded ASCII)
    /// - 2 bytes:  zone start index (little-endian WORD)
    ///
    /// Equivalent to: constructor(instrumentChunk: RIFFChunk)
    pub fn new(chunk: &mut RIFFChunk) -> Self {
        let name = read_binary_string_indexed(&mut chunk.data, 20);
        let zone_start_index = read_little_endian_indexed(&mut chunk.data, 2) as usize;

        let mut instrument = BasicInstrument::new();
        instrument.name = name;

        SoundFontInstrument {
            instrument,
            zone_start_index,
            zones_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// SoundFontInstrumentZoneSink impl
// ---------------------------------------------------------------------------

impl SoundFontInstrumentZoneSink for SoundFontInstrument {
    /// Returns the number of bag entries (zones) for this instrument.
    /// Used by `apply_instrument_zones` to iterate the correct number of zones.
    /// Equivalent to: instrument.zonesCount
    fn zones_count(&self) -> usize {
        self.zones_count
    }

    /// Appends a parsed instrument zone to the underlying BasicInstrument.
    /// Equivalent to: instrument.zones.push(zone)  (via createSoundFontZone)
    fn push_zone(&mut self, zone: BasicInstrumentZone) {
        self.instrument.zones.push(zone);
    }

    /// Returns a mutable reference to the instrument's global zone.
    /// Equivalent to: instrument.globalZone
    fn global_zone_mut(&mut self) -> &mut BasicZone {
        &mut self.instrument.global_zone
    }
}

// ---------------------------------------------------------------------------
// read_instruments
// ---------------------------------------------------------------------------

/// Reads all SoundFont instruments from an INST sub-chunk.
/// The last entry in the chunk is the EOI (End Of Instruments) sentinel and is discarded.
///
/// `zones_count` for each instrument is calculated as the difference between
/// the next instrument's `zone_start_index` and the current one's.
///
/// Equivalent to: function readInstruments(instrumentChunk: RIFFChunk): SoundFontInstrument[]
pub fn read_instruments(chunk: &mut RIFFChunk) -> Vec<SoundFontInstrument> {
    let mut instruments: Vec<SoundFontInstrument> = Vec::new();

    while chunk.data.len() > chunk.data.current_index {
        let instrument = SoundFontInstrument::new(chunk);

        // Set the previous instrument's zones_count using the current instrument's zone_start_index.
        // Equivalent to: previous.zonesCount = instrument.zoneStartIndex - previous.zoneStartIndex
        if let Some(previous) = instruments.last_mut() {
            previous.zones_count = instrument.zone_start_index - previous.zone_start_index;
        }

        instruments.push(instrument);
    }

    // Remove EOI (End Of Instruments sentinel record)
    instruments.pop();

    instruments
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::basic_instrument_zone::BasicInstrumentZone;
    use crate::utils::indexed_array::IndexedByteArray;

    // ── helpers ─────────────────────────────────────────────────────────────

    /// Builds a raw 22-byte INST record: 20-byte name + 2-byte zone start index.
    fn make_inst_bytes(name: &str, zone_start: u16) -> Vec<u8> {
        let mut bytes = vec![0u8; 22];
        let name_bytes = name.as_bytes();
        let copy_len = name_bytes.len().min(20);
        bytes[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
        let zs = zone_start.to_le_bytes();
        bytes[20] = zs[0];
        bytes[21] = zs[1];
        bytes
    }

    /// Wraps raw bytes into a RIFFChunk for testing.
    fn make_chunk(data: Vec<u8>) -> RIFFChunk {
        let len = data.len();
        RIFFChunk::new(
            "inst".to_string(),
            len as u32,
            IndexedByteArray::from_vec(data),
        )
    }

    // ── SoundFontInstrument::new ─────────────────────────────────────────────

    #[test]
    fn test_new_reads_name() {
        let mut chunk = make_chunk(make_inst_bytes("Piano", 0));
        let inst = SoundFontInstrument::new(&mut chunk);
        assert_eq!(inst.instrument.name, "Piano");
    }

    #[test]
    fn test_new_null_padded_name_stops_at_null() {
        let mut chunk = make_chunk(make_inst_bytes("AB", 0));
        let inst = SoundFontInstrument::new(&mut chunk);
        assert_eq!(inst.instrument.name, "AB");
    }

    #[test]
    fn test_new_reads_zone_start_index() {
        let mut chunk = make_chunk(make_inst_bytes("Piano", 42));
        let inst = SoundFontInstrument::new(&mut chunk);
        assert_eq!(inst.zone_start_index, 42);
    }

    #[test]
    fn test_new_reads_zone_start_index_large() {
        let mut chunk = make_chunk(make_inst_bytes("Organ", 0xABCD));
        let inst = SoundFontInstrument::new(&mut chunk);
        assert_eq!(inst.zone_start_index, 0xABCD);
    }

    #[test]
    fn test_new_zones_count_is_zero() {
        let mut chunk = make_chunk(make_inst_bytes("Piano", 5));
        let inst = SoundFontInstrument::new(&mut chunk);
        assert_eq!(inst.zones_count, 0);
    }

    #[test]
    fn test_new_base_instrument_zones_empty() {
        let mut chunk = make_chunk(make_inst_bytes("Piano", 0));
        let inst = SoundFontInstrument::new(&mut chunk);
        assert!(inst.instrument.zones.is_empty());
    }

    #[test]
    fn test_new_advances_cursor_by_22() {
        let mut chunk = make_chunk(make_inst_bytes("Piano", 0));
        SoundFontInstrument::new(&mut chunk);
        assert_eq!(chunk.data.current_index, 22);
    }

    #[test]
    fn test_new_sequential_reads_advance_cursor() {
        let mut data = make_inst_bytes("Piano", 0);
        data.extend(make_inst_bytes("Organ", 3));
        let mut chunk = make_chunk(data);
        SoundFontInstrument::new(&mut chunk);
        assert_eq!(chunk.data.current_index, 22);
        SoundFontInstrument::new(&mut chunk);
        assert_eq!(chunk.data.current_index, 44);
    }

    // ── read_instruments ─────────────────────────────────────────────────────

    #[test]
    fn test_read_instruments_empty_chunk_returns_empty() {
        let mut chunk = make_chunk(vec![]);
        let instruments = read_instruments(&mut chunk);
        assert!(instruments.is_empty());
    }

    #[test]
    fn test_read_instruments_single_record_is_eoi_returns_empty() {
        // A single record = EOI sentinel → removed → empty result
        let mut chunk = make_chunk(make_inst_bytes("EOS", 0));
        let instruments = read_instruments(&mut chunk);
        assert!(instruments.is_empty());
    }

    #[test]
    fn test_read_instruments_one_instrument_plus_eoi() {
        let mut data = make_inst_bytes("Piano", 0);
        data.extend(make_inst_bytes("EOS", 3)); // EOI
        let mut chunk = make_chunk(data);
        let instruments = read_instruments(&mut chunk);
        assert_eq!(instruments.len(), 1);
        assert_eq!(instruments[0].instrument.name, "Piano");
    }

    #[test]
    fn test_read_instruments_eoi_not_in_result() {
        let mut data = make_inst_bytes("Piano", 0);
        data.extend(make_inst_bytes("EOS", 0));
        let mut chunk = make_chunk(data);
        let instruments = read_instruments(&mut chunk);
        assert_eq!(instruments.len(), 1);
        assert_ne!(instruments[0].instrument.name, "EOS");
    }

    #[test]
    fn test_read_instruments_sets_zones_count() {
        // Piano: zone_start=0, Organ: zone_start=3 → Piano.zones_count=3
        // Organ: zone_start=3, EOI: zone_start=8  → Organ.zones_count=5
        let mut data = make_inst_bytes("Piano", 0);
        data.extend(make_inst_bytes("Organ", 3));
        data.extend(make_inst_bytes("EOS", 8));
        let mut chunk = make_chunk(data);
        let instruments = read_instruments(&mut chunk);
        assert_eq!(instruments.len(), 2);
        assert_eq!(instruments[0].zones_count, 3);
        assert_eq!(instruments[1].zones_count, 5);
    }

    #[test]
    fn test_read_instruments_zones_count_zero_when_same_start() {
        // Piano and Organ share the same zone_start=5 → Piano.zones_count=0
        let mut data = make_inst_bytes("Piano", 5);
        data.extend(make_inst_bytes("Organ", 5));
        data.extend(make_inst_bytes("EOS", 5));
        let mut chunk = make_chunk(data);
        let instruments = read_instruments(&mut chunk);
        assert_eq!(instruments[0].zones_count, 0);
    }

    #[test]
    fn test_read_instruments_multiple() {
        let mut data = make_inst_bytes("Piano", 0);
        data.extend(make_inst_bytes("Organ", 5));
        data.extend(make_inst_bytes("Guitar", 10));
        data.extend(make_inst_bytes("EOS", 15));
        let mut chunk = make_chunk(data);
        let instruments = read_instruments(&mut chunk);
        assert_eq!(instruments.len(), 3);
        assert_eq!(instruments[0].instrument.name, "Piano");
        assert_eq!(instruments[1].instrument.name, "Organ");
        assert_eq!(instruments[2].instrument.name, "Guitar");
    }

    #[test]
    fn test_read_instruments_multiple_zone_counts() {
        // Piano: 0..5=5, Organ: 5..10=5, Guitar: 10..15=5
        let mut data = make_inst_bytes("Piano", 0);
        data.extend(make_inst_bytes("Organ", 5));
        data.extend(make_inst_bytes("Guitar", 10));
        data.extend(make_inst_bytes("EOS", 15));
        let mut chunk = make_chunk(data);
        let instruments = read_instruments(&mut chunk);
        assert_eq!(instruments[0].zones_count, 5);
        assert_eq!(instruments[1].zones_count, 5);
        assert_eq!(instruments[2].zones_count, 5);
    }

    // ── SoundFontInstrumentZoneSink impl ─────────────────────────────────────

    #[test]
    fn test_zones_count_trait_reflects_field() {
        let mut chunk = make_chunk(make_inst_bytes("Piano", 0));
        let mut inst = SoundFontInstrument::new(&mut chunk);
        inst.zones_count = 7;
        assert_eq!(inst.zones_count(), 7);
    }

    #[test]
    fn test_push_zone_appends_to_instrument_zones() {
        let mut chunk = make_chunk(make_inst_bytes("Piano", 0));
        let mut inst = SoundFontInstrument::new(&mut chunk);
        let zone = BasicInstrumentZone::new(0, 0, 0);
        inst.push_zone(zone);
        assert_eq!(inst.instrument.zones.len(), 1);
    }

    #[test]
    fn test_push_zone_multiple_appends() {
        let mut chunk = make_chunk(make_inst_bytes("Piano", 0));
        let mut inst = SoundFontInstrument::new(&mut chunk);
        inst.push_zone(BasicInstrumentZone::new(0, 0, 0));
        inst.push_zone(BasicInstrumentZone::new(0, 0, 1));
        assert_eq!(inst.instrument.zones.len(), 2);
    }

    #[test]
    fn test_push_zone_stores_correct_sample_idx() {
        let mut chunk = make_chunk(make_inst_bytes("Piano", 0));
        let mut inst = SoundFontInstrument::new(&mut chunk);
        inst.push_zone(BasicInstrumentZone::new(0, 0, 42));
        assert_eq!(inst.instrument.zones[0].sample_idx, 42);
    }

    #[test]
    fn test_global_zone_mut_initially_empty() {
        let mut chunk = make_chunk(make_inst_bytes("Piano", 0));
        let mut inst = SoundFontInstrument::new(&mut chunk);
        let global = inst.global_zone_mut();
        assert!(global.generators.is_empty());
        assert!(global.modulators.is_empty());
    }

    #[test]
    fn test_global_zone_mut_can_add_generators() {
        use crate::soundbank::basic_soundbank::generator::Generator;
        use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
        let mut chunk = make_chunk(make_inst_bytes("Piano", 0));
        let mut inst = SoundFontInstrument::new(&mut chunk);
        inst.global_zone_mut()
            .add_generators(&[Generator::new_unvalidated(gt::PAN, 50.0)]);
        assert_eq!(inst.instrument.global_zone.generators.len(), 1);
    }
}
