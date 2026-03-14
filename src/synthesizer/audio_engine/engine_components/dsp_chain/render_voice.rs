/// render_voice.rs
/// purpose: Renders a single voice to the stereo output buffers.
/// Ported from: src/synthesizer/audio_engine/engine_components/dsp_chain/render_voice.ts
use std::sync::OnceLock;

use crate::midi::enums::midi_controllers;
use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
use crate::synthesizer::audio_engine::engine_components::dsp_chain::lfo::{get_lfo_value, get_lfo_value_sine};
use crate::synthesizer::audio_engine::engine_components::unit_converter::{
    abs_cents_to_hz, cb_attenuation_to_gain, timecents_to_seconds,
};
use crate::synthesizer::audio_engine::engine_components::voice::Voice;
use crate::synthesizer::audio_engine::synthesizer_core::MidiChannel;
use crate::synthesizer::enums::custom_controllers;
use crate::utils::loggin::spessa_synth_warn;

const HALF_PI: f64 = std::f64::consts::PI / 2.0;
const MIN_PAN: i32 = -500;
const MAX_PAN: i32 = 500;
const PAN_RESOLUTION: i32 = MAX_PAN - MIN_PAN; // 1000

/// Pre-computed pan table for left channel (cos law).
/// Index 0 = full left (pan -500), index 1000 = full right (pan +500).
static PAN_TABLE_LEFT: OnceLock<[f32; 1001]> = OnceLock::new();

/// Pre-computed pan table for right channel (sin law).
static PAN_TABLE_RIGHT: OnceLock<[f32; 1001]> = OnceLock::new();

fn get_pan_table_left() -> &'static [f32; 1001] {
    PAN_TABLE_LEFT.get_or_init(|| {
        let mut table = [0f32; 1001];
        for pan in MIN_PAN..=MAX_PAN {
            let real_pan = (pan - MIN_PAN) as f64 / PAN_RESOLUTION as f64;
            let idx = (pan - MIN_PAN) as usize;
            table[idx] = (HALF_PI * real_pan).cos() as f32;
        }
        table
    })
}

fn get_pan_table_right() -> &'static [f32; 1001] {
    PAN_TABLE_RIGHT.get_or_init(|| {
        let mut table = [0f32; 1001];
        for pan in MIN_PAN..=MAX_PAN {
            let real_pan = (pan - MIN_PAN) as f64 / PAN_RESOLUTION as f64;
            let idx = (pan - MIN_PAN) as usize;
            table[idx] = (HALF_PI * real_pan).sin() as f32;
        }
        table
    })
}

impl MidiChannel {
    /// Renders a single voice to the stereo output and mono effect send buffers.
    ///
    /// Handles tuning (MTS, scale tuning, portamento, LFOs, envelopes),
    /// wavetable synthesis, lowpass filter, volume envelope, panning, and effect sends.
    ///
    /// # Parameters
    /// - `voice`: The voice to render (mutated in place).
    /// - `time_now`: Current playback time in seconds.
    /// - `output_l/r`: Main stereo output buffers (start_index-based).
    /// - `reverb_input`, `chorus_input`, `delay_input`: Mono effect send buffers (zero-indexed).
    /// - `start_index`: Starting sample index in the output buffers.
    /// - `sample_count`: Number of samples to render.
    /// - `master_gain`, `reverb_gain`, `chorus_gain`, `delay_gain`: Global gain values.
    /// - `midi_volume`: Global MIDI volume scale.
    /// - `pan_left`, `pan_right`: Global pan (master pan) multipliers.
    /// - `enable_effects`: Whether to write to effect buffers.
    /// - `delay_active`: Whether delay effect is active.
    /// - `pan_smoothing_factor`: Smoothing coefficient for pan changes.
    /// - `tunings`: MIDI Tuning Standard table (128×128 floats, -1 if unused).
    #[allow(clippy::too_many_arguments)]
    pub fn render_voice(
        &self,
        voice: &mut Voice,
        time_now: f64,
        output_l: &mut [f32],
        output_r: &mut [f32],
        reverb_input: &mut [f32],
        chorus_input: &mut [f32],
        delay_input: &mut [f32],
        start_index: usize,
        sample_count: usize,
        master_gain: f64,
        reverb_gain: f64,
        chorus_gain: f64,
        delay_gain: f64,
        midi_volume: f64,
        pan_left: f64,
        pan_right: f64,
        enable_effects: bool,
        delay_active: bool,
        pan_smoothing_factor: f64,
        tunings: &[f32],
        insertion_input_l: &mut [f32],
        insertion_input_r: &mut [f32],
        insertion_active: bool,
    ) {
        // Check if the voice has entered release
        if !voice.is_in_release && time_now >= voice.release_start_time {
            voice.is_in_release = true;
            let should_deactivate = voice.vol_env.start_release(
                &voice.modulated_generators,
                voice.target_key,
                voice.override_release_vol_env,
            );
            if should_deactivate {
                voice.is_active = false;
            }
            voice.mod_env.start_release(&voice.modulated_generators);

            // Looping mode 3: disable looping on release
            if voice.looping_mode == 3 {
                voice.oscillators[voice.oscillator_type as usize].is_looping = false;
            }
        }
        voice.has_rendered = true;

        // Sanity check: voice might have been deactivated while we were preparing to render
        if !voice.is_active {
            return;
        }

        // --- TUNING ---
        let mut target_key = voice.target_key;

        // Fine tune (soundfont) + MTS octave tuning + channel tuning + pitch offset (drum params)
        let mut cents = voice.modulated_generators[gt::FINE_TUNE as usize] as f64
            + self.channel_octave_tuning[voice.midi_note as usize] as f64
            + self.channel_tuning_cents as f64
            + voice.pitch_offset;
        let mut semitones = voice.modulated_generators[gt::COARSE_TUNE as usize] as f64;

        // MIDI Tuning Standard
        if let Some(preset) = &self.preset {
            let tune_idx = (preset.program as usize) * 128 + voice.real_key as usize;
            if tune_idx < tunings.len() {
                let tuning = tunings[tune_idx];
                if tuning >= 0.0 {
                    let trunc_key = tuning.trunc() as i16;
                    target_key = trunc_key;
                    cents += (tuning - tuning.trunc()) as f64 * 100.0;
                }
            }
        }

        // Portamento
        if voice.portamento_from_key > -1 {
            let elapsed = ((time_now - voice.start_time) / voice.portamento_duration).min(1.0);
            let diff = target_key as f64 - voice.portamento_from_key as f64;
            semitones -= diff * (1.0 - elapsed);
        }

        // Scale tuning: cents per key (usually 100)
        cents += (target_key - voice.root_key) as f64
            * voice.modulated_generators[gt::SCALE_TUNING as usize] as f64;

        // LFO + envelope modulation accumulators
        let mut lowpass_excursion = 0.0f64;
        let mut volume_excursion_centibels = 0.0f64;

        // --- Vibrato LFO ---
        let vib_pitch_depth = voice.modulated_generators[gt::VIB_LFO_TO_PITCH as usize];
        let vib_vol_depth = voice.modulated_generators[gt::VIB_LFO_TO_VOLUME as usize];
        let vib_filter_depth = voice.modulated_generators[gt::VIB_LFO_TO_FILTER_FC as usize];
        if vib_pitch_depth != 0 || vib_vol_depth != 0 || vib_filter_depth != 0 {
            let vib_start = voice.start_time
                + timecents_to_seconds(
                    voice.modulated_generators[gt::DELAY_VIB_LFO as usize] as i32,
                ) as f64;
            let vib_freq_hz =
                abs_cents_to_hz(voice.modulated_generators[gt::FREQ_VIB_LFO as usize] as i32)
                    as f64;
            let vib_lfo_value = get_lfo_value(vib_start, vib_freq_hz, time_now);
            let mod_mult =
                self.custom_controllers[custom_controllers::MODULATION_MULTIPLIER as usize];
            cents += vib_lfo_value * vib_pitch_depth as f64 * mod_mult as f64;
            // Negate because Audigy starts with an increase rather than decrease
            volume_excursion_centibels += -vib_lfo_value * vib_vol_depth as f64;
            lowpass_excursion += vib_lfo_value * vib_filter_depth as f64;
        }

        // --- Mod LFO ---
        let mod_pitch_depth = voice.modulated_generators[gt::MOD_LFO_TO_PITCH as usize];
        let mod_vol_depth = voice.modulated_generators[gt::MOD_LFO_TO_VOLUME as usize];
        let mod_filter_depth = voice.modulated_generators[gt::MOD_LFO_TO_FILTER_FC as usize];
        if mod_pitch_depth != 0 || mod_filter_depth != 0 || mod_vol_depth != 0 {
            let mod_start = voice.start_time
                + timecents_to_seconds(
                    voice.modulated_generators[gt::DELAY_MOD_LFO as usize] as i32,
                ) as f64;
            let mod_freq_hz =
                abs_cents_to_hz(voice.modulated_generators[gt::FREQ_MOD_LFO as usize] as i32)
                    as f64;
            let mod_lfo_value = get_lfo_value(mod_start, mod_freq_hz, time_now);
            let mod_mult =
                self.custom_controllers[custom_controllers::MODULATION_MULTIPLIER as usize];
            cents += mod_lfo_value * mod_pitch_depth as f64 * mod_mult as f64;
            volume_excursion_centibels += -mod_lfo_value * mod_vol_depth as f64;
            lowpass_excursion += mod_lfo_value * mod_filter_depth as f64;
        }

        // --- Channel vibrato (GS NRPN) ---
        // Only when modulation wheel is zero (to prevent overlap)
        if self.midi_controllers[midi_controllers::MODULATION_WHEEL as usize] == 0
            && self.channel_vibrato.depth > 0.0
        {
            cents += get_lfo_value_sine(
                voice.start_time + self.channel_vibrato.delay,
                self.channel_vibrato.rate,
                time_now,
            ) * self.channel_vibrato.depth;
        }

        // --- Mod envelope ---
        let mod_env_pitch_depth = voice.modulated_generators[gt::MOD_ENV_TO_PITCH as usize];
        let mod_env_filter_depth = voice.modulated_generators[gt::MOD_ENV_TO_FILTER_FC as usize];
        if mod_env_filter_depth != 0 || mod_env_pitch_depth != 0 {
            let mod_env = voice.mod_env.process(voice.release_start_time, time_now);
            lowpass_excursion += mod_env * mod_env_filter_depth as f64;
            cents += mod_env * mod_env_pitch_depth as f64;
        }

        // Resonance offset (does not affect filter gain)
        volume_excursion_centibels -= voice.resonance_offset as f64;

        // Compute final playback rate
        let cents_total = (cents + semitones * 100.0) as i32;
        if cents_total != voice.tuning_cents as i32 {
            voice.tuning_cents = cents_total as f64;
            voice.tuning_ratio = f64::powf(2.0, cents_total as f64 / 1200.0);
        }

        // Gain target from initial attenuation generator
        let gain_target = cb_attenuation_to_gain(
            voice.modulated_generators[gt::INITIAL_ATTENUATION as usize] as i32,
        ) as f64;

        // --- SYNTHESIS ---
        // Resize buffer if necessary (should be rare)
        if voice.buffer.len() < sample_count {
            spessa_synth_warn(&format!(
                "Buffer size changed from {} to {}! Memory allocation!",
                voice.buffer.len(),
                sample_count
            ));
            voice.buffer.resize(sample_count, 0.0);
        }

        // Looping mode 2: start-on-release. Only process vol env, no oscillator.
        if voice.looping_mode == 2 && !voice.is_in_release {
            voice.is_active = voice.vol_env.process(
                sample_count,
                &mut voice.buffer,
                gain_target,
                volume_excursion_centibels,
            );
            return;
        }

        // Wavetable oscillator
        let osc_active = voice.oscillators[voice.oscillator_type as usize].process(
            sample_count,
            voice.tuning_ratio,
            &mut voice.buffer,
        );
        voice.is_active = osc_active;

        // Lowpass filter
        voice.filter.process(
            sample_count,
            &voice.modulated_generators,
            &mut voice.buffer,
            lowpass_excursion,
        );

        // Volume envelope
        let env_active = voice.vol_env.process(
            sample_count,
            &mut voice.buffer,
            gain_target,
            volume_excursion_centibels,
        );

        // Both oscillator AND envelope must be active for voice to continue
        voice.is_active &= env_active;

        // --- PAN + MIX DOWN ---
        let pan = if voice.override_pan != 0.0 {
            voice.override_pan
        } else {
            // Smoothly approach target pan to avoid clicks
            let target_pan = voice.modulated_generators[gt::PAN as usize] as f64;
            voice.current_pan += (target_pan - voice.current_pan) * pan_smoothing_factor;
            voice.current_pan
        };

        let gain = master_gain * midi_volume * voice.gain_modifier as f64;
        // Match TS: (pan + 500) | 0 — add first (f64), then truncate to i32
        let index = ((pan + 500.0) as i32).clamp(0, PAN_RESOLUTION as i32) as usize;
        let pan_left_table = get_pan_table_left();
        let pan_right_table = get_pan_table_right();
        // Keep gain_left/gain_right as f64 to match JS Float32Array behavior:
        // JS computes gainLeft * buffer[i] in f64, then truncates when writing to Float32Array.
        let gain_left = pan_left_table[index] as f64 * gain * pan_left;
        let gain_right = pan_right_table[index] as f64 * gain * pan_right;

        let buffer = &voice.buffer;

        // If insertion is enabled for this channel, route to insertion buffers instead
        if self.insertion_enabled && insertion_active {
            for (i, &s) in buffer.iter().enumerate().take(sample_count) {
                insertion_input_l[i] = (insertion_input_l[i] as f64 + gain_left * s as f64) as f32;
                insertion_input_r[i] = (insertion_input_r[i] as f64 + gain_right * s as f64) as f32;
            }
            return;
        }

        // Emulate JS Float32Array += semantics:
        // outputL[idx] = f32(f64(outputL[idx]) + gainLeft * f64(buffer[i]))
        for (i, &s) in buffer.iter().enumerate().take(sample_count) {
            let idx = i + start_index;
            output_l[idx] = (output_l[idx] as f64 + gain_left * s as f64) as f32;
            output_r[idx] = (output_r[idx] as f64 + gain_right * s as f64) as f32;
        }

        if !enable_effects {
            return;
        }

        // --- REVERB SEND (mono) ---
        let reverb_send_gen = voice.modulated_generators[gt::REVERB_EFFECTS_SEND as usize] as f64
            * voice.reverb_send;
        if reverb_send_gen > 0.0 {
            let reverb_gain_total = reverb_gain * gain * (reverb_send_gen / 1000.0);
            for (i, &samp) in buffer.iter().enumerate().take(sample_count) {
                reverb_input[i] = (reverb_input[i] as f64 + reverb_gain_total * samp as f64) as f32;
            }
        }

        // --- CHORUS SEND (mono) ---
        let chorus_send_gen = voice.modulated_generators[gt::CHORUS_EFFECTS_SEND as usize] as f64
            * voice.chorus_send;
        if chorus_send_gen > 0.0 {
            let chorus_gain_total = chorus_gain * gain * (chorus_send_gen / 1000.0);
            for (i, &samp) in buffer.iter().enumerate().take(sample_count) {
                chorus_input[i] = (chorus_input[i] as f64 + chorus_gain_total * samp as f64) as f32;
            }
        }

        // --- DELAY SEND (mono) ---
        if delay_active {
            let delay_send_cc = self.midi_controllers[midi_controllers::VARIATION_DEPTH as usize] as f64
                * voice.delay_send;
            if delay_send_cc > 0.0 {
                let delay_gain_total = gain * delay_gain * ((delay_send_cc as i32 >> 7) as f64 / 127.0);
                for (i, &samp) in buffer.iter().enumerate().take(sample_count) {
                    delay_input[i] = (delay_input[i] as f64 + delay_gain_total * samp as f64) as f32;
                }
            }
        }
    }
}
