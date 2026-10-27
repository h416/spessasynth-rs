//! MIDI note on handler.
//! Ported from: src/synthesizer/audio_engine/channel/note_on.ts
use crate::midi::enums::midi_controllers;
use crate::soundbank::basic_soundbank::generator_types::{
    GENERATORS_AMOUNT, generator_types as gt,
};
use crate::soundbank::basic_soundbank::modulator::Modulator;
use crate::synthesizer::audio_engine::voice::compute_modulator::{
    compute_modulators, SourceFilter,
};
use crate::synthesizer::audio_engine::channel::portamento_time::portamento_time_to_seconds;
use crate::synthesizer::audio_engine::synth_constants::{
    GENERATOR_OVERRIDE_NO_CHANGE_VALUE, MIN_EXCLUSIVE_LENGTH,
};
use crate::soundbank::types::VoiceParameters;
use crate::synthesizer::audio_engine::sound_bank_manager::SoundBankManager;
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::audio_engine::user_drum_set::FALLBACK_PATCH;
use crate::synthesizer::audio_engine::voice::unit_converter::timecents_to_seconds;
use crate::synthesizer::types::{CachedVoiceList, NoteOnCallback, SynthProcessorEvent};
use crate::utils::loggin::spessa_synth_warn;

/// Clamps a value between min and max.
///
/// Mirrors TS `clamp = (num, min, max) => Math.max(min, Math.min(max, num))`
/// (min-then-max order). This matches the old `max(min).min(max)` behavior
/// whenever `min <= max`, but differs when `min > max`: TS clamps to `min`
/// in that degenerate case (e.g. `clamp(5, 0, -1) === 0`), while
/// `max(min).min(max)` would instead yield `-1`. The degenerate case arises
/// for zero-length samples where `end_exclusive - 1.0 == -1.0`.
#[inline]
fn clamp_f64(val: f64, min: f64, max: f64) -> f64 {
    val.min(max).max(min)
}

/// `Math.round` semantics: rounds half *up* (towards +infinity), which differs from
/// Rust's `f64::round` (half away from zero) on exact `.5` ties at negative values.
/// The random-pan values are exact multiples of `1000 / 2^32`, so ties are reachable.
#[inline]
fn js_round(val: f64) -> f64 {
    (val + 0.5).floor()
}

impl SynthesizerCore {
    /// Sends a MIDI Note On message and starts a note.
    ///
    /// Equivalent to: noteOn(midiNote, velocity) on MIDIChannel
    ///
    /// # Parameters
    /// - `channel`: MIDI channel index (0-based)
    /// - `midi_note`: MIDI note number (0-127)
    /// - `velocity`: Note velocity (0-127). Velocity 0 sends note-off instead.
    pub fn note_on(&mut self, channel: usize, midi_note: u8, velocity: u8) {
        self.note_on_internal(channel, midi_note, velocity, true);
    }

    /// Sends a MIDI Note On message and starts a note.
    ///
    /// `emit` mirrors TS's `noteOn(midiNote, velocity, emit = true)`: when `false` (used
    /// only by the mono-mode legato retrigger from `note_off`), the Note On ID for this
    /// note is peeked instead of consumed, and the public `NoteOn` event is not fired.
    ///
    /// Equivalent to: noteOn(midiNote, velocity, emit)
    pub(crate) fn note_on_internal(
        &mut self,
        channel: usize,
        midi_note: u8,
        velocity: u8,
        emit: bool,
    ) {
        if velocity < 1 {
            self.note_off_channel(channel, midi_note);
            return;
        }
        let velocity = velocity.min(127);

        let black = self.system_parameters.black_midi_mode;
        if
        // If black MIDI conditions are met...
        (black && self.voice_count > 200 && velocity < 40)
            || (black && velocity < 10)
            // Or channel is muted...
            || self.midi_channels[channel].is_muted
            // Or channel has no preset...
            || self.midi_channels[channel].preset.is_none()
        {
            return;
        }

        // Apply Velocity Sense and clamp
        let real_velocity = clamp_f64(
            ((velocity as f64)
                * (self.midi_channels[channel].midi_parameters.velocity_sense_depth as f64 / 64.0)
                + (self.midi_channels[channel].midi_parameters.velocity_sense_offset as f64 - 64.0)
                    * 2.0)
                .floor(),
            0.0,
            127.0,
        ) as u8;

        // Sanity check
        if midi_note > 127 {
            return;
        }
        // Drum keys may not receive note on
        if self.midi_channels[channel].drum_channel
            && !self.midi_channels[channel].drum_params[midi_note as usize].rx_note_on
        {
            return;
        }

        // Note which we should grab presets from (strictly internal)
        let mut sound_bank_note =
            midi_note as i32 + self.midi_channels[channel].current_key_shift();

        // MIDI Tuning Standard
        let program = self.midi_channels[channel].preset.as_ref().unwrap().program;
        let tune = self.tunings[program as usize * 128 + midi_note as usize];
        if tune >= 0.0 {
            // Overwrite the note with MIDI tuning standard!
            sound_bank_note = tune.trunc() as i32;
        }

        // The key shift may push the note out of range
        if !(0..=127).contains(&sound_bank_note) {
            return;
        }
        let sound_bank_note = sound_bank_note as u8;

        // Monophonic retrigger
        if self.midi_channels[channel]
            .system_parameters
            .monophonic_retrigger
            .unwrap_or(self.system_parameters.monophonic_retrigger)
            || self.midi_channels[channel].midi_parameters.assign_mode == 0
        {
            let current_time = self.current_time;
            // TS default: killNote(midiNote) uses releaseTime = -12_000.
            self.midi_channels[channel].kill_note(midi_note, -12_000, &mut self.voices, current_time);
        }

        // Gain
        let mut voice_gain = 1.0f64;

        // Portamento
        let ch = &mut self.midi_channels[channel];
        let previous_note = ch.last_portamento_note;
        let portamento_enabled = ch.portamento_force
            || ch.midi_controllers[midi_controllers::PORTAMENTO_ON_OFF as usize] >= 8192;

        // 14-bit MIDI CC -> 7-bit value
        let portamento_time = ch.midi_controllers[midi_controllers::PORTAMENTO_TIME as usize] >> 7;

        let can_apply_portamento = portamento_enabled // Enabled?
            && !ch.drum_channel // Not a drum channel?
            && previous_note >= 0 // Valid note?
            && previous_note != midi_note as i32 // Not the same note?
            && portamento_time > 0; // Non-instant time?

        let mut porta_from_key: i32 = -1;
        let mut porta_time: f64 = 0.0;

        if can_apply_portamento {
            let key_distance = (midi_note as i32 - previous_note).abs();

            porta_from_key = previous_note;
            porta_time = portamento_time_to_seconds(portamento_time as u8, key_distance as f64);

            ch.portamento_force = false;
        }

        // Always track the last note, even if portamento isn't applied.
        // See: https://github.com/spessasus/spessasynth_core/issues/77
        ch.last_portamento_note = midi_note as i32;

        ch.playing_notes[midi_note as usize] = true;

        // Mono mode
        if !self.midi_channels[channel].midi_parameters.poly_mode {
            let last_mono_note = self.midi_channels[channel].last_mono_note;
            if last_mono_note >= 0 && last_mono_note != midi_note as i32 {
                let current_time = self.current_time;
                self.midi_channels[channel].kill_note(
                    last_mono_note as u8,
                    -12_000,
                    &mut self.voices,
                    current_time,
                );
            }
            self.midi_channels[channel].last_mono_note = midi_note as i32;
            self.midi_channels[channel].last_mono_velocity = velocity;
        }

        // Get voices
        // A missing preset yields an empty voice list rather than aborting the note-on:
        // TS `getVoices` returns `[]` and execution continues, so the random-pan generator
        // is still advanced and the note ID still consumed. Bailing out early here would
        // desync the deterministic generator from the TS one.
        let cached_voices: CachedVoiceList = 'voices: {
            let (preset_clone, bank_idx) = {
                let ch = &self.midi_channels[channel];
                match (&ch.preset, ch.preset_bank_idx) {
                    (Some(p), Some(bi)) => (p.clone(), bi),
                    _ => break 'voices Vec::new(),
                }
            };
            self.get_cached_voices_impl(bank_idx, &preset_clone, sound_bank_note, real_velocity)
        };

        // Overrides
        // Zero means disabled
        let mut pan_override: f64 = 0.0;
        let mut exclusive_override: i32 = 0;
        let mut pitch_offset: f64 = 0.0;
        let mut reverb_gain: f64 = 1.0;
        let mut chorus_gain: f64 = 1.0;
        let mut variation_gain: f64 = 1.0;
        if self.midi_channels[channel].midi_parameters.random_pan {
            // The range is -500 to 500
            pan_override = js_round(self.random_generator.next_f64() * 1000.0 - 500.0);
        }

        // Drum parameters
        if self.midi_channels[channel].drum_channel {
            let p = &self.midi_channels[channel].drum_params[midi_note as usize];
            let drum_pan = p.pan as i32 - 64;
            // If pan is different from default then it's overridden
            if drum_pan != 0 {
                if drum_pan == -64 {
                    // Random pan
                    pan_override = js_round(self.random_generator.next_f64() * 1000.0 - 500.0);
                } else {
                    let channel_pan = (self.midi_channels[channel].midi_controllers
                        [midi_controllers::PAN as usize]
                        >> 7) as i32
                        - 64;
                    let target_pan = (drum_pan + channel_pan).clamp(-63, 63);
                    // Ensure that override is applied, even for zero
                    pan_override = (target_pan as f64 / 63.0) * 500.0;
                    if pan_override == 0.0 {
                        pan_override = 1.0;
                    }
                }
            }

            pitch_offset = p.pitch_fine + p.pitch_coarse * 100.0;
            exclusive_override = p.assign_group as i32;
            reverb_gain = p.reverb_send as f64 / 127.0;
            chorus_gain = p.chorus_send as f64 / 127.0;
            variation_gain = p.variation_send as f64 / 127.0;
            voice_gain = (p.level as f64 / 120.0).powi(2);
            self.gs_delay_active = self.gs_delay_active || variation_gain > 0.0;
        }

        // Groups voices from this Note On together (overlapping notes / mono legato
        // retrigger). A non-emitting (mono legato) retrigger peeks the current ID
        // instead of consuming it.
        let note_id = if emit {
            let id = self.midi_channels[channel].note_on_id[midi_note as usize];
            self.midi_channels[channel].note_on_id[midi_note as usize] += 1;
            id
        } else {
            self.midi_channels[channel].note_on_id[midi_note as usize]
        };

        // -----------------------------------------------------------------------
        // Assign and configure a voice slot for each cached voice
        // -----------------------------------------------------------------------
        let num_cached = cached_voices.len();
        for (_ci, cached) in cached_voices.iter().enumerate() {
            // Assign a free voice slot (returns index, not reference)
            let voice_idx = self.assign_voice_idx();

            // Setup basic voice state (raw played note).
            self.voices[voice_idx].setup(self.current_time, channel as u8, midi_note, note_id);

            // Select the correct oscillator
            // Channel takes precedence
            self.voices[voice_idx].oscillator_type = self.midi_channels[channel]
                .system_parameters
                .interpolation_type
                .unwrap_or(self.system_parameters.interpolation_type);

            // Copy unmodulated generators from cache
            if cached.generators.len() == GENERATORS_AMOUNT {
                self.voices[voice_idx]
                    .generators
                    .copy_from_slice(&cached.generators);
            } else {
                spessa_synth_warn(&format!(
                    "CachedVoice has {} generators, expected {}",
                    cached.generators.len(),
                    GENERATORS_AMOUNT
                ));
                for (i, &g) in cached.generators.iter().enumerate() {
                    if i < GENERATORS_AMOUNT {
                        self.voices[voice_idx].generators[i] = g;
                    }
                }
            }

            // Copy other cached fields (effective target key and velocity may be
            // overridden by generators).
            self.voices[voice_idx].target_key = cached.target_key;
            self.voices[voice_idx].velocity = cached.velocity as u8;
            self.voices[voice_idx].exclusive_class = cached.exclusive_class as i32;
            self.voices[voice_idx].root_key = cached.root_key;
            self.voices[voice_idx].looping_mode = cached.looping_mode;

            // Set sample data on the active oscillator
            let osc_idx = self.voices[voice_idx].oscillator_type as usize;
            self.voices[voice_idx].oscillators[osc_idx].sample_data =
                Some(cached.sample_data.clone());
            self.voices[voice_idx].oscillators[osc_idx].playback_step = cached.playback_step;

            // -----------------------------------------------------------------------
            // Set modulators: merge dynamic modulators when the manager is active.
            // Equivalent to: if (this.dynamicModulators.active) { ... } else { ... }
            // -----------------------------------------------------------------------
            let dynamic_active = self.midi_channels[channel].sys_ex_modulators.active;

            let final_modulators: Vec<Modulator> = if dynamic_active {
                // Clone cached modulators and apply SysEx overrides
                let mut mods: Vec<Modulator> = cached.modulators.clone();
                // Copy the SysEx modulator entries to avoid borrow issues
                let sysex_mods: Vec<Modulator> = self.midi_channels[channel]
                    .sys_ex_modulators
                    .modulator_list
                    .iter()
                    .map(|e| e.modulator.clone())
                    .collect();
                for sysex_mod in sysex_mods {
                    match mods
                        .iter()
                        .position(|m| Modulator::is_identical(m, &sysex_mod, false))
                    {
                        Some(pos) => mods[pos] = sysex_mod,
                        None => mods.push(sysex_mod),
                    }
                }
                mods
            } else {
                cached.modulators.clone()
            };

            // Convert Modulator -> DecodedModulator and assign to voice
            self.voices[voice_idx].modulators = final_modulators
                .iter()
                .map(|m| m.to_decoded())
                .collect();

            // Resize modulator_values table if needed
            let mod_len = self.voices[voice_idx].modulators.len();
            if mod_len > self.voices[voice_idx].modulator_values.len() {
                spessa_synth_warn(&format!(
                    "{} modulators! Increasing modulatorValues table.",
                    mod_len
                ));
                self.voices[voice_idx].modulator_values = vec![0i16; mod_len];
            }

            // -----------------------------------------------------------------------
            // Apply generator overrides (from setGeneratorOverride calls)
            // -----------------------------------------------------------------------
            if self.midi_channels[channel].generator_overrides_enabled {
                let overrides = self.midi_channels[channel].generator_overrides;
                for (gen_type, &override_val) in overrides.iter().enumerate() {
                    if override_val != GENERATOR_OVERRIDE_NO_CHANGE_VALUE {
                        self.voices[voice_idx].generators[gen_type] = override_val;
                    }
                }
            }

            // -----------------------------------------------------------------------
            // Exclusive class: kill same-class voices (except in mono mode, already killed)
            // -----------------------------------------------------------------------
            // Apply drum exclusive class override
            if exclusive_override != 0 {
                self.voices[voice_idx].exclusive_class = exclusive_override;
            }
            let excl_class = self.voices[voice_idx].exclusive_class;
            if excl_class != 0 && self.midi_channels[channel].midi_parameters.poly_mode {
                let ch_channel = self.midi_channels[channel].channel;
                let voice_count_ch = self.midi_channels[channel].voice_count;
                let current_time = self.current_time;
                if voice_count_ch > 0 {
                    let mut vc = 0u32;
                    for (i, v) in self.voices.iter_mut().enumerate() {
                        if i == voice_idx {
                            continue;
                        }
                        if v.is_active
                            && v.channel == ch_channel
                            && v.exclusive_class == excl_class
                            && v.has_rendered
                        {
                            v.exclusive_release(current_time, MIN_EXCLUSIVE_LENGTH);
                            vc += 1;
                            if vc >= voice_count_ch {
                                break;
                            }
                        }
                    }
                }
            }

            // -----------------------------------------------------------------------
            // Compute all modulators (fills modulated_generators)
            // -----------------------------------------------------------------------
            let mut modulated = [0i16; GENERATORS_AMOUNT];
            compute_modulators(
                &self.midi_channels[channel],
                &mut self.voices[voice_idx],
                &mut modulated,
                SourceFilter::All,
                0,
            );
            self.voices[voice_idx].modulated_generators = modulated;

            // -----------------------------------------------------------------------
            // Initialize envelopes and filter
            // -----------------------------------------------------------------------
            let modulated_copy = self.voices[voice_idx].modulated_generators;
            let target_key = self.voices[voice_idx].target_key;
            let start_time = self.voices[voice_idx].start_time;

            self.voices[voice_idx]
                .vol_env
                .init(&modulated_copy, target_key);
            self.voices[voice_idx]
                .mod_env
                .init(&modulated_copy, start_time, target_key);
            self.voices[voice_idx].filter.init();

            // Calculate LFO start times
            let now = self.current_time;
            self.voices[voice_idx].vib_lfo_start_time = now
                + timecents_to_seconds(modulated_copy[gt::DELAY_VIB_LFO as usize] as i32);
            self.voices[voice_idx].mod_lfo_start_time = now
                + timecents_to_seconds(modulated_copy[gt::DELAY_MOD_LFO as usize] as i32);

            // -----------------------------------------------------------------------
            // Compute sample offsets (from modulatedGenerators)
            // -----------------------------------------------------------------------
            let start_offset = modulated_copy[gt::START_ADDRS_OFFSET as usize] as i32
                + modulated_copy[gt::START_ADDRS_COARSE_OFFSET as usize] as i32 * 32_768;
            let end_offset = modulated_copy[gt::END_ADDR_OFFSET as usize] as i32
                + modulated_copy[gt::END_ADDRS_COARSE_OFFSET as usize] as i32 * 32_768;
            let loop_start_offset = modulated_copy[gt::STARTLOOP_ADDRS_OFFSET as usize] as i32
                + modulated_copy[gt::STARTLOOP_ADDRS_COARSE_OFFSET as usize] as i32 * 32_768;
            let loop_end_offset = modulated_copy[gt::ENDLOOP_ADDRS_OFFSET as usize] as i32
                + modulated_copy[gt::ENDLOOP_ADDRS_COARSE_OFFSET as usize] as i32 * 32_768;

            let osc_idx = self.voices[voice_idx].oscillator_type as usize;
            let sample_len = match &self.voices[voice_idx].oscillators[osc_idx].sample_data {
                Some(d) => d.len(),
                None => 0,
            };
            // End is exclusive, not inclusive.
            // Testcase: https://github.com/spessasus/spessasynth_core/issues/90
            let end_exclusive = sample_len as f64; // Length, not length - 1

            self.voices[voice_idx].oscillators[osc_idx].cursor =
                clamp_f64(start_offset as f64, 0.0, end_exclusive - 1.0);
            self.voices[voice_idx].oscillators[osc_idx].end =
                clamp_f64(end_exclusive + end_offset as f64, 0.0, end_exclusive);

            let loop_start_raw = cached.loop_start as f64 + loop_start_offset as f64;
            let loop_end_raw = cached.loop_end as f64 + loop_end_offset as f64;
            let mut ls = clamp_f64(loop_start_raw, 0.0, end_exclusive);
            let mut le = clamp_f64(loop_end_raw, 0.0, end_exclusive);

            // Swap if needed
            if le < ls {
                std::mem::swap(&mut ls, &mut le);
            }

            // Disable loop if range is < 1 sample (for continuous modes 1 and 3)
            let looping_mode = self.voices[voice_idx].looping_mode;
            if le - ls < 1.0 && (looping_mode == 1 || looping_mode == 3) {
                self.voices[voice_idx].looping_mode = 0;
            }

            self.voices[voice_idx].oscillators[osc_idx].loop_start = ls;
            self.voices[voice_idx].oscillators[osc_idx].loop_end = le;
            self.voices[voice_idx].oscillators[osc_idx].loop_length = le - ls;
            let lm = self.voices[voice_idx].looping_mode;
            self.voices[voice_idx].oscillators[osc_idx].is_looping =
                lm == 1 || lm == 3;
            self.voices[voice_idx].oscillators[osc_idx].has_looped = false;

            // -----------------------------------------------------------------------
            // Apply portamento, pan override, gain modifier, and initial current pan
            // -----------------------------------------------------------------------
            self.voices[voice_idx].portamento_from_key = porta_from_key;
            self.voices[voice_idx].portamento_duration = porta_time;
            self.voices[voice_idx].override_pan = pan_override;
            self.voices[voice_idx].gain_modifier = voice_gain;
            self.voices[voice_idx].pitch_offset = pitch_offset;
            self.voices[voice_idx].reverb_gain = reverb_gain;
            self.voices[voice_idx].chorus_gain = chorus_gain;
            self.voices[voice_idx].variation_gain = variation_gain;

            // Set initial pan to prevent a brief pop from center to actual pan
            // TS: voice.currentPan = clamp(panOverride || modulatedGenerators[pan])
            let initial_pan = if pan_override != 0.0 {
                pan_override
            } else {
                modulated_copy[gt::PAN as usize] as f64
            };
            self.voices[voice_idx].current_pan = initial_pan.clamp(-500.0, 500.0);
        }

        // -----------------------------------------------------------------------
        // Update voice count and fire events
        // -----------------------------------------------------------------------
        self.midi_channels[channel].voice_count += num_cached as u32;

        let enable = self.system_parameters.events_enabled;
        if let Some(ev) = self.midi_channels[channel].build_channel_property_event(enable) {
            self.call_event(ev);
        }
        if emit {
            self.call_event(SynthProcessorEvent::NoteOn(NoteOnCallback {
                midi_note,
                channel: channel as u8,
                velocity,
            }));
        }
    }

    /// Returns the voice parameters of a GS user drum set key, with the index of the bank
    /// they come from.
    /// Equivalent to: UserDrumSet.getVoiceParameters(midiNote, velocity)
    fn get_user_drum_voice_parameters(
        &self,
        drum_set: usize,
        midi_note: u8,
        velocity: u8,
    ) -> Option<(Vec<VoiceParameters>, usize)> {
        let set = &self.sound_bank_manager.user_drum_sets[drum_set];
        let binding = &set.key_params[midi_note as usize];
        // TS: soundBankManager.systemGetter = () => this.midiParameters.system
        let system = self.midi_parameters.system;
        let resolve = |patch| {
            self.sound_bank_manager
                .get_preset_and_bank_idx(patch, system)
                // Protect from binding to self as well
                .filter(|&(_, bank_idx)| SoundBankManager::user_drum_set_of(bank_idx).is_none())
        };
        let (resolved, bank_idx) =
            resolve(set.bound_patch(midi_note as usize)).or_else(|| resolve(FALLBACK_PATCH))?;
        let bank = &self.sound_bank_manager.sound_bank_list[bank_idx].sound_bank;
        let mut params = resolved.get_voice_parameters(
            binding.source_note_number,
            velocity,
            &bank.instruments,
            &bank.default_modulators,
        );

        // Ensure that the key sounds as intended, similarly to 'PGAL' DLS chunk alias
        for p in params.iter_mut() {
            if p.generators[gt::KEY_NUM as usize] < 0 {
                p.generators[gt::KEY_NUM as usize] = binding.source_note_number as i16;
            }
        }
        Some((params, bank_idx))
    }

    /// Gets or computes (and caches) voices for a specific bank index and preset.
    ///
    /// Called from `note_on` after cloning the preset out of the channel/bank to
    /// avoid simultaneous borrow conflicts.
    ///
    /// Equivalent to: getVoicesForPreset + caching logic from synthesizer_core.ts
    fn get_cached_voices_impl(
        &mut self,
        bank_idx: usize,
        preset: &crate::soundbank::basic_soundbank::basic_preset::BasicPreset,
        midi_note: u8,
        velocity: u8,
    ) -> crate::synthesizer::types::CachedVoiceList {
        use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;

        // Build cache key from preset's patch info
        let patch = MidiPatch {
            program: preset.program,
            bank_msb: preset.bank_msb,
            bank_lsb: preset.bank_lsb,
            is_gm_gs_drum: preset.is_gm_gs_drum,
        };
        let cache_key = self.get_cached_voice_index(&patch, midi_note, velocity);

        // Check cache first
        let user_drum_generation = SoundBankManager::user_drum_set_of(bank_idx)
            .map(|i| self.sound_bank_manager.user_drum_sets[i].generation.get());
        match user_drum_generation {
            Some(generation) => {
                if let Some((g, cached)) = self.user_drum_cached_voices.get(&cache_key)
                    && *g == generation
                {
                    return cached.clone();
                }
            }
            None => {
                if let Some(cached) = self.cached_voices.get(&cache_key) {
                    return cached.clone();
                }
            }
        }

        // Not cached: compute voice parameters (borrow bank immutably, release after block)
        // A GS user drum set takes its voices from the drum set each key is bound to.
        let (voice_params, bank_idx) = match SoundBankManager::user_drum_set_of(bank_idx) {
            Some(drum_set) => match self.get_user_drum_voice_parameters(drum_set, midi_note, velocity) {
                Some(found) => found,
                // No drums at all (only user drum sets are present)
                None => (Vec::new(), bank_idx),
            },
            None => {
                let bank = &self.sound_bank_manager.sound_bank_list[bank_idx].sound_bank;
                (
                    preset.get_voice_parameters(
                        midi_note,
                        velocity,
                        &bank.instruments,
                        &bank.default_modulators,
                    ),
                    bank_idx,
                )
            }
        };

        // Build CachedVoice list
        let mut voices = crate::synthesizer::types::CachedVoiceList::new();
        for vp in voice_params {
            let (audio_data, original_key, loop_start, loop_end, sr_hz, pc_cents) = {
                let bank = &self.sound_bank_manager.sound_bank_list[bank_idx].sound_bank;
                let sample = match bank.samples.get(vp.sample_idx) {
                    Some(s) => s,
                    None => {
                        crate::utils::loggin::spessa_synth_warn(&format!(
                            "note_on: invalid sample index {}",
                            vp.sample_idx
                        ));
                        continue;
                    }
                };
                match &sample.audio_data {
                    Some(data) => (
                        data.clone(),
                        sample.original_key as i16,
                        sample.loop_start,
                        sample.loop_end,
                        sample.sample_rate as f64,
                        sample.pitch_correction as f64,
                    ),
                    None => {
                        crate::utils::loggin::spessa_synth_warn(&format!(
                            "Discarding invalid sample: {}",
                            sample.name
                        ));
                        continue;
                    }
                }
            };

            let cv = crate::synthesizer::audio_engine::voice::voice_cache::CachedVoice::from_bank_params(
                vp,
                audio_data,
                original_key,
                loop_start,
                loop_end,
                sr_hz,
                pc_cents,
                midi_note,
                velocity,
                self.sample_rate,
            );
            voices.push(cv);
        }

        // Cache the result and return
        match user_drum_generation {
            Some(generation) => {
                self.user_drum_cached_voices
                    .insert(cache_key, (generation, voices.clone()));
            }
            None => {
                self.cached_voices.insert(cache_key, voices.clone());
            }
        }
        voices
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
    use crate::synthesizer::types::{SynthProcessorEvent, SynthProcessorOptions};
    use std::sync::{Arc, Mutex};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Creates a SynthesizerCore with event capture.
    fn make_core() -> (SynthesizerCore, Arc<Mutex<Vec<SynthProcessorEvent>>>) {
        let events: Arc<Mutex<Vec<SynthProcessorEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let ev_clone = Arc::clone(&events);
        let core = SynthesizerCore::new(
            move |ev| {
                ev_clone.lock().unwrap().push(ev);
            },
            44100.0,
            SynthProcessorOptions {
                events_enabled: true,
                ..Default::default()
            },
        );
        (core, events)
    }

    /// Creates a SynthesizerCore with one MIDI channel (no sound bank loaded).
    fn make_core_with_channel() -> (SynthesizerCore, Arc<Mutex<Vec<SynthProcessorEvent>>>) {
        let (mut core, events) = make_core();
        core.create_midi_channel(false);
        (core, events)
    }

    // -----------------------------------------------------------------------
    // clamp_f64
    // -----------------------------------------------------------------------

    #[test]
    fn test_clamp_f64_below_min() {
        assert_eq!(clamp_f64(-5.0, 0.0, 1.0), 0.0);
    }

    #[test]
    fn test_clamp_f64_above_max() {
        assert_eq!(clamp_f64(5.0, 0.0, 1.0), 1.0);
    }

    #[test]
    fn test_clamp_f64_in_range() {
        assert_eq!(clamp_f64(0.5, 0.0, 1.0), 0.5);
    }

    #[test]
    fn test_clamp_f64_at_boundaries() {
        assert_eq!(clamp_f64(0.0, 0.0, 1.0), 0.0);
        assert_eq!(clamp_f64(1.0, 0.0, 1.0), 1.0);
    }

    /// Regression: when `min > max` (degenerate case from a zero-length
    /// sample where `end_exclusive - 1.0 == -1.0`), TS's
    /// `Math.max(min, Math.min(max, num))` returns `min`, not `max`.
    /// `clamp_f64` must match this order to avoid producing a negative
    /// wavetable cursor that panics on cast to `usize`.
    #[test]
    fn test_clamp_f64_min_greater_than_max_matches_ts_order() {
        assert_eq!(clamp_f64(5.0, 0.0, -1.0), 0.0);
    }

    // -----------------------------------------------------------------------
    // velocity == 0 → note off path (no NoteOn event fired)
    // -----------------------------------------------------------------------

    #[test]
    fn test_velocity_zero_does_not_fire_note_on_event() {
        let (mut core, events) = make_core_with_channel();
        // velocity 0 → treated as note-off; must not fire NoteOn
        core.note_on(0, 60, 0);
        let evs = events.lock().unwrap();
        let has_note_on = evs.iter().any(|e| matches!(e, SynthProcessorEvent::NoteOn(_)));
        assert!(!has_note_on, "velocity=0 must not fire a NoteOn event");
    }

    #[test]
    fn test_velocity_zero_does_not_add_voices() {
        let (mut core, _events) = make_core_with_channel();
        core.note_on(0, 60, 0);
        assert_eq!(
            core.midi_channels[0].voice_count, 0,
            "velocity=0 must not add voices"
        );
    }

    // -----------------------------------------------------------------------
    // Muted channel → early return
    // -----------------------------------------------------------------------

    #[test]
    fn test_muted_channel_no_voice_added() {
        let (mut core, _events) = make_core_with_channel();
        core.midi_channels[0].is_muted = true;
        core.note_on(0, 60, 100);
        assert_eq!(
            core.midi_channels[0].voice_count, 0,
            "muted channel must not add voices"
        );
    }

    #[test]
    fn test_muted_channel_no_note_on_event() {
        let (mut core, events) = make_core_with_channel();
        core.midi_channels[0].is_muted = true;
        core.note_on(0, 60, 100);
        let evs = events.lock().unwrap();
        let has_note_on = evs.iter().any(|e| matches!(e, SynthProcessorEvent::NoteOn(_)));
        assert!(!has_note_on, "muted channel must not fire NoteOn event");
    }

    // -----------------------------------------------------------------------
    // No preset loaded → early return
    // -----------------------------------------------------------------------

    #[test]
    fn test_no_preset_no_voice_added() {
        let (mut core, _events) = make_core_with_channel();
        // A fresh core without a loaded sound bank has preset = None
        assert!(
            core.midi_channels[0].preset.is_none(),
            "test expects channel with no preset"
        );
        core.note_on(0, 60, 100);
        assert_eq!(
            core.midi_channels[0].voice_count, 0,
            "no preset must not add voices"
        );
    }

    // -----------------------------------------------------------------------
    // Out-of-range shifted note / raw note bounds → no voices
    // -----------------------------------------------------------------------

    #[test]
    fn test_sound_bank_note_above_127_no_voice_added() {
        let (mut core, _events) = make_core_with_channel();
        // note 100 + shift 64 = 164 → out of the 0-127 zone range, no zones match
        core.midi_channels[0].channel_transpose_key_shift = 64;
        core.note_on(0, 100, 100);
        assert_eq!(
            core.midi_channels[0].voice_count, 0,
            "out-of-range sound bank note must not add voices"
        );
    }

    #[test]
    fn test_sound_bank_note_below_0_no_voice_added() {
        let (mut core, _events) = make_core_with_channel();
        // note 0 + shift -64 = -64 → out of the 0-127 zone range, no zones match
        core.midi_channels[0].channel_transpose_key_shift = -64;
        core.note_on(0, 0, 100);
        assert_eq!(
            core.midi_channels[0].voice_count, 0,
            "out-of-range sound bank note must not add voices"
        );
    }

    #[test]
    fn test_valid_note_no_preset_no_voice_added() {
        let (mut core, _events) = make_core_with_channel();
        // note 127 → valid raw note, but no preset → bails at preset check
        core.note_on(0, 127, 100);
        assert_eq!(
            core.midi_channels[0].voice_count, 0,
            "valid key with no preset must not add voices"
        );
    }

    // -----------------------------------------------------------------------
    // Black MIDI mode filters
    // -----------------------------------------------------------------------

    #[test]
    fn test_black_midi_very_low_velocity_ignored() {
        let (mut core, _events) = make_core_with_channel();
        core.system_parameters.black_midi_mode = true;
        // velocity 9 < 10 → always filtered in black MIDI mode
        core.note_on(0, 60, 9);
        assert_eq!(
            core.midi_channels[0].voice_count, 0,
            "black MIDI mode with velocity < 10 must not add voices"
        );
    }

    #[test]
    fn test_black_midi_high_voice_count_low_velocity_ignored() {
        let (mut core, _events) = make_core_with_channel();
        core.system_parameters.black_midi_mode = true;
        // Simulate high total voice count (> 200) + velocity 39 < 40
        core.voice_count = 201;
        core.note_on(0, 60, 39);
        // voice_count on channel stays 0; synthesizer-level voice_count unchanged
        assert_eq!(
            core.midi_channels[0].voice_count, 0,
            "black MIDI mode with voice_count > 200 and velocity < 40 must not add voices"
        );
        assert_eq!(
            core.voice_count, 201,
            "synthesizer voice_count must not be modified by early return"
        );
    }

    #[test]
    fn test_black_midi_high_velocity_not_filtered_by_black_midi() {
        // Velocity 50 with black MIDI mode should not be filtered by the black MIDI guard,
        // but will bail at the preset check (no sound bank loaded).
        let (mut core, _events) = make_core_with_channel();
        core.system_parameters.black_midi_mode = true;
        core.voice_count = 201;
        core.note_on(0, 60, 50); // velocity 50 >= 40 → passes black MIDI guard
        // No preset → voice_count remains 0 (bailed at preset check)
        assert_eq!(core.midi_channels[0].voice_count, 0);
    }

    #[test]
    fn test_black_midi_velocity_exactly_10_not_filtered() {
        // velocity == 10 must NOT be filtered by the "velocity < 10" rule
        let (mut core, _events) = make_core_with_channel();
        core.system_parameters.black_midi_mode = true;
        core.voice_count = 0; // Low voice count so only the velocity rule applies
        core.note_on(0, 60, 10); // velocity 10 → not filtered; bails at preset check
        assert_eq!(core.midi_channels[0].voice_count, 0); // no preset, but reached preset guard
    }
}
