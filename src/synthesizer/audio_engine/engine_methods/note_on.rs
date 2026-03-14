/// note_on.rs
/// purpose: MIDI note on handler for SynthesizerCore.
/// Ported from: src/synthesizer/audio_engine/engine_methods/note_on.ts
use crate::midi::enums::midi_controllers;
use crate::soundbank::basic_soundbank::generator_types::{
    GENERATORS_AMOUNT, generator_types as gt,
};
use crate::soundbank::basic_soundbank::modulator::Modulator;
use crate::synthesizer::audio_engine::engine_components::compute_modulator::{
    compute_modulators, SourceFilter,
};
use crate::synthesizer::audio_engine::engine_methods::portamento_time::portamento_time_to_seconds;
use crate::synthesizer::audio_engine::engine_components::synth_constants::{
    GENERATOR_OVERRIDE_NO_CHANGE_VALUE, MIN_EXCLUSIVE_LENGTH,
};
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::enums::custom_controllers;
use crate::synthesizer::types::{NoteOnCallback, SynthProcessorEvent};
use crate::utils::loggin::spessa_synth_warn;

/// Clamps a value between min and max.
#[inline]
fn clamp_f64(val: f64, min: f64, max: f64) -> f64 {
    val.max(min).min(max)
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
        if velocity == 0 {
            // velocity 0 = note off (per MIDI spec)
            let current_time = self.current_time;
            let black_midi = self.master_parameters.black_midi_mode;
            let evs = self.midi_channels[channel].note_off(
                midi_note,
                &mut self.voices,
                current_time,
                black_midi,
            );
            for ev in evs {
                self.call_event(ev);
            }
            return;
        }
        let velocity = velocity.min(127);

        // Black MIDI mode: drop low-velocity notes when voice count is high
        let voice_count_total = self.voice_count;
        if (self.master_parameters.black_midi_mode
            && voice_count_total > 200
            && velocity < 40)
            || (self.master_parameters.black_midi_mode && velocity < 10)
            || self.midi_channels[channel].is_muted
        {
            return;
        }

        // Require a preset to be loaded
        if self.midi_channels[channel].preset.is_none() {
            return;
        }

        // Compute real key with channel transpose and custom key shift
        let key_shift = self.midi_channels[channel].channel_transpose_key_shift;
        let custom_key_shift = self.midi_channels[channel].custom_controllers
            [custom_controllers::CHANNEL_KEY_SHIFT as usize] as i32;
        let real_key_i =
            midi_note as i32 + key_shift as i32 + custom_key_shift;
        if !(0..=127).contains(&real_key_i) {
            return;
        }
        let real_key = real_key_i as u8;

        // Apply MIDI Tuning Standard (MTS) if active
        let program = self.midi_channels[channel]
            .preset
            .as_ref()
            .unwrap()
            .program;
        let mts_idx = program as usize * 128 + real_key as usize;
        let mut internal_midi_note = real_key;
        if mts_idx < self.tunings.len() {
            let tune = self.tunings[mts_idx];
            if tune >= 0.0 {
                internal_midi_note = tune.trunc() as u8;
            }
        }

        // Monophonic retrigger: kill any current note before starting new one
        if self.master_parameters.monophonic_retrigger_mode {
            let current_time = self.current_time;
            self.midi_channels[channel].kill_note(
                midi_note,
                0,
                &mut self.voices,
                current_time,
            );
        }

        // Key velocity override from key modifier manager
        let key_vel = self
            .key_modifier_manager
            .get_velocity(channel as u8, real_key);
        let effective_velocity = if key_vel > -1 {
            key_vel as u8
        } else {
            velocity
        };

        // Gain override from key modifier manager
        let voice_gain = self
            .key_modifier_manager
            .get_gain(channel as u8, real_key);

        // Portamento: compute glide duration if enabled
        let portamento_time_cc = (self.midi_channels[channel].midi_controllers
            [midi_controllers::PORTAMENTO_TIME as usize]
            >> 7) as u8;
        let porta_control = (self.midi_channels[channel].midi_controllers
            [midi_controllers::PORTAMENTO_CONTROL as usize]
            >> 7) as u8;
        let portamento_on = self.midi_channels[channel].midi_controllers
            [midi_controllers::PORTAMENTO_ON_OFF as usize]
            >= 8192; // (64 << 7)

        let mut portamento_from_key: i32 = -1;
        let mut portamento_duration: f64 = 0.0;

        if !self.midi_channels[channel].drum_channel
            && porta_control as i32 != internal_midi_note as i32
            && portamento_on
            && portamento_time_cc > 0
        {
            if porta_control > 0 {
                // Key 0 means initial portamento (no glide)
                let diff =
                    (internal_midi_note as i32 - porta_control as i32).unsigned_abs() as u8;
                portamento_duration =
                    portamento_time_to_seconds(portamento_time_cc, diff as f64);
                portamento_from_key = porta_control as i32;
            }
            // Update portamento control to current note
            let current_time = self.current_time;
            let current_system = self.master_parameters.midi_system;
            let enable = self.enable_event_system;
            let evs = self.midi_channels[channel].controller_change(
                midi_controllers::PORTAMENTO_CONTROL,
                internal_midi_note,
                &mut self.voices,
                current_time,
                current_system,
                enable,
            );
            for ev in evs {
                self.call_event(ev);
            }
        }

        // Mono mode: release all active voices on this channel
        if !self.midi_channels[channel].poly_mode {
            let ch_channel = self.midi_channels[channel].channel;
            let voice_count_ch = self.midi_channels[channel].voice_count;
            let current_time = self.current_time;
            if voice_count_ch > 0 {
                let mut vc = 0u32;
                for v in self.voices.iter_mut() {
                    if v.is_active && v.channel == ch_channel {
                        v.exclusive_release(current_time, 0.0);
                        vc += 1;
                        if vc >= voice_count_ch {
                            break;
                        }
                    }
                }
            }
        }

        // -----------------------------------------------------------------------
        // Get cached voices for this note (with key modifier override support)
        // -----------------------------------------------------------------------
        let override_patch = self
            .key_modifier_manager
            .has_override_patch(channel as u8, internal_midi_note);

        let cached_voices = if override_patch {
            // Key modifier overrides the patch for this note
            let patch = match self
                .key_modifier_manager
                .get_patch(channel as u8, internal_midi_note)
            {
                Ok(p) => p,
                Err(_) => return,
            };
            // Find the bank index for the override patch
            if let Some((_, bank_idx)) = self
                .sound_bank_manager
                .get_preset_and_bank_idx(patch, self.master_parameters.midi_system)
            {
                // Get a preset clone from the bank so we can release the borrow
                let preset_clone = self.sound_bank_manager.sound_bank_list[bank_idx]
                    .sound_bank
                    .presets
                    .iter()
                    .find(|p| {
                        p.program == patch.program
                            && p.bank_msb == patch.bank_msb
                            && p.bank_lsb == patch.bank_lsb
                    })
                    .cloned();
                if let Some(preset) = preset_clone {
                    self.get_cached_voices_impl(
                        bank_idx,
                        &preset,
                        internal_midi_note,
                        effective_velocity,
                    )
                } else {
                    return;
                }
            } else {
                return;
            }
        } else {
            // Use channel's current preset
            let (preset_clone, bank_idx) = {
                let ch = &self.midi_channels[channel];
                match (&ch.preset, ch.preset_bank_idx) {
                    (Some(p), Some(bi)) => (p.clone(), bi),
                    _ => return,
                }
            };
            self.get_cached_voices_impl(
                bank_idx,
                &preset_clone,
                internal_midi_note,
                effective_velocity,
            )
        };

        if cached_voices.is_empty() {
            return;
        }

        // Drum parameters and pan override
        let mut pan_override: f64 = 0.0;
        let mut pitch_offset: f64 = 0.0;
        let mut reverb_send: f64 = 1.0;
        let mut chorus_send: f64 = 1.0;
        let mut delay_send: f64 = 1.0;
        let mut exclusive_override: i32 = 0;
        let mut voice_gain = voice_gain;

        if self.midi_channels[channel].drum_channel {
            let p = &self.midi_channels[channel].drum_params[internal_midi_note as usize];

            // Check if note on is allowed
            if !p.rx_note_on {
                return;
            }

            // Pan handling
            let drum_pan = p.pan;
            if drum_pan != 64 {
                if drum_pan == 0 {
                    // Random pan
                    let bits = self.current_time.to_bits();
                    let h = bits
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    let normalized = (h >> 33) as f64 / u32::MAX as f64;
                    pan_override = (normalized * 1000.0 - 500.0).round();
                } else {
                    // Calculate with channel pan
                    let channel_pan = (self.midi_channels[channel].midi_controllers
                        [midi_controllers::PAN as usize]
                        >> 7) as i32
                        - 64;
                    let target_pan =
                        (drum_pan as i32 - 64 + channel_pan).clamp(-63, 63);
                    let target_pan = if target_pan == 0 { 1 } else { target_pan };
                    pan_override = (target_pan as f64 / 63.0) * 500.0;
                }
            }

            pitch_offset = p.pitch;
            exclusive_override = p.exclusive_class as i32;
            reverb_send = p.reverb_gain;
            chorus_send = p.chorus_gain;
            delay_send = p.delay_gain;

            // Gain override only if not set by key modifier
            if voice_gain == 1.0 {
                voice_gain = p.gain;
            }
        } else if self.midi_channels[channel].random_pan {
            // Non-drum random pan
            let bits = self.current_time.to_bits();
            let h = bits
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let normalized = (h >> 33) as f64 / u32::MAX as f64;
            pan_override = (normalized * 1000.0 - 500.0).round();
        };

        // -----------------------------------------------------------------------
        // Assign and configure a voice slot for each cached voice
        // -----------------------------------------------------------------------
        let num_cached = cached_voices.len();
        for (_ci, cached) in cached_voices.iter().enumerate() {
            // Assign a free voice slot (returns index, not reference)
            let voice_idx = self.assign_voice_idx();

            // Setup basic voice state
            self.voices[voice_idx].setup(
                self.current_time,
                channel as u8,
                internal_midi_note,
                effective_velocity,
                real_key,
            );

            // Select the active oscillator type
            self.voices[voice_idx].oscillator_type =
                self.master_parameters.interpolation_type;

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

            // Copy other cached fields
            self.voices[voice_idx].exclusive_class = cached.exclusive_class as i32;
            self.voices[voice_idx].root_key = cached.root_key;
            self.voices[voice_idx].looping_mode = cached.looping_mode;
            self.voices[voice_idx].target_key = cached.target_key;

            // Set sample data on the active oscillator
            let osc_idx = self.voices[voice_idx].oscillator_type as usize;
            self.voices[voice_idx].oscillators[osc_idx].sample_data =
                Some(cached.sample_data.clone());
            self.voices[voice_idx].oscillators[osc_idx].playback_step = cached.playback_step;

            // -----------------------------------------------------------------------
            // Set modulators: merge SysEx dynamic modulators if present
            // -----------------------------------------------------------------------
            let sysex_count = self.midi_channels[channel]
                .sys_ex_modulators
                .modulator_list
                .len();

            let final_modulators: Vec<Modulator> = if sysex_count > 0 {
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
            if excl_class != 0 && self.midi_channels[channel].poly_mode {
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
            let midi_note_i16 = self.voices[voice_idx].midi_note as i16;

            self.voices[voice_idx]
                .vol_env
                .init(&modulated_copy, target_key);
            self.voices[voice_idx]
                .mod_env
                .init(&modulated_copy, start_time, midi_note_i16);
            self.voices[voice_idx].filter.init();

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
            let last_sample = if sample_len > 0 { (sample_len - 1) as f64 } else { 0.0 };

            self.voices[voice_idx].oscillators[osc_idx].cursor =
                clamp_f64(start_offset as f64, 0.0, last_sample);
            self.voices[voice_idx].oscillators[osc_idx].end =
                clamp_f64(last_sample + end_offset as f64, 0.0, last_sample);

            let loop_start_raw = cached.loop_start as f64 + loop_start_offset as f64;
            let loop_end_raw = cached.loop_end as f64 + loop_end_offset as f64;
            let mut ls = clamp_f64(loop_start_raw, 0.0, last_sample);
            let mut le = clamp_f64(loop_end_raw, 0.0, last_sample);

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

            // -----------------------------------------------------------------------
            // Apply portamento, pan override, gain modifier, and initial current pan
            // -----------------------------------------------------------------------
            self.voices[voice_idx].portamento_from_key = portamento_from_key;
            self.voices[voice_idx].portamento_duration = portamento_duration;
            self.voices[voice_idx].override_pan = pan_override;
            self.voices[voice_idx].gain_modifier = voice_gain;
            self.voices[voice_idx].pitch_offset = pitch_offset;
            self.voices[voice_idx].reverb_send = reverb_send;
            self.voices[voice_idx].chorus_send = chorus_send;
            self.voices[voice_idx].delay_send = delay_send;

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

        let enable = self.enable_event_system;
        if let Some(ev) = self.midi_channels[channel].build_channel_property_event(enable) {
            self.call_event(ev);
        }
        self.call_event(SynthProcessorEvent::NoteOn(NoteOnCallback {
            midi_note,
            channel: channel as u8,
            velocity,
        }));
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
        if let Some(cached) = self.cached_voices.get(&cache_key) {
            return cached.clone();
        }

        // Not cached: compute voice parameters (borrow bank immutably, release after block)
        let voice_params = {
            let bank = &self.sound_bank_manager.sound_bank_list[bank_idx].sound_bank;
            preset.get_voice_parameters(
                midi_note,
                velocity,
                &bank.instruments,
                &bank.default_modulators,
            )
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

            let cv = crate::synthesizer::audio_engine::engine_components::voice_cache::CachedVoice::from_bank_params(
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
        self.cached_voices.insert(cache_key, voices.clone());
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
                enable_event_system: true,
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
    // real_key out of range → early return
    // -----------------------------------------------------------------------

    #[test]
    fn test_real_key_above_127_no_voice_added() {
        let (mut core, _events) = make_core_with_channel();
        // note 100 + shift 64 = 164 > 127
        core.midi_channels[0].channel_transpose_key_shift = 64;
        core.note_on(0, 100, 100);
        assert_eq!(
            core.midi_channels[0].voice_count, 0,
            "real_key > 127 must not add voices"
        );
    }

    #[test]
    fn test_real_key_below_0_no_voice_added() {
        let (mut core, _events) = make_core_with_channel();
        // note 0 + shift -64 = -64 < 0
        core.midi_channels[0].channel_transpose_key_shift = -64;
        core.note_on(0, 0, 100);
        assert_eq!(
            core.midi_channels[0].voice_count, 0,
            "real_key < 0 must not add voices"
        );
    }

    #[test]
    fn test_real_key_at_127_boundary_proceeds_to_preset_check() {
        let (mut core, _events) = make_core_with_channel();
        // note 127 + shift 0 = 127 → valid key, but no preset → bails at preset check
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
        core.master_parameters.black_midi_mode = true;
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
        core.master_parameters.black_midi_mode = true;
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
        core.master_parameters.black_midi_mode = true;
        core.voice_count = 201;
        core.note_on(0, 60, 50); // velocity 50 >= 40 → passes black MIDI guard
        // No preset → voice_count remains 0 (bailed at preset check)
        assert_eq!(core.midi_channels[0].voice_count, 0);
    }

    #[test]
    fn test_black_midi_velocity_exactly_10_not_filtered() {
        // velocity == 10 must NOT be filtered by the "velocity < 10" rule
        let (mut core, _events) = make_core_with_channel();
        core.master_parameters.black_midi_mode = true;
        core.voice_count = 0; // Low voice count so only the velocity rule applies
        core.note_on(0, 60, 10); // velocity 10 → not filtered; bails at preset check
        assert_eq!(core.midi_channels[0].voice_count, 0); // no preset, but reached preset guard
    }
}
