//! Renders a single voice into the channel's voice send buffers.
//! Ported from: src/synthesizer/audio_engine/channel/render_voice.ts
//!
//! Voices do not write to the master output. Each voice is summed into a mono f64 `VoiceSendBuffer` shared by the voices of the
//! channel with the same pan index and send levels; `mix_voice_send_buffer` then pans it
//! into the channel output (`output_left/right`, f64) and the effect sends once per block.
//! The processor mixes the channel outputs into the master output.
//!
//! In TS `renderVoice` reaches the processor through `this.synthCore`. A Rust channel does
//! not hold a reference to the processor, so the processor passes the state renderVoice
//! reads as a `RenderVoiceContext`.
use std::sync::OnceLock;

use crate::midi::enums::midi_controllers;
use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
use crate::synthesizer::audio_engine::channel::midi_channel::MidiChannel;
use crate::synthesizer::audio_engine::voice::unit_converter::{
    abs_cents_to_hz, cb_attenuation_to_gain, cb_attenuation_to_gain_f64,
};
use crate::synthesizer::audio_engine::voice::voice::Voice;

const TWO_PI: f64 = std::f64::consts::PI * 2.0;

const HALF_PI: f64 = std::f64::consts::PI / 2.0;

/// The maximum number of voice send buffers per channel.
/// Voices that don't fit are mixed directly in renderVoice() with the shared processor buffer.
/// Equivalent to: MAX_VOICE_SEND_BUFFERS
const MAX_VOICE_SEND_BUFFERS: usize = 16;

const MIN_PAN: i32 = -500;
const MAX_PAN: i32 = 500;
const PAN_RESOLUTION: i32 = MAX_PAN - MIN_PAN; // 1000

/// Pan lookup tables (Float32Array in TS): (left, right).
static PAN_TABLES: OnceLock<([f32; 1001], [f32; 1001])> = OnceLock::new();

fn pan_tables() -> &'static ([f32; 1001], [f32; 1001]) {
    PAN_TABLES.get_or_init(|| {
        let mut left = [0f32; 1001];
        let mut right = [0f32; 1001];
        for pan in MIN_PAN..=MAX_PAN {
            // Clamp to 0-1
            let real_pan = (pan - MIN_PAN) as f64 / PAN_RESOLUTION as f64;
            let table_index = (pan - MIN_PAN) as usize;
            left[table_index] = (HALF_PI * real_pan).cos() as f32;
            right[table_index] = (HALF_PI * real_pan).sin() as f32;
        }
        (left, right)
    })
}

/// A mono sum of the voices of a channel that share the pan and the effect send levels.
/// This allows to group voices and have them mixed only once, where the group is then
/// mixed with sends, which is faster.
/// Equivalent to: class VoiceSendBuffer
pub struct VoiceSendBuffer {
    /// The summed voice audio, with the output gain applied (Float64Array).
    pub buffer: Vec<f64>,
    /// The pan table index.
    pub pan_index: usize,
    /// The reverb send level. (0-1)
    pub reverb: f64,
    /// The chorus send level. (0-1)
    pub chorus: f64,
    /// The GS delay or XG variation send level. (0-1)
    pub variation: f64,
}

impl VoiceSendBuffer {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            buffer: vec![0.0; buffer_size],
            pan_index: 0,
            reverb: 0.0,
            chorus: 0.0,
            variation: 0.0,
        }
    }
}

/// The processor's effect send buffers (Float64Array), filled by `mix_voice_send_buffer`.
/// GS effects are mono, XG effects have stereo inputs.
pub struct EffectSends<'a> {
    pub gs_reverb_send: &'a mut [f64],
    pub gs_chorus_send: &'a mut [f64],
    pub gs_delay_send: &'a mut [f64],
    pub xg_reverb_send_l: &'a mut [f64],
    pub xg_reverb_send_r: &'a mut [f64],
    pub xg_chorus_send_l: &'a mut [f64],
    pub xg_chorus_send_r: &'a mut [f64],
    pub xg_variation_send_l: &'a mut [f64],
    pub xg_variation_send_r: &'a mut [f64],
}

/// Processor state read by `render_voice` (TS: `this.synthCore.*`).
pub struct RenderVoiceContext<'a> {
    /// Equivalent to: core.sampleRate
    pub sample_rate: f64,
    /// Equivalent to: core.maxBufferSize
    pub max_buffer_size: usize,
    /// Equivalent to: core.tunings
    pub tunings: &'a [f32],
    /// Equivalent to: core.panSmoothingFactor
    pub pan_smoothing_factor: f64,
    /// Equivalent to: systemParameters.effectsEnabled
    pub effects_enabled: bool,
    /// Equivalent to: systemParameters.reverbGain
    pub reverb_gain: f64,
    /// Equivalent to: systemParameters.chorusGain
    pub chorus_gain: f64,
    /// Equivalent to: systemParameters.gsDelayGain
    pub gs_delay_gain: f64,
    /// Equivalent to: systemParameters.xgVariationGain
    pub xg_variation_gain: f64,
    /// Equivalent to: systemParameters.customVibrato
    pub custom_vibrato: bool,
    /// Equivalent to: core.midiParameters.system === "xg"
    pub is_xg: bool,
    /// Equivalent to: core.xgVariationBlock.insertionMode
    pub xg_variation_insertion_mode: bool,
    /// Equivalent to: core.gsDelayActive
    pub gs_delay_active: bool,
    /// Equivalent to: core.gsInsertionActive
    pub gs_insertion_active: bool,
    /// The voice buffer (Float64Array).
    /// Equivalent to: core.voiceBuffer
    pub voice_buffer: &'a mut [f64],
    /// Equivalent to: core.sharedVoiceSendBuffer
    pub shared_voice_send_buffer: &'a mut VoiceSendBuffer,
    /// Equivalent to: core.gsReverbSend etc.
    pub sends: EffectSends<'a>,
}

/// Where the voice is summed into.
enum SendTarget {
    /// One of `channel.voice_send_buffers`.
    Channel(usize),
    /// `core.sharedVoiceSendBuffer`, mixed at the end of `render_voice`.
    Shared,
}

impl MidiChannel {
    /// Renders a voice to the stereo output buffer.
    /// `sample_count` is the only thing needed as the buffers are 0-based.
    /// Equivalent to: renderVoice(voice, timeNow, sampleCount)
    pub fn render_voice(
        &mut self,
        voice: &mut Voice,
        time_now: f64,
        sample_count: usize,
        core: &mut RenderVoiceContext,
    ) {
        // Check if release
        if !voice.is_in_release && time_now >= voice.release_start_time {
            // Release the voice here
            voice.is_in_release = true;
            if voice.vol_env.start_release(
                &voice.modulated_generators,
                voice.target_key,
                voice.override_release_vol_env,
            ) {
                voice.is_active = false;
            }
            voice.mod_env.start_release(&voice.modulated_generators);

            // Looping mode 3
            if voice.looping_mode == 3 {
                voice.wavetable_mut().is_looping = false;
            }
        }
        voice.has_rendered = true;

        // Important sanity check, as we may disable the voice now
        // Testcase: mono mode with chords
        if !voice.is_active {
            return;
        }

        let sample_rate = core.sample_rate;
        let modulated = voice.modulated_generators;

        // CALCULATION START
        // TUNING
        let mut target_key = voice.target_key;
        // Calculate tuning
        let mut cents = voice.pitch_offset // Voice pitch offset
            + modulated[gt::FINE_TUNE as usize] as f64 // Soundfont fine tune
            + self.channel_octave_tuning[target_key as usize] as f64 // MTS octave tuning
            + self.current_tuning(); // Channel tuning
        let mut semitones = modulated[gt::COARSE_TUNE as usize] as f64; // Soundfont coarse tuning

        // MIDI Tuning Standard
        // Use `midiNote` here since it was used for selecting the preset if tuning was active
        if let Some(preset) = &self.preset {
            let tuning = core.tunings[preset.program as usize * 128 + voice.midi_note as usize];
            if tuning != -1.0 {
                // Tuning is encoded as float
                // For example: 60.56 means key 60 and 56 cents (or 0.56 semitones)
                // Override key
                let tuning = tuning as f64;
                target_key = tuning.trunc() as i16;
                // Add microtonal tuning
                semitones += tuning - target_key as f64;
            }
        }

        // Portamento
        if voice.portamento_from_key > -1 {
            // 0 to 1
            let elapsed = ((time_now - voice.start_time) / voice.portamento_duration).min(1.0);
            let diff = target_key as f64 - voice.portamento_from_key as f64;
            // Zero progress means the pitch being in fromKey, full progress means the normal pitch
            semitones -= diff * (1.0 - elapsed);
        }

        // Calculate tuning by key using soundfont's scale tuning
        cents += (target_key - voice.root_key) as f64 * modulated[gt::SCALE_TUNING as usize] as f64;

        // Low pass excursion with LFO and mod envelope
        let mut lowpass_excursion = 0.0f64;
        let mut volume_excursion_centibels = 0.0f64;
        let mut voice_gain =
            voice.gain_modifier * (1.0 + modulated[gt::AMPLITUDE as usize] as f64 / 1000.0);

        // Vibrato LFO
        if time_now >= voice.vib_lfo_start_time {
            let vib_pitch_depth = modulated[gt::VIB_LFO_TO_PITCH as usize] as f64;
            let vib_vol_depth = modulated[gt::VIB_LFO_TO_VOLUME as usize] as f64;
            let vib_filter_depth = modulated[gt::VIB_LFO_TO_FILTER_FC as usize] as f64;
            let vib_amplitude_depth = modulated[gt::VIB_LFO_AMPLITUDE_DEPTH as usize] as f64;
            if vib_pitch_depth != 0.0
                || vib_vol_depth != 0.0
                || vib_filter_depth != 0.0
                || vib_amplitude_depth != 0.0
            {
                let vib_freq_hz = (abs_cents_to_hz(modulated[gt::FREQ_VIB_LFO as usize] as i32)
                    + modulated[gt::VIB_LFO_RATE as usize] as f64 / 100.0)
                    .max(0.0);
                let rate_inc = (vib_freq_hz * sample_count as f64) / sample_rate;
                let vib_lfo_value = 1.0 - 4.0 * (voice.vib_lfo_phase - 0.5).abs();
                voice.vib_lfo_phase += rate_inc;
                if voice.vib_lfo_phase >= 1.0 {
                    voice.vib_lfo_phase -= 1.0;
                }
                cents += vib_lfo_value * vib_pitch_depth;
                // Low pass frequency
                lowpass_excursion += vib_lfo_value * vib_filter_depth;

                // Vol env volume offset
                // Negate the lfo value because audigy starts with increase rather than decrease
                volume_excursion_centibels += -vib_lfo_value * vib_vol_depth;

                // Amplitude depth
                // Like SCVA: double gain at peak, 0 at lowest (times depth)
                voice_gain *= 1.0 + vib_lfo_value * (vib_amplitude_depth / 1000.0);
            }
        }

        // Mod LFO
        if time_now >= voice.mod_lfo_start_time {
            let mod_pitch_depth = modulated[gt::MOD_LFO_TO_PITCH as usize] as f64;
            let mod_vol_depth = modulated[gt::MOD_LFO_TO_VOLUME as usize] as f64;
            let mod_filter_depth = modulated[gt::MOD_LFO_TO_FILTER_FC as usize] as f64;
            let mod_amplitude_depth = modulated[gt::MOD_LFO_AMPLITUDE_DEPTH as usize] as f64;
            // Don't compute mod lfo unless necessary
            if mod_pitch_depth != 0.0
                || mod_filter_depth != 0.0
                || mod_vol_depth != 0.0
                || mod_amplitude_depth != 0.0
            {
                let mod_freq_hz = (abs_cents_to_hz(modulated[gt::FREQ_MOD_LFO as usize] as i32)
                    + modulated[gt::MOD_LFO_RATE as usize] as f64 / 100.0)
                    .max(0.0);
                let rate_inc = (mod_freq_hz * sample_count as f64) / sample_rate;
                let mod_lfo_value = 1.0 - 4.0 * (voice.mod_lfo_phase - 0.5).abs();
                voice.mod_lfo_phase += rate_inc;
                if voice.mod_lfo_phase >= 1.0 {
                    voice.mod_lfo_phase -= 1.0;
                }
                cents += mod_lfo_value * mod_pitch_depth;
                // Vol env volume offset
                // Negate the lfo value because audigy starts with increase rather than decrease
                volume_excursion_centibels += -mod_lfo_value * mod_vol_depth;
                // Low pass frequency
                lowpass_excursion += mod_lfo_value * mod_filter_depth;

                // Amplitude depth
                // Like SCVA: double gain at peak, 0 at lowest (times depth)
                voice_gain *= 1.0 + mod_lfo_value * (mod_amplitude_depth / 1000.0);
            }
        }

        // Channel vibrato (custom vibrato)
        // TODO: Replace this with proper GS vibrato someday
        if core.custom_vibrato
            && self.midi_controllers[midi_controllers::MODULATION_WHEEL as usize] == 0
            && self.channel_vibrato.depth > 0.0
        {
            // Inlined LFO from 4.2.0
            let vib_start = voice.start_time + self.channel_vibrato.delay;
            if time_now >= vib_start {
                let elapsed = time_now - vib_start;

                // 2pif t gives a full sine cycle at the specified frequency
                cents += (TWO_PI * self.channel_vibrato.rate * elapsed).sin()
                    * self.channel_vibrato.depth;
            }
        }

        // Mod env
        let mod_env_pitch_depth = modulated[gt::MOD_ENV_TO_PITCH as usize];
        let mod_env_filter_depth = modulated[gt::MOD_ENV_TO_FILTER_FC as usize];
        // Don't compute mod env unless necessary
        if mod_env_filter_depth != 0 || mod_env_pitch_depth != 0 {
            let mod_env = voice.mod_env.process(voice.release_start_time, time_now);
            // Apply values
            lowpass_excursion += mod_env * mod_env_filter_depth as f64;
            cents += mod_env * mod_env_pitch_depth as f64;
        }

        // Default resonant modulator: it does not affect the filter gain (neither XG nor GS did that)
        volume_excursion_centibels -= voice.resonance_offset;

        // Finally, calculate the playback rate
        let cents_total = cents + semitones * 100.0;
        let cents_rounded = cents_total as i32;
        // Round for testing if equal,
        // But let's allow sub-microtonal tunings, because why not? :-)
        if cents_rounded != voice.tuning_cents as i32 {
            voice.tuning_cents = cents_rounded as f64;
            voice.tuning_ratio = 2f64.powf(cents_total / 1200.0);
        }

        // Gain target
        let gain_target = cb_attenuation_to_gain(modulated[gt::INITIAL_ATTENUATION as usize] as i32)
            * cb_attenuation_to_gain_f64(volume_excursion_centibels);

        // Looping mode 2: start on release. process only volEnv
        if voice.looping_mode == 2 && !voice.is_in_release {
            voice.is_active = voice.vol_env.process(sample_count, gain_target);
            return;
        }

        // SYNTHESIS
        let buffer = &mut *core.voice_buffer;
        // Wave table oscillator
        let tuning_ratio = voice.tuning_ratio;
        voice.is_active = voice.wavetable_mut().process(sample_count, tuning_ratio, buffer);

        // Vol env (output gain calculation)
        // Get the previous value
        let mut gain = voice.vol_env.output_gain;
        // Compute the new value
        let env_active = voice.vol_env.process(sample_count, gain_target);

        // Note, we do not use &&= as it short-circuits!
        // And we don't do = either as wavetable might've marked it as inactive (end of sample)
        voice.is_active = voice.is_active && env_active;

        // Pan
        let pan = if voice.override_pan != 0.0 {
            voice.override_pan
        } else {
            // Smooth out pan to prevent clicking
            voice.current_pan +=
                (modulated[gt::PAN as usize] as f64 - voice.current_pan) * core.pan_smoothing_factor;
            voice.current_pan
        };

        let output_gain = self.current_gain() * voice_gain;
        let pan_index = ((pan + self.current_pan()).clamp(-500.0, 500.0) + 500.0) as i32 as usize;

        // Do not send to effects if:
        // - Either effects are disabled
        // - Or insertion is active on this channel (Insertion takes over the voice data)
        // - Or the channel is assigned to an XG insertion effect.
        // XG ignores per-drum sends when insertion is enabled, and only the post-insertion
        // (global) audio is sent (so whole drum audio, even if send is 0 for a specific drum)
        // Send levels, relative to the voice's output gain (0-1)
        let mut reverb_level = 0.0;
        let mut chorus_level = 0.0;
        let mut variation_level = 0.0;
        if core.effects_enabled
            && !(self.midi_parameters.efx_assign && core.gs_insertion_active)
            && !self.xg_insertion_assigned
        {
            let reverb_send = modulated[gt::REVERB_EFFECTS_SEND as usize] as f64 * voice.reverb_gain;
            let chorus_send = modulated[gt::CHORUS_EFFECTS_SEND as usize] as f64 * voice.chorus_gain;
            if reverb_send > 0.0 {
                reverb_level = core.reverb_gain * (reverb_send / 1000.0);
            }
            if chorus_send > 0.0 {
                chorus_level = core.chorus_gain * (chorus_send / 1000.0);
            }

            // XG variation send is system-only, in insertion mode CC94 does nothing.
            // GS delay is only sent to when it's active.
            let is_xg = core.is_xg;
            if if is_xg { !core.xg_variation_insertion_mode } else { core.gs_delay_active } {
                let variation_send = self.midi_controllers[midi_controllers::VARIATION_DEPTH as usize]
                    as f64
                    * voice.variation_gain;
                if variation_send > 0.0 {
                    variation_level = (((variation_send as i32) >> 7) as f64 / 127.0)
                        * if is_xg { core.xg_variation_gain } else { core.gs_delay_gain };
                }
            }
        }

        // Voices sharing the pan and the send levels are summed into one mono buffer,
        // which is mixed into the channel output
        // and the effect sends once per block.
        // This way, each voice only performs a single mixing loop, which is faster.
        // Then output gain is applied together with the volume envelope.

        // Find a free voice send buffer
        let mut target: Option<SendTarget> = None;
        let count = self.voice_send_buffer_count;
        for i in 0..count {
            let send_buffer = &self.voice_send_buffers[i];
            if send_buffer.pan_index == pan_index
                && send_buffer.reverb == reverb_level
                && send_buffer.chorus == chorus_level
                && send_buffer.variation == variation_level
            {
                // Found a match
                target = Some(SendTarget::Channel(i));
                break;
            }
        }
        let target = match target {
            Some(t) => t,
            None => {
                let t = if count < self.voice_send_buffers.len() {
                    // Use the free one
                    self.voice_send_buffer_count += 1;
                    SendTarget::Channel(count)
                } else if count < MAX_VOICE_SEND_BUFFERS {
                    // Create a new one
                    self.voice_send_buffers
                        .push(VoiceSendBuffer::new(core.max_buffer_size));
                    self.voice_send_buffer_count += 1;
                    SendTarget::Channel(count)
                } else {
                    // Nothing found, use synth's temporary buffer
                    SendTarget::Shared
                };
                let send_buffer = match t {
                    SendTarget::Channel(i) => &mut self.voice_send_buffers[i],
                    SendTarget::Shared => &mut *core.shared_voice_send_buffer,
                };
                send_buffer.pan_index = pan_index;
                send_buffer.reverb = reverb_level;
                send_buffer.chorus = chorus_level;
                send_buffer.variation = variation_level;
                t
            }
        };

        let out: &mut [f64] = match target {
            SendTarget::Channel(i) => &mut self.voice_send_buffers[i].buffer,
            SendTarget::Shared => &mut core.shared_voice_send_buffer.buffer,
        };
        let buffer = &*core.voice_buffer;
        gain *= output_gain;
        // Calculate increase
        let gain_inc = (voice.vol_env.output_gain * output_gain - gain) / sample_count as f64;

        // Low pass filter and gain (inlined for performance, confirmed with node.js)
        {
            let f = &mut voice.filter;
            let initial_fc = modulated[gt::INITIAL_FILTER_FC as usize] as f64;

            if f.initialized {
                // Note:
                // We only smooth out the initialFc part,
                // the modulation envelope and LFO excursions are not smoothed.
                f.current_initial_fc += (initial_fc - f.current_initial_fc) * f.smoothing_constant;
            } else {
                // Filter initialization, set the current fc to target
                f.initialized = true;
                f.current_initial_fc = initial_fc;
            }

            // The final cutoff for this calculation
            let target_cutoff = f.current_initial_fc + lowpass_excursion;
            let modulated_resonance = modulated[gt::INITIAL_FILTER_Q as usize];
            // Note:
            // the check for initialFC is because of the filter optimization
            // (if cents are the maximum then the filter is open)
            // filter cannot use this optimization if it's dynamic (see #53), and
            // the filter can only be dynamic if the initial filter is not open
            if f.current_initial_fc > 13_499.0 && target_cutoff > 13_499.0 && modulated_resonance == 0
            {
                f.current_initial_fc = 13_500.0;
                // Filter is open, apply gain
                for i in 0..sample_count {
                    out[i] += buffer[i] * gain;
                    gain += gain_inc;
                }
            } else {
                // Check if the frequency has changed. if so, calculate new coefficients
                if (f.last_target_cutoff - target_cutoff).abs() > 1.0
                    || f.resonance_cb != modulated_resonance
                {
                    f.last_target_cutoff = target_cutoff;
                    f.resonance_cb = modulated_resonance;
                    f.calculate_coefficients(target_cutoff);
                }

                // Filter the input
                // Initial filtering code was ported from meltysynth created by sinshu.
                let (a0, a1, a2, a3, a4) = (f.a0, f.a1, f.a2, f.a3, f.a4);
                let (mut x1, mut x2, mut y1, mut y2) = (f.x1, f.x2, f.y1, f.y2);
                // Two samples per iteration, the second one directly from y1:
                // f0 = p0 - a3 * y1, where p0 = a0 * in0 + a1 * x1 + a2 * x2 - a4 * y2
                // f1 = q1 - a3 * f0 - a4 * y1 = q1 - a3 * p0 + (a3 * a3 - a4) * y1
                let b3 = a3 * a3 - a4;
                let mut i = 0;
                while i + 1 < sample_count {
                    let in0 = buffer[i];
                    let in1 = buffer[i + 1];
                    let p0 = a0 * in0 + a1 * x1 + a2 * x2 - a4 * y2;
                    let q1 = a0 * in1 + a1 * in0 + a2 * x1;
                    let f0 = p0 - a3 * y1;
                    let f1 = q1 - a3 * p0 + b3 * y1;
                    x2 = in0;
                    x1 = in1;
                    y2 = f0;
                    y1 = f1;

                    // Apply filter and THEN gain
                    // Per SF2 spec apply order, also see
                    // https://github.com/FluidSynth/fluidsynth/issues/1427
                    out[i] += f0 * gain;
                    gain += gain_inc;
                    out[i + 1] += f1 * gain;
                    gain += gain_inc;
                    i += 2;
                }
                // Odd sample count, process the last sample
                if i < sample_count {
                    let input = buffer[i];
                    let filtered = a0 * input + a1 * x1 + a2 * x2 - a4 * y2 - a3 * y1;
                    x2 = x1;
                    x1 = input;
                    y2 = y1;
                    y1 = filtered;
                    out[i] += filtered * gain;
                }
                f.x1 = x1;
                f.x2 = x2;
                f.y1 = y1;
                f.y2 = y2;
            }
        }

        // No buffer available.
        if let SendTarget::Shared = target {
            mix_voice_send_buffer(
                &mut self.output_left,
                &mut self.output_right,
                core.shared_voice_send_buffer,
                &mut core.sends,
                core.is_xg,
                sample_count,
            );
        }
    }

    /// Mixes all used voice send buffers of this channel, then marks them free.
    /// Equivalent to the per-channel loop in SpessaSynthProcessor.process:
    /// `for (i < c.voiceSendBufferCount) mixVoiceSendBuffer(c, c.voiceSendBuffers[i], ...)`
    pub fn mix_voice_send_buffers(
        &mut self,
        sends: &mut EffectSends,
        is_xg: bool,
        sample_count: usize,
    ) {
        if self.voice_send_buffer_count > 0 {
            for i in 0..self.voice_send_buffer_count {
                mix_voice_send_buffer(
                    &mut self.output_left,
                    &mut self.output_right,
                    &mut self.voice_send_buffers[i],
                    sends,
                    is_xg,
                    sample_count,
                );
            }
            self.voice_send_buffer_count = 0;
        }
    }
}

/// Mixes a voice send buffer into the channel output and the effect sends, then clears it.
/// Equivalent to: mixVoiceSendBuffer(channel, buffer, sampleCount)
pub fn mix_voice_send_buffer(
    output_left: &mut [f64],
    output_right: &mut [f64],
    buffer: &mut VoiceSendBuffer,
    sends: &mut EffectSends,
    is_xg: bool,
    sample_count: usize,
) {
    let (pan_table_left, pan_table_right) = pan_tables();
    let input = &mut buffer.buffer;
    let pan_left = pan_table_left[buffer.pan_index] as f64;
    let pan_right = pan_table_right[buffer.pan_index] as f64;
    for i in 0..sample_count {
        let s = input[i];
        output_left[i] += pan_left * s;
        output_right[i] += pan_right * s;
    }

    let (reverb, chorus, variation) = (buffer.reverb, buffer.chorus, buffer.variation);
    if is_xg {
        // XG effects have stereo inputs
        if reverb > 0.0 {
            send_to_stereo(
                input,
                sends.xg_reverb_send_l,
                sends.xg_reverb_send_r,
                reverb * pan_left,
                reverb * pan_right,
                sample_count,
            );
        }
        if chorus > 0.0 {
            send_to_stereo(
                input,
                sends.xg_chorus_send_l,
                sends.xg_chorus_send_r,
                chorus * pan_left,
                chorus * pan_right,
                sample_count,
            );
        }
        if variation > 0.0 {
            send_to_stereo(
                input,
                sends.xg_variation_send_l,
                sends.xg_variation_send_r,
                variation * pan_left,
                variation * pan_right,
                sample_count,
            );
        }
    } else {
        // GS effects are mono
        if reverb > 0.0 {
            send_to_mono(input, sends.gs_reverb_send, reverb, sample_count);
        }
        if chorus > 0.0 {
            send_to_mono(input, sends.gs_chorus_send, chorus, sample_count);
        }
        if variation > 0.0 {
            send_to_mono(input, sends.gs_delay_send, variation, sample_count);
        }
    }
    input[..sample_count].fill(0.0);
}

/// Adds the voice audio to a mono effect send.
/// Equivalent to: sendToMono(input, output, gain, sampleCount)
fn send_to_mono(input: &[f64], output: &mut [f64], gain: f64, sample_count: usize) {
    for i in 0..sample_count {
        output[i] += gain * input[i];
    }
}

/// Adds the voice audio to a stereo effect send.
/// Equivalent to: sendToStereo(input, outputLeft, outputRight, gainLeft, gainRight, sampleCount)
fn send_to_stereo(
    input: &[f64],
    output_left: &mut [f64],
    output_right: &mut [f64],
    gain_left: f64,
    gain_right: f64,
    sample_count: usize,
) {
    for i in 0..sample_count {
        let s = input[i];
        output_left[i] += s * gain_left;
        output_right[i] += s * gain_right;
    }
}
