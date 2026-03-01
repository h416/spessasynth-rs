/// basic_soundbank.rs
/// purpose: BasicSoundBank struct - a single SoundFont2/DLS sound bank.
/// Ported from: src/soundbank/basic_soundbank/basic_soundbank.ts
///
/// # Skipped features
/// - `trimSoundBank()` → ported as free function `trim_sound_bank` in `used_keys_loaded.rs`
/// - `writeSF2()`, `writeDLS()`, `getSampleSoundBankFile()` → write-only, out of MIDI→WAV scope
/// - `addCompletePresets()` → replaced by `clone_preset_from` / `merge_sound_banks`
///
/// # TypeScript vs Rust design differences
///
/// - All object references replaced with Vec indices
/// - `clone_sample/instrument/preset` take explicit source-bank slices for cross-bank cloning
/// - `remove_unused_elements` / `delete_*` perform full index recomputation after removal
/// - `get_preset` returns `Option<&BasicPreset>` (None on empty bank) to satisfy `PresetResolver`
/// - `parentSoundBank` in `BasicPreset` is removed (circular ownership); passed as params instead
use crate::soundbank::basic_soundbank::basic_instrument::BasicInstrument;
use crate::soundbank::basic_soundbank::basic_instrument_zone::BasicInstrumentZone;
use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::basic_preset_zone::BasicPresetZone;
use crate::soundbank::basic_soundbank::basic_sample::BasicSample;
use crate::soundbank::basic_soundbank::midi_patch::{MidiPatch, sorter};
use crate::soundbank::basic_soundbank::modulator::{Modulator, SPESSASYNTH_DEFAULT_MODULATORS};
use crate::soundbank::basic_soundbank::preset_resolver::PresetResolver;
use crate::soundbank::basic_soundbank::preset_selector::select_preset;
use crate::soundbank::types::{SF2VersionTag, SoundBankInfoData};
use crate::synthesizer::types::SynthSystem;
use crate::utils::loggin::spessa_synth_info;
use crate::utils::midi_hacks::BankSelectHacks;

// ---------------------------------------------------------------------------
// BasicSoundBank
// ---------------------------------------------------------------------------

/// Represents a single sound bank (SF2 or DLS).
/// Equivalent to: class BasicSoundBank
pub struct BasicSoundBank {
    /// Sound bank metadata.
    /// Equivalent to: public soundBankInfo: SoundBankInfoData
    pub sound_bank_info: SoundBankInfoData,

    /// Sound bank's presets (sorted after `flush()`).
    /// Equivalent to: public presets: BasicPreset[]
    pub presets: Vec<BasicPreset>,

    /// Sound bank's samples.
    /// Equivalent to: public samples: BasicSample[]
    pub samples: Vec<BasicSample>,

    /// Sound bank's instruments.
    /// Equivalent to: public instruments: BasicInstrument[]
    pub instruments: Vec<BasicInstrument>,

    /// Sound bank's default modulators.
    /// Equivalent to: public defaultModulators: Modulator[]
    pub default_modulators: Vec<Modulator>,

    /// Whether the sound bank has custom default modulators (DMOD).
    /// Equivalent to: public customDefaultModulators = false
    pub custom_default_modulators: bool,

    /// Whether the sound bank contains valid XG drum presets.
    /// Equivalent to: private _isXGBank = false
    _is_xg_bank: bool,
}

impl Default for BasicSoundBank {
    fn default() -> Self {
        Self {
            sound_bank_info: SoundBankInfoData {
                name: "Unnamed".to_string(),
                version: SF2VersionTag { major: 2, minor: 4 },
                creation_date: String::new(),
                sound_engine: "E-mu 10K2".to_string(),
                engineer: None,
                product: None,
                copyright: None,
                comment: None,
                subject: None,
                rom_info: None,
                software: Some("SpessaSynth".to_string()),
                rom_version: None,
            },
            presets: Vec::new(),
            samples: Vec::new(),
            instruments: Vec::new(),
            default_modulators: SPESSASYNTH_DEFAULT_MODULATORS
                .iter()
                .map(Modulator::copy_from)
                .collect(),
            custom_default_modulators: false,
            _is_xg_bank: false,
        }
    }
}

impl BasicSoundBank {
    /// Creates a new, empty `BasicSoundBank`.
    /// Equivalent to: constructor()
    pub fn new() -> Self {
        Self::default()
    }

    // -----------------------------------------------------------------------
    // isXGBank getter
    // -----------------------------------------------------------------------

    /// Returns `true` if the sound bank contains valid XG drum presets.
    /// Equivalent to: public get isXGBank()
    #[inline]
    pub fn is_xg_bank(&self) -> bool {
        self._is_xg_bank
    }

    // -----------------------------------------------------------------------
    // Static methods
    // -----------------------------------------------------------------------

    /// Deep-clones all data from another sound bank (all indices stay valid).
    /// Equivalent to: public static copyFrom(bank: BasicSoundBank)
    pub fn copy_from(bank: &BasicSoundBank) -> Self {
        Self {
            sound_bank_info: bank.sound_bank_info.clone(),
            presets: bank.presets.clone(),
            instruments: bank.instruments.clone(),
            samples: bank.samples.clone(),
            default_modulators: bank.default_modulators.clone(),
            custom_default_modulators: bank.custom_default_modulators,
            _is_xg_bank: bank._is_xg_bank,
        }
    }

    /// Merges sound banks: the first bank's info is preserved; unique presets
    /// from later banks are added if they don't exist by patch match.
    ///
    /// Equivalent to: public static mergeSoundBanks(...soundBanks: BasicSoundBank[])
    pub fn merge_sound_banks(mut sound_banks: Vec<BasicSoundBank>) -> BasicSoundBank {
        assert!(!sound_banks.is_empty(), "No sound banks provided!");
        let main = sound_banks.remove(0);
        let info = main.sound_bank_info.clone();

        let mut result = BasicSoundBank::new();
        result.sound_bank_info = info;

        // Clone all presets from the main bank
        for pi in 0..main.presets.len() {
            let preset = main.presets[pi].clone();
            result.clone_preset_from(&preset, &main.instruments, &main.samples);
        }

        // Add unique presets from subsequent banks
        for src in &sound_banks {
            for pi in 0..src.presets.len() {
                let preset = &src.presets[pi];
                let patch = MidiPatch {
                    program: preset.program,
                    bank_msb: preset.bank_msb,
                    bank_lsb: preset.bank_lsb,
                    is_gm_gs_drum: preset.is_gm_gs_drum,
                };
                let already_exists = result.presets.iter().any(|p| p.matches(&patch));
                if !already_exists {
                    let preset_clone = preset.clone();
                    result.clone_preset_from(&preset_clone, &src.instruments, &src.samples);
                }
            }
        }

        result
    }

    // -----------------------------------------------------------------------
    // Adding elements
    // -----------------------------------------------------------------------

    /// Adds a single preset to the sound bank.
    pub fn add_preset(&mut self, preset: BasicPreset) {
        self.presets.push(preset);
    }

    /// Adds multiple presets to the sound bank.
    /// Equivalent to: public addPresets(...presets: BasicPreset[])
    pub fn add_presets(&mut self, presets: Vec<BasicPreset>) {
        self.presets.extend(presets);
    }

    /// Adds a single instrument to the sound bank.
    pub fn add_instrument(&mut self, instrument: BasicInstrument) {
        self.instruments.push(instrument);
    }

    /// Adds multiple instruments to the sound bank.
    /// Equivalent to: public addInstruments(...instruments: BasicInstrument[])
    pub fn add_instruments(&mut self, instruments: Vec<BasicInstrument>) {
        self.instruments.extend(instruments);
    }

    /// Adds a single sample to the sound bank.
    pub fn add_sample(&mut self, sample: BasicSample) {
        self.samples.push(sample);
    }

    /// Adds multiple samples to the sound bank.
    /// Equivalent to: public addSamples(...samples: BasicSample[])
    pub fn add_samples(&mut self, samples: Vec<BasicSample>) {
        self.samples.extend(samples);
    }

    // -----------------------------------------------------------------------
    // Clone methods (deep copy from source slices)
    // -----------------------------------------------------------------------

    /// Clones a sample from source slices into this bank.
    /// If a sample with the same name already exists, returns its index.
    /// Recursively clones the linked stereo partner.
    ///
    /// `src_samples` must be the samples array of the bank that `sample` came from.
    ///
    /// Equivalent to: public cloneSample(sample: BasicSample): BasicSample
    pub fn clone_sample_from(
        &mut self,
        sample: &BasicSample,
        src_samples: &[BasicSample],
    ) -> usize {
        // Return existing if name matches (deduplication by name)
        if let Some(idx) = self.samples.iter().position(|s| s.name == sample.name) {
            return idx;
        }

        // Build the new sample (no audio data yet)
        let mut new_sample = BasicSample::new(
            sample.name.clone(),
            sample.sample_rate,
            sample.original_key,
            sample.pitch_correction,
            sample.sample_type,
            sample.loop_start,
            sample.loop_end,
        );
        if sample.is_compressed() {
            if let Some(ref cd) = sample.compressed_data {
                new_sample.set_compressed_data(cd.clone());
            }
        } else if let Some(ref audio) = sample.audio_data {
            new_sample.set_audio_data(audio.clone(), sample.sample_rate);
        }

        let new_idx = self.samples.len();
        self.samples.push(new_sample);

        // Recursively clone linked stereo partner
        if let Some(linked_orig_idx) = sample.linked_sample_idx
            && let Some(linked) = src_samples.get(linked_orig_idx)
        {
            // Clone the linked sample object to avoid borrow conflicts
            let linked_clone = linked.clone();
            let cloned_linked_idx = self.clone_sample_from(&linked_clone, src_samples);
            // Set the link only if the linked sample has no link yet
            if self.samples[cloned_linked_idx].linked_sample_idx.is_none() {
                self.samples[new_idx].linked_sample_idx = Some(cloned_linked_idx);
            }
        }

        new_idx
    }

    /// Clones an instrument (and its samples) from source slices into this bank.
    /// If an instrument with the same name already exists, returns its index.
    ///
    /// `src_samples` must be the samples array of the bank that `instrument` came from.
    ///
    /// Equivalent to: public cloneInstrument(instrument: BasicInstrument): BasicInstrument
    pub fn clone_instrument_from(
        &mut self,
        instrument: &BasicInstrument,
        src_samples: &[BasicSample],
    ) -> usize {
        // Deduplication by name
        if let Some(idx) = self
            .instruments
            .iter()
            .position(|i| i.name == instrument.name)
        {
            return idx;
        }

        let mut new_instrument = BasicInstrument::new();
        new_instrument.name = instrument.name.clone();
        new_instrument.global_zone = instrument.global_zone.clone();

        let new_inst_idx = self.instruments.len();
        self.instruments.push(new_instrument);

        // Clone each zone
        for zone in &instrument.zones.clone() {
            let src_sample = match src_samples.get(zone.sample_idx) {
                Some(s) => s.clone(),
                None => continue,
            };
            let new_sample_idx = self.clone_sample_from(&src_sample, src_samples);

            // Manually create the instrument zone (avoids borrow-checker issues)
            let mut new_zone = BasicInstrumentZone::new(new_inst_idx, 0, new_sample_idx);
            new_zone.zone = zone.zone.clone();
            new_zone.use_count = zone.use_count;
            self.instruments[new_inst_idx].zones.push(new_zone);

            // Register the sample → instrument back-reference
            if let Some(s) = self.samples.get_mut(new_sample_idx) {
                s.link_to(new_inst_idx);
            }
        }

        new_inst_idx
    }

    /// Clones a preset (with its instruments and samples) from source slices into this bank.
    /// If a preset with the same name already exists, returns its index.
    ///
    /// Equivalent to: public clonePreset(preset: BasicPreset): BasicPreset
    pub fn clone_preset_from(
        &mut self,
        preset: &BasicPreset,
        src_instruments: &[BasicInstrument],
        src_samples: &[BasicSample],
    ) -> usize {
        // Deduplication by name
        if let Some(idx) = self.presets.iter().position(|p| p.name == preset.name) {
            return idx;
        }

        let mut new_preset = BasicPreset::new();
        new_preset.name = preset.name.clone();
        new_preset.bank_msb = preset.bank_msb;
        new_preset.bank_lsb = preset.bank_lsb;
        new_preset.is_gm_gs_drum = preset.is_gm_gs_drum;
        new_preset.program = preset.program;
        new_preset.library = preset.library;
        new_preset.genre = preset.genre;
        new_preset.morphology = preset.morphology;
        new_preset.global_zone = preset.global_zone.clone();

        let new_preset_idx = self.presets.len();
        self.presets.push(new_preset);

        // Clone each zone
        for zone in &preset.zones.clone() {
            let src_instrument = match src_instruments.get(zone.instrument_idx) {
                Some(i) => i.clone(),
                None => continue,
            };
            let new_inst_idx = self.clone_instrument_from(&src_instrument, src_samples);

            // Manually create the preset zone
            let mut new_zone = BasicPresetZone::new(new_preset_idx, new_inst_idx);
            new_zone.zone = zone.zone.clone();
            self.presets[new_preset_idx].zones.push(new_zone);

            // Register the instrument → preset back-reference
            if let Some(i) = self.instruments.get_mut(new_inst_idx) {
                i.link_to(new_preset_idx);
            }
        }

        new_preset_idx
    }

    // -----------------------------------------------------------------------
    // flush
    // -----------------------------------------------------------------------

    /// Sorts presets by patch and updates internal values.
    /// Equivalent to: public flush()
    pub fn flush(&mut self) {
        self.presets.sort_by(|a, b| {
            let pa = MidiPatch {
                program: a.program,
                bank_msb: a.bank_msb,
                bank_lsb: a.bank_lsb,
                is_gm_gs_drum: a.is_gm_gs_drum,
            };
            let pb = MidiPatch {
                program: b.program,
                bank_msb: b.bank_msb,
                bank_lsb: b.bank_lsb,
                is_gm_gs_drum: b.is_gm_gs_drum,
            };
            sorter(&pa, &pb)
        });
        self.parse_internal();
    }

    // -----------------------------------------------------------------------
    // Deletion and cleanup
    // -----------------------------------------------------------------------

    /// Removes all instruments and samples that are no longer referenced.
    /// Equivalent to: public removeUnusedElements()
    pub fn remove_unused_elements(&mut self) {
        let n_inst = self.instruments.len();

        // Phase 1: delete unused zones from each instrument; mark unused instruments
        {
            let instruments = &mut self.instruments;
            let samples = &mut self.samples;
            for (i, instrument) in instruments.iter_mut().enumerate() {
                instrument.delete_unused_zones(i, samples);
            }
        }
        let keep_inst: Vec<bool> = (0..n_inst)
            .map(|i| self.instruments[i].use_count() > 0)
            .collect();

        // Call delete() on instruments being removed (unlinks their samples)
        {
            let instruments = &mut self.instruments;
            let samples = &mut self.samples;
            for i in 0..n_inst {
                if !keep_inst[i] {
                    let snap = instruments[i].clone();
                    snap.delete(i, samples);
                }
            }
        }

        // Build instrument reindex map and remove unused instruments
        let inst_reindex = Self::build_reindex(&keep_inst);
        let kept_instruments: Vec<BasicInstrument> = self
            .instruments
            .drain(..)
            .enumerate()
            .filter_map(|(i, inst)| if keep_inst[i] { Some(inst) } else { None })
            .collect();
        self.instruments = kept_instruments;
        self.apply_instrument_reindex(&inst_reindex);

        // Phase 2: remove samples with use_count == 0
        let keep_samp: Vec<bool> = (0..self.samples.len())
            .map(|i| self.samples[i].use_count() > 0)
            .collect();
        for (i, sample) in self.samples.iter_mut().enumerate() {
            if !keep_samp[i] {
                sample.unlink_sample();
            }
        }
        let samp_reindex = Self::build_reindex(&keep_samp);
        let kept_samples: Vec<BasicSample> = self
            .samples
            .drain(..)
            .enumerate()
            .filter_map(|(i, s)| if keep_samp[i] { Some(s) } else { None })
            .collect();
        self.samples = kept_samples;
        self.apply_sample_reindex(&samp_reindex);
    }

    /// Removes the instrument at `instrument_idx` and reindexes all references.
    /// Equivalent to: public deleteInstrument(instrument: BasicInstrument)
    pub fn delete_instrument(&mut self, instrument_idx: usize) {
        assert!(
            instrument_idx < self.instruments.len(),
            "instrument_idx out of range"
        );
        let snap = self.instruments[instrument_idx].clone();
        snap.delete(instrument_idx, &mut self.samples);
        self.instruments.remove(instrument_idx);

        let reindex = Self::build_shift_reindex(self.instruments.len() + 1, instrument_idx);
        self.apply_instrument_reindex(&reindex);
    }

    /// Removes the preset at `preset_idx` and reindexes all references.
    /// Equivalent to: public deletePreset(preset: BasicPreset)
    pub fn delete_preset(&mut self, preset_idx: usize) {
        assert!(preset_idx < self.presets.len(), "preset_idx out of range");
        let snap = self.presets[preset_idx].clone();
        snap.delete(preset_idx, &mut self.instruments);
        self.presets.remove(preset_idx);

        let reindex = Self::build_shift_reindex(self.presets.len() + 1, preset_idx);
        self.apply_preset_reindex(&reindex);
    }

    /// Removes the sample at `sample_idx` and reindexes all references.
    /// Equivalent to: public deleteSample(sample: BasicSample)
    pub fn delete_sample(&mut self, sample_idx: usize) {
        assert!(sample_idx < self.samples.len(), "sample_idx out of range");
        self.samples[sample_idx].unlink_sample();
        self.samples.remove(sample_idx);

        let reindex = Self::build_shift_reindex(self.samples.len() + 1, sample_idx);
        self.apply_sample_reindex(&reindex);
    }

    // -----------------------------------------------------------------------
    // get_preset
    // -----------------------------------------------------------------------

    /// Returns the most appropriate preset for the given MIDI patch and system.
    /// Returns `None` if the sound bank contains no presets.
    ///
    /// Equivalent to: public getPreset(patch: MIDIPatch, system: SynthSystem): BasicPreset
    /// Note: TypeScript panics if presets is empty; Rust returns None.
    pub fn get_preset(&self, patch: MidiPatch, system: SynthSystem) -> Option<&BasicPreset> {
        if self.presets.is_empty() {
            return None;
        }
        Some(select_preset(&self.presets, patch, system))
    }

    // -----------------------------------------------------------------------
    // destroy_sound_bank
    // -----------------------------------------------------------------------

    /// Clears all presets, instruments, and samples.
    /// Equivalent to: public destroySoundBank()
    pub fn destroy_sound_bank(&mut self) {
        self.presets.clear();
        self.instruments.clear();
        self.samples.clear();
    }

    // -----------------------------------------------------------------------
    // parsing_error (protected)
    // -----------------------------------------------------------------------

    /// Panics with an SF parsing error message.
    /// Equivalent to: protected parsingError(error: string)
    pub fn parsing_error(&self, error: &str) -> ! {
        panic!("SF parsing error: {} The file may be corrupted.", error);
    }

    // -----------------------------------------------------------------------
    // parse_internal (protected)
    // -----------------------------------------------------------------------

    /// Detects XG drum presets and sets `_is_xg_bank`.
    ///
    /// XG rule: at least one preset with bank_msb ∈ {120, 126, 127}; all such
    /// presets must have an allowed XG program number.
    ///
    /// Equivalent to: protected parseInternal()
    pub fn parse_internal(&mut self) {
        self._is_xg_bank = false;

        // Allowed XG drum program numbers (0-indexed, matching the spec's 1-indexed values minus 1)
        const ALLOWED: &[u8] = &[
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 16, 17, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 40, 41,
            48, 56, 57, 58, 64, 65, 66, 126, 127,
        ];

        for preset in &self.presets {
            if BankSelectHacks::is_xg_drums(preset.bank_msb) {
                self._is_xg_bank = true;
                if !ALLOWED.contains(&preset.program) {
                    self._is_xg_bank = false;
                    spessa_synth_info(&format!(
                        "This bank is not valid XG. Preset {}:{} is not a valid XG drum. \
                         XG mode will use presets on bank 128.",
                        preset.bank_msb, preset.program
                    ));
                    break;
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // print_info (protected)
    // -----------------------------------------------------------------------

    /// Logs sound bank metadata to the console.
    /// Equivalent to: protected printInfo()
    pub fn print_info(&self) {
        let i = &self.sound_bank_info;
        spessa_synth_info(&format!("name: \"{}\"", i.name));
        spessa_synth_info(&format!(
            "version: \"{}.{}\"",
            i.version.major, i.version.minor
        ));
        spessa_synth_info(&format!("creation_date: \"{}\"", i.creation_date));
        spessa_synth_info(&format!("sound_engine: \"{}\"", i.sound_engine));
        if let Some(ref v) = i.software {
            spessa_synth_info(&format!("software: \"{}\"", v));
        }
        if let Some(ref v) = i.engineer {
            spessa_synth_info(&format!("engineer: \"{}\"", v));
        }
        if let Some(ref v) = i.product {
            spessa_synth_info(&format!("product: \"{}\"", v));
        }
        if let Some(ref v) = i.copyright {
            spessa_synth_info(&format!("copyright: \"{}\"", v));
        }
        if let Some(ref v) = i.comment {
            spessa_synth_info(&format!("comment: \"{}\"", v));
        }
    }

    // -----------------------------------------------------------------------
    // Index management helpers (private)
    // -----------------------------------------------------------------------

    /// Builds a reindex map from a keep-mask: `result[old] = Some(new)` if kept.
    fn build_reindex(keep: &[bool]) -> Vec<Option<usize>> {
        let mut new_idx = 0;
        keep.iter()
            .map(|&k| {
                if k {
                    let r = Some(new_idx);
                    new_idx += 1;
                    r
                } else {
                    None
                }
            })
            .collect()
    }

    /// Builds a reindex map for removing exactly one element at `removed_idx`.
    /// Array had `old_len` elements before removal.
    fn build_shift_reindex(old_len: usize, removed_idx: usize) -> Vec<Option<usize>> {
        (0..old_len)
            .map(|old| {
                if old == removed_idx {
                    None
                } else if old > removed_idx {
                    Some(old - 1)
                } else {
                    Some(old)
                }
            })
            .collect()
    }

    /// Applies an instrument reindex map to all cross-references.
    fn apply_instrument_reindex(&mut self, new_idxs: &[Option<usize>]) {
        // Update instrument_idx in preset zones
        for preset in &mut self.presets {
            for zone in &mut preset.zones {
                if let Some(&Some(new_idx)) = new_idxs.get(zone.instrument_idx) {
                    zone.instrument_idx = new_idx;
                }
            }
        }
        // Update parent_instrument_idx in instrument zones
        for instrument in &mut self.instruments {
            for zone in &mut instrument.zones {
                if let Some(&Some(new_idx)) = new_idxs.get(zone.parent_instrument_idx) {
                    zone.parent_instrument_idx = new_idx;
                }
            }
        }
        // Update sample linked_to (instrument back-references)
        for sample in &mut self.samples {
            sample.linked_to = sample
                .linked_to
                .iter()
                .filter_map(|&old| new_idxs.get(old).copied().flatten())
                .collect();
        }
    }

    /// Applies a sample reindex map to all cross-references.
    fn apply_sample_reindex(&mut self, new_idxs: &[Option<usize>]) {
        // Update sample_idx in instrument zones
        for instrument in &mut self.instruments {
            for zone in &mut instrument.zones {
                if let Some(&Some(new_idx)) = new_idxs.get(zone.sample_idx) {
                    zone.sample_idx = new_idx;
                }
            }
        }
        // Update linked_sample_idx in samples
        for sample in &mut self.samples {
            if let Some(old_linked) = sample.linked_sample_idx {
                sample.linked_sample_idx = new_idxs.get(old_linked).copied().flatten();
            }
        }
    }

    /// Applies a preset reindex map to all cross-references.
    fn apply_preset_reindex(&mut self, new_idxs: &[Option<usize>]) {
        // Update parent_preset_idx in preset zones
        for preset in &mut self.presets {
            for zone in &mut preset.zones {
                if let Some(&Some(new_idx)) = new_idxs.get(zone.parent_preset_idx) {
                    zone.parent_preset_idx = new_idx;
                }
            }
        }
        // Update instrument linked_to (preset back-references)
        for instrument in &mut self.instruments {
            instrument.linked_to = instrument
                .linked_to
                .iter()
                .filter_map(|&old| new_idxs.get(old).copied().flatten())
                .collect();
        }
    }
}

// ---------------------------------------------------------------------------
// PresetResolver implementation
// ---------------------------------------------------------------------------

/// Allows `BasicMIDI::get_used_programs_and_keys` to accept `&dyn PresetResolver`
/// without creating a direct dependency on `BasicSoundBank`.
impl PresetResolver for BasicSoundBank {
    fn get_preset(&self, patch: MidiPatch, system: SynthSystem) -> Option<&BasicPreset> {
        if self.presets.is_empty() {
            return None;
        }
        Some(select_preset(&self.presets, patch, system))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
    use crate::soundbank::enums::sample_types;
    use crate::synthesizer::types::SynthSystem;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_sample(name: &str) -> BasicSample {
        let mut s = BasicSample::new(
            name.to_string(),
            44_100,
            60,
            0,
            sample_types::MONO_SAMPLE,
            0,
            10,
        );
        s.set_audio_data(vec![0.0; 11], 44_100);
        s
    }

    fn make_instrument(name: &str) -> BasicInstrument {
        BasicInstrument::with_name(name)
    }

    fn make_preset(name: &str, program: u8, bank_msb: u8) -> BasicPreset {
        let mut p = BasicPreset::with_name(name);
        p.program = program;
        p.bank_msb = bank_msb;
        p
    }

    fn any_patch() -> MidiPatch {
        MidiPatch {
            program: 0,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        }
    }

    // -----------------------------------------------------------------------
    // BasicSoundBank::new / Default
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_presets_empty() {
        let bank = BasicSoundBank::new();
        assert!(bank.presets.is_empty());
    }

    #[test]
    fn test_new_instruments_empty() {
        let bank = BasicSoundBank::new();
        assert!(bank.instruments.is_empty());
    }

    #[test]
    fn test_new_samples_empty() {
        let bank = BasicSoundBank::new();
        assert!(bank.samples.is_empty());
    }

    #[test]
    fn test_new_default_modulators_nonempty() {
        let bank = BasicSoundBank::new();
        assert!(!bank.default_modulators.is_empty());
    }

    #[test]
    fn test_new_custom_default_modulators_false() {
        let bank = BasicSoundBank::new();
        assert!(!bank.custom_default_modulators);
    }

    #[test]
    fn test_new_is_xg_bank_false() {
        let bank = BasicSoundBank::new();
        assert!(!bank.is_xg_bank());
    }

    #[test]
    fn test_new_sound_bank_info_name() {
        let bank = BasicSoundBank::new();
        assert_eq!(bank.sound_bank_info.name, "Unnamed");
    }

    #[test]
    fn test_new_sound_bank_info_version() {
        let bank = BasicSoundBank::new();
        assert_eq!(
            bank.sound_bank_info.version,
            SF2VersionTag { major: 2, minor: 4 }
        );
    }

    // -----------------------------------------------------------------------
    // add_preset / add_instrument / add_sample
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_preset_increases_length() {
        let mut bank = BasicSoundBank::new();
        bank.add_preset(make_preset("A", 0, 0));
        assert_eq!(bank.presets.len(), 1);
    }

    #[test]
    fn test_add_presets_batch() {
        let mut bank = BasicSoundBank::new();
        bank.add_presets(vec![make_preset("A", 0, 0), make_preset("B", 1, 0)]);
        assert_eq!(bank.presets.len(), 2);
    }

    #[test]
    fn test_add_instrument_increases_length() {
        let mut bank = BasicSoundBank::new();
        bank.add_instrument(make_instrument("Piano"));
        assert_eq!(bank.instruments.len(), 1);
    }

    #[test]
    fn test_add_sample_increases_length() {
        let mut bank = BasicSoundBank::new();
        bank.add_sample(make_sample("PianoC4"));
        assert_eq!(bank.samples.len(), 1);
    }

    // -----------------------------------------------------------------------
    // destroy_sound_bank
    // -----------------------------------------------------------------------

    #[test]
    fn test_destroy_clears_presets() {
        let mut bank = BasicSoundBank::new();
        bank.add_preset(make_preset("A", 0, 0));
        bank.destroy_sound_bank();
        assert!(bank.presets.is_empty());
    }

    #[test]
    fn test_destroy_clears_instruments() {
        let mut bank = BasicSoundBank::new();
        bank.add_instrument(make_instrument("X"));
        bank.destroy_sound_bank();
        assert!(bank.instruments.is_empty());
    }

    #[test]
    fn test_destroy_clears_samples() {
        let mut bank = BasicSoundBank::new();
        bank.add_sample(make_sample("S"));
        bank.destroy_sound_bank();
        assert!(bank.samples.is_empty());
    }

    // -----------------------------------------------------------------------
    // parse_internal / is_xg_bank
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_internal_no_xg_drums_is_false() {
        let mut bank = BasicSoundBank::new();
        bank.add_preset(make_preset("Piano", 0, 0));
        bank.parse_internal();
        assert!(!bank.is_xg_bank());
    }

    #[test]
    fn test_parse_internal_xg_drums_valid_program_is_true() {
        let mut bank = BasicSoundBank::new();
        // bank_msb = 127 (XG drums), program = 0 (allowed)
        bank.add_preset(make_preset("Drums", 0, 127));
        bank.parse_internal();
        assert!(bank.is_xg_bank());
    }

    #[test]
    fn test_parse_internal_xg_drums_invalid_program_is_false() {
        let mut bank = BasicSoundBank::new();
        // bank_msb = 127, program = 100 (not in allowed list)
        bank.add_preset(make_preset("BadDrums", 100, 127));
        bank.parse_internal();
        assert!(!bank.is_xg_bank());
    }

    #[test]
    fn test_parse_internal_bank_120_valid() {
        let mut bank = BasicSoundBank::new();
        bank.add_preset(make_preset("Drums120", 0, 120));
        bank.parse_internal();
        assert!(bank.is_xg_bank());
    }

    // -----------------------------------------------------------------------
    // flush
    // -----------------------------------------------------------------------

    #[test]
    fn test_flush_sorts_presets_by_program() {
        let mut bank = BasicSoundBank::new();
        bank.add_preset(make_preset("Z", 5, 0));
        bank.add_preset(make_preset("A", 0, 0));
        bank.add_preset(make_preset("M", 2, 0));
        bank.flush();
        let programs: Vec<u8> = bank.presets.iter().map(|p| p.program).collect();
        assert_eq!(programs, vec![0, 2, 5]);
    }

    #[test]
    fn test_flush_calls_parse_internal() {
        let mut bank = BasicSoundBank::new();
        // XG drum preset - valid program
        bank.add_preset(make_preset("XGDrums", 0, 127));
        bank.flush();
        assert!(bank.is_xg_bank());
    }

    // -----------------------------------------------------------------------
    // get_preset / PresetResolver
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_preset_returns_none_when_empty() {
        let bank = BasicSoundBank::new();
        assert!(bank.get_preset(any_patch(), SynthSystem::Gm).is_none());
    }

    #[test]
    fn test_get_preset_returns_some_when_has_presets() {
        let mut bank = BasicSoundBank::new();
        bank.add_preset(make_preset("Piano", 0, 0));
        assert!(bank.get_preset(any_patch(), SynthSystem::Gm).is_some());
    }

    #[test]
    fn test_preset_resolver_trait_empty_returns_none() {
        let bank = BasicSoundBank::new();
        let resolver: &dyn PresetResolver = &bank;
        assert!(resolver.get_preset(any_patch(), SynthSystem::Gm).is_none());
    }

    #[test]
    fn test_preset_resolver_trait_returns_some() {
        let mut bank = BasicSoundBank::new();
        bank.add_preset(make_preset("Piano", 0, 0));
        let resolver: &dyn PresetResolver = &bank;
        assert!(resolver.get_preset(any_patch(), SynthSystem::Gm).is_some());
    }

    #[test]
    fn test_preset_resolver_box_dyn() {
        let mut bank = BasicSoundBank::new();
        bank.add_preset(make_preset("Piano", 0, 0));
        let resolver: Box<dyn PresetResolver> = Box::new(bank);
        assert!(resolver.get_preset(any_patch(), SynthSystem::Gm).is_some());
    }

    // -----------------------------------------------------------------------
    // copy_from
    // -----------------------------------------------------------------------

    #[test]
    fn test_copy_from_clones_preset_count() {
        let mut src = BasicSoundBank::new();
        src.add_preset(make_preset("Piano", 0, 0));
        src.add_preset(make_preset("Guitar", 25, 0));
        let dst = BasicSoundBank::copy_from(&src);
        assert_eq!(dst.presets.len(), 2);
    }

    #[test]
    fn test_copy_from_clones_info_name() {
        let mut src = BasicSoundBank::new();
        src.sound_bank_info.name = "MyBank".to_string();
        let dst = BasicSoundBank::copy_from(&src);
        assert_eq!(dst.sound_bank_info.name, "MyBank");
    }

    #[test]
    fn test_copy_from_independent_preset_mutation() {
        let mut src = BasicSoundBank::new();
        src.add_preset(make_preset("Piano", 0, 0));
        let mut dst = BasicSoundBank::copy_from(&src);
        dst.presets[0].name = "Organ".to_string();
        assert_eq!(src.presets[0].name, "Piano");
    }

    // -----------------------------------------------------------------------
    // clone_sample_from
    // -----------------------------------------------------------------------

    #[test]
    fn test_clone_sample_from_adds_to_samples() {
        let src_samples = vec![make_sample("Violin")];
        let mut bank = BasicSoundBank::new();
        bank.clone_sample_from(&src_samples[0], &src_samples);
        assert_eq!(bank.samples.len(), 1);
        assert_eq!(bank.samples[0].name, "Violin");
    }

    #[test]
    fn test_clone_sample_from_deduplicates_by_name() {
        let src_samples = vec![make_sample("Piano")];
        let mut bank = BasicSoundBank::new();
        let idx1 = bank.clone_sample_from(&src_samples[0], &src_samples);
        let idx2 = bank.clone_sample_from(&src_samples[0], &src_samples);
        assert_eq!(idx1, idx2);
        assert_eq!(bank.samples.len(), 1);
    }

    #[test]
    fn test_clone_sample_from_copies_audio_data() {
        let mut src = make_sample("S");
        src.set_audio_data(vec![0.1, 0.2, 0.3], 44_100);
        let src_samples = vec![src];
        let mut bank = BasicSoundBank::new();
        bank.clone_sample_from(&src_samples[0], &src_samples);
        assert_eq!(bank.samples[0].audio_data.as_ref().unwrap().len(), 3);
    }

    // -----------------------------------------------------------------------
    // clone_instrument_from
    // -----------------------------------------------------------------------

    #[test]
    fn test_clone_instrument_from_adds_to_instruments() {
        let src_samples: Vec<BasicSample> = vec![make_sample("S")];
        let src_instruments = vec![make_instrument("Piano")];
        let mut bank = BasicSoundBank::new();
        bank.clone_instrument_from(&src_instruments[0], &src_samples);
        assert_eq!(bank.instruments.len(), 1);
        assert_eq!(bank.instruments[0].name, "Piano");
    }

    #[test]
    fn test_clone_instrument_from_deduplicates_by_name() {
        let src_samples: Vec<BasicSample> = vec![];
        let src_instruments = vec![make_instrument("Guitar")];
        let mut bank = BasicSoundBank::new();
        let idx1 = bank.clone_instrument_from(&src_instruments[0], &src_samples);
        let idx2 = bank.clone_instrument_from(&src_instruments[0], &src_samples);
        assert_eq!(idx1, idx2);
        assert_eq!(bank.instruments.len(), 1);
    }

    // -----------------------------------------------------------------------
    // clone_preset_from
    // -----------------------------------------------------------------------

    #[test]
    fn test_clone_preset_from_adds_to_presets() {
        let src_samples: Vec<BasicSample> = vec![];
        let src_instruments: Vec<BasicInstrument> = vec![];
        let src_presets = vec![make_preset("Piano", 0, 0)];
        let mut bank = BasicSoundBank::new();
        bank.clone_preset_from(&src_presets[0], &src_instruments, &src_samples);
        assert_eq!(bank.presets.len(), 1);
        assert_eq!(bank.presets[0].name, "Piano");
    }

    #[test]
    fn test_clone_preset_from_deduplicates_by_name() {
        let src_samples: Vec<BasicSample> = vec![];
        let src_instruments: Vec<BasicInstrument> = vec![];
        let src_presets = vec![make_preset("Violin", 40, 0)];
        let mut bank = BasicSoundBank::new();
        let idx1 = bank.clone_preset_from(&src_presets[0], &src_instruments, &src_samples);
        let idx2 = bank.clone_preset_from(&src_presets[0], &src_instruments, &src_samples);
        assert_eq!(idx1, idx2);
        assert_eq!(bank.presets.len(), 1);
    }

    // -----------------------------------------------------------------------
    // merge_sound_banks
    // -----------------------------------------------------------------------

    #[test]
    fn test_merge_single_bank_returns_same_presets() {
        let mut bank = BasicSoundBank::new();
        bank.add_preset(make_preset("Piano", 0, 0));
        bank.add_preset(make_preset("Guitar", 25, 0));
        let merged = BasicSoundBank::merge_sound_banks(vec![bank]);
        assert_eq!(merged.presets.len(), 2);
    }

    #[test]
    fn test_merge_two_banks_deduplicates() {
        let mut b1 = BasicSoundBank::new();
        b1.add_preset(make_preset("Piano", 0, 0));

        let mut b2 = BasicSoundBank::new();
        b2.add_preset(make_preset("Piano", 0, 0)); // duplicate
        b2.add_preset(make_preset("Guitar", 25, 0));

        let merged = BasicSoundBank::merge_sound_banks(vec![b1, b2]);
        // "Piano" from b1 + "Guitar" from b2 = 2 unique presets
        assert_eq!(merged.presets.len(), 2);
    }

    #[test]
    fn test_merge_preserves_first_bank_info() {
        let mut b1 = BasicSoundBank::new();
        b1.sound_bank_info.name = "First".to_string();
        let mut b2 = BasicSoundBank::new();
        b2.sound_bank_info.name = "Second".to_string();
        let merged = BasicSoundBank::merge_sound_banks(vec![b1, b2]);
        assert_eq!(merged.sound_bank_info.name, "First");
    }

    // -----------------------------------------------------------------------
    // delete_preset / reindexing
    // -----------------------------------------------------------------------

    #[test]
    fn test_delete_preset_removes_from_list() {
        let mut bank = BasicSoundBank::new();
        bank.add_preset(make_preset("Piano", 0, 0));
        bank.add_preset(make_preset("Guitar", 25, 0));
        bank.delete_preset(0);
        assert_eq!(bank.presets.len(), 1);
        assert_eq!(bank.presets[0].name, "Guitar");
    }

    #[test]
    fn test_delete_instrument_removes_from_list() {
        let mut bank = BasicSoundBank::new();
        bank.add_instrument(make_instrument("A"));
        bank.add_instrument(make_instrument("B"));
        // A has use_count == 0, so we can delete it
        bank.delete_instrument(0);
        assert_eq!(bank.instruments.len(), 1);
        assert_eq!(bank.instruments[0].name, "B");
    }

    #[test]
    fn test_delete_sample_removes_from_list() {
        let mut bank = BasicSoundBank::new();
        bank.add_sample(make_sample("X"));
        bank.add_sample(make_sample("Y"));
        bank.delete_sample(0);
        assert_eq!(bank.samples.len(), 1);
        assert_eq!(bank.samples[0].name, "Y");
    }

    // -----------------------------------------------------------------------
    // remove_unused_elements
    // -----------------------------------------------------------------------

    #[test]
    fn test_remove_unused_elements_clears_orphaned_instruments() {
        let mut bank = BasicSoundBank::new();
        // Instrument with use_count == 0 (not linked to any preset)
        bank.add_instrument(make_instrument("Orphan"));
        bank.remove_unused_elements();
        assert!(bank.instruments.is_empty());
    }

    #[test]
    fn test_remove_unused_elements_clears_orphaned_samples() {
        let mut bank = BasicSoundBank::new();
        // Sample with use_count == 0 (not linked to any instrument)
        bank.add_sample(make_sample("OrphanSample"));
        bank.remove_unused_elements();
        assert!(bank.samples.is_empty());
    }

    // -----------------------------------------------------------------------
    // parsing_error
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "SF parsing error")]
    fn test_parsing_error_panics() {
        let bank = BasicSoundBank::new();
        bank.parsing_error("bad chunk");
    }

    // -----------------------------------------------------------------------
    // build_reindex / build_shift_reindex (private via indirect testing)
    // -----------------------------------------------------------------------

    #[test]
    fn test_shift_reindex_lower_elements_unchanged() {
        // Removing element at idx 2 from a 4-element array
        let reindex = BasicSoundBank::build_shift_reindex(4, 2);
        assert_eq!(reindex[0], Some(0));
        assert_eq!(reindex[1], Some(1));
        assert_eq!(reindex[2], None); // removed
        assert_eq!(reindex[3], Some(2)); // shifted
    }
}
