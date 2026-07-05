/// synthesizer_core.rs
/// purpose: SynthesizerCore struct — the core synthesis engine which interacts with channels.
/// Ported from: src/synthesizer/audio_engine/synthesizer_core.ts
///
/// # Design note
/// MidiChannel lives in channel/midi_channel.rs. It does not hold a back-reference to
/// SynthesizerCore (unlike TypeScript's `this.synthCore`), so there is no ownership cycle
/// between the two structs; methods on MidiChannel receive the needed data as parameters.
use std::collections::HashMap;

use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
use crate::soundbank::enums::modulator_sources;
use crate::synthesizer::audio_engine::channel::midi_channel::MidiChannel;
use crate::synthesizer::audio_engine::channel::parameters::midi::NON_CC_INDEX_OFFSET;
use crate::synthesizer::audio_engine::effects::chorus::SpessaSynthChorus;
use crate::synthesizer::audio_engine::effects::delay::SpessaSynthDelay;
use crate::synthesizer::audio_engine::effects::insertion::{
    self, InsertionProcessor,
    thru::ThruFx,
};
use crate::synthesizer::audio_engine::effects::reverb::SpessaSynthReverb;
use crate::synthesizer::audio_engine::voice::lowpass_filter::LowpassFilter;
use crate::synthesizer::audio_engine::key_modifier_manager::KeyModifierManager;
use crate::synthesizer::audio_engine::parameters::system::DEFAULT_MASTER_PARAMETERS;
use crate::synthesizer::audio_engine::sound_bank_manager::SoundBankManager;
use crate::synthesizer::audio_engine::synth_constants::DEFAULT_PERCUSSION;
use crate::synthesizer::audio_engine::voice::voice::Voice;
use crate::synthesizer::enums::custom_controllers;
use crate::synthesizer::types::{
    CachedVoiceList, MasterParameterType, SynthProcessorEvent, SynthProcessorOptions, SynthSystem,
};
use crate::utils::loggin::{spessa_synth_info, spessa_synth_warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Gain smoothing factor for rapid volume changes. Must be run every sample.
/// Equivalent to: GAIN_SMOOTHING_FACTOR
const GAIN_SMOOTHING_FACTOR: f64 = 0.01;

/// Pan smoothing factor for rapid pan changes.
/// Equivalent to: PAN_SMOOTHING_FACTOR
const PAN_SMOOTHING_FACTOR: f64 = 0.05;

// ---------------------------------------------------------------------------
// ScheduledEvent
// ---------------------------------------------------------------------------

/// A MIDI event scheduled for a future time.
/// Equivalent to: { callback: () => unknown; time: number }
struct ScheduledEvent {
    callback: Box<dyn FnOnce(&mut SynthesizerCore)>,
    time: f64,
}

// ---------------------------------------------------------------------------
// SynthesizerCore
// ---------------------------------------------------------------------------

/// The core synthesis engine which interacts with channels and holds all synth parameters.
/// Equivalent to: class SynthesizerCore
pub struct SynthesizerCore {
    /// Voice pool. All voices are pre-allocated.
    /// Equivalent to: voices: Voice[]
    pub voices: Vec<Voice>,

    /// All MIDI channels.
    /// Equivalent to: midiChannels: MIDIChannel[]
    pub midi_channels: Vec<MidiChannel>,

    /// Sound bank manager.
    /// Equivalent to: soundBankManager: SoundBankManager
    pub sound_bank_manager: SoundBankManager,

    /// Key modifier manager for custom key overrides.
    /// Equivalent to: keyModifierManager: KeyModifierManager
    pub key_modifier_manager: KeyModifierManager,

    /// Audio sample rate in Hz.
    /// Equivalent to: sampleRate
    pub sample_rate: f64,

    /// MIDI Tuning Standard table: tunings[program * 128 + key] = note.cents
    /// -1.0 means no change.
    /// Equivalent to: tunings: Float32Array(128 * 128).fill(-1)
    pub tunings: Vec<f32>,

    /// Master synthesizer parameters.
    /// Equivalent to: masterParameters
    pub master_parameters: MasterParameterType,

    /// Current synthesizer time in seconds.
    /// Equivalent to: currentTime
    pub current_time: f64,

    /// Overall MIDI volume gain (0.0–1.0, set by SysEx).
    /// Equivalent to: midiVolume
    pub midi_volume: f64,

    /// True if chorus and reverb effects are enabled.
    /// Equivalent to: enableEffects
    pub enable_effects: bool,

    /// True if the event system is enabled.
    /// Equivalent to: enableEventSystem
    pub enable_event_system: bool,

    /// Reverb effect processor.
    pub reverb_processor: SpessaSynthReverb,

    /// Chorus effect processor.
    pub chorus_processor: SpessaSynthChorus,

    /// Delay effect processor.
    pub delay_processor: SpessaSynthDelay,

    /// Whether delay effect is active (enabled via SysEx).
    pub delay_active: bool,

    /// Mono reverb input buffer (zero-indexed, cleared each render call).
    reverb_input: Vec<f32>,

    /// Mono chorus input buffer (zero-indexed, cleared each render call).
    chorus_input: Vec<f32>,

    /// Mono delay input buffer (zero-indexed, cleared each render call).
    delay_input: Vec<f32>,

    /// Left channel pan (0.0–1.0).
    /// Equivalent to: panLeft
    pub pan_left: f64,

    /// Right channel pan (0.0–1.0).
    /// Equivalent to: panRight
    pub pan_right: f64,

    /// Gain smoothing factor adjusted to the sample rate.
    /// Equivalent to: gainSmoothingFactor
    pub gain_smoothing_factor: f64,

    /// Pan smoothing factor adjusted to the sample rate.
    /// Equivalent to: panSmoothingFactor
    pub pan_smoothing_factor: f64,

    /// Callback invoked for synthesizer events.
    /// Equivalent to: eventCallbackHandler
    pub event_callback: Box<dyn Fn(SynthProcessorEvent)>,

    /// Cache of computed voice lists, keyed by get_cached_voice_index output.
    /// Equivalent to: cachedVoices: Map<number, CachedVoiceList>
    pub cached_voices: HashMap<u64, CachedVoiceList>,

    /// Total active voice count.
    /// Equivalent to: voiceCount
    pub voice_count: u32,

    /// Last time voice priorities were assigned (avoids redundant work in a quantum).
    /// Equivalent to: lastPriorityAssignmentTime (private)
    last_priority_assignment_time: f64,

    /// Event queue for future-scheduled messages.
    /// Equivalent to: eventQueue (private)
    event_queue: Vec<ScheduledEvent>,

    /// Duration of a single sample in seconds.
    /// Equivalent to: sampleTime (private)
    sample_time: f64,

    /// Insertion effect processor.
    pub insertion_processor: Box<dyn InsertionProcessor>,

    /// True if any channel has insertion enabled (optimization flag).
    pub insertion_active: bool,

    /// Stereo insertion input buffers (zero-indexed).
    insertion_input_l: Vec<f32>,
    insertion_input_r: Vec<f32>,

    /// Parameter cache for insertion snapshot tracking (255 = unchanged).
    pub insertion_params: [u8; 20],
}

impl SynthesizerCore {
    /// Creates a new SynthesizerCore.
    /// Equivalent to: constructor(eventCallbackHandler, missingPresetHandler, sampleRate, options)
    pub fn new(
        event_callback: impl Fn(SynthProcessorEvent) + 'static,
        sample_rate: f64,
        options: SynthProcessorOptions,
    ) -> Self {
        let gain_smoothing_factor = GAIN_SMOOTHING_FACTOR * (44_100.0 / sample_rate);
        let pan_smoothing_factor = PAN_SMOOTHING_FACTOR * (44_100.0 / sample_rate);
        LowpassFilter::init_cache(sample_rate);

        // Initialize voice pool
        let voice_cap = DEFAULT_MASTER_PARAMETERS.voice_cap as usize;
        let mut voices = Vec::with_capacity(voice_cap);
        for _ in 0..voice_cap {
            voices.push(Voice::new(sample_rate));
        }

        let tunings = vec![-1.0f32; 128 * 128];

        let buf_size = 128;
        Self {
            voices,
            midi_channels: Vec::new(),
            sound_bank_manager: SoundBankManager::new(|| {}),
            key_modifier_manager: KeyModifierManager::new(),
            sample_rate,
            tunings,
            master_parameters: DEFAULT_MASTER_PARAMETERS,
            current_time: options.initial_time,
            midi_volume: 1.0,
            enable_effects: options.enable_effects,
            enable_event_system: options.enable_event_system,
            reverb_processor: SpessaSynthReverb::new(sample_rate),
            chorus_processor: SpessaSynthChorus::new(sample_rate),
            delay_processor: SpessaSynthDelay::new(sample_rate),
            delay_active: false,
            reverb_input: vec![0.0; buf_size],
            chorus_input: vec![0.0; buf_size],
            delay_input: vec![0.0; buf_size],
            pan_left: 0.5,
            pan_right: 0.5,
            gain_smoothing_factor,
            pan_smoothing_factor,
            event_callback: Box::new(event_callback),
            cached_voices: HashMap::new(),
            voice_count: 0,
            last_priority_assignment_time: 0.0,
            event_queue: Vec::new(),
            sample_time: 1.0 / sample_rate,
            insertion_processor: Box::new(ThruFx::new(sample_rate)),
            insertion_active: false,
            insertion_input_l: vec![0.0; buf_size],
            insertion_input_r: vec![0.0; buf_size],
            insertion_params: [255u8; 20],
        }
    }

    /// Dispatches an event through the event callback.
    /// Equivalent to: callEvent(eventName, eventData)
    pub fn call_event(&self, event: SynthProcessorEvent) {
        if self.enable_event_system {
            (self.event_callback)(event);
        }
    }

    /// Assigns the first available (inactive) voice, stealing the lowest-priority one if needed.
    /// Equivalent to: assignVoice()
    pub fn assign_voice(&mut self) -> &mut Voice {
        let voice_cap = self.master_parameters.voice_cap as usize;
        // Find an inactive voice
        for i in 0..voice_cap {
            if !self.voices[i].is_active {
                self.voices[i].priority = i32::MAX;
                return &mut self.voices[i];
            }
        }
        // All voices active — assign priorities and steal the lowest
        self.assign_voice_priorities();
        let mut lowest_idx = 0;
        for i in 1..voice_cap {
            if self.voices[i].priority < self.voices[lowest_idx].priority {
                lowest_idx = i;
            }
        }
        self.voices[lowest_idx].priority = i32::MAX;
        &mut self.voices[lowest_idx]
    }

    /// Like `assign_voice()` but returns the voice index instead of a reference.
    /// Used by `note_on` to allow simultaneous borrows of `voices` and `midi_channels`.
    pub(crate) fn assign_voice_idx(&mut self) -> usize {
        let voice_cap = self.master_parameters.voice_cap as usize;
        for i in 0..voice_cap {
            if !self.voices[i].is_active {
                self.voices[i].priority = i32::MAX;
                return i;
            }
        }
        self.assign_voice_priorities();
        let mut lowest_idx = 0;
        for i in 1..voice_cap {
            if self.voices[i].priority < self.voices[lowest_idx].priority {
                lowest_idx = i;
            }
        }
        self.voices[lowest_idx].priority = i32::MAX;
        lowest_idx
    }

    /// Stops all notes on all channels.
    /// Equivalent to: stopAllChannels(force)
    pub fn stop_all_channels(&mut self, force: bool) {
        spessa_synth_info("Stop all received!");
        let current_time = self.current_time;
        let voices = &mut self.voices;
        let mut events = Vec::new();
        for channel in self.midi_channels.iter_mut() {
            let evs = channel.stop_all_notes(voices, current_time, force);
            events.extend(evs);
        }
        for event in events {
            self.call_event(event);
        }
    }

    /// Creates a new MIDI channel and optionally fires events.
    /// Equivalent to: createMIDIChannel(sendEvent)
    pub fn create_midi_channel(&mut self, send_event: bool) {
        let channel_number = self.midi_channels.len() as u8;
        let (preset, bank_idx) = self.get_default_preset_and_idx();
        let mut channel = MidiChannel::new(preset, bank_idx, channel_number);

        // Channel 9 (0-based) is the default percussion channel.
        if channel_number % 16 == DEFAULT_PERCUSSION {
            channel.drum_channel = true;
        }

        self.midi_channels.push(channel);

        if send_event {
            self.call_event(SynthProcessorEvent::NewChannel);
            let ch = self.midi_channels.last().unwrap();
            if let Some(ev) = ch.build_channel_property_event(self.enable_event_system) {
                self.call_event(ev);
            }
        }
    }

    /// Returns the default preset clone and its bank index (if loaded).
    fn get_default_preset_and_idx(&self) -> (Option<BasicPreset>, Option<usize>) {
        let patch = MidiPatch {
            program: 0,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        };
        if let Some((preset, bank_idx)) = self
            .sound_bank_manager
            .get_preset_and_bank_idx(patch, SynthSystem::Xg)
        {
            (Some(preset.clone()), Some(bank_idx))
        } else {
            (None, None)
        }
    }

    /// Resets all controllers on all channels.
    /// Equivalent to: resetAllControllers(system = DEFAULT_SYNTH_MODE)
    pub fn reset_all_controllers(&mut self, system: SynthSystem) {
        self.call_event(SynthProcessorEvent::AllControllerReset);
        self.master_parameters.midi_system = system;
        // Reset private fields
        self.tunings.fill(-1.0);
        self.set_midi_volume(1.0);
        // Default effect macros: Hall2, Chorus3, Delay1
        self.set_reverb_macro(4);
        self.set_chorus_macro(2);
        self.set_delay_macro(0);
        self.delay_active = false;
        self.reset_insertion();

        let enable_event_system = self.enable_event_system;
        let current_time = self.current_time;
        let mut events = Vec::new();

        // Reset controllers and preset for each channel (TS: ch.resetControllers(false); ch.resetPreset())
        for ch_idx in 0..self.midi_channels.len() {
            let mut sub = self.midi_channels[ch_idx].reset_controllers(
                false, // do not send CC events
                &mut self.voices,
                current_time,
                system,
                enable_event_system,
            );
            events.append(&mut sub);

            let mut sub = self.midi_channels[ch_idx].reset_preset(
                &self.sound_bank_manager,
                system,
                enable_event_system,
            );
            events.append(&mut sub);
        }

        for ch_idx in 0..self.midi_channels.len() {
            // Restore locked controller events.
            for cc_num in 0..128usize {
                if self.midi_channels[ch_idx].locked_controllers[cc_num] {
                    use crate::midi::enums::MidiController;
                    use crate::synthesizer::types::ControllerChangeCallback;
                    events.push(SynthProcessorEvent::ControllerChange(
                        ControllerChangeCallback {
                            channel: ch_idx as u8,
                            controller_number: cc_num as MidiController,
                            controller_value: (self.midi_channels[ch_idx].midi_controllers[cc_num]
                                >> 7) as u8,
                        },
                    ));
                }
            }

            // Restore pitch wheel event.
            let pitch_lock_idx = NON_CC_INDEX_OFFSET + modulator_sources::PITCH_WHEEL as usize;
            if !self.midi_channels[ch_idx].locked_controllers[pitch_lock_idx] {
                use crate::synthesizer::types::PitchWheelCallback;
                let val = self.midi_channels[ch_idx].midi_controllers[pitch_lock_idx];
                events.push(SynthProcessorEvent::PitchWheel(PitchWheelCallback {
                    channel: ch_idx as u8,
                    pitch: val as u16,
                    midi_note: -1,
                }));
            }

            // Restore channel pressure event.
            let cp_lock_idx = NON_CC_INDEX_OFFSET + modulator_sources::CHANNEL_PRESSURE as usize;
            if !self.midi_channels[ch_idx].locked_controllers[cp_lock_idx] {
                use crate::synthesizer::types::ChannelPressureCallback;
                let val = self.midi_channels[ch_idx].midi_controllers[cp_lock_idx] >> 7;
                events.push(SynthProcessorEvent::ChannelPressure(
                    ChannelPressureCallback {
                        channel: ch_idx as u8,
                        pressure: val as u8,
                    },
                ));
            }
        }

        for event in events {
            self.call_event(event);
        }
    }

    /// Renders audio for the current quantum.
    /// Equivalent to: renderAudio(outputs, reverb, chorus, startIndex, sampleCount)
    pub fn render_audio(
        &mut self,
        outputs: &mut [Vec<f32>],
        start_index: usize,
        sample_count: usize,
    ) {
        // Process scheduled events.
        self.process_event_queue();

        let quantum_size = if sample_count > 0 {
            sample_count
        } else {
            outputs[0].len().saturating_sub(start_index)
        };

        // Clear voice counts.
        for ch in self.midi_channels.iter_mut() {
            ch.clear_voice_count();
        }
        self.voice_count = 0;

        let enable_effects = self.enable_effects;
        let master_gain = self.master_parameters.master_gain;
        let reverb_gain = self.master_parameters.reverb_gain;
        let chorus_gain = self.master_parameters.chorus_gain;
        let delay_gain = self.master_parameters.delay_gain;
        let midi_volume = self.midi_volume;
        let pan_left = self.pan_left;
        let pan_right = self.pan_right;
        let pan_smoothing_factor = self.pan_smoothing_factor;
        let current_time = self.current_time;
        let delay_active = self.delay_active;
        let insertion_active = self.insertion_active;
        let out_len = outputs[0].len();

        // Grow and clear effect input buffers if effects are enabled.
        if enable_effects {
            if self.reverb_input.len() < quantum_size {
                self.reverb_input.resize(quantum_size, 0.0);
                self.chorus_input.resize(quantum_size, 0.0);
                self.delay_input.resize(quantum_size, 0.0);
            } else {
                self.reverb_input[..quantum_size].fill(0.0);
                self.chorus_input[..quantum_size].fill(0.0);
                if delay_active {
                    self.delay_input[..quantum_size].fill(0.0);
                }
            }

            // Grow and clear insertion input buffers if insertion is active.
            if insertion_active {
                if self.insertion_input_l.len() < quantum_size {
                    self.insertion_input_l.resize(quantum_size, 0.0);
                    self.insertion_input_r.resize(quantum_size, 0.0);
                } else {
                    self.insertion_input_l[..quantum_size].fill(0.0);
                    self.insertion_input_r[..quantum_size].fill(0.0);
                }
            }
        }

        // Render active voices.
        // SAFETY: voices, midi_channels, and effect buffers are separate Vec fields — no aliasing.
        // We use raw pointers for the output slices to avoid borrow conflicts while
        // also mutably borrowing self.midi_channels[ch_idx].
        let out_l_ptr = outputs[0].as_mut_ptr();
        let out_r_ptr = outputs[1].as_mut_ptr();
        let rev_ptr = self.reverb_input.as_mut_ptr();
        let chr_ptr = self.chorus_input.as_mut_ptr();
        let dly_ptr = self.delay_input.as_mut_ptr();
        let rev_len = self.reverb_input.len();
        let chr_len = self.chorus_input.len();
        let dly_len = self.delay_input.len();
        let ins_l_ptr = self.insertion_input_l.as_mut_ptr();
        let ins_r_ptr = self.insertion_input_r.as_mut_ptr();
        let ins_l_len = self.insertion_input_l.len();
        let ins_r_len = self.insertion_input_r.len();

        for v_idx in 0..self.voices.len() {
            if !self.voices[v_idx].is_active {
                continue;
            }
            let ch_idx = self.voices[v_idx].channel as usize;
            if self.midi_channels[ch_idx].is_muted {
                continue;
            }

            self.midi_channels[ch_idx].voice_count += 1;
            self.voice_count += 1;

            let out_l_slice = unsafe { std::slice::from_raw_parts_mut(out_l_ptr, out_len) };
            let out_r_slice = unsafe { std::slice::from_raw_parts_mut(out_r_ptr, out_len) };
            let rev_slice = unsafe { std::slice::from_raw_parts_mut(rev_ptr, rev_len) };
            let chr_slice = unsafe { std::slice::from_raw_parts_mut(chr_ptr, chr_len) };
            let dly_slice = unsafe { std::slice::from_raw_parts_mut(dly_ptr, dly_len) };
            let ins_l_slice = unsafe { std::slice::from_raw_parts_mut(ins_l_ptr, ins_l_len) };
            let ins_r_slice = unsafe { std::slice::from_raw_parts_mut(ins_r_ptr, ins_r_len) };

            self.midi_channels[ch_idx].render_voice(
                &mut self.voices[v_idx],
                current_time,
                out_l_slice,
                out_r_slice,
                rev_slice,
                chr_slice,
                dly_slice,
                start_index,
                quantum_size,
                master_gain,
                reverb_gain,
                chorus_gain,
                delay_gain,
                midi_volume,
                pan_left,
                pan_right,
                enable_effects,
                delay_active,
                pan_smoothing_factor,
                &self.tunings,
                ins_l_slice,
                ins_r_slice,
                insertion_active,
            );

        }

        // Process effect chain: Insertion → Chorus → Delay → Reverb
        if enable_effects {
            let (out_left, out_rest) = outputs.split_at_mut(1);
            let out_l = &mut out_left[0];
            let out_r = &mut out_rest[0];

            // Insertion first (if active)
            if insertion_active {
                let ins_l = self.insertion_input_l[..quantum_size].to_vec();
                let ins_r = self.insertion_input_r[..quantum_size].to_vec();
                self.insertion_processor.process(
                    &ins_l,
                    &ins_r,
                    out_l,
                    out_r,
                    &mut self.reverb_input,
                    &mut self.chorus_input,
                    &mut self.delay_input,
                    start_index,
                    quantum_size,
                );
            }

            // Chorus sends to reverb and delay
            let chorus_in = self.chorus_input[..quantum_size].to_vec();
            self.chorus_processor.process(
                &chorus_in,
                out_l,
                out_r,
                &mut self.reverb_input,
                &mut self.delay_input,
                start_index,
                quantum_size,
            );

            // Delay sends to reverb (only if active and not XG)
            if delay_active && self.master_parameters.midi_system != SynthSystem::Xg {
                let delay_in = self.delay_input[..quantum_size].to_vec();
                self.delay_processor.process(
                    &delay_in,
                    out_l,
                    out_r,
                    &mut self.reverb_input,
                    start_index,
                    quantum_size,
                );
            }

            // Reverb goes directly to output
            let reverb_in = self.reverb_input[..quantum_size].to_vec();
            self.reverb_processor.process(
                &reverb_in,
                out_l,
                out_r,
                start_index,
                quantum_size,
            );
        }

        // Fire voice count change events.
        let enable_event_system = self.enable_event_system;
        let mut events = Vec::new();
        for ch in self.midi_channels.iter() {
            if let Some(ev) = ch.update_voice_count(enable_event_system) {
                events.push(ev);
            }
        }
        for event in events {
            self.call_event(event);
        }

        // Advance time.
        self.current_time += quantum_size as f64 * self.sample_time;
    }

    /// Sets the reverb macro (SC-8850 manual page 81).
    pub fn set_reverb_macro(&mut self, macro_num: u8) {
        let rev = &mut self.reverb_processor;
        rev.set_level(64);
        rev.set_pre_delay_time(0);
        rev.set_character(macro_num);
        match macro_num {
            1 => {
                // Room2
                rev.set_pre_lowpass(4);
                rev.set_time(56);
                rev.set_delay_feedback(0);
            }
            2 => {
                // Room3
                rev.set_pre_lowpass(0);
                rev.set_time(72);
                rev.set_delay_feedback(0);
            }
            3 => {
                // Hall1
                rev.set_pre_lowpass(4);
                rev.set_time(72);
                rev.set_delay_feedback(0);
            }
            4 => {
                // Hall2
                rev.set_pre_lowpass(0);
                rev.set_time(64);
                rev.set_delay_feedback(0);
            }
            5 => {
                // Plate
                rev.set_pre_lowpass(0);
                rev.set_time(88);
                rev.set_delay_feedback(0);
            }
            6 => {
                // Delay
                rev.set_pre_lowpass(0);
                rev.set_time(32);
                rev.set_delay_feedback(40);
            }
            7 => {
                // Panning delay
                rev.set_pre_lowpass(0);
                rev.set_time(64);
                rev.set_delay_feedback(32);
            }
            _ => {
                // Room1 (default)
                rev.set_character(0);
                rev.set_pre_lowpass(3);
                rev.set_time(80);
                rev.set_delay_feedback(0);
                rev.set_pre_delay_time(0);
            }
        }
    }

    /// Sets the chorus macro (SC-8850 manual page 83).
    pub fn set_chorus_macro(&mut self, macro_num: u8) {
        let chr = &mut self.chorus_processor;
        chr.set_level(64);
        chr.set_pre_lowpass(0);
        chr.set_delay(127);
        chr.set_send_level_to_delay(0);
        chr.set_send_level_to_reverb(0);
        match macro_num {
            1 => {
                // Chorus2
                chr.set_feedback(5);
                chr.set_delay(80);
                chr.set_rate(9);
                chr.set_depth(19);
            }
            2 => {
                // Chorus3
                chr.set_feedback(8);
                chr.set_delay(80);
                chr.set_rate(3);
                chr.set_depth(19);
            }
            3 => {
                // Chorus4
                chr.set_feedback(16);
                chr.set_delay(64);
                chr.set_rate(9);
                chr.set_depth(16);
            }
            4 => {
                // FbChorus
                chr.set_feedback(64);
                chr.set_delay(127);
                chr.set_rate(2);
                chr.set_depth(24);
            }
            5 => {
                // Flanger
                chr.set_feedback(112);
                chr.set_delay(127);
                chr.set_rate(1);
                chr.set_depth(5);
            }
            6 => {
                // SDelay
                chr.set_feedback(0);
                chr.set_depth(127);
                chr.set_rate(0);
                chr.set_depth(127);
            }
            7 => {
                // SDelayFb
                chr.set_feedback(80);
                chr.set_depth(127);
                chr.set_rate(0);
                chr.set_depth(127);
            }
            _ => {
                // Chorus1 (default)
                chr.set_feedback(0);
                chr.set_delay(112);
                chr.set_rate(3);
                chr.set_depth(5);
            }
        }
    }

    /// Sets the delay macro (SC-8850 manual page 85).
    pub fn set_delay_macro(&mut self, macro_num: u8) {
        let dly = &mut self.delay_processor;
        dly.set_level(64);
        dly.set_pre_lowpass(0);
        dly.set_send_level_to_reverb(0);
        dly.set_level_right(0);
        dly.set_level_left(0);
        dly.set_level_center(127);
        match macro_num {
            1 => {
                // Delay2
                dly.set_time_center(106);
                dly.set_time_ratio_left(1);
                dly.set_time_ratio_right(1);
                dly.set_feedback(80);
            }
            2 => {
                // Delay3
                dly.set_time_center(115);
                dly.set_time_ratio_left(1);
                dly.set_time_ratio_right(1);
                dly.set_feedback(72);
            }
            3 => {
                // Delay4
                dly.set_time_center(83);
                dly.set_time_ratio_left(1);
                dly.set_time_ratio_right(1);
                dly.set_feedback(72);
            }
            4 => {
                // PanDelay1
                dly.set_time_center(105);
                dly.set_time_ratio_left(12);
                dly.set_time_ratio_right(24);
                dly.set_level_center(0);
                dly.set_level_left(125);
                dly.set_level_right(60);
                dly.set_feedback(74);
            }
            5 => {
                // PanDelay2
                dly.set_time_center(109);
                dly.set_time_ratio_left(12);
                dly.set_time_ratio_right(24);
                dly.set_level_center(0);
                dly.set_level_left(125);
                dly.set_level_right(60);
                dly.set_feedback(71);
            }
            6 => {
                // PanDelay3
                dly.set_time_center(115);
                dly.set_time_ratio_left(12);
                dly.set_time_ratio_right(24);
                dly.set_level_center(0);
                dly.set_level_left(120);
                dly.set_level_right(64);
                dly.set_feedback(73);
            }
            7 => {
                // PanDelay4
                dly.set_time_center(93);
                dly.set_time_ratio_left(12);
                dly.set_time_ratio_right(24);
                dly.set_level_center(0);
                dly.set_level_left(120);
                dly.set_level_right(64);
                dly.set_feedback(72);
            }
            8 => {
                // DelayToReverb
                dly.set_time_center(109);
                dly.set_time_ratio_left(12);
                dly.set_time_ratio_right(24);
                dly.set_level_center(0);
                dly.set_level_left(114);
                dly.set_level_right(60);
                dly.set_feedback(61);
                dly.set_send_level_to_reverb(36);
            }
            9 => {
                // PanRepeat
                dly.set_time_center(110);
                dly.set_time_ratio_left(21);
                dly.set_time_ratio_right(32);
                dly.set_level_center(97);
                dly.set_level_left(127);
                dly.set_level_right(67);
                dly.set_feedback(40);
            }
            _ => {
                // Delay1 (default)
                dly.set_time_center(97);
                dly.set_time_ratio_left(1);
                dly.set_time_ratio_right(1);
                dly.set_feedback(80);
            }
        }
    }

    /// Resets the insertion effect to defaults.
    /// Equivalent to: resetInsertion()
    pub fn reset_insertion(&mut self) {
        self.insertion_active = false;
        self.insertion_processor = Box::new(ThruFx::new(self.sample_rate));
        self.insertion_processor.set_send_level_to_reverb(40.0 / 127.0);
        self.insertion_processor.set_send_level_to_chorus(0.0);
        self.insertion_processor.set_send_level_to_delay(0.0);
        self.insertion_params = [255u8; 20];
        for ch in self.midi_channels.iter_mut() {
            ch.insertion_enabled = false;
        }
    }

    /// Gets voices for a channel+note+velocity, applying key modifier overrides.
    /// Equivalent to: getVoices(channel, midiNote, velocity)
    pub fn get_voices(&self, channel: u8, midi_note: u8, velocity: u8) -> CachedVoiceList {
        let channel_obj = &self.midi_channels[channel as usize];

        let override_patch = self
            .key_modifier_manager
            .has_override_patch(channel, midi_note);

        if override_patch {
            let patch = match self.key_modifier_manager.get_patch(channel, midi_note) {
                Ok(p) => p,
                Err(_) => return Vec::new(),
            };
            if let Some((preset, bank_idx)) = self
                .sound_bank_manager
                .get_preset_and_bank_idx(patch, self.master_parameters.midi_system)
            {
                let bank = &self.sound_bank_manager.sound_bank_list[bank_idx].sound_bank;
                return self.get_voices_for_preset(preset, bank, midi_note, velocity);
            }
            return Vec::new();
        }

        // Use channel's stored preset.
        if let (Some(preset), Some(bank_idx)) = (&channel_obj.preset, channel_obj.preset_bank_idx) {
            let bank = &self.sound_bank_manager.sound_bank_list[bank_idx].sound_bank;
            return self.get_voices_for_preset(preset, bank, midi_note, velocity);
        }
        Vec::new()
    }

    /// Gets voices for a given preset+note+velocity from the sound bank (no cache).
    ///
    /// Access audio data via `sample.audio_data` directly (SF2 data is always pre-loaded;
    /// SF3 vorbis data must be pre-decoded before synthesis starts).
    ///
    /// Equivalent to: getVoicesForPreset(preset, midiNote, velocity)
    pub fn get_voices_for_preset(
        &self,
        preset: &BasicPreset,
        bank: &crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank,
        midi_note: u8,
        velocity: u8,
    ) -> CachedVoiceList {
        use crate::synthesizer::audio_engine::voice::voice_cache::CachedVoice;

        let voice_params = preset.get_voice_parameters(
            midi_note,
            velocity,
            &bank.instruments,
            &bank.default_modulators,
        );

        let mut voices = CachedVoiceList::new();
        for vp in voice_params {
            let sample = match bank.samples.get(vp.sample_idx) {
                Some(s) => s,
                None => {
                    spessa_synth_warn(&format!(
                        "get_voices_for_preset: invalid sample index {}",
                        vp.sample_idx
                    ));
                    continue;
                }
            };

            // Audio data must be pre-loaded (SF2) or pre-decoded (SF3 vorbis).
            let audio_data = match &sample.audio_data {
                Some(data) => data.clone(),
                None => {
                    spessa_synth_warn(&format!(
                        "Discarding invalid sample: {}",
                        sample.name
                    ));
                    continue;
                }
            };

            let cv = CachedVoice::from_bank_params(
                vp,
                audio_data,
                sample.original_key as i16,
                sample.loop_start,
                sample.loop_end,
                sample.sample_rate as f64,
                sample.pitch_correction as f64,
                midi_note,
                velocity,
                self.sample_rate,
            );
            voices.push(cv);
        }
        voices
    }

    /// Clears the voice cache.
    /// Equivalent to: clearCache()
    pub fn clear_cache(&mut self) {
        self.cached_voices.clear();
    }

    /// Sets the MIDI volume (raised to e as per GM2 spec).
    /// Equivalent to: setMIDIVolume(volume) (protected)
    pub fn set_midi_volume(&mut self, volume: f64) {
        self.midi_volume = volume.powf(std::f64::consts::E);
    }

    /// Sets the master tuning for all channels.
    /// Equivalent to: setMasterTuning(cents) (protected)
    pub fn set_master_tuning(&mut self, cents: f64) {
        let cents = cents.round();
        for ch in self.midi_channels.iter_mut() {
            ch.set_custom_controller(custom_controllers::MASTER_TUNING, cents);
        }
    }

    /// Destroys the synthesizer, releasing all resources.
    /// Equivalent to: destroySynthProcessor()
    pub fn destroy(&mut self) {
        self.voices.clear();
        for ch in self.midi_channels.iter_mut() {
            ch.locked_controllers.clear();
            ch.preset = None;
        }
        self.clear_cache();
        self.midi_channels.clear();
        self.sound_bank_manager.destroy();
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Processes all scheduled events whose time has arrived.
    /// Equivalent to: event queue processing in renderAudio
    fn process_event_queue(&mut self) {
        if self.event_queue.is_empty() {
            return;
        }
        let time = self.current_time;
        while !self.event_queue.is_empty() {
            if self.event_queue[0].time > time {
                break;
            }
            let event = self.event_queue.remove(0);
            (event.callback)(self);
        }
    }

    /// Assigns priorities to all voices for voice-stealing decisions.
    /// Equivalent to: assignVoicePriorities() (private)
    fn assign_voice_priorities(&mut self) {
        if (self.last_priority_assignment_time - self.current_time).abs() < f64::EPSILON {
            return;
        }
        self.last_priority_assignment_time = self.current_time;
        for voice in self.voices.iter_mut() {
            voice.priority = 0;
            let ch_idx = voice.channel as usize;
            if ch_idx < self.midi_channels.len() && self.midi_channels[ch_idx].drum_channel {
                voice.priority += 5;
            }
            if voice.is_in_release {
                voice.priority -= 5;
            }
            voice.priority += (voice.velocity as i32) / 25;
            voice.priority -= voice.vol_env.state as i32;
            if voice.is_in_release {
                voice.priority -= 5;
            }
            voice.priority -= (voice.vol_env.attenuation_cb / 200.0) as i32;
        }
    }

    /// Computes the cache key for a given patch+note+velocity.
    /// Equivalent to: getCachedVoiceIndex(patch, midiNote, velocity) (private)
    pub(crate) fn get_cached_voice_index(&self, patch: &MidiPatch, midi_note: u8, velocity: u8) -> u64 {
        let (bank_msb, bank_lsb) = if patch.is_gm_gs_drum {
            (128u64, 0u64)
        } else {
            (patch.bank_msb as u64, patch.bank_lsb as u64)
        };
        let program = patch.program as u64;
        let note = midi_note as u64;
        let vel = velocity as u64;

        bank_msb + bank_lsb * 128 + program * 16_384 + 2_097_152 * note + 268_435_456 * vel
    }

    /// Pushes a callback to the event queue to be called at the given time.
    /// Used by process_message to schedule future MIDI events.
    pub(crate) fn schedule_event(
        &mut self,
        callback: impl FnOnce(&mut SynthesizerCore) + 'static,
        time: f64,
    ) {
        self.event_queue
            .push(ScheduledEvent { callback: Box::new(callback), time });
    }
}
