/// basic_preset.rs
/// purpose: BasicPreset struct - an SF2 preset with zones, a global zone,
///          bank/program assignment, and voice parameter computation.
/// Ported from: src/soundbank/basic_soundbank/basic_preset.ts
///
/// # TypeScript vs Rust design differences
///
/// In TypeScript, `parentSoundBank: BasicSoundBank` is held as a field and
/// referenced inside `isXGDrums` / `isAnyDrums` / `getVoiceParameters`.
///
/// In Rust, to avoid circular ownership, `parentSoundBank` is not stored as a field.
/// Instead, the necessary information is passed as method arguments:
///
/// - `is_xg_drums(is_xg_bank: bool)` — receives `parentSoundBank.isXGBank` as a parameter
/// - `get_voice_parameters(midi_note, velocity, instruments, default_modulators)`
///   — receives `parentSoundBank.defaultModulators` and `instruments` as parameters
use std::fmt;

use crate::soundbank::basic_soundbank::basic_instrument::BasicInstrument;
use crate::soundbank::basic_soundbank::basic_instrument_zone::BasicInstrumentZone;
use crate::soundbank::basic_soundbank::basic_preset_zone::BasicPresetZone;
use crate::soundbank::basic_soundbank::basic_zone::BasicZone;
use crate::soundbank::basic_soundbank::generator::Generator;
use crate::soundbank::basic_soundbank::generator_types::{
    DEFAULT_GENERATOR_VALUES, GENERATOR_LIMITS, GENERATORS_AMOUNT,
    generator_types as gt,
};
use crate::soundbank::basic_soundbank::midi_patch::{
    MidiPatch, to_midi_string, to_named_midi_string,
};
use crate::soundbank::basic_soundbank::modulator::Modulator;
use crate::soundbank::soundfont::write::types::ExtendedSF2Chunks;
use crate::soundbank::types::{GenericRange, VoiceParameters};
use crate::utils::little_endian::{write_dword, write_word};
use crate::utils::midi_hacks::BankSelectHacks;
use crate::utils::string::write_binary_string_indexed;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// SF2 PHDR record size in bytes.
/// Equivalent to: export const PHDR_BYTE_SIZE = 38
pub const PHDR_BYTE_SIZE: usize = 38;

// ---------------------------------------------------------------------------
// Module-private helpers
// ---------------------------------------------------------------------------

/// Returns true if `number` is within `[range.min, range.max]`.
/// Equivalent to: private static isInRange(range, number)
#[inline]
fn is_in_range(range: &GenericRange, number: f64) -> bool {
    number >= range.min && number <= range.max
}

/// Appends modulators from `adder` to `main`, skipping duplicates.
/// Two modulators are considered identical if `Modulator::is_identical` returns true (checkAmount = false).
/// Equivalent to: private static addUniqueModulators(main, adder)
fn add_unique_modulators(main: &mut Vec<Modulator>, adder: &[Modulator]) {
    for added_mod in adder {
        if !main
            .iter()
            .any(|mm| Modulator::is_identical(added_mod, mm, false))
        {
            main.push(added_mod.clone());
        }
    }
}

/// Returns the intersection of two ranges: `[max(r1.min, r2.min), min(r1.max, r2.max)]`.
/// Equivalent to: private static subtractRanges(r1, r2)
#[inline]
fn subtract_ranges(r1: &GenericRange, r2: &GenericRange) -> GenericRange {
    GenericRange {
        min: r1.min.max(r2.min),
        max: r1.max.min(r2.max),
    }
}

// ---------------------------------------------------------------------------
// BasicPreset
// ---------------------------------------------------------------------------

/// Represents a single SF2 preset (program + bank + zones).
/// Equivalent to: class BasicPreset implements MIDIPatchNamed
#[derive(Clone, Debug)]
pub struct BasicPreset {
    /// Preset name.
    /// Equivalent to: public name = ""
    pub name: String,

    /// MIDI program number (0–127).
    /// Equivalent to: public program = 0
    pub program: u8,

    /// Bank MSB (0–127).
    /// Equivalent to: public bankMSB = 0
    pub bank_msb: u8,

    /// Bank LSB (0–127).
    /// Equivalent to: public bankLSB = 0
    pub bank_lsb: u8,

    /// True if this preset is a GM/GS drum preset.
    /// Equivalent to: public isGMGSDrum = false
    pub is_gm_gs_drum: bool,

    /// Preset zones (non-global).
    /// Equivalent to: public zones: BasicPresetZone[] = []
    pub zones: Vec<BasicPresetZone>,

    /// Global zone (generators/modulators applied to all zones).
    /// `BasicGlobalZone` is a type alias for `BasicZone`.
    /// Equivalent to: public readonly globalZone: BasicGlobalZone
    pub global_zone: BasicZone,

    /// Unused SF2 metadata.
    /// Equivalent to: public library = 0
    pub library: u32,

    /// Unused SF2 metadata.
    /// Equivalent to: public genre = 0
    pub genre: u32,

    /// Unused SF2 metadata.
    /// Equivalent to: public morphology = 0
    pub morphology: u32,
}

impl Default for BasicPreset {
    fn default() -> Self {
        Self {
            name: String::new(),
            program: 0,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
            zones: Vec::new(),
            global_zone: BasicZone::new(),
            library: 0,
            genre: 0,
            morphology: 0,
        }
    }
}

impl BasicPreset {
    /// Creates a new, empty BasicPreset.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a BasicPreset with the given name.
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    // -----------------------------------------------------------------------
    // Drum detection
    // -----------------------------------------------------------------------

    /// Returns true if this is an XG drum preset.
    /// Equivalent to: public get isXGDrums()
    pub fn is_xg_drums(&self, is_xg_bank: bool) -> bool {
        is_xg_bank && BankSelectHacks::is_xg_drums(self.bank_msb)
    }

    /// Returns true if this is any kind of drum preset (GM/GS or XG).
    /// Equivalent to: public get isAnyDrums()
    pub fn is_any_drums(&self, is_xg_bank: bool) -> bool {
        self.is_gm_gs_drum || (is_xg_bank && BankSelectHacks::is_xg_drums(self.bank_msb))
    }

    // -----------------------------------------------------------------------
    // matches / to_midi_string
    // -----------------------------------------------------------------------

    /// Returns true if this preset matches the given MIDI patch.
    /// Equivalent to: public matches(preset: MIDIPatch)
    pub fn matches(&self, patch: &MidiPatch) -> bool {
        let self_patch = MidiPatch {
            program: self.program,
            bank_msb: self.bank_msb,
            bank_lsb: self.bank_lsb,
            is_gm_gs_drum: self.is_gm_gs_drum,
        };
        crate::soundbank::basic_soundbank::midi_patch::matches(&self_patch, patch)
    }

    /// Returns the MIDI string representation (bankLSB:bankMSB:program or DRUM:program).
    /// Equivalent to: public toMIDIString()
    pub fn to_midi_string(&self) -> String {
        to_midi_string(&MidiPatch {
            program: self.program,
            bank_msb: self.bank_msb,
            bank_lsb: self.bank_lsb,
            is_gm_gs_drum: self.is_gm_gs_drum,
        })
    }

    // -----------------------------------------------------------------------
    // delete / deleteZone / createZone
    // -----------------------------------------------------------------------

    /// Unlinks all zones' instruments from this preset.
    /// Equivalent to: public delete()
    pub fn delete(&self, preset_idx: usize, instruments: &mut [BasicInstrument]) {
        for z in &self.zones {
            if let Some(instrument) = instruments.get_mut(z.instrument_idx) {
                instrument.unlink_from(preset_idx);
            }
        }
    }

    /// Removes the zone at `index`, unlinking its instrument from this preset.
    /// Equivalent to: public deleteZone(index: number)
    pub fn delete_zone(
        &mut self,
        index: usize,
        preset_idx: usize,
        instruments: &mut [BasicInstrument],
    ) {
        if index < self.zones.len() {
            let instr_idx = self.zones[index].instrument_idx;
            if let Some(instrument) = instruments.get_mut(instr_idx) {
                instrument.unlink_from(preset_idx);
            }
            self.zones.remove(index);
        }
    }

    /// Creates a new preset zone for `instrument_idx`, links the instrument,
    /// and returns the index of the newly created zone.
    /// Equivalent to: public createZone(instrument: BasicInstrument): BasicPresetZone
    pub fn create_zone(
        &mut self,
        preset_idx: usize,
        instrument_idx: usize,
        instruments: &mut [BasicInstrument],
    ) -> usize {
        let zone = BasicPresetZone::new(preset_idx, instrument_idx);
        self.zones.push(zone);
        if let Some(instrument) = instruments.get_mut(instrument_idx) {
            instrument.link_to(preset_idx);
        }
        self.zones.len() - 1
    }

    // -----------------------------------------------------------------------
    // get_voice_parameters
    // -----------------------------------------------------------------------

    /// Returns the voice synthesis data for this preset at the given MIDI note and velocity.
    ///
    /// # Parameters
    /// - `midi_note`: MIDI note number (0–127)
    /// - `velocity`: MIDI velocity (0–127)
    /// - `instruments`: The soundbank's instrument list
    /// - `default_modulators`: The soundbank's default modulator list
    ///   (= `parentSoundBank.defaultModulators`)
    ///
    /// Equivalent to: public getVoiceParameters(midiNote, velocity): VoiceParameters[]
    pub fn get_voice_parameters(
        &self,
        midi_note: u8,
        velocity: u8,
        instruments: &[BasicInstrument],
        default_modulators: &[Modulator],
    ) -> Vec<VoiceParameters> {
        let midi_note_f = midi_note as f64;
        let velocity_f = velocity as f64;
        let mut voice_params: Vec<VoiceParameters> = Vec::new();

        for preset_zone in &self.zones {
            // ── Key/vel range filter (preset level) ──────────────────────────
            let pz_key_range = if preset_zone.zone.has_key_range() {
                &preset_zone.zone.key_range
            } else {
                &self.global_zone.key_range
            };
            let pz_vel_range = if preset_zone.zone.has_vel_range() {
                &preset_zone.zone.vel_range
            } else {
                &self.global_zone.vel_range
            };

            if !is_in_range(pz_key_range, midi_note_f) || !is_in_range(pz_vel_range, velocity_f) {
                continue;
            }

            // ── Instrument lookup ─────────────────────────────────────────
            let instrument = match instruments.get(preset_zone.instrument_idx) {
                Some(i) if !i.zones.is_empty() => i,
                _ => continue,
            };
            // ── Preset generators (offsets) ───────────────────────────────
            // First global, then local (local overrides).
            let mut preset_generators = [0i16; GENERATORS_AMOUNT];
            for gn in &self.global_zone.generators {
                if gn.generator_type >= 0 && (gn.generator_type as usize) < GENERATORS_AMOUNT {
                    preset_generators[gn.generator_type as usize] = gn.generator_value;
                }
            }
            for gn in &preset_zone.zone.generators {
                if gn.generator_type >= 0 && (gn.generator_type as usize) < GENERATORS_AMOUNT {
                    preset_generators[gn.generator_type as usize] = gn.generator_value;
                }
            }

            // ── Preset modulators (local + unique globals) ────────────────
            let mut preset_modulators: Vec<Modulator> = preset_zone
                .zone
                .modulators
                .iter()
                .map(Modulator::copy_from)
                .collect();
            add_unique_modulators(&mut preset_modulators, &self.global_zone.modulators);

            // ── Instrument zone loop ──────────────────────────────────────
            for inst_zone in &instrument.zones {
                // Key/vel range filter (instrument level)
                let iz_key_range = if inst_zone.zone.has_key_range() {
                    &inst_zone.zone.key_range
                } else {
                    &instrument.global_zone.key_range
                };
                let iz_vel_range = if inst_zone.zone.has_vel_range() {
                    &inst_zone.zone.vel_range
                } else {
                    &instrument.global_zone.vel_range
                };

                if !is_in_range(iz_key_range, midi_note_f) || !is_in_range(iz_vel_range, velocity_f)
                {
                    continue;
                }

                // ── Build modulator list ──────────────────────────────────
                // Start with local zone modulators, add unique from inst global zone,
                // then add unique default modulators.
                let mut modulators: Vec<Modulator> = inst_zone
                    .zone
                    .modulators
                    .iter()
                    .map(Modulator::copy_from)
                    .collect();
                add_unique_modulators(&mut modulators, &instrument.global_zone.modulators);
                add_unique_modulators(&mut modulators, default_modulators);

                // Sum preset modulators into instrument modulators (SF2 spec §9.5).
                for preset_mod in &preset_modulators {
                    match modulators
                        .iter()
                        .position(|m| Modulator::is_identical(preset_mod, m, false))
                    {
                        Some(pos) => {
                            // Sum the transform amounts (creates a new modulator).
                            modulators[pos] = modulators[pos].sum_transform(preset_mod);
                        }
                        None => {
                            modulators.push(preset_mod.clone());
                        }
                    }
                }

                // ── Build generator array ─────────────────────────────────
                // Start from defaults, override with global zone, override with local.
                let mut generators = DEFAULT_GENERATOR_VALUES;
                for gn in &instrument.global_zone.generators {
                    if gn.generator_type >= 0 && (gn.generator_type as usize) < GENERATORS_AMOUNT {
                        generators[gn.generator_type as usize] = gn.generator_value;
                    }
                }
                for gn in &inst_zone.zone.generators {
                    if gn.generator_type >= 0 && (gn.generator_type as usize) < GENERATORS_AMOUNT {
                        generators[gn.generator_type as usize] = gn.generator_value;
                    }
                }

                // Sum preset generator offsets (clamped to i16 range).
                for i in 0..GENERATORS_AMOUNT {
                    let sum = generators[i] as i32 + preset_generators[i] as i32;
                    generators[i] = sum.clamp(-32_768, 32_767) as i16;
                }

                // EMU initial attenuation correction: multiply by 0.4.
                // All EMU sound cards have this quirk; all SF2 players emulate it.
                let ia_idx = gt::INITIAL_ATTENUATION as usize;
                generators[ia_idx] = (generators[ia_idx] as f64 * 0.4).floor() as i16;

                voice_params.push(VoiceParameters {
                    generators,
                    modulators,
                    sample_idx: inst_zone.sample_idx,
                });
            }
        }

        voice_params
    }

    // -----------------------------------------------------------------------
    // to_flattened_instrument
    // -----------------------------------------------------------------------

    /// Combines this preset into a flat instrument, merging preset and instrument layers.
    /// Used for DLS conversion.
    ///
    /// # Notes
    /// - The returned instrument has no index in a soundbank; sample back-references
    ///   (via `BasicSample::linked_to`) are NOT maintained.
    /// - The caller is responsible for properly inserting the instrument into a
    ///   soundbank and updating sample links if required.
    ///
    /// Equivalent to: public toFlattenedInstrument(): BasicInstrument
    pub fn to_flattened_instrument(&self, instruments: &[BasicInstrument]) -> BasicInstrument {
        let mut output_instrument = BasicInstrument::with_name(self.name.clone());

        // Collect global preset generators / modulators / ranges.
        let global_preset_generators: Vec<Generator> =
            self.global_zone.generators.to_vec();
        let global_preset_modulators: Vec<Modulator> =
            self.global_zone.modulators.to_vec();
        let global_preset_key_range = self.global_zone.key_range.clone();
        let global_preset_vel_range = self.global_zone.vel_range.clone();

        for preset_zone in &self.zones {
            let instrument = match instruments.get(preset_zone.instrument_idx) {
                Some(i) => i,
                None => panic!(
                    "Preset '{}': zone references non-existent instrument index {}",
                    self.name, preset_zone.instrument_idx
                ),
            };

            // Effective preset zone ranges (use global if not set locally).
            let pz_key_range = if preset_zone.zone.has_key_range() {
                preset_zone.zone.key_range.clone()
            } else {
                global_preset_key_range.clone()
            };
            let pz_vel_range = if preset_zone.zone.has_vel_range() {
                preset_zone.zone.vel_range.clone()
            } else {
                global_preset_vel_range.clone()
            };

            // Build preset generators (local + unique globals).
            let mut preset_generators: Vec<Generator> =
                preset_zone.zone.generators.to_vec();
            add_unique_generators(&mut preset_generators, &global_preset_generators);

            // Build preset modulators (local + unique globals).
            let mut preset_modulators: Vec<Modulator> =
                preset_zone.zone.modulators.to_vec();
            add_unique_modulators(&mut preset_modulators, &global_preset_modulators);

            // Collect global instrument generators / modulators / ranges.
            let global_inst_generators: Vec<Generator> =
                instrument.global_zone.generators.to_vec();
            let global_inst_modulators: Vec<Modulator> =
                instrument.global_zone.modulators.to_vec();
            let global_inst_key_range = instrument.global_zone.key_range.clone();
            let global_inst_vel_range = instrument.global_zone.vel_range.clone();

            for inst_zone in &instrument.zones {
                // Effective instrument zone ranges.
                let iz_key_range = if inst_zone.zone.has_key_range() {
                    inst_zone.zone.key_range.clone()
                } else {
                    global_inst_key_range.clone()
                };
                let iz_vel_range = if inst_zone.zone.has_vel_range() {
                    inst_zone.zone.vel_range.clone()
                } else {
                    global_inst_vel_range.clone()
                };

                // Intersect with preset zone ranges.
                let final_key_range = subtract_ranges(&iz_key_range, &pz_key_range);
                let final_vel_range = subtract_ranges(&iz_vel_range, &pz_vel_range);

                // Discard if ranges are inverted (no overlap).
                if final_key_range.max < final_key_range.min
                    || final_vel_range.max < final_vel_range.min
                {
                    continue;
                }

                // Build instrument generators (local + unique globals).
                let mut inst_generators: Vec<Generator> =
                    inst_zone.zone.generators.to_vec();
                add_unique_generators(&mut inst_generators, &global_inst_generators);

                // Build instrument modulators (local + unique globals).
                let mut inst_modulators: Vec<Modulator> =
                    inst_zone.zone.modulators.to_vec();
                add_unique_modulators(&mut inst_modulators, &global_inst_modulators);

                // Sum preset modulators (amounts) into the final modulator list.
                let mut final_mod_list: Vec<Modulator> = inst_modulators.clone();
                for mod_ in &preset_modulators {
                    match final_mod_list
                        .iter()
                        .position(|m| Modulator::is_identical(mod_, m, false))
                    {
                        Some(pos) => {
                            final_mod_list[pos] = final_mod_list[pos].sum_transform(mod_);
                        }
                        None => {
                            final_mod_list.push(mod_.clone());
                        }
                    }
                }

                // Build final generator list starting from instrument generators.
                let mut final_gen_list: Vec<Generator> = inst_generators.to_vec();
                for gn in &preset_generators {
                    // Skip these types: they belong to a different layer.
                    let gt = gn.generator_type;
                    if gt == gt::VEL_RANGE
                        || gt == gt::KEY_RANGE
                        || gt == gt::INSTRUMENT
                        || gt == gt::END_OPER
                        || gt == gt::SAMPLE_MODES
                    {
                        continue;
                    }
                    match inst_generators.iter().position(|g| g.generator_type == gt) {
                        None => {
                            // Sum to the default value.
                            let def = GENERATOR_LIMITS
                                .get(gt as usize)
                                .and_then(|l| *l)
                                .map(|l| l.def)
                                .unwrap_or(0);
                            let new_amount = def + gn.generator_value as i32;
                            final_gen_list.push(Generator::new(gt, new_amount as f64));
                        }
                        Some(pos) => {
                            let new_amount = final_gen_list[pos].generator_value as i32
                                + gn.generator_value as i32;
                            final_gen_list[pos] = Generator::new(gt, new_amount as f64);
                        }
                    }
                }

                // Remove unwanted generators and those equal to their default value.
                final_gen_list.retain(|g| {
                    let gt = g.generator_type;
                    if gt == gt::SAMPLE_ID
                        || gt == gt::KEY_RANGE
                        || gt == gt::VEL_RANGE
                        || gt == gt::END_OPER
                        || gt == gt::INSTRUMENT
                    {
                        return false;
                    }
                    let def = GENERATOR_LIMITS
                        .get(gt as usize)
                        .and_then(|l| *l)
                        .map(|l| l.def);
                    match def {
                        Some(d) => g.generator_value as i32 != d,
                        None => true, // No limit defined; keep the generator
                    }
                });

                // Create the zone in the output instrument.
                // NOTE: sample back-references are NOT maintained here (no soundbank index).
                let mut zone = BasicInstrumentZone::new(usize::MAX, 0, inst_zone.sample_idx);

                // Set ranges (clear "full range" back to default).
                let mut key_range = final_key_range;
                let mut vel_range = final_vel_range;
                if key_range.min == 0.0 && key_range.max == 127.0 {
                    key_range.min = -1.0;
                }
                if vel_range.min == 0.0 && vel_range.max == 127.0 {
                    vel_range.min = -1.0;
                }
                zone.zone.key_range = key_range;
                zone.zone.vel_range = vel_range;

                zone.zone.add_generators(&final_gen_list);
                zone.zone.add_modulators(&final_mod_list);

                output_instrument.zones.push(zone);
            }
        }

        output_instrument
    }

    // -----------------------------------------------------------------------
    // write
    // -----------------------------------------------------------------------

    /// Writes this preset's PHDR record to `phdr_data`.
    /// `index` is the starting bag (zone) index for this preset.
    /// Equivalent to: public write(phdrData: ExtendedSF2Chunks, index: number)
    pub fn write(&self, phdr_data: &mut ExtendedSF2Chunks, index: usize) {
        // Name: first 20 chars to pdta, next 20 chars to xdta.
        let first_20: String = self.name.chars().take(20).collect();
        let rest: String = self.name.chars().skip(20).collect();
        write_binary_string_indexed(&mut phdr_data.pdta, &first_20, 20);
        write_binary_string_indexed(&mut phdr_data.xdta, &rest, 20);

        // Program number.
        write_word(&mut phdr_data.pdta, self.program as u32);

        // Bank number.
        let w_bank: u32 = if self.is_gm_gs_drum {
            0x80 // Drum flag
        } else if self.bank_msb == 0 {
            self.bank_lsb as u32 // Write LSB when MSB is zero (XG)
        } else {
            self.bank_msb as u32
        };
        write_word(&mut phdr_data.pdta, w_bank);

        // Skip wBank + wProgram fields in xdta (4 bytes).
        phdr_data.xdta.current_index += 4;

        // Bag start index: low 16 bits to pdta, high 16 bits to xdta.
        write_word(&mut phdr_data.pdta, (index & 0xFFFF) as u32);
        write_word(&mut phdr_data.xdta, (index >> 16) as u32);

        // 3 unused DWORDs (spec requires them to be present).
        write_dword(&mut phdr_data.pdta, self.library);
        write_dword(&mut phdr_data.pdta, self.genre);
        write_dword(&mut phdr_data.pdta, self.morphology);

        // Skip corresponding 12 bytes in xdta.
        phdr_data.xdta.current_index += 12;
    }
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

impl fmt::Display for BasicPreset {
    /// Equivalent to: public toString()
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let patch = MidiPatch {
            program: self.program,
            bank_msb: self.bank_msb,
            bank_lsb: self.bank_lsb,
            is_gm_gs_drum: self.is_gm_gs_drum,
        };
        let named = crate::soundbank::basic_soundbank::midi_patch::MidiPatchNamed {
            patch,
            name: self.name.clone(),
        };
        write!(f, "{}", to_named_midi_string(&named))
    }
}

// ---------------------------------------------------------------------------
// Module-private: add_unique_generators
// ---------------------------------------------------------------------------

/// Appends generators from `adder` to `main`, skipping types that already exist.
/// Equivalent to: local `addUnique(main, adder)` in `toFlattenedInstrument`
fn add_unique_generators(main: &mut Vec<Generator>, adder: &[Generator]) {
    for gn in adder {
        if !main.iter().any(|mg| mg.generator_type == gn.generator_type) {
            main.push(gn.clone());
        }
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
    use crate::soundbank::basic_soundbank::generator_types::{
        GENERATORS_AMOUNT, generator_types as gt,
    };
    use crate::soundbank::basic_soundbank::modulator::Modulator;
    use crate::soundbank::soundfont::write::types::ExtendedSF2Chunks;
    use crate::utils::indexed_array::IndexedByteArray;

    // ── helpers ─────────────────────────────────────────────────────────────

    fn make_chunks() -> ExtendedSF2Chunks {
        ExtendedSF2Chunks {
            pdta: IndexedByteArray::new(64),
            xdta: IndexedByteArray::new(64),
        }
    }

    fn make_sample() -> BasicSample {
        BasicSample::new(
            "test".to_string(),
            44100,
            60,
            0,
            crate::soundbank::enums::sample_types::MONO_SAMPLE,
            0,
            0,
        )
    }

    /// Creates a minimal instrument with one zone covering the full key/vel range.
    fn make_instrument_with_zone(sample_idx: usize) -> BasicInstrument {
        let mut inst = BasicInstrument::with_name("TestInst");
        let zone = BasicInstrumentZone::new(0, 0, sample_idx);
        // Zone covers full range (key: -1..127, vel: -1..127 = all notes)
        inst.zones.push(zone);
        inst
    }

    // ── new / default ────────────────────────────────────────────────────────

    #[test]
    fn test_new_name_empty() {
        let p = BasicPreset::new();
        assert_eq!(p.name, "");
    }

    #[test]
    fn test_new_program_zero() {
        let p = BasicPreset::new();
        assert_eq!(p.program, 0);
    }

    #[test]
    fn test_new_bank_msb_zero() {
        let p = BasicPreset::new();
        assert_eq!(p.bank_msb, 0);
    }

    #[test]
    fn test_new_bank_lsb_zero() {
        let p = BasicPreset::new();
        assert_eq!(p.bank_lsb, 0);
    }

    #[test]
    fn test_new_is_gm_gs_drum_false() {
        let p = BasicPreset::new();
        assert!(!p.is_gm_gs_drum);
    }

    #[test]
    fn test_new_zones_empty() {
        let p = BasicPreset::new();
        assert!(p.zones.is_empty());
    }

    #[test]
    fn test_with_name() {
        let p = BasicPreset::with_name("Piano");
        assert_eq!(p.name, "Piano");
    }

    // ── is_xg_drums / is_any_drums ───────────────────────────────────────────

    #[test]
    fn test_is_xg_drums_true_when_xg_bank_and_xg_drum_bank() {
        let mut p = BasicPreset::new();
        p.bank_msb = 120; // XG drum bank
        assert!(p.is_xg_drums(true));
    }

    #[test]
    fn test_is_xg_drums_false_when_not_xg_bank() {
        let mut p = BasicPreset::new();
        p.bank_msb = 120;
        assert!(!p.is_xg_drums(false));
    }

    #[test]
    fn test_is_xg_drums_false_when_not_xg_drum_bank() {
        let mut p = BasicPreset::new();
        p.bank_msb = 0;
        assert!(!p.is_xg_drums(true));
    }

    #[test]
    fn test_is_any_drums_true_for_gm_gs_drum() {
        let mut p = BasicPreset::new();
        p.is_gm_gs_drum = true;
        assert!(p.is_any_drums(false));
    }

    #[test]
    fn test_is_any_drums_true_for_xg_drum() {
        let mut p = BasicPreset::new();
        p.bank_msb = 127; // XG drum bank
        assert!(p.is_any_drums(true));
    }

    #[test]
    fn test_is_any_drums_false_for_non_drum() {
        let p = BasicPreset::new();
        assert!(!p.is_any_drums(false));
        assert!(!p.is_any_drums(true));
    }

    // ── matches ──────────────────────────────────────────────────────────────

    #[test]
    fn test_matches_same_patch() {
        let mut p = BasicPreset::new();
        p.program = 10;
        p.bank_msb = 2;
        p.bank_lsb = 3;
        let patch = MidiPatch {
            program: 10,
            bank_msb: 2,
            bank_lsb: 3,
            is_gm_gs_drum: false,
        };
        assert!(p.matches(&patch));
    }

    #[test]
    fn test_matches_different_program() {
        let p = BasicPreset::new();
        let patch = MidiPatch {
            program: 5,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        };
        assert!(!p.matches(&patch));
    }

    #[test]
    fn test_matches_drum_vs_drum() {
        let mut p = BasicPreset::new();
        p.is_gm_gs_drum = true;
        p.program = 0;
        let patch = MidiPatch {
            program: 0,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: true,
        };
        assert!(p.matches(&patch));
    }

    // ── to_midi_string / Display ─────────────────────────────────────────────

    #[test]
    fn test_to_midi_string_normal() {
        let mut p = BasicPreset::new();
        p.program = 0;
        p.bank_msb = 0;
        p.bank_lsb = 0;
        assert_eq!(p.to_midi_string(), "0:0:0");
    }

    #[test]
    fn test_to_midi_string_drum() {
        let mut p = BasicPreset::new();
        p.is_gm_gs_drum = true;
        p.program = 0;
        assert_eq!(p.to_midi_string(), "DRUM:0");
    }

    #[test]
    fn test_display_includes_name() {
        let p = BasicPreset::with_name("Piano");
        assert!(p.to_string().contains("Piano"));
    }

    // ── create_zone ──────────────────────────────────────────────────────────

    #[test]
    fn test_create_zone_appends_zone() {
        let mut p = BasicPreset::new();
        let mut instruments = vec![BasicInstrument::new()];
        p.create_zone(0, 0, &mut instruments);
        assert_eq!(p.zones.len(), 1);
    }

    #[test]
    fn test_create_zone_links_instrument() {
        let mut p = BasicPreset::new();
        let mut instruments = vec![BasicInstrument::new()];
        p.create_zone(0, 0, &mut instruments);
        assert!(instruments[0].linked_to.contains(&0));
    }

    #[test]
    fn test_create_zone_returns_index() {
        let mut p = BasicPreset::new();
        let mut instruments = vec![BasicInstrument::new(), BasicInstrument::new()];
        let idx0 = p.create_zone(0, 0, &mut instruments);
        let idx1 = p.create_zone(0, 1, &mut instruments);
        assert_eq!(idx0, 0);
        assert_eq!(idx1, 1);
    }

    // ── delete_zone ──────────────────────────────────────────────────────────

    #[test]
    fn test_delete_zone_removes_zone() {
        let mut p = BasicPreset::new();
        let mut instruments = vec![BasicInstrument::new()];
        p.create_zone(0, 0, &mut instruments);
        p.delete_zone(0, 0, &mut instruments);
        assert!(p.zones.is_empty());
    }

    #[test]
    fn test_delete_zone_unlinks_instrument() {
        let mut p = BasicPreset::new();
        let mut instruments = vec![BasicInstrument::new()];
        p.create_zone(0, 0, &mut instruments);
        assert!(instruments[0].linked_to.contains(&0));
        p.delete_zone(0, 0, &mut instruments);
        assert!(!instruments[0].linked_to.contains(&0));
    }

    #[test]
    fn test_delete_zone_out_of_bounds_is_noop() {
        let mut p = BasicPreset::new();
        let mut instruments = vec![BasicInstrument::new()];
        p.delete_zone(99, 0, &mut instruments); // should not panic
    }

    // ── delete ───────────────────────────────────────────────────────────────

    #[test]
    fn test_delete_unlinks_all_instruments() {
        let mut p = BasicPreset::new();
        let mut instruments = vec![BasicInstrument::new(), BasicInstrument::new()];
        p.create_zone(0, 0, &mut instruments);
        p.create_zone(0, 1, &mut instruments);
        p.delete(0, &mut instruments);
        assert!(instruments[0].linked_to.is_empty());
        assert!(instruments[1].linked_to.is_empty());
    }

    // ── is_in_range ──────────────────────────────────────────────────────────

    #[test]
    fn test_is_in_range_exactly_at_min() {
        let r = GenericRange {
            min: 10.0,
            max: 50.0,
        };
        assert!(is_in_range(&r, 10.0));
    }

    #[test]
    fn test_is_in_range_exactly_at_max() {
        let r = GenericRange {
            min: 10.0,
            max: 50.0,
        };
        assert!(is_in_range(&r, 50.0));
    }

    #[test]
    fn test_is_in_range_below_min() {
        let r = GenericRange {
            min: 10.0,
            max: 50.0,
        };
        assert!(!is_in_range(&r, 9.0));
    }

    #[test]
    fn test_is_in_range_above_max() {
        let r = GenericRange {
            min: 10.0,
            max: 50.0,
        };
        assert!(!is_in_range(&r, 51.0));
    }

    // ── subtract_ranges ──────────────────────────────────────────────────────

    #[test]
    fn test_subtract_ranges_overlap() {
        let r1 = GenericRange {
            min: 10.0,
            max: 60.0,
        };
        let r2 = GenericRange {
            min: 20.0,
            max: 80.0,
        };
        let result = subtract_ranges(&r1, &r2);
        assert_eq!(result.min, 20.0);
        assert_eq!(result.max, 60.0);
    }

    #[test]
    fn test_subtract_ranges_no_overlap() {
        let r1 = GenericRange {
            min: 10.0,
            max: 30.0,
        };
        let r2 = GenericRange {
            min: 40.0,
            max: 60.0,
        };
        let result = subtract_ranges(&r1, &r2);
        assert!(result.max < result.min); // inverted = no overlap
    }

    // ── add_unique_modulators ────────────────────────────────────────────────

    #[test]
    fn test_add_unique_modulators_adds_new() {
        let mut main: Vec<Modulator> = Vec::new();
        let m = Modulator::default();
        add_unique_modulators(&mut main, &[m]);
        assert_eq!(main.len(), 1);
    }

    #[test]
    fn test_add_unique_modulators_skips_duplicate() {
        let m = Modulator::default();
        let mut main = vec![m.clone()];
        add_unique_modulators(&mut main, &[m]);
        assert_eq!(main.len(), 1); // duplicate skipped
    }

    // ── PHDR_BYTE_SIZE ───────────────────────────────────────────────────────

    #[test]
    fn test_phdr_byte_size() {
        assert_eq!(PHDR_BYTE_SIZE, 38);
    }

    // ── write ────────────────────────────────────────────────────────────────

    #[test]
    fn test_write_encodes_name_in_pdta() {
        let p = BasicPreset::with_name("Piano");
        let mut chunks = make_chunks();
        p.write(&mut chunks, 0);
        assert_eq!(chunks.pdta[0], b'P');
        assert_eq!(chunks.pdta[1], b'i');
        assert_eq!(chunks.pdta[2], b'a');
        assert_eq!(chunks.pdta[3], b'n');
        assert_eq!(chunks.pdta[4], b'o');
    }

    #[test]
    fn test_write_encodes_program_in_pdta() {
        let mut p = BasicPreset::new();
        p.program = 42;
        let mut chunks = make_chunks();
        p.write(&mut chunks, 0);
        // Bytes 20-21 = program as LE word
        let prog = u16::from_le_bytes([chunks.pdta[20], chunks.pdta[21]]);
        assert_eq!(prog, 42);
    }

    #[test]
    fn test_write_encodes_bank_msb_when_nonzero() {
        let mut p = BasicPreset::new();
        p.bank_msb = 5;
        let mut chunks = make_chunks();
        p.write(&mut chunks, 0);
        // Bytes 22-23 = wBank
        let bank = u16::from_le_bytes([chunks.pdta[22], chunks.pdta[23]]);
        assert_eq!(bank, 5);
    }

    #[test]
    fn test_write_encodes_bank_lsb_when_msb_zero() {
        let mut p = BasicPreset::new();
        p.bank_msb = 0;
        p.bank_lsb = 7;
        let mut chunks = make_chunks();
        p.write(&mut chunks, 0);
        let bank = u16::from_le_bytes([chunks.pdta[22], chunks.pdta[23]]);
        assert_eq!(bank, 7);
    }

    #[test]
    fn test_write_encodes_drum_flag_as_0x80() {
        let mut p = BasicPreset::new();
        p.is_gm_gs_drum = true;
        p.bank_msb = 5; // should be overridden by drum flag
        let mut chunks = make_chunks();
        p.write(&mut chunks, 0);
        let bank = u16::from_le_bytes([chunks.pdta[22], chunks.pdta[23]]);
        assert_eq!(bank, 0x80);
    }

    #[test]
    fn test_write_encodes_bag_index_low16() {
        let p = BasicPreset::new();
        let mut chunks = make_chunks();
        p.write(&mut chunks, 0x1234);
        // Bytes 24-25 = index low 16 bits
        let idx = u16::from_le_bytes([chunks.pdta[24], chunks.pdta[25]]);
        assert_eq!(idx, 0x1234);
    }

    #[test]
    fn test_write_encodes_bag_index_high16_in_xdta() {
        let p = BasicPreset::new();
        let mut chunks = make_chunks();
        p.write(&mut chunks, 0x0001_2345);
        // xdta bytes 24-25 = index >> 16
        let idx_hi = u16::from_le_bytes([chunks.xdta[24], chunks.xdta[25]]);
        assert_eq!(idx_hi, 1);
    }

    #[test]
    fn test_write_encodes_library_genre_morphology() {
        let mut p = BasicPreset::new();
        p.library = 0xDEAD_BEEF;
        p.genre = 0x1234_5678;
        p.morphology = 0xABCD_EF01;
        let mut chunks = make_chunks();
        p.write(&mut chunks, 0);
        // Bytes 26-29 = library, 30-33 = genre, 34-37 = morphology
        let lib = u32::from_le_bytes([
            chunks.pdta[26],
            chunks.pdta[27],
            chunks.pdta[28],
            chunks.pdta[29],
        ]);
        assert_eq!(lib, 0xDEAD_BEEF);
        let gn = u32::from_le_bytes([
            chunks.pdta[30],
            chunks.pdta[31],
            chunks.pdta[32],
            chunks.pdta[33],
        ]);
        assert_eq!(gn, 0x1234_5678);
    }

    // ── get_voice_parameters ─────────────────────────────────────────────────

    fn make_default_modulators() -> Vec<Modulator> {
        crate::soundbank::basic_soundbank::modulator::SPESSASYNTH_DEFAULT_MODULATORS.clone()
    }

    #[test]
    fn test_get_voice_parameters_empty_zones_returns_empty() {
        let p = BasicPreset::new();
        let instruments = vec![];
        let default_mods = make_default_modulators();
        let result = p.get_voice_parameters(60, 100, &instruments, &default_mods);
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_voice_parameters_out_of_key_range_filtered() {
        let mut p = BasicPreset::new();
        let mut instruments = vec![make_instrument_with_zone(0)];

        // Create a zone with a narrow key range (C4 = 60 only).
        let zone_idx = p.create_zone(0, 0, &mut instruments);
        // Set zone key range to 60-70 only
        p.zones[zone_idx].zone.key_range = GenericRange {
            min: 60.0,
            max: 70.0,
        };

        let default_mods = make_default_modulators();
        // Query note 59 (out of range)
        let result = p.get_voice_parameters(59, 100, &instruments, &default_mods);
        assert!(
            result.is_empty(),
            "Expected no voices for out-of-range note"
        );
    }

    #[test]
    fn test_get_voice_parameters_in_key_range_returns_voice() {
        let mut p = BasicPreset::new();
        let mut instruments = vec![make_instrument_with_zone(0)];

        let zone_idx = p.create_zone(0, 0, &mut instruments);
        p.zones[zone_idx].zone.key_range = GenericRange {
            min: 60.0,
            max: 70.0,
        };

        let default_mods = make_default_modulators();
        let result = p.get_voice_parameters(60, 100, &instruments, &default_mods);
        assert!(!result.is_empty(), "Expected a voice for in-range note");
    }

    #[test]
    fn test_get_voice_parameters_out_of_vel_range_filtered() {
        let mut p = BasicPreset::new();
        let mut instruments = vec![make_instrument_with_zone(0)];

        let zone_idx = p.create_zone(0, 0, &mut instruments);
        p.zones[zone_idx].zone.vel_range = GenericRange {
            min: 64.0,
            max: 127.0,
        };

        let default_mods = make_default_modulators();
        // Velocity 10 is below range
        let result = p.get_voice_parameters(60, 10, &instruments, &default_mods);
        assert!(result.is_empty(), "Expected no voices for out-of-vel-range");
    }

    #[test]
    fn test_get_voice_parameters_returns_correct_sample_idx() {
        let mut p = BasicPreset::new();
        let mut instruments = vec![make_instrument_with_zone(7)]; // sample_idx = 7

        p.create_zone(0, 0, &mut instruments);

        let default_mods = make_default_modulators();
        let result = p.get_voice_parameters(60, 100, &instruments, &default_mods);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].sample_idx, 7);
    }

    #[test]
    fn test_get_voice_parameters_generators_have_correct_length() {
        let mut p = BasicPreset::new();
        let mut instruments = vec![make_instrument_with_zone(0)];
        p.create_zone(0, 0, &mut instruments);

        let default_mods = make_default_modulators();
        let result = p.get_voice_parameters(60, 100, &instruments, &default_mods);
        assert_eq!(result[0].generators.len(), GENERATORS_AMOUNT);
    }

    #[test]
    fn test_get_voice_parameters_includes_default_modulators() {
        let mut p = BasicPreset::new();
        let mut instruments = vec![make_instrument_with_zone(0)];
        p.create_zone(0, 0, &mut instruments);

        let default_mods = make_default_modulators();
        let mod_count = default_mods.len();
        let result = p.get_voice_parameters(60, 100, &instruments, &default_mods);
        // Should have at least as many modulators as the defaults.
        assert!(result[0].modulators.len() >= mod_count);
    }

    #[test]
    fn test_get_voice_parameters_initial_filter_fc_default_is_13500() {
        let mut p = BasicPreset::new();
        let mut instruments = vec![make_instrument_with_zone(0)];
        p.create_zone(0, 0, &mut instruments);

        let default_mods = make_default_modulators();
        let result = p.get_voice_parameters(60, 100, &instruments, &default_mods);
        // initialFilterFc (index 8) has default 13500.
        assert_eq!(result[0].generators[gt::INITIAL_FILTER_FC as usize], 13500);
    }

    #[test]
    fn test_get_voice_parameters_emu_attenuation_correction() {
        // Set instrument zone initialAttenuation to 100.
        // After EMU correction: floor(100 * 0.4) = 40.
        let mut p = BasicPreset::new();
        let mut inst = BasicInstrument::with_name("TestInst");
        let mut zone = BasicInstrumentZone::new(0, 0, 0);
        zone.zone
            .set_generator(gt::INITIAL_ATTENUATION, Some(100.0), false);
        inst.zones.push(zone);
        let mut instruments = vec![inst];

        p.create_zone(0, 0, &mut instruments);

        let default_mods = make_default_modulators();
        let result = p.get_voice_parameters(60, 100, &instruments, &default_mods);
        let ia = result[0].generators[gt::INITIAL_ATTENUATION as usize];
        assert_eq!(ia, 40, "EMU correction: floor(100*0.4) = 40");
    }

    #[test]
    fn test_get_voice_parameters_preset_generator_sums_to_instrument() {
        // Preset zone: PAN = +50 (offset)
        // Instrument zone: PAN not set (default 0)
        // Expected: generators[PAN] = default(0) + 50 = 50
        let mut p = BasicPreset::new();
        let mut inst = BasicInstrument::with_name("TestInst");
        let zone = BasicInstrumentZone::new(0, 0, 0);
        inst.zones.push(zone);
        let mut instruments = vec![inst];

        let zone_idx = p.create_zone(0, 0, &mut instruments);
        p.zones[zone_idx]
            .zone
            .set_generator(gt::PAN, Some(50.0), false);

        let default_mods = make_default_modulators();
        let result = p.get_voice_parameters(60, 100, &instruments, &default_mods);
        // PAN default is 0, + preset offset 50 = 50
        assert_eq!(result[0].generators[gt::PAN as usize], 50);
    }

    #[test]
    fn test_get_voice_parameters_instrument_global_zone_applied() {
        // Instrument global zone has PAN=100, local zone has no PAN.
        // Expected: generators[PAN] = 100.
        let mut p = BasicPreset::new();
        let mut inst = BasicInstrument::with_name("TestInst");
        inst.global_zone.set_generator(gt::PAN, Some(100.0), false);
        let zone = BasicInstrumentZone::new(0, 0, 0);
        inst.zones.push(zone);
        let mut instruments = vec![inst];

        p.create_zone(0, 0, &mut instruments);

        let default_mods = make_default_modulators();
        let result = p.get_voice_parameters(60, 100, &instruments, &default_mods);
        assert_eq!(result[0].generators[gt::PAN as usize], 100);
    }

    #[test]
    fn test_get_voice_parameters_instrument_local_zone_overrides_global() {
        // Global zone: PAN=100; local zone: PAN=200.
        // Expected: generators[PAN] = 200 (local overrides global).
        let mut p = BasicPreset::new();
        let mut inst = BasicInstrument::with_name("TestInst");
        inst.global_zone.set_generator(gt::PAN, Some(100.0), false);
        let mut zone = BasicInstrumentZone::new(0, 0, 0);
        zone.zone.set_generator(gt::PAN, Some(200.0), false);
        inst.zones.push(zone);
        let mut instruments = vec![inst];

        p.create_zone(0, 0, &mut instruments);

        let default_mods = make_default_modulators();
        let result = p.get_voice_parameters(60, 100, &instruments, &default_mods);
        // PAN local (200) overrides global (100), then preset offset (0) is added → 200
        // But PAN is clamped to [-500, 500], so 200 stays 200
        assert_eq!(result[0].generators[gt::PAN as usize], 200);
    }

    #[test]
    fn test_get_voice_parameters_multiple_instrument_zones() {
        // Instrument has 2 zones: one covering 0-60, one covering 61-127.
        let mut p = BasicPreset::new();
        let mut inst = BasicInstrument::with_name("TestInst");

        let mut zone1 = BasicInstrumentZone::new(0, 0, 0);
        zone1.zone.key_range = GenericRange {
            min: 0.0,
            max: 60.0,
        };
        inst.zones.push(zone1);

        let mut zone2 = BasicInstrumentZone::new(0, 0, 1);
        zone2.zone.key_range = GenericRange {
            min: 61.0,
            max: 127.0,
        };
        inst.zones.push(zone2);

        let mut instruments = vec![inst];
        p.create_zone(0, 0, &mut instruments);

        let default_mods = make_default_modulators();

        // Note 50: only zone1 should respond
        let result = p.get_voice_parameters(50, 100, &instruments, &default_mods);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].sample_idx, 0);

        // Note 70: only zone2 should respond
        let result = p.get_voice_parameters(70, 100, &instruments, &default_mods);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].sample_idx, 1);
    }

    // ── to_flattened_instrument ──────────────────────────────────────────────

    #[test]
    fn test_to_flattened_instrument_name_matches_preset() {
        let p = BasicPreset::with_name("Strings");
        let instruments: Vec<BasicInstrument> = vec![];
        let result = p.to_flattened_instrument(&instruments);
        assert_eq!(result.name, "Strings");
    }

    #[test]
    fn test_to_flattened_instrument_empty_preset_has_no_zones() {
        let p = BasicPreset::new();
        let instruments: Vec<BasicInstrument> = vec![];
        let result = p.to_flattened_instrument(&instruments);
        assert!(result.zones.is_empty());
    }

    #[test]
    fn test_to_flattened_instrument_creates_zones_from_instrument_zones() {
        let mut p = BasicPreset::new();
        let mut inst = BasicInstrument::with_name("TestInst");
        let zone = BasicInstrumentZone::new(0, 0, 5); // sample_idx=5
        inst.zones.push(zone);
        let mut instruments = vec![inst];
        p.create_zone(0, 0, &mut instruments);

        let result = p.to_flattened_instrument(&instruments);
        assert_eq!(result.zones.len(), 1);
        assert_eq!(result.zones[0].sample_idx, 5);
    }

    #[test]
    fn test_to_flattened_instrument_discards_non_overlapping_zones() {
        // Preset zone covers 60-70; instrument zone covers 80-90 → no overlap.
        let mut p = BasicPreset::new();
        let mut inst = BasicInstrument::with_name("TestInst");
        let mut iz = BasicInstrumentZone::new(0, 0, 0);
        iz.zone.key_range = GenericRange {
            min: 80.0,
            max: 90.0,
        };
        inst.zones.push(iz);
        let mut instruments = vec![inst];

        let pz_idx = p.create_zone(0, 0, &mut instruments);
        p.zones[pz_idx].zone.key_range = GenericRange {
            min: 60.0,
            max: 70.0,
        };

        let result = p.to_flattened_instrument(&instruments);
        assert!(
            result.zones.is_empty(),
            "Non-overlapping zones should be discarded"
        );
    }

    #[test]
    fn test_to_flattened_instrument_intersects_ranges() {
        // Preset: 40-80; Instrument: 60-100 → intersection = 60-80
        let mut p = BasicPreset::new();
        let mut inst = BasicInstrument::with_name("TestInst");
        let mut iz = BasicInstrumentZone::new(0, 0, 0);
        iz.zone.key_range = GenericRange {
            min: 60.0,
            max: 100.0,
        };
        inst.zones.push(iz);
        let mut instruments = vec![inst];

        let pz_idx = p.create_zone(0, 0, &mut instruments);
        p.zones[pz_idx].zone.key_range = GenericRange {
            min: 40.0,
            max: 80.0,
        };

        let result = p.to_flattened_instrument(&instruments);
        assert_eq!(result.zones.len(), 1);
        assert_eq!(result.zones[0].zone.key_range.min, 60.0);
        assert_eq!(result.zones[0].zone.key_range.max, 80.0);
    }
}
