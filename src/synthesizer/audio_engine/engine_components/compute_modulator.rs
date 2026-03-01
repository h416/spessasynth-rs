/// compute_modulator.rs
/// purpose: function for computing all modulators on a voice.
/// Ported from: src/synthesizer/audio_engine/engine_components/compute_modulator.ts
///
/// # TypeScript vs Rust design differences
///
/// ## this: MIDIChannel → ChannelContext trait
/// TypeScript's `this: MIDIChannel` is abstracted into a `ChannelContext` trait in Rust.
/// Implement this trait when `MIDIChannel` is ported.
///
/// ## Voice → VoiceContext trait
/// `Voice` is also not yet ported, so it is abstracted into a `VoiceContext` trait.
///
/// ## Separation of modulated_generators
/// TypeScript: directly overwrites `voice.modulatedGenerators`.
/// Rust: since `voice.compute_single_modulator(&mut self)` requires the same `&mut Voice` borrow,
/// `modulated_generators` is passed separately as `&mut [i16]` by the caller.
/// When Voice is ported, extract via `voice.modulated_generators_mut()` before calling this function.
///
/// ## generator_offsets (Mode 2)
/// TypeScript: creates an offset-applied copy with `generators = new Int16Array(generators)`.
/// Rust: uses the helper function `effective_base` to compute offsets per index.
use crate::soundbank::basic_soundbank::generator_types::GENERATOR_LIMITS;
use crate::soundbank::basic_soundbank::modulator::DecodedModulator;
use crate::soundbank::enums::modulator_sources;
use crate::synthesizer::audio_engine::engine_components::controller_tables::NON_CC_INDEX_OFFSET;

// ---------------------------------------------------------------------------
// SourceFilter
// ---------------------------------------------------------------------------

/// Represents the mode for `computeModulators`.
/// Equivalent to: `-1 | 0 | 1` (TypeScript union)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SourceFilter {
    /// Compute all modulators (default).
    /// Equivalent to: `sourceUsesCC = -1`
    #[default]
    All,
    /// Compute only modulators using non-CC (modulator enum) sources.
    /// Equivalent to: `sourceUsesCC = 0`
    NonCC,
    /// Compute only modulators using CC (MIDI controller) sources.
    /// Equivalent to: `sourceUsesCC = 1`
    CC,
}

// ---------------------------------------------------------------------------
// ChannelContext trait
// ---------------------------------------------------------------------------

/// Operations that `compute_modulators` requires from `MIDIChannel`.
/// Implement this trait when porting `MIDIChannel`.
pub trait ChannelContext {
    /// Whether generator offsets are enabled.
    /// Equivalent to: `this.generatorOffsetsEnabled`
    fn generator_offsets_enabled(&self) -> bool;

    /// Per-channel generator offsets (same size as the number of generators).
    /// Equivalent to: `this.generatorOffsets`
    fn generator_offsets(&self) -> &[i16];

    /// Whether to use per-note pitch wheels.
    /// Equivalent to: `this.perNotePitch`
    fn per_note_pitch(&self) -> bool;

    /// Per-note pitch wheel values (indexed by realKey).
    /// Equivalent to: `this.pitchWheels`
    fn pitch_wheels(&self) -> &[i16];

    /// MIDI controller table (14-bit, `CONTROLLER_TABLE_SIZE` size).
    /// Equivalent to: `this.midiControllers`
    fn midi_controllers(&self) -> &[i16];
}

// ---------------------------------------------------------------------------
// VoiceContext trait
// ---------------------------------------------------------------------------

/// Operations that `compute_modulators` requires from `Voice`.
/// Implement this trait when porting `Voice`.
pub trait VoiceContext {
    /// The voice's decoded modulator list.
    /// Equivalent to: `voice.modulators` (DecodedModulator[])
    fn decoded_modulators(&self) -> &[DecodedModulator];

    /// The voice's base generator values (i16).
    /// Equivalent to: `voice.generators`
    fn generators(&self) -> &[i16];

    /// The voice's actual MIDI key number (for per-note pitch).
    /// Equivalent to: `voice.realKey`
    fn real_key(&self) -> usize;

    /// Computes the modulator at the specified index, caches and returns the value.
    /// Also writes the result to `modulator_values()[index]`.
    ///
    /// Equivalent to: `voice.computeModulator(midiControllers, pitch, i)`
    fn compute_single_modulator(
        &mut self,
        midi_controllers: &[i16],
        pitch: i16,
        index: usize,
    ) -> f64;

    /// Cached modulator values (same size as the number of modulators).
    /// Stored as i16 to match TypeScript's `Int16Array`.
    /// Equivalent to: `voice.modulatorValues`
    fn modulator_values(&self) -> &[i16];
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Returns the generator base value after applying offsets.
/// Equivalent to: `generators[i]` after `generators[i] += this.generatorOffsets[i]`
#[inline]
fn effective_base(gens: &[i16], offsets: &[i16], i: usize, offsets_enabled: bool) -> i16 {
    let base = gens.get(i).copied().unwrap_or(0);
    if offsets_enabled {
        let off = offsets.get(i).copied().unwrap_or(0);
        base.saturating_add(off)
    } else {
        base
    }
}

/// Extracts the CC flag from bit7 of `source_enum` (SF2 spec).
/// Equivalent to: `primarySource.isCC`
#[inline]
fn source_is_cc(source_enum: u16) -> bool {
    (source_enum >> 7) & 1 != 0
}

/// Extracts the source index from bits 0-6 of `source_enum` (SF2 spec).
/// Equivalent to: `primarySource.index`
#[inline]
fn source_index(source_enum: u16) -> usize {
    (source_enum & 0x7F) as usize
}

// ---------------------------------------------------------------------------
// compute_modulators
// ---------------------------------------------------------------------------

/// Computes voice modulators and updates `modulated_generators`.
///
/// - `filter = SourceFilter::All` (default) : Compute all modulators (called every render cycle).
/// - `filter = SourceFilter::CC | NonCC`   : Update only modulators affected by the specified source
///   (optimization path for controller changes).
///
/// `modulated_generators` must be extracted from Voice by the caller (for borrow separation).
///
/// Equivalent to:
/// ```ts
/// export function computeModulators(
///     this: MIDIChannel, voice: Voice,
///     sourceUsesCC: -1 | 0 | 1 = -1, sourceIndex = 0)
/// ```
pub fn compute_modulators<C: ChannelContext, V: VoiceContext>(
    channel: &C,
    voice: &mut V,
    modulated_generators: &mut [i16],
    filter: SourceFilter,
    source_index_param: usize,
) {
    // Get pitch wheel value
    // Equivalent to:
    //   const pitch = this.perNotePitch
    //     ? this.pitchWheels[voice.realKey]
    //     : this.midiControllers[modulatorSources.pitchWheel + NON_CC_INDEX_OFFSET]
    let pitch: i16 = if channel.per_note_pitch() {
        let key = voice.real_key();
        channel.pitch_wheels().get(key).copied().unwrap_or(0)
    } else {
        let pitch_idx = modulator_sources::PITCH_WHEEL as usize + NON_CC_INDEX_OFFSET;
        channel
            .midi_controllers()
            .get(pitch_idx)
            .copied()
            .unwrap_or(0)
    };

    let midi_controllers = channel.midi_controllers();

    match filter {
        // ---------------------------------------------------------------
        // All mode: compute all modulators
        // ---------------------------------------------------------------
        SourceFilter::All => {
            // Step 1: Copy generators → modulated_generators (with offsets applied)
            // Equivalent to:
            //   if (this.generatorOffsetsEnabled) {
            //     generators = new Int16Array(generators);
            //     generators[i] += this.generatorOffsets[i];
            //   }
            //   modulatedGenerators.set(generators);
            {
                let base = voice.generators();
                let offsets = channel.generator_offsets();
                let offsets_enabled = channel.generator_offsets_enabled();
                let len = base.len().min(modulated_generators.len());
                for (i, mg) in modulated_generators.iter_mut().enumerate().take(len) {
                    *mg = effective_base(base, offsets, i, offsets_enabled);
                }
            }

            // Step 2: Compute each modulator and add to destination
            // Equivalent to:
            //   for (let i = 0; i < modulators.length; i++) {
            //     modulatedGenerators[mod.destination] = clamp(
            //         modulatedGenerators[mod.destination] + voice.computeModulator(...))
            let num_mods = voice.decoded_modulators().len();
            for i in 0..num_mods {
                // Get destination (limiting borrow scope)
                let dest_raw = { voice.decoded_modulators()[i].destination };
                if dest_raw < 0 || (dest_raw as usize) >= modulated_generators.len() {
                    continue; // Skip invalid destination
                }
                let dest = dest_raw as usize;

                // Compute modulator value (separate borrow needed for &mut self)
                let mod_val = voice.compute_single_modulator(midi_controllers, pitch, i);

                // Clamp and add (prevent i16 overflow)
                // TS: modulatedGenerators[mod.destination] + voice.computeModulator(...)
                // TS does f64 addition first, then Int16Array truncates to i16.
                // Use f64 to match TypeScript's number type precision.
                let current = modulated_generators[dest] as f64;
                let new_val = (current + mod_val) as i32;
                let new_val = new_val.clamp(-32_768, 32_767);
                modulated_generators[dest] = new_val as i16;
            }

            // Step 3: Clamp to GENERATOR_LIMITS
            // Equivalent to:
            //   for (let gen = 0; gen < modulatedGenerators.length; gen++) {
            //     const limit = generatorLimits[gen];
            //     if (!limit) continue;
            //     modulatedGenerators[gen] = Math.min(limit.max, Math.max(limit.min, ...))
            for (g, mg) in modulated_generators.iter_mut().enumerate() {
                if let Some(Some(limit)) = GENERATOR_LIMITS.get(g) {
                    let clamped = (*mg as i32).clamp(limit.min, limit.max);
                    *mg = clamped as i16;
                }
            }
        }

        // ---------------------------------------------------------------
        // Optimized mode: update only modulators affected by specified source
        // ---------------------------------------------------------------
        SourceFilter::NonCC | SourceFilter::CC => {
            let target_is_cc = filter == SourceFilter::CC;
            let num_mods = voice.decoded_modulators().len();

            for i in 0..num_mods {
                // Get modulator source info and destination
                // Equivalent to: mod.primarySource.isCC / .index / secondarySource.*
                let (dest_raw, influenced) = {
                    let mod_ = &voice.decoded_modulators()[i];
                    let prim_is_cc = source_is_cc(mod_.source_enum);
                    let prim_idx = source_index(mod_.source_enum);
                    let sec_is_cc = source_is_cc(mod_.secondary_source_enum);
                    let sec_idx = source_index(mod_.secondary_source_enum);

                    // Equivalent to:
                    //   (mod.primarySource.isCC === sourceCC &&
                    //    mod.primarySource.index === sourceIndex) ||
                    //   (mod.secondarySource.isCC === sourceCC &&
                    //    mod.secondarySource.index === sourceIndex)
                    let influenced = (prim_is_cc == target_is_cc && prim_idx == source_index_param)
                        || (sec_is_cc == target_is_cc && sec_idx == source_index_param);

                    (mod_.destination, influenced)
                };

                if !influenced {
                    continue;
                }
                if dest_raw < 0 || (dest_raw as usize) >= modulated_generators.len() {
                    continue;
                }
                let dest = dest_raw as usize;

                // Base generator value (with offsets applied)
                // Equivalent to: let outputValue = generators[destination]
                let base_val = {
                    let gens = voice.generators();
                    let offsets = channel.generator_offsets();
                    let offsets_enabled = channel.generator_offsets_enabled();
                    effective_base(gens, offsets, dest, offsets_enabled) as i32
                };

                // Compute modulator (also writes to voice.modulatorValues[i])
                // Equivalent to: voice.computeModulator(this.midiControllers, pitch, i)
                voice.compute_single_modulator(midi_controllers, pitch, i);

                // Sum all modulator values for this destination
                // Equivalent to:
                //   for (let j = 0; j < modulators.length; j++) {
                //     if (modulators[j].destination === destination) {
                //       outputValue += voice.modulatorValues[j];
                //     }
                //   }
                // Sum all modulator values (i16, matching TS Int16Array) for this destination.
                // TS: outputValue += voice.modulatorValues[j]; (Int16Array → integer sum)
                let output = {
                    let mods = voice.decoded_modulators();
                    let vals = voice.modulator_values();
                    let mut sum = base_val;
                    for (j, mod_) in mods.iter().enumerate() {
                        if mod_.destination == dest_raw {
                            sum += vals.get(j).copied().unwrap_or(0) as i32;
                        }
                    }
                    sum
                };

                // Apply limits
                // Equivalent to:
                //   modulatedGenerators[destination] = Math.max(limits.min,
                //       Math.min(outputValue, limits.max))
                if let Some(Some(limit)) = GENERATOR_LIMITS.get(dest) {
                    modulated_generators[dest] = output.max(limit.min).min(limit.max) as i16;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
    use crate::soundbank::basic_soundbank::modulator::DecodedModulator;

    // -----------------------------------------------------------------------
    // Mock implementations
    // -----------------------------------------------------------------------

    struct MockChannel {
        offsets_enabled: bool,
        offsets: Vec<i16>,
        per_note_pitch: bool,
        pitch_wheels: Vec<i16>,
        midi_controllers: Vec<i16>,
    }

    impl MockChannel {
        fn simple(midi_controllers: Vec<i16>) -> Self {
            Self {
                offsets_enabled: false,
                offsets: vec![0; 64],
                per_note_pitch: false,
                pitch_wheels: vec![0; 128],
                midi_controllers,
            }
        }
    }

    impl ChannelContext for MockChannel {
        fn generator_offsets_enabled(&self) -> bool {
            self.offsets_enabled
        }
        fn generator_offsets(&self) -> &[i16] {
            &self.offsets
        }
        fn per_note_pitch(&self) -> bool {
            self.per_note_pitch
        }
        fn pitch_wheels(&self) -> &[i16] {
            &self.pitch_wheels
        }
        fn midi_controllers(&self) -> &[i16] {
            &self.midi_controllers
        }
    }

    struct MockVoice {
        modulators: Vec<DecodedModulator>,
        generators: Vec<i16>,
        modulator_values: Vec<i16>,
        real_key: usize,
        /// Simple mock that returns a fixed value without using closures
        fixed_mod_value: f64,
    }

    impl MockVoice {
        fn new(generators: Vec<i16>) -> Self {
            Self {
                modulators: vec![],
                generators,
                modulator_values: vec![],
                real_key: 60,
                fixed_mod_value: 0.0,
            }
        }
        fn with_modulators(mut self, mods: Vec<DecodedModulator>, mod_val: f64) -> Self {
            self.modulator_values = vec![0; mods.len()];
            self.modulators = mods;
            self.fixed_mod_value = mod_val;
            self
        }
    }

    impl VoiceContext for MockVoice {
        fn decoded_modulators(&self) -> &[DecodedModulator] {
            &self.modulators
        }
        fn generators(&self) -> &[i16] {
            &self.generators
        }
        fn real_key(&self) -> usize {
            self.real_key
        }
        fn compute_single_modulator(
            &mut self,
            _midi_controllers: &[i16],
            _pitch: i16,
            index: usize,
        ) -> f64 {
            // Store as i16 (matching TS Int16Array truncation), return full f64 value
            if let Some(v) = self.modulator_values.get_mut(index) {
                *v = self.fixed_mod_value as i16;
            }
            self.fixed_mod_value
        }
        fn modulator_values(&self) -> &[i16] {
            &self.modulator_values
        }
    }

    /// Helper to create a decoded modulator
    fn make_mod(dest: i16, source_enum: u16, secondary_source_enum: u16) -> DecodedModulator {
        DecodedModulator::new(source_enum, secondary_source_enum, dest, 0, 0)
    }

    // source_enum: CC flag bit7 = 1, index = 7 (CC7 = main volume)
    const CC7_SOURCE: u16 = (1 << 7) | 7;
    // source_enum: non-CC, index = 14 (pitchWheel = 14)
    const PITCH_WHEEL_SOURCE: u16 = 14; // bit7 = 0

    // -----------------------------------------------------------------------
    // SourceFilter::All - generators copy tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_all_copies_generators_to_modulated() {
        let mut ch = MockChannel::simple(vec![0; 147]);
        let mut voice = MockVoice::new(vec![100, 200, 300]);
        let mut modulated = vec![0i16; 3];

        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::All, 0);

        // No modulators, no offsets → generators copied as-is
        assert_eq!(modulated[0], 100);
        assert_eq!(modulated[1], 200);
        assert_eq!(modulated[2], 300);
    }

    #[test]
    fn test_all_applies_generator_offsets() {
        let mut ch = MockChannel::simple(vec![0; 147]);
        ch.offsets_enabled = true;
        ch.offsets = vec![10, -5, 0];
        let mut voice = MockVoice::new(vec![100, 200, 300]);
        let mut modulated = vec![0i16; 3];

        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::All, 0);

        assert_eq!(modulated[0], 110); // 100 + 10
        assert_eq!(modulated[1], 195); // 200 + (-5)
        assert_eq!(modulated[2], 300); // 300 + 0
    }

    // -----------------------------------------------------------------------
    // SourceFilter::All - modulator addition tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_all_adds_modulator_value_to_destination() {
        let ch = MockChannel::simple(vec![0; 147]);
        // Modulator targeting PAN (17), value +50.0
        let mod_ = make_mod(gt::PAN, 0, 0);
        let mut voice = MockVoice::new(vec![0; 63]).with_modulators(vec![mod_], 50.0);
        let mut modulated = vec![0i16; 63];

        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::All, 0);

        // 50 is added to PAN(17) and clamped by GENERATOR_LIMITS
        // PAN limit: min=-500, max=500 → 50 is within range
        assert_eq!(modulated[gt::PAN as usize], 50);
    }

    #[test]
    fn test_all_clamps_modulator_sum_to_i16_range() {
        let ch = MockChannel::simple(vec![0; 147]);
        // Verify large value added to PAN is clamped to i16 range
        let mod_ = make_mod(gt::PAN, 0, 0);
        let mut voice = MockVoice::new(vec![0; 63]).with_modulators(vec![mod_], 40_000.0); // exceeds i16::MAX
        let mut modulated = vec![0i16; 63];

        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::All, 0);

        // First clamped to 32_767, then clamped to PAN limit.max=500
        assert_eq!(modulated[gt::PAN as usize], 500);
    }

    #[test]
    fn test_all_clamps_to_generator_limits() {
        let ch = MockChannel::simple(vec![0; 147]);
        // Case where initial value exceeds limit.max
        let mut voice = MockVoice::new(vec![0; 63]);
        let mut modulated = vec![0i16; 63];
        // Set PAN above limit.max(500)
        modulated[gt::PAN as usize] = 600;

        // No modulators, generators are 0 → after set it's 0, after limit → 0
        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::All, 0);

        // generators are 0 so modulated is 0 → still 0 after limit
        assert_eq!(modulated[gt::PAN as usize], 0);
    }

    #[test]
    fn test_all_applies_generator_limits_from_generators() {
        let ch = MockChannel::simple(vec![0; 147]);
        // PAN limit: min=-500, max=500
        // generators[17] = 600 → clamped to limit.max=500
        let mut gens = vec![0i16; 63];
        gens[gt::PAN as usize] = 600;
        let mut voice = MockVoice::new(gens);
        let mut modulated = vec![0i16; 63];

        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::All, 0);

        assert_eq!(modulated[gt::PAN as usize], 500);
    }

    #[test]
    fn test_all_skips_invalid_modulator_destination() {
        let ch = MockChannel::simple(vec![0; 147]);
        // destination = -1 (INVALID) → skip
        let mod_ = make_mod(gt::INVALID, 0, 0);
        let mut voice = MockVoice::new(vec![0; 63]).with_modulators(vec![mod_], 100.0);
        let mut modulated = vec![0i16; 63];

        // Should not panic
        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::All, 0);
    }

    // -----------------------------------------------------------------------
    // SourceFilter::All - pitch tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_all_uses_midi_controllers_pitch_when_per_note_off() {
        // per_note_pitch = false → uses midiControllers[PITCH_WHEEL + OFFSET]
        // No modulators here so pitch isn't used, but verify no panic
        let mut ch = MockChannel::simple(vec![0; 147]);
        ch.per_note_pitch = false;
        // pitch_wheel index = 14 + 128 = 142
        if let Some(v) = ch.midi_controllers.get_mut(142) {
            *v = 1000;
        }
        let mut voice = MockVoice::new(vec![0; 63]);
        let mut modulated = vec![0i16; 63];

        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::All, 0);
        // OK if no panic
    }

    #[test]
    fn test_all_uses_per_note_pitch_wheels_when_enabled() {
        let mut ch = MockChannel::simple(vec![0; 147]);
        ch.per_note_pitch = true;
        ch.pitch_wheels = vec![500; 128];
        let mut voice = MockVoice::new(vec![0; 63]);
        voice.real_key = 60;
        let mut modulated = vec![0i16; 63];

        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::All, 0);
        // OK if no panic
    }

    // -----------------------------------------------------------------------
    // SourceFilter::CC / NonCC - optimized mode
    // -----------------------------------------------------------------------

    #[test]
    fn test_cc_mode_updates_matching_destination() {
        let ch = MockChannel::simple(vec![0; 147]);
        // CC7 (main volume) source → CC flag bit7=1, index=7
        let mod_ = make_mod(gt::PAN, CC7_SOURCE, 0);
        let mut voice = MockVoice::new(vec![0; 63]).with_modulators(vec![mod_], 30.0);
        let mut modulated = vec![0i16; 63];

        // Filter by CC source_index = 7
        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::CC, 7);

        assert_eq!(modulated[gt::PAN as usize], 30);
    }

    #[test]
    fn test_cc_mode_skips_non_matching_source() {
        let ch = MockChannel::simple(vec![0; 147]);
        // source_index = 1 vs mod index=7 → skip
        let mod_ = make_mod(gt::PAN, CC7_SOURCE, 0);
        let mut voice = MockVoice::new(vec![0; 63]).with_modulators(vec![mod_], 30.0);
        let mut modulated = vec![0i16; 63];

        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::CC, 1);

        assert_eq!(
            modulated[gt::PAN as usize],
            0,
            "non-matching source should not update"
        );
    }

    #[test]
    fn test_noncc_mode_matches_pitch_wheel_source() {
        let ch = MockChannel::simple(vec![0; 147]);
        // non-CC, index=14 (pitchWheel)
        let mod_ = make_mod(gt::PAN, PITCH_WHEEL_SOURCE, 0);
        let mut voice = MockVoice::new(vec![0; 63]).with_modulators(vec![mod_], 20.0);
        let mut modulated = vec![0i16; 63];

        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::NonCC, 14);

        assert_eq!(modulated[gt::PAN as usize], 20);
    }

    #[test]
    fn test_cc_mode_skips_noncc_source() {
        let ch = MockChannel::simple(vec![0; 147]);
        // non-CC source → does not match SourceFilter::CC
        let mod_ = make_mod(gt::PAN, PITCH_WHEEL_SOURCE, 0);
        let mut voice = MockVoice::new(vec![0; 63]).with_modulators(vec![mod_], 20.0);
        let mut modulated = vec![0i16; 63];

        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::CC, 14);

        assert_eq!(
            modulated[gt::PAN as usize],
            0,
            "CC filter should not match non-CC source"
        );
    }

    #[test]
    fn test_cc_mode_secondary_source_match() {
        let ch = MockChannel::simple(vec![0; 147]);
        // primary is non-CC, secondary is CC7 → matches SourceFilter::CC, source_index=7
        let mod_ = make_mod(gt::PAN, PITCH_WHEEL_SOURCE, CC7_SOURCE);
        let mut voice = MockVoice::new(vec![0; 63]).with_modulators(vec![mod_], 15.0);
        let mut modulated = vec![0i16; 63];

        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::CC, 7);

        assert_eq!(modulated[gt::PAN as usize], 15);
    }

    #[test]
    fn test_optimized_mode_sums_all_mods_for_destination() {
        let ch = MockChannel::simple(vec![0; 147]);
        // Two modulators with the same destination (PAN)
        let mod1 = make_mod(gt::PAN, CC7_SOURCE, 0);
        let mod2 = make_mod(gt::PAN, 0, 0); // secondary, non-matching
        let mut voice = MockVoice::new(vec![0; 63]).with_modulators(vec![mod1, mod2], 10.0);
        let mut modulated = vec![0i16; 63];

        // Filter by CC7 → mod1 is affected → sum all PAN modulators
        // mod1.value=10, mod2.value=10 → sum=20
        // However mod2's source doesn't match CC7 (source_enum=0 → index=0, is_cc=false)
        // → only mod1 is computed, modulator_values = [10, 0]
        // sum = base(0) + 10 + 0 = 10
        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::CC, 7);

        // modulator_values[0] = 10 (computed), modulator_values[1] = 0 (not computed)
        // output = 0 + 10 + 0 = 10 → clamped to PAN limits (-500..500) = 10
        assert_eq!(modulated[gt::PAN as usize], 10);
    }

    #[test]
    fn test_optimized_mode_applies_generator_base_value() {
        let ch = MockChannel::simple(vec![0; 147]);
        let mod_ = make_mod(gt::PAN, CC7_SOURCE, 0);
        // PAN base value = 50
        let mut gens = vec![0i16; 63];
        gens[gt::PAN as usize] = 50;
        let mut voice = MockVoice::new(gens).with_modulators(vec![mod_], 30.0);
        let mut modulated = vec![0i16; 63];

        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::CC, 7);

        // output = base(50) + mod(30) = 80
        assert_eq!(modulated[gt::PAN as usize], 80);
    }

    #[test]
    fn test_optimized_mode_with_generator_offsets() {
        let mut ch = MockChannel::simple(vec![0; 147]);
        ch.offsets_enabled = true;
        ch.offsets = vec![5; 63];
        let mod_ = make_mod(gt::PAN, CC7_SOURCE, 0);
        let mut gens = vec![0i16; 63];
        gens[gt::PAN as usize] = 100;
        let mut voice = MockVoice::new(gens).with_modulators(vec![mod_], 0.0);
        let mut modulated = vec![0i16; 63];

        compute_modulators(&ch, &mut voice, &mut modulated, SourceFilter::CC, 7);

        // base = 100 + offset(5) = 105, mod = 0 → output = 105
        assert_eq!(modulated[gt::PAN as usize], 105);
    }

    // -----------------------------------------------------------------------
    // source_is_cc / source_index helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_source_is_cc_false_for_bit7_zero() {
        assert!(!source_is_cc(0b0000_0111)); // bit7=0
    }

    #[test]
    fn test_source_is_cc_true_for_bit7_one() {
        assert!(source_is_cc(0b1000_0111)); // bit7=1
    }

    #[test]
    fn test_source_index_extracts_low_7_bits() {
        assert_eq!(source_index(0b1001_0101), 0b001_0101); // = 21
        assert_eq!(source_index(0b0111_1111), 0x7F); // max = 127
        assert_eq!(source_index(0b1000_0000), 0);
    }

    // -----------------------------------------------------------------------
    // effective_base helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_effective_base_without_offset() {
        let gens = [10i16, 20, 30];
        assert_eq!(effective_base(&gens, &[], 1, false), 20);
    }

    #[test]
    fn test_effective_base_with_offset() {
        let gens = [10i16, 20, 30];
        let offsets = [1i16, 5, -3];
        assert_eq!(effective_base(&gens, &offsets, 1, true), 25); // 20 + 5
    }

    #[test]
    fn test_effective_base_saturates_on_overflow() {
        let gens = [i16::MAX];
        let offsets = [1i16];
        assert_eq!(effective_base(&gens, &offsets, 0, true), i16::MAX);
    }

    #[test]
    fn test_effective_base_out_of_bounds_returns_zero() {
        let gens: [i16; 0] = [];
        assert_eq!(effective_base(&gens, &[], 5, false), 0);
    }
}
