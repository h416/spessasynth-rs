/// synthesizer_core.rs
/// purpose: SynthesizerCore struct — the core synthesis engine which interacts with channels.
/// Ported from: src/synthesizer/audio_engine/synthesizer_core.ts (spessasynth_core 4.3.0)
///
/// # Design note
/// MidiChannel lives in channel/midi_channel.rs. It does not hold a back-reference to
/// SynthesizerCore (unlike TypeScript's `this.synthCore`), so there is no ownership cycle
/// between the two structs; methods on MidiChannel receive the needed data as parameters.
///
/// # Changes from 4.2.0 (reviewed against the 4.3.0 diff)
/// Orchestration-layer changes ported in Task 20:
/// - `masterParameters` split into `midiParameters: GlobalMIDIParameter` +
///   `systemParameters: GlobalSystemParameter` (see `parameters/{midi,system}.rs`).
///   `enableEffects`/`enableEventSystem` core fields folded into the system parameters.
/// - `maxBufferSize` (from `SynthProcessorOptions`): all effect input buffers are allocated
///   once at that size, the 4.2.0 grow-on-demand path is gone, and rendering more samples
///   than `maxBufferSize` panics (TS throws).
/// - `voiceCount` became private with a public getter.
/// - `assignVoice` gained the `autoAllocateVoices` path (allocate a new voice instead of
///   stealing when the cap is hit; cap grows by 1; logs instead of firing an event). Note:
///   TS 4.3.0 has an upstream quirk here — `allocateNewVoices(1)` already pushes the new
///   voice and then `this.voices.push(v)` pushes the *same object* again (aliased at two
///   indices). A Rust `Vec<Voice>` cannot alias one element at two indices; only the intent
///   (allocate one, return it) is ported. The duplicate slot is beyond the cap and unused.
/// - `resetAllControllers` renamed to `reset`, now firing `Reset(system)` (was
///   `allControllerReset`), calling `resetMIDIParameters(system)` and respecting
///   `delayLock`. The 4.2.0 post-reset locked-controller / pitch-wheel / channel-pressure
///   event-restoration block was removed upstream (TS 4.3.0 fires those from
///   `ch.reset(false)` instead) — removed here too. TODO(Task 21): the per-channel reset
///   still uses the legacy `reset_controllers`/`reset_preset` pair (TS 4.3.0: `ch.reset()`),
///   which does not re-fire locked-controller events.
/// - `setReverbMacro`/`setChorusMacro`/`setDelayMacro`: now guarded by the corresponding
///   lock system parameter; an *invalid* macro number warns and returns instead of falling
///   back to macro 0 (Room1/Chorus1/Delay1); fires `effectChange`.
/// - `resetInsertion`: sendLevelToReverb now scaled by `EFX_SENDS_GAIN_CORRECTION`;
///   parameter cache grown 20 → 23 entries (params + 3 sends at indices 20/21/22) with the
///   new `resetInsertionParams()` helper; fires `effectChange`.
/// - `getInsertionSnapshot`: `{type, params, channels}` — the separate send-level fields are
///   gone (folded into params[20..23]).
/// - `setMIDIVolume`/`setMasterTuning` were removed upstream (replaced by the
///   `midiParameters.gain`/`fineTune` flow applied in the channels' `updateInternalParams`).
///   TODO(Task 21): they are kept here as legacy plumbing called by
///   `set_midi_parameter` and the (pre-4.3.0) SysEx handlers, because the current render
///   path still consumes `midi_volume`/the MASTER_TUNING custom controller. In particular
///   the 4.2.0 GM2 `volume^E` curve is still applied — TS 4.3.0 applies the gain linearly.
/// - `processMessage` lost its `force` parameter (and the force-kill Note Off branch).
/// - `createMIDIChannel`: fires `ChannelAdded` (was `newChannel` + `sendChannelProperty`).
///   Pre-existing phase-1 divergence kept: the TS constructor-side `channel.setDrums(true)`
///   for event-sending channels is not called (the Rust port sets `drum_channel` by
///   `channel % 16 == 9` instead) — revisit in Task 21.
/// - The per-channel `updateVoiceCount()` loop at the end of `process()` was removed
///   upstream (channel voice-count events move to the channel side) — removed here too.
/// - `process()` no longer skips voices on muted channels in TS 4.3.0 (muting became part of
///   the channel gain computed in `updateInternalParams`). TODO(Task 21): the `is_muted`
///   skip is KEPT here, because the current channel code has no gain-based muting yet —
///   removing it would audibly un-mute muted channels.
/// - TODO(Task 21-23, not ported — channel/voice/effects internals): `voiceBuffer` (shared
///   per-voice render buffer), `ch.midiParameters.rxChannel` (currently `rx_channel` is not
///   implemented in the Rust channel; `customChannelNumbers` dispatch is likewise not
///   implemented), `ch.setMIDIParameter("pressure", ...)`, `ch.destroy()`, per-channel
///   `systemParameters.presetLock` juggling in the preset-list refresh, and effect processor
///   constructors taking `maxBufferSize`.
use std::collections::HashMap;

use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
use crate::synthesizer::audio_engine::channel::midi_channel::MidiChannel;
use crate::synthesizer::audio_engine::effects::chorus::SpessaSynthChorus;
use crate::synthesizer::audio_engine::effects::delay::SpessaSynthDelay;
use crate::synthesizer::audio_engine::effects::insertion::{
    self, InsertionProcessor,
    thru::ThruFx,
};
use crate::synthesizer::audio_engine::effects::reverb::SpessaSynthReverb;
use crate::synthesizer::audio_engine::voice::lowpass_filter::LowpassFilter;
use crate::synthesizer::audio_engine::key_modifier_manager::KeyModifierManager;
use crate::synthesizer::audio_engine::parameters::midi::{
    DEFAULT_GLOBAL_MIDI_PARAMETERS, GlobalMIDIParameter,
};
use crate::synthesizer::audio_engine::parameters::system::{
    DEFAULT_GLOBAL_SYSTEM_PARAMETERS, GlobalSystemParameter,
};
use crate::synthesizer::audio_engine::sound_bank_manager::SoundBankManager;
use crate::synthesizer::audio_engine::synth_constants::{
    DEFAULT_PERCUSSION, EFX_SENDS_GAIN_CORRECTION,
};
use crate::synthesizer::audio_engine::voice::voice::Voice;
use crate::synthesizer::enums::custom_controllers;
use crate::synthesizer::types::{
    CachedVoiceList, EffectChangeCallback, EffectKind, SynthProcessorEvent, SynthProcessorOptions,
};
use crate::soundbank::types::MIDISystem;
use crate::utils::loggin::SpessaLog;

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
/// Equivalent to: { message, channelOffset, time } (the TS eventQueue entry)
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

    /// The maximum allowed buffer size to render.
    /// Equivalent to: maxBufferSize (new in TS 4.3.0)
    pub max_buffer_size: usize,

    /// MIDI Tuning Standard table: tunings[program * 128 + key] = note.cents
    /// -1.0 means no change.
    /// Equivalent to: tunings: Float32Array(128 * 128).fill(-1)
    pub tunings: Vec<f32>,

    /// The global MIDI parameters of the synthesizer.
    /// Equivalent to: midiParameters (new in TS 4.3.0)
    pub midi_parameters: GlobalMIDIParameter,

    /// The system parameters of the synthesizer.
    /// Equivalent to: systemParameters (new in TS 4.3.0)
    pub system_parameters: GlobalSystemParameter,

    /// Current synthesizer time in seconds.
    /// Equivalent to: currentTime
    pub current_time: f64,

    /// Overall MIDI volume gain (set by SysEx master volume, GM2 `volume^E` curve).
    /// Legacy 4.2.0 plumbing — removed upstream in 4.3.0; see module doc TODO(Task 21).
    pub midi_volume: f64,

    /// Reverb effect processor.
    pub reverb_processor: SpessaSynthReverb,

    /// Chorus effect processor.
    pub chorus_processor: SpessaSynthChorus,

    /// Delay effect processor.
    pub delay_processor: SpessaSynthDelay,

    /// Whether delay effect is active (enabled via SysEx).
    pub delay_active: bool,

    /// Mono reverb input buffer (fixed at max_buffer_size, cleared each render call).
    reverb_input: Vec<f32>,

    /// Mono chorus input buffer (fixed at max_buffer_size, cleared each render call).
    chorus_input: Vec<f32>,

    /// Mono delay input buffer (fixed at max_buffer_size, cleared each render call).
    delay_input: Vec<f32>,

    /// The pan of the left channel (0.0–1.0), derived from
    /// `system_parameters.pan + midi_parameters.pan`.
    /// Legacy 4.2.0 plumbing — removed upstream in 4.3.0; see module doc TODO(Task 21).
    pub pan_left: f64,

    /// The pan of the right channel (0.0–1.0).
    /// Legacy 4.2.0 plumbing — removed upstream in 4.3.0; see module doc TODO(Task 21).
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

    /// Total active voice count. Private in TS 4.3.0 (`_voiceCount` + getter).
    pub(crate) voice_count: u32,

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

    /// Stereo insertion input buffers (fixed at max_buffer_size).
    insertion_input_l: Vec<f32>,
    insertion_input_r: Vec<f32>,

    /// For insertion snapshot tracking.
    /// 20 parameters (0-19) + 3 sends (indices 20-22).
    /// Index to GS is Addr3 - 3 (for example EFX PARAMETER 1 is 0x03 and here it's 0).
    /// Note: 255 means "no change".
    /// Equivalent to: insertionParams: Uint8Array(23) (was 20 in 4.2.0)
    pub insertion_params: [u8; 23],
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

        let buf_size = options.max_buffer_size;

        // Initialize voice pool
        let voice_cap = DEFAULT_GLOBAL_SYSTEM_PARAMETERS.voice_cap as usize;
        let mut voices = Vec::with_capacity(voice_cap);
        for _ in 0..voice_cap {
            voices.push(Voice::new(sample_rate));
        }

        let tunings = vec![-1.0f32; 128 * 128];

        // TS sets effectsEnabled/eventsEnabled via setSystemParameter in the constructor
        // (which early-returns for the default values); the Rust struct-literal form below
        // is equivalent since no channels exist yet.
        let mut system_parameters = DEFAULT_GLOBAL_SYSTEM_PARAMETERS;
        system_parameters.effects_enabled = options.effects_enabled;
        system_parameters.events_enabled = options.events_enabled;

        let mut core = Self {
            voices,
            midi_channels: Vec::new(),
            sound_bank_manager: SoundBankManager::new(|| {}),
            key_modifier_manager: KeyModifierManager::new(),
            sample_rate,
            max_buffer_size: buf_size,
            tunings,
            midi_parameters: DEFAULT_GLOBAL_MIDI_PARAMETERS,
            system_parameters,
            current_time: options.initial_time,
            midi_volume: 1.0,
            reverb_processor: SpessaSynthReverb::new(sample_rate, buf_size),
            chorus_processor: SpessaSynthChorus::new(sample_rate, buf_size),
            delay_processor: SpessaSynthDelay::new(sample_rate, buf_size),
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
            insertion_params: [255u8; 23],
        };
        core.reset_insertion_params(); // Initial setup
        core
    }

    /// Current total amount of voices that are playing.
    /// Equivalent to: get voiceCount()
    pub fn voice_count(&self) -> u32 {
        self.voice_count
    }

    /// Dispatches an event through the event callback.
    /// Equivalent to: callEvent(eventName, eventData)
    pub fn call_event(&self, event: SynthProcessorEvent) {
        if self.system_parameters.events_enabled {
            (self.event_callback)(event);
        }
    }

    /// Recomputes the legacy shared stereo pan (`pan_left`/`pan_right`) from the sum of the
    /// system and MIDI global pans (TS 4.3.0 adds them per channel in updateInternalParams;
    /// legacy plumbing until Task 21 — see module doc).
    pub(crate) fn update_legacy_pan(&mut self) {
        let pan = self.system_parameters.pan + self.midi_parameters.pan;
        // Convert from [-1, 1] to [0, 1] where 0 = left
        let p = pan / 2.0 + 0.5;
        self.pan_left = 1.0 - p;
        self.pan_right = p;
    }

    /// Assigns the first available (inactive) voice, allocating a new one
    /// (autoAllocateVoices) or stealing the lowest-priority one if none is free.
    /// Equivalent to: assignVoice()
    pub fn assign_voice(&mut self) -> &mut Voice {
        let idx = self.assign_voice_idx();
        &mut self.voices[idx]
    }

    /// Like `assign_voice()` but returns the voice index instead of a reference.
    /// Used by `note_on` to allow simultaneous borrows of `voices` and `midi_channels`.
    pub(crate) fn assign_voice_idx(&mut self) -> usize {
        let voice_cap = self.system_parameters.voice_cap as usize;
        for i in 0..voice_cap {
            if !self.voices[i].is_active {
                // Prevent this voice from being stolen
                self.voices[i].priority = i32::MAX;
                return i;
            }
        }
        // No match, assign priorities
        if self.system_parameters.auto_allocate_voices {
            // Allocate a new voice and return it (see module doc note on the TS 4.3.0
            // duplicate-push quirk here).
            self.allocate_new_voices(1);
            self.system_parameters.voice_cap += 1;
            SpessaLog::info("Allocating a new voice!");
            return self.voices.len() - 1;
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
        SpessaLog::info("Stop all received!");
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
        // (Pre-existing phase-1 divergence from TS's `channel.setDrums(true)` — see module doc.)
        if channel_number % 16 == DEFAULT_PERCUSSION {
            channel.drum_channel = true;
        }

        self.midi_channels.push(channel);

        if send_event {
            self.call_event(SynthProcessorEvent::ChannelAdded);
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
            .get_preset_and_bank_idx(patch, MIDISystem::Xg)
        {
            (Some(preset.clone()), Some(bank_idx))
        } else {
            (None, None)
        }
    }

    /// Executes a full system reset of the synthesizer.
    /// This will reset all controllers to their default values,
    /// except for the locked controllers.
    /// Equivalent to: reset(system = DEFAULT_SYNTH_MODE) (renamed from resetAllControllers)
    pub fn reset(&mut self, system: MIDISystem) {
        // Call here because there are returns in this function.
        self.call_event(SynthProcessorEvent::Reset(system));
        self.reset_midi_parameters(system);
        // Reset private props
        self.tunings.fill(-1.0);
        // TODO(Task 21): portSelectChannelOffset = 0; customChannelNumbers = false;
        // (neither is implemented in the Rust core yet — see module doc)
        // Hall2 default
        self.set_reverb_macro(4);
        // Chorus3 default
        self.set_chorus_macro(2);
        // Delay1 default
        self.set_delay_macro(0);
        if !self.system_parameters.delay_lock {
            self.delay_active = false;
        }
        self.reset_insertion();

        let events_enabled = self.system_parameters.events_enabled;
        let current_time = self.current_time;
        let mut events = Vec::new();

        // Reset channels
        // Do not send CC changes as we call reset
        // TODO(Task 21): TS 4.3.0 calls ch.reset(false) — the legacy
        // reset_controllers/reset_preset pair is kept until the channel restructuring.
        for ch_idx in 0..self.midi_channels.len() {
            let mut sub = self.midi_channels[ch_idx].reset_controllers(
                false, // do not send CC events
                &mut self.voices,
                current_time,
                system,
                events_enabled,
            );
            events.append(&mut sub);

            let mut sub = self.midi_channels[ch_idx].reset_preset(
                &self.sound_bank_manager,
                system,
                events_enabled,
            );
            events.append(&mut sub);
        }

        for event in events {
            self.call_event(event);
        }
    }

    /// Renders audio for the current quantum.
    /// Equivalent to: process(outputs, effectsLeft, effectsRight, startIndex, samples)
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

        // TS 4.3.0: throw if the requested quantum exceeds the fixed buffer size.
        assert!(
            quantum_size <= self.max_buffer_size,
            "Requested {} samples, but maxBufferSize is {}",
            quantum_size,
            self.max_buffer_size
        );

        // Clear the buffers (fixed size — the 4.2.0 grow-on-demand path is gone).
        self.reverb_input.fill(0.0);
        self.chorus_input.fill(0.0);
        if self.delay_active {
            self.delay_input.fill(0.0);
        }
        if self.insertion_active {
            self.insertion_input_l.fill(0.0);
            self.insertion_input_r.fill(0.0);
        }

        // Clear voice counts.
        for ch in self.midi_channels.iter_mut() {
            ch.clear_voice_count();
        }
        self.voice_count = 0;

        let effects_enabled = self.system_parameters.effects_enabled;
        let master_gain = self.system_parameters.gain;
        let reverb_gain = self.system_parameters.reverb_gain;
        let chorus_gain = self.system_parameters.chorus_gain;
        let delay_gain = self.system_parameters.delay_gain;
        let midi_volume = self.midi_volume;
        // 4.3.0: global master pan normalized to [-1, 1] (globalSystem.pan + globalMIDI.pan),
        // folded additively into the per-voice pan index inside render_voice.
        let global_pan = self.system_parameters.pan + self.midi_parameters.pan;
        let pan_smoothing_factor = self.pan_smoothing_factor;
        let current_time = self.current_time;
        let delay_active = self.delay_active;
        let insertion_active = self.insertion_active;
        let out_len = outputs[0].len();

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

        // Process voices (up to the cap, matching TS).
        let cap = (self.system_parameters.voice_cap as usize).min(self.voices.len());
        for v_idx in 0..cap {
            if !self.voices[v_idx].is_active {
                continue;
            }
            let ch_idx = self.voices[v_idx].channel as usize;
            // TODO(Task 21): TS 4.3.0 removed this skip (muting moved into the channel gain);
            // kept because the current channel code has no gain-based muting yet.
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
                self.sample_rate,
                master_gain,
                reverb_gain,
                chorus_gain,
                delay_gain,
                midi_volume,
                global_pan,
                effects_enabled,
                delay_active,
                pan_smoothing_factor,
                &self.tunings,
                ins_l_slice,
                ins_r_slice,
                insertion_active,
            );

        }

        // Process effect chain: Insertion → Chorus → Delay → Reverb
        if effects_enabled {
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

            // CC#94 in XG is variation, not delay
            if delay_active && self.midi_parameters.system != MIDISystem::Xg {
                // Process delay
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

        // (TS 4.3.0 removed the per-channel updateVoiceCount() event loop here.)

        // Advance the time appropriately
        self.current_time += quantum_size as f64 * self.sample_time;
    }

    /// Sets the reverb macro (SC-8850 manual page 81).
    /// Equivalent to: setReverbMacro(macro)
    pub fn set_reverb_macro(&mut self, macro_num: u8) {
        if self.system_parameters.reverb_lock {
            return;
        }
        let rev = &mut self.reverb_processor;
        rev.set_level(64);
        rev.set_pre_delay_time(0);
        rev.set_character(macro_num);
        match macro_num {
            0 => {
                // Room1
                rev.set_character(0);
                rev.set_pre_lowpass(3);
                rev.set_time(80);
                rev.set_delay_feedback(0);
                rev.set_pre_delay_time(0);
            }
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
                // Check for invalid macros (TS 4.3.0: warn + return instead of Room1 fallback)
                // Testcase: 18 - Dichromatic Lotus Butterfly ~ Ancients (ZUN).mid
                SpessaLog::warn(&format!("Invalid reverb macro: {}", macro_num));
                return;
            }
        }
        self.call_event(SynthProcessorEvent::EffectChange(EffectChangeCallback {
            effect: EffectKind::Reverb,
            parameter: 0,
            value: macro_num as i32,
        }));
    }

    /// Sets the chorus macro (SC-8850 manual page 83).
    /// Equivalent to: setChorusMacro(macro)
    pub fn set_chorus_macro(&mut self, macro_num: u8) {
        if self.system_parameters.chorus_lock {
            return;
        }
        let chr = &mut self.chorus_processor;
        chr.set_level(64);
        chr.set_pre_lowpass(0);
        chr.set_delay(127);
        chr.set_send_level_to_delay(0);
        chr.set_send_level_to_reverb(0);
        match macro_num {
            0 => {
                // Chorus1
                chr.set_feedback(0);
                chr.set_delay(112);
                chr.set_rate(3);
                chr.set_depth(5);
            }
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
                // Check for invalid macros (TS 4.3.0: warn + return instead of Chorus1 fallback)
                // Testcase: 18 - Dichromatic Lotus Butterfly ~ Ancients (ZUN).mid
                SpessaLog::warn(&format!("Invalid chorus macro: {}", macro_num));
                return;
            }
        }
        self.call_event(SynthProcessorEvent::EffectChange(EffectChangeCallback {
            effect: EffectKind::Chorus,
            parameter: 0,
            value: macro_num as i32,
        }));
    }

    /// Sets the delay macro (SC-8850 manual page 85).
    /// Equivalent to: setDelayMacro(macro)
    pub fn set_delay_macro(&mut self, macro_num: u8) {
        if self.system_parameters.delay_lock {
            return;
        }
        let dly = &mut self.delay_processor;
        dly.set_level(64);
        dly.set_pre_lowpass(0);
        dly.set_send_level_to_reverb(0);
        dly.set_level_right(0);
        dly.set_level_left(0);
        dly.set_level_center(127);
        match macro_num {
            0 => {
                // Delay1
                dly.set_time_center(97);
                dly.set_time_ratio_left(1);
                dly.set_time_ratio_right(1);
                dly.set_feedback(80);
            }
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
                // Check for invalid macros (TS 4.3.0: warn + return instead of Delay1 fallback)
                // Testcase: 18 - Dichromatic Lotus Butterfly ~ Ancients (ZUN).mid
                SpessaLog::warn(&format!("Invalid delay macro: {}", macro_num));
                return;
            }
        }
        self.call_event(SynthProcessorEvent::EffectChange(EffectChangeCallback {
            effect: EffectKind::Delay,
            parameter: 0,
            value: macro_num as i32,
        }));
    }

    /// Resets the insertion parameter cache to "no change" + the default sends.
    /// Equivalent to: resetInsertionParams() (protected, new in TS 4.3.0)
    pub(crate) fn reset_insertion_params(&mut self) {
        // No change
        self.insertion_params.fill(255);
        self.insertion_params[20] = 40; // Reverb
        self.insertion_params[21] = 0; // Chorus
        self.insertion_params[22] = 0; // Delay
    }

    /// Resets the insertion effect to defaults.
    /// Equivalent to: resetInsertion()
    pub fn reset_insertion(&mut self) {
        if self.system_parameters.insertion_effect_lock {
            return;
        }
        self.insertion_active = false;
        self.insertion_processor = Box::new(ThruFx::new(self.sample_rate));
        self.insertion_processor.reset();
        self.insertion_processor
            .set_send_level_to_reverb(40.0 / 127.0 * EFX_SENDS_GAIN_CORRECTION);
        self.insertion_processor.set_send_level_to_chorus(0.0);
        self.insertion_processor.set_send_level_to_delay(0.0);
        self.reset_insertion_params();
        // Legacy compensation (not in TS, where ch.reset handles efxAssign — Task 21):
        for ch in self.midi_channels.iter_mut() {
            ch.insertion_enabled = false;
        }
        let efx_type = self.insertion_processor.effect_type();
        self.call_event(SynthProcessorEvent::EffectChange(EffectChangeCallback {
            effect: EffectKind::Insertion,
            parameter: 0,
            value: efx_type as i32,
        }));
    }

    /// Returns the insertion effect snapshot.
    /// Equivalent to: getInsertionSnapshot() (protected in TS 4.3.0; the send-level fields
    /// were folded into params[20..23])
    pub fn get_insertion_snapshot(
        &self,
    ) -> crate::synthesizer::audio_engine::synthesizer_snapshot::InsertionProcessorSnapshot {
        crate::synthesizer::audio_engine::synthesizer_snapshot::InsertionProcessorSnapshot {
            efx_type: self.insertion_processor.effect_type(),
            params: self.insertion_params,
            channels: self
                .midi_channels
                .iter()
                .map(|c| c.insertion_enabled)
                .collect(),
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
                .get_preset_and_bank_idx(patch, self.midi_parameters.system)
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
                    SpessaLog::warn(&format!(
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
                    SpessaLog::warn(&format!(
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

    /// Sets the global MIDI gain (applied linearly, matching 4.3.0).
    ///
    /// 4.2.0 raised the master volume to `e` (GM2 §4.1 squared-ish curve) in
    /// `setMIDIVolume`. 4.3.0 removed that curve: `midiParameters.gain` is applied
    /// linearly in the channels. Keep this as the shared `midi_volume` plumbing but
    /// store the value linearly so it equals `midi_parameters.gain`.
    /// Equivalent to: setMIDIParameter("gain", value) (4.3.0)
    pub fn set_midi_volume(&mut self, volume: f64) {
        self.midi_volume = volume;
    }

    /// Sets the master tuning for all channels.
    /// Legacy 4.2.0 plumbing — removed upstream in 4.3.0 (replaced by
    /// `midiParameters.fineTune`); see module doc TODO(Task 21).
    /// Equivalent to: setMasterTuning(cents) (4.2.0, protected)
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
        // TODO(Task 21): TS 4.3.0 calls c.destroy() per channel.
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
    /// Equivalent to: event queue processing in process()
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
        SpessaLog::info("Polyphony exceeded, stealing voices");
        self.last_priority_assignment_time = self.current_time;
        let cap = (self.system_parameters.voice_cap as usize).min(self.voices.len());
        for voice in self.voices.iter_mut().take(cap) {
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
        // TS sorts the queue by time after each push.
        self.event_queue
            .sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
    }

    /// Registers a factory-constructed insertion processor (used by tests and the snapshot
    /// SysEx path via the insertion module's `create_insertion_processor`).
    #[allow(dead_code)]
    pub(crate) fn set_insertion_processor_by_type(&mut self, efx_type: u16) {
        if let Some(proc) = insertion::create_insertion_processor(efx_type, self.sample_rate, self.max_buffer_size) {
            self.insertion_processor = proc;
        } else {
            self.insertion_processor = Box::new(ThruFx::new(self.sample_rate));
        }
    }
}
