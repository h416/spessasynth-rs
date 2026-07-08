/// processor.rs
/// purpose: SpessaSynthProcessor - the main public API wrapping SynthesizerCore.
/// Ported from: src/synthesizer/processor.ts (spessasynth_core 4.3.0)
///
/// # Design note
/// TypeScript's SpessaSynthProcessor wraps SynthesizerCore and adds:
///   - savedSnapshot: re-applied after embedded sound bank changes
///   - onEventCall / onMissingPreset: callback fields
///
/// In Rust, the event callback is owned by SynthesizerCore directly.
/// onMissingPreset is omitted (SynthesizerCore does not call it in the Rust port).
///
/// Additional impl SynthesizerCore blocks are defined here for channel-level
/// wrapper methods and MIDI message dispatch (process_message), since those
/// belong to synthesizer_core.ts but require access to the synthesizer state
/// that is most naturally expressed as SynthesizerCore methods.
///
/// # Changes from 4.2.0 (reviewed against the 4.3.0 diff)
/// - `currentSynthTime` → `currentTime` (pure rename; same `synthCore.currentTime`).
/// - `totalVoicesAmount` → `voiceCount`.
/// - `resetAllControllers(system = DEFAULT_SYNTH_MODE)` → `reset()` (no argument; the core's
///   `reset(system)` keeps the parameter).
/// - `clearEmbeddedBank` → `clearEmbeddedSoundBank`.
/// - `applySynthesizerSnapshot` → `applySnapshot` (now via the free-standing
///   `applySnapshot`/`getSynthesizerSnapshot` core functions; no post-apply "Finished
///   applying snapshot!" log).
/// - `setMasterParameter`/`getMasterParameter`/`getAllMasterParameters` →
///   `setSystemParameter` + read-only `midiParameters`/`systemParameters` getters
///   (replacing the `enableEffects`/`enableEventSystem` get/set pairs).
/// - `getInsertionSnapshot` public wrapper removed (became protected on the core).
/// - `processMessage` lost its `force` parameter.
/// - `killVoices` (deprecated no-op) removed upstream; never existed in Rust.
/// - The constructor creates 16 channels via a literal (MIDI_CHANNEL_COUNT was removed).
use crate::midi::enums::midi_message_types;
use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
use crate::soundbank::sound_bank_loader::load_sound_bank;
use crate::synthesizer::audio_engine::parameters::midi::GlobalMIDIParameter;
use crate::synthesizer::audio_engine::parameters::system::{
    GlobalSystemParameter, GlobalSystemParameterChange,
};
use crate::synthesizer::audio_engine::synth_constants::{
    DEFAULT_SYNTH_MODE, embedded_sound_bank_id,
};
use crate::synthesizer::audio_engine::synthesizer_snapshot::{
    apply_snapshot, get_synthesizer_snapshot, SynthesizerSnapshot,
};
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::types::{
    CachedVoiceList, SynthMethodOptions, SynthProcessorEvent, SynthProcessorOptions,
};
use crate::utils::loggin::SpessaLog;

// ---------------------------------------------------------------------------
// Additional SynthesizerCore methods — channel-level wrappers
// ---------------------------------------------------------------------------

impl SynthesizerCore {
    /// Sends note-off to a channel, dispatching events.
    /// Equivalent to: midiChannels[channel].noteOff(midiNote) (SpessaSynthProcessor context)
    pub fn note_off_channel(&mut self, channel: usize, midi_note: u8) {
        let current_time = self.current_time;
        let black_midi_mode = self.system_parameters.black_midi_mode;
        let voices = &mut self.voices;
        let events =
            self.midi_channels[channel].note_off(midi_note, voices, current_time, black_midi_mode);
        for ev in events {
            self.call_event(ev);
        }
    }

    /// Sends a controller change to a channel, dispatching events.
    /// Equivalent to: midiChannels[channel].controllerChange(controller, value)
    pub fn controller_change_channel(&mut self, channel: usize, controller: u8, value: u8) {
        let current_time = self.current_time;
        let current_system = self.midi_parameters.system;
        let events_enabled = self.system_parameters.events_enabled;
        let voices = &mut self.voices;
        let events = self.midi_channels[channel].controller_change(
            controller,
            value,
            voices,
            current_time,
            current_system,
            events_enabled,
        );
        for ev in events {
            self.call_event(ev);
        }
    }

    /// Sends a program change to a channel, dispatching events.
    /// Equivalent to: midiChannels[channel].programChange(program)
    pub fn program_change_channel(&mut self, channel: usize, program: u8) {
        let current_system = self.midi_parameters.system;
        let events_enabled = self.system_parameters.events_enabled;
        let events = self.midi_channels[channel].program_change(
            program,
            &self.sound_bank_manager,
            current_system,
            events_enabled,
        );
        for ev in events {
            self.call_event(ev);
        }
    }

    /// Sends a pitch wheel message to a channel, dispatching events.
    /// Equivalent to: midiChannels[channel].pitchWheel(pitch, midiNote)
    pub fn pitch_wheel_channel(&mut self, channel: usize, pitch: i16, midi_note: i32) {
        let events_enabled = self.system_parameters.events_enabled;
        let voices = &mut self.voices;
        let events =
            self.midi_channels[channel].pitch_wheel(voices, pitch, midi_note, events_enabled);
        for ev in events {
            self.call_event(ev);
        }
    }

    /// Sends a channel pressure message to a channel, dispatching events.
    /// Equivalent to: midiChannels[channel].setMIDIParameter("pressure", pressure)
    /// (TODO(Task 21): the channel-side setMIDIParameter does not exist yet; the legacy
    /// channelPressure channel method is used.)
    pub fn channel_pressure_channel(&mut self, channel: usize, pressure: u8) {
        let voices = &mut self.voices;
        let events = self.midi_channels[channel].channel_pressure(voices, pressure);
        for ev in events {
            self.call_event(ev);
        }
    }

    /// Sends a poly pressure message to a channel, dispatching events.
    /// Equivalent to: midiChannels[channel].polyPressure(midiNote, pressure)
    pub fn poly_pressure_channel(&mut self, channel: usize, midi_note: u8, pressure: u8) {
        let voices = &mut self.voices;
        let events = self.midi_channels[channel].poly_pressure(voices, midi_note, pressure);
        for ev in events {
            self.call_event(ev);
        }
    }

    /// Executes a system exclusive message.
    /// Dispatches to the appropriate handler based on the manufacturer byte.
    /// Equivalent to: systemExclusiveInternal(syx, channelOffset)
    pub fn system_exclusive(&mut self, syx: &[u8], channel_offset: usize) {
        use crate::utils::other::array_to_hex_string;

        // Ensure that the device ID matches (-1 accepts all)
        if self.system_parameters.device_id != -1
            && syx[1] != 0x7f // 0x7f means broadcast
            && self.system_parameters.device_id != syx[1] as i32
        {
            return;
        }

        let manufacturer = syx[0];
        match manufacturer {
            // Non-realtime GM / Realtime GM
            0x7e | 0x7f => {
                self.handle_gm(syx, channel_offset);
            }
            // Roland GS
            0x41 => {
                self.handle_gs(syx, channel_offset);
            }
            // Yamaha XG
            0x43 => {
                self.handle_xg(syx, channel_offset);
            }
            _ => {
                SpessaLog::info(&format!(
                    "Unrecognized SysEx: {} (unknown manufacturer)",
                    array_to_hex_string(syx)
                ));
            }
        }
    }

    /// Processes a raw MIDI message.
    /// If options.time > current_time, the dispatch is scheduled for later;
    /// otherwise it executes immediately.
    /// Equivalent to: processMessage(message, channelOffset, options) in synthesizer_core.ts
    /// (the 4.2.0 `force` parameter was removed in TS 4.3.0)
    pub fn process_message(
        &mut self,
        message: &[u8],
        channel_offset: usize,
        options: SynthMethodOptions,
    ) {
        let time = options.time;
        if time > self.current_time {
            let msg = message.to_vec();
            self.schedule_event(
                move |core| core.dispatch_message_internal(&msg, channel_offset),
                time,
            );
        } else {
            let msg = message.to_vec();
            self.dispatch_message_internal(&msg, channel_offset);
        }
    }

    /// Dispatches the actual MIDI event bytes.
    /// Equivalent to: processMessageInternal(message, channelOffset) (private)
    fn dispatch_message_internal(&mut self, message: &[u8], channel_offset: usize) {
        if message.is_empty() {
            return;
        }
        let status_byte = message[0];
        let status = status_byte & 0xF0;

        if (0x80..=0xE0).contains(&status) {
            // Channel message
            let channel = (status_byte & 0x0F) as usize + channel_offset;
            if channel >= self.midi_channels.len() {
                return;
            }
            match status {
                midi_message_types::NOTE_ON => {
                    if message.len() < 3 {
                        return;
                    }
                    let velocity = message[2];
                    if velocity > 0 {
                        self.note_on(channel, message[1], velocity);
                    } else {
                        self.note_off_channel(channel, message[1]);
                    }
                }
                midi_message_types::NOTE_OFF => {
                    if message.len() < 2 {
                        return;
                    }
                    self.note_off_channel(channel, message[1]);
                }
                midi_message_types::PITCH_WHEEL => {
                    if message.len() < 3 {
                        return;
                    }
                    // pitch = LSB | (MSB << 7)
                    let pitch = ((message[2] as i16) << 7) | message[1] as i16;
                    self.pitch_wheel_channel(channel, pitch, -1);
                }
                midi_message_types::CONTROLLER_CHANGE => {
                    if message.len() < 3 {
                        return;
                    }
                    self.controller_change_channel(channel, message[1], message[2]);
                }
                midi_message_types::PROGRAM_CHANGE => {
                    if message.len() < 2 {
                        return;
                    }
                    self.program_change_channel(channel, message[1]);
                }
                midi_message_types::POLY_PRESSURE => {
                    if message.len() < 2 {
                        return;
                    }
                    // Note: original TypeScript uses message[0] (status byte) as midiNote,
                    // and message[1] as pressure — faithfully porting as-is.
                    self.poly_pressure_channel(channel, message[0], message[1]);
                }
                midi_message_types::CHANNEL_PRESSURE => {
                    if message.len() < 2 {
                        return;
                    }
                    self.channel_pressure_channel(channel, message[1]);
                }
                _ => {}
            }
        } else {
            // System message
            match status_byte {
                midi_message_types::SYSTEM_EXCLUSIVE => {
                    self.system_exclusive(message.get(1..).unwrap_or(&[]), channel_offset);
                }
                midi_message_types::RESET => {
                    // Do not **force** stop channels (breaks seamless loops, for example th06)
                    self.stop_all_channels(false);
                    self.reset(DEFAULT_SYNTH_MODE);
                }
                _ => {}
            }
        }
    }

    /// Renders per-channel audio.
    /// Stub: processSplit from synthesizer_core.ts is not yet ported.
    pub fn render_audio_split(
        &mut self,
        _reverb: &mut [Vec<f32>],
        _chorus: &mut [Vec<f32>],
        _separate: &mut Vec<Vec<Vec<f32>>>,
        _start_index: usize,
        _sample_count: usize,
    ) {
        // TODO: Port from synthesizer_core.ts processSplit
    }
}

// ---------------------------------------------------------------------------
// SpessaSynthProcessor
// ---------------------------------------------------------------------------

/// The main synthesizer processor, wrapping SynthesizerCore.
/// Equivalent to: class SpessaSynthProcessor
pub struct SpessaSynthProcessor {
    /// Core synthesis engine.
    /// Equivalent to: private readonly synthCore: SynthesizerCore
    pub synth_core: SynthesizerCore,

    /// Saved snapshot for re-applying after sound bank changes.
    /// Equivalent to: private savedSnapshot?: SynthesizerSnapshot
    saved_snapshot: Option<SynthesizerSnapshot>,

    /// Audio sample rate in Hz.
    /// Equivalent to: public readonly sampleRate: number
    pub sample_rate: f64,
}

impl SpessaSynthProcessor {
    /// Creates a new synthesizer processor.
    /// Equivalent to: constructor(sampleRate, opts)
    pub fn new(
        sample_rate: f64,
        event_callback: impl Fn(SynthProcessorEvent) + 'static,
        options: SynthProcessorOptions,
    ) -> Self {
        let mut core = SynthesizerCore::new(event_callback, sample_rate, options);
        for _ in 0..16 {
            // Don't send events as we're creating the initial channels
            core.create_midi_channel(false);
        }
        SpessaLog::info("SpessaSynth is ready!");
        Self {
            sample_rate,
            synth_core: core,
            saved_snapshot: None,
        }
    }

    // -----------------------------------------------------------------------
    // Properties (Rust getters for TypeScript get)
    // -----------------------------------------------------------------------

    /// The global MIDI parameters of the synthesizer.
    /// These are only editable via MIDI messages.
    /// Equivalent to: get midiParameters()
    pub fn midi_parameters(&self) -> &GlobalMIDIParameter {
        &self.synth_core.midi_parameters
    }

    /// The global system parameters of the synthesizer.
    /// These are only editable via the API.
    /// Equivalent to: get systemParameters()
    pub fn system_parameters(&self) -> &GlobalSystemParameter {
        &self.synth_core.system_parameters
    }

    /// Current total amount of voices that are currently playing.
    /// Equivalent to: get voiceCount() (renamed from totalVoicesAmount in TS 4.3.0)
    pub fn voice_count(&self) -> u32 {
        self.synth_core.voice_count()
    }

    /// The current time of the synthesizer, in seconds.
    /// Equivalent to: get currentTime() (renamed from currentSynthTime in TS 4.3.0)
    pub fn current_time(&self) -> f64 {
        self.synth_core.current_time
    }

    // -----------------------------------------------------------------------
    // System parameters
    // -----------------------------------------------------------------------

    /// Sets a system parameter of the synthesizer.
    /// Equivalent to: setSystemParameter(type, value)
    pub fn set_system_parameter(&mut self, change: GlobalSystemParameterChange) {
        self.synth_core.set_system_parameter(change);
    }

    // -----------------------------------------------------------------------
    // System control
    // -----------------------------------------------------------------------

    /// Executes a full synthesizer reset.
    /// This will reset all controllers to their default values,
    /// except for the locked controllers.
    /// Equivalent to: reset() (renamed from resetAllControllers in TS 4.3.0)
    pub fn reset(&mut self) {
        self.synth_core.reset(DEFAULT_SYNTH_MODE);
    }

    // -----------------------------------------------------------------------
    // Snapshot
    // -----------------------------------------------------------------------

    /// Applies the snapshot to this `SpessaSynthProcessor` instance.
    /// Equivalent to: applySnapshot(snapshot) (renamed from applySynthesizerSnapshot)
    pub fn apply_snapshot(&mut self, snapshot: SynthesizerSnapshot) {
        self.saved_snapshot = Some(snapshot.clone());
        apply_snapshot(&mut self.synth_core, &snapshot);
        self.reset();
    }

    /// Gets a synthesizer snapshot from this processor instance.
    /// Equivalent to: getSnapshot()
    pub fn get_snapshot(&self) -> SynthesizerSnapshot {
        get_synthesizer_snapshot(&self.synth_core)
    }

    // -----------------------------------------------------------------------
    // Sound bank management
    // -----------------------------------------------------------------------

    /// Sets the embedded sound bank (highest priority).
    /// Re-applies the saved snapshot after loading.
    /// Equivalent to: setEmbeddedSoundBank(bank, offset)
    pub fn set_embedded_sound_bank(&mut self, bank: Vec<u8>, offset: u8) {
        let loaded_font = load_sound_bank(bank);
        let id = embedded_sound_bank_id().to_string();
        self.synth_core
            .sound_bank_manager
            .add_sound_bank(loaded_font, id.clone(), offset);
        // Rearrange so the embedded bank is first (most important)
        let mut order = self.synth_core.sound_bank_manager.priority_order();
        order.retain(|x| x != &id);
        order.insert(0, id);
        self.synth_core.sound_bank_manager.set_priority_order(&order);
        // Re-apply snapshot if one was saved
        if let Some(snapshot) = self.saved_snapshot.clone() {
            self.apply_snapshot(snapshot);
        }
        SpessaLog::info(&format!("Embedded sound bank set at offset {}", offset));
    }

    /// Removes the embedded sound bank from the synthesizer.
    /// Equivalent to: clearEmbeddedSoundBank() (renamed from clearEmbeddedBank in TS 4.3.0)
    pub fn clear_embedded_sound_bank(&mut self) {
        let id = embedded_sound_bank_id();
        if self
            .synth_core
            .sound_bank_manager
            .sound_bank_list
            .iter()
            .any(|s| s.id == id)
        {
            self.synth_core.sound_bank_manager.delete_sound_bank(id);
        }
    }

    // -----------------------------------------------------------------------
    // Channel management
    // -----------------------------------------------------------------------

    /// Creates a new MIDI channel and adds it to the synthesizer.
    /// Equivalent to: createMIDIChannel()
    pub fn create_midi_channel(&mut self) {
        self.synth_core.create_midi_channel(true);
    }

    /// Stops all notes on all channels.
    /// Equivalent to: stopAllChannels(force = false)
    pub fn stop_all_channels(&mut self, force: bool) {
        self.synth_core.stop_all_channels(force);
    }

    /// Destroys the synthesizer, releasing all resources.
    /// Equivalent to: destroySynthProcessor()
    pub fn destroy_synth_processor(&mut self) {
        self.synth_core.destroy();
    }

    // -----------------------------------------------------------------------
    // MIDI event dispatchers
    // -----------------------------------------------------------------------

    /// Sends a MIDI controller change.
    /// Equivalent to: controllerChange(channel, controller, value)
    pub fn controller_change(&mut self, channel: usize, controller: u8, value: u8) {
        self.synth_core
            .controller_change_channel(channel, controller, value);
    }

    /// Sends a MIDI Note On message.
    /// Equivalent to: noteOn(channel, midiNote, velocity)
    pub fn note_on(&mut self, channel: usize, midi_note: u8, velocity: u8) {
        self.synth_core.note_on(channel, midi_note, velocity);
    }

    /// Sends a MIDI Note Off message.
    /// Equivalent to: noteOff(channel, midiNote)
    pub fn note_off(&mut self, channel: usize, midi_note: u8) {
        self.synth_core.note_off_channel(channel, midi_note);
    }

    /// Sends a MIDI Poly Pressure (aftertouch) message.
    /// Equivalent to: polyPressure(channel, midiNote, pressure)
    pub fn poly_pressure(&mut self, channel: usize, midi_note: u8, pressure: u8) {
        self.synth_core
            .poly_pressure_channel(channel, midi_note, pressure);
    }

    /// Sends a MIDI Channel Pressure (aftertouch) message.
    /// Equivalent to: channelPressure(channel, pressure)
    pub fn channel_pressure(&mut self, channel: usize, pressure: u8) {
        self.synth_core.channel_pressure_channel(channel, pressure);
    }

    /// Sends a MIDI Pitch Wheel message.
    /// pitch: 0–16383 (8192 = center); midi_note: -1 for the regular pitch wheel.
    /// Equivalent to: pitchWheel(channel, pitch, midiNote = -1)
    pub fn pitch_wheel(&mut self, channel: usize, pitch: i16, midi_note: i32) {
        self.synth_core
            .pitch_wheel_channel(channel, pitch, midi_note);
    }

    /// Sends a MIDI program change.
    /// Equivalent to: programChange(channel, programNumber)
    pub fn program_change(&mut self, channel: usize, program: u8) {
        self.synth_core.program_change_channel(channel, program);
    }

    /// Processes a raw MIDI message.
    /// Equivalent to: processMessage(message, channelOffset, options)
    /// (the 4.2.0 `force` parameter was removed in TS 4.3.0)
    pub fn process_message(
        &mut self,
        message: &[u8],
        channel_offset: usize,
        options: SynthMethodOptions,
    ) {
        self.synth_core
            .process_message(message, channel_offset, options);
    }

    /// Executes a system exclusive message.
    /// Equivalent to: systemExclusive(syx, channelOffset)
    pub fn system_exclusive(&mut self, syx: &[u8], channel_offset: usize) {
        self.synth_core.system_exclusive(syx, channel_offset);
    }

    /// Clears the voice cache.
    /// Equivalent to: clearCache()
    pub fn clear_cache(&mut self) {
        self.synth_core.clear_cache();
    }

    /// Gets voices for a preset.
    /// Equivalent to: getVoicesForPreset(preset, midiNote, velocity)
    pub fn get_voices_for_preset(
        &self,
        preset: &BasicPreset,
        bank: &BasicSoundBank,
        midi_note: u8,
        velocity: u8,
    ) -> CachedVoiceList {
        self.synth_core
            .get_voices_for_preset(preset, bank, midi_note, velocity)
    }

    /// Renders audio to stereo output buffers.
    /// Effects are now integrated — reverb/chorus/delay are processed internally.
    pub fn render_audio(
        &mut self,
        outputs: &mut [Vec<f32>],
        start_index: usize,
        sample_count: usize,
    ) {
        self.synth_core
            .render_audio(outputs, start_index, sample_count);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::types::MIDISystem;
    use crate::synthesizer::types::{SynthProcessorEvent, SynthProcessorOptions};
    use std::sync::{Arc, Mutex};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_processor() -> (SpessaSynthProcessor, Arc<Mutex<Vec<SynthProcessorEvent>>>) {
        let events: Arc<Mutex<Vec<SynthProcessorEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let ev_clone = Arc::clone(&events);
        let proc = SpessaSynthProcessor::new(
            44100.0,
            move |ev| {
                ev_clone.lock().unwrap().push(ev);
            },
            SynthProcessorOptions::default(),
        );
        (proc, events)
    }

    fn event_count(events: &Arc<Mutex<Vec<SynthProcessorEvent>>>) -> usize {
        events.lock().unwrap().len()
    }

    // -----------------------------------------------------------------------
    // new — constructor
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_creates_16_midi_channels() {
        let (proc, _) = make_processor();
        assert_eq!(proc.synth_core.midi_channels.len(), 16);
    }

    #[test]
    fn test_new_sample_rate_stored() {
        let (proc, _) = make_processor();
        assert!((proc.sample_rate - 44100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_new_channel_9_is_drum() {
        let (proc, _) = make_processor();
        assert!(proc.synth_core.midi_channels[9].drum_channel);
    }

    #[test]
    fn test_new_channel_0_is_not_drum() {
        let (proc, _) = make_processor();
        assert!(!proc.synth_core.midi_channels[0].drum_channel);
    }

    // -----------------------------------------------------------------------
    // system_parameters / midi_parameters getters
    // -----------------------------------------------------------------------

    #[test]
    fn test_effects_enabled_default_true() {
        let (proc, _) = make_processor();
        assert!(proc.system_parameters().effects_enabled);
    }

    #[test]
    fn test_set_effects_enabled_false() {
        let (mut proc, _) = make_processor();
        proc.set_system_parameter(GlobalSystemParameterChange::EffectsEnabled(false));
        assert!(!proc.system_parameters().effects_enabled);
    }

    #[test]
    fn test_events_enabled_default_true() {
        let (proc, _) = make_processor();
        assert!(proc.system_parameters().events_enabled);
    }

    #[test]
    fn test_set_events_enabled_false() {
        let (mut proc, _) = make_processor();
        proc.set_system_parameter(GlobalSystemParameterChange::EventsEnabled(false));
        assert!(!proc.system_parameters().events_enabled);
    }

    #[test]
    fn test_midi_parameters_default_system_gs() {
        let (proc, _) = make_processor();
        assert_eq!(proc.midi_parameters().system, MIDISystem::Gs);
    }

    // -----------------------------------------------------------------------
    // voice_count / current_time
    // -----------------------------------------------------------------------

    #[test]
    fn test_voice_count_initially_zero() {
        let (proc, _) = make_processor();
        assert_eq!(proc.voice_count(), 0);
    }

    #[test]
    fn test_current_time_initially_zero() {
        let (proc, _) = make_processor();
        assert!((proc.current_time() - 0.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // set_system_parameter
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_system_parameter_gain() {
        let (mut proc, _) = make_processor();
        proc.set_system_parameter(GlobalSystemParameterChange::Gain(0.5));
        assert!((proc.system_parameters().gain - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_set_system_parameter_voice_cap() {
        let (mut proc, _) = make_processor();
        proc.set_system_parameter(GlobalSystemParameterChange::VoiceCap(100));
        assert_eq!(proc.system_parameters().voice_cap, 100);
    }

    #[test]
    fn test_set_system_parameter_device_id() {
        let (mut proc, _) = make_processor();
        proc.set_system_parameter(GlobalSystemParameterChange::DeviceId(5));
        assert_eq!(proc.system_parameters().device_id, 5);
    }

    // -----------------------------------------------------------------------
    // reset
    // -----------------------------------------------------------------------

    #[test]
    fn test_reset_fires_event() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        proc.reset();
        // Reset event should be emitted
        let evs = events.lock().unwrap();
        assert!(
            evs.len() > before,
            "Expected at least one event after reset"
        );
        let has_reset = evs.iter().any(|e| {
            matches!(e, SynthProcessorEvent::Reset(_))
        });
        assert!(has_reset, "Expected Reset event");
    }

    // -----------------------------------------------------------------------
    // create_midi_channel
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_midi_channel_adds_channel() {
        let (mut proc, _) = make_processor();
        let before = proc.synth_core.midi_channels.len();
        proc.create_midi_channel();
        assert_eq!(proc.synth_core.midi_channels.len(), before + 1);
    }

    #[test]
    fn test_create_midi_channel_fires_channel_added_event() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        proc.create_midi_channel();
        let evs = events.lock().unwrap();
        assert!(evs.len() > before);
        let has_channel_added = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::ChannelAdded)
        });
        assert!(has_channel_added);
    }

    // -----------------------------------------------------------------------
    // stop_all_channels
    // -----------------------------------------------------------------------

    #[test]
    fn test_stop_all_channels_fires_stop_events() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        proc.stop_all_channels(false);
        let evs = events.lock().unwrap();
        // Should have emitted at least one StopAll per channel
        assert!(evs.len() > before);
    }

    // -----------------------------------------------------------------------
    // destroy_synth_processor
    // -----------------------------------------------------------------------

    #[test]
    fn test_destroy_clears_channels() {
        let (mut proc, _) = make_processor();
        proc.destroy_synth_processor();
        assert!(proc.synth_core.midi_channels.is_empty());
    }

    // -----------------------------------------------------------------------
    // note_on
    // -----------------------------------------------------------------------

    #[test]
    fn test_note_on_no_preset_does_not_panic() {
        // Without a sound bank loaded, note_on silently returns (no preset → no voice).
        // This test verifies the function handles missing preset gracefully.
        let (mut proc, _) = make_processor();
        proc.note_on(0, 60, 100); // should not panic
    }

    // -----------------------------------------------------------------------
    // note_off
    // -----------------------------------------------------------------------

    #[test]
    fn test_note_off_fires_note_off_event() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        proc.note_off(0, 60);
        let evs = events.lock().unwrap();
        let has_note_off = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::NoteOff(_))
        });
        assert!(has_note_off);
    }

    // -----------------------------------------------------------------------
    // pitch_wheel
    // -----------------------------------------------------------------------

    #[test]
    fn test_pitch_wheel_fires_event() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        proc.pitch_wheel(0, 8192, -1);
        let evs = events.lock().unwrap();
        let has_pw = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::PitchWheel(_))
        });
        assert!(has_pw);
    }

    // -----------------------------------------------------------------------
    // channel_pressure
    // -----------------------------------------------------------------------

    #[test]
    fn test_channel_pressure_fires_event() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        proc.channel_pressure(0, 64);
        let evs = events.lock().unwrap();
        let has_cp = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::ChannelPressure(_))
        });
        assert!(has_cp);
    }

    // -----------------------------------------------------------------------
    // poly_pressure
    // -----------------------------------------------------------------------

    #[test]
    fn test_poly_pressure_fires_event() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        proc.poly_pressure(0, 60, 64);
        let evs = events.lock().unwrap();
        let has_pp = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::PolyPressure(_))
        });
        assert!(has_pp);
    }

    // -----------------------------------------------------------------------
    // get_snapshot / apply_snapshot round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_snapshot_captures_channel_count() {
        let (proc, _) = make_processor();
        let snap = proc.get_snapshot();
        assert_eq!(
            snap.midi_channels.len(),
            proc.synth_core.midi_channels.len()
        );
    }

    #[test]
    fn test_apply_snapshot_saves_snapshot_internally() {
        let (mut proc, _) = make_processor();
        let snap = proc.get_snapshot();
        proc.apply_snapshot(snap);
        assert!(proc.saved_snapshot.is_some());
    }

    // -----------------------------------------------------------------------
    // process_message — immediate dispatch
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_message_note_on_no_preset_does_not_panic() {
        // Without a sound bank, note_on silently returns; no panic expected.
        let (mut proc, _) = make_processor();
        proc.process_message(&[0x90, 60, 100], 0, SynthMethodOptions::default());
    }

    #[test]
    fn test_process_message_note_on_velocity_zero_is_note_off() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        // Note-on with velocity 0 → note-off
        proc.process_message(&[0x90, 60, 0], 0, SynthMethodOptions::default());
        let evs = events.lock().unwrap();
        let has_note_off = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::NoteOff(_))
        });
        assert!(has_note_off);
    }

    #[test]
    fn test_process_message_note_off_dispatches() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        // Note-off: status 0x80 (ch 0), note 60
        proc.process_message(&[0x80, 60, 0], 0, SynthMethodOptions::default());
        let evs = events.lock().unwrap();
        let has_note_off = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::NoteOff(_))
        });
        assert!(has_note_off);
    }

    #[test]
    fn test_process_message_pitch_wheel_dispatches() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        // Pitch wheel: status 0xE0 (ch 0), LSB=0, MSB=64 → pitch=64<<7=8192
        proc.process_message(&[0xE0, 0x00, 0x40], 0, SynthMethodOptions::default());
        let evs = events.lock().unwrap();
        let has_pw = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::PitchWheel(_))
        });
        assert!(has_pw);
    }

    #[test]
    fn test_process_message_program_change_dispatches() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        // Program change: status 0xC0 (ch 0), program 10
        proc.process_message(&[0xC0, 10], 0, SynthMethodOptions::default());
        // Program change fires ProgramChange event only when preset found; with no bank loaded,
        // it might not fire. Just ensure no panic.
        drop(events.lock().unwrap()); // no panic = pass
        let _ = before;
    }

    #[test]
    fn test_process_message_controller_change_dispatches() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        // Controller change: status 0xB0 (ch 0), CC 7 (volume), value 100
        proc.process_message(&[0xB0, 7, 100], 0, SynthMethodOptions::default());
        let evs = events.lock().unwrap();
        let has_cc = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::ControllerChange(_))
        });
        assert!(has_cc);
    }

    #[test]
    fn test_process_message_reset_fires_reset_event() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        // System Reset: 0xFF
        proc.process_message(&[0xFF], 0, SynthMethodOptions::default());
        let evs = events.lock().unwrap();
        let has_reset = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::Reset(_))
        });
        assert!(has_reset);
    }

    #[test]
    fn test_process_message_channel_offset_applied_to_pitch_wheel() {
        // Use pitch wheel (which fires on any channel, no preset needed)
        // to verify channel_offset is applied.
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        // Pitch wheel on MIDI ch 0 with channel_offset=1 → should affect ch 1
        proc.process_message(&[0xE0, 0x00, 0x40], 1, SynthMethodOptions::default());
        let evs = events.lock().unwrap();
        let has_ch1_pw = evs.iter().skip(before).any(|e| {
            if let SynthProcessorEvent::PitchWheel(cb) = e {
                cb.channel == 1
            } else {
                false
            }
        });
        assert!(has_ch1_pw);
    }

    // -----------------------------------------------------------------------
    // process_message — scheduled dispatch
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_message_scheduled_not_fired_immediately() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        // Schedule far in the future
        let future_time = SynthMethodOptions { time: 9999.0 };
        proc.process_message(&[0x90, 60, 100], 0, future_time);
        // Should not have fired yet
        assert_eq!(event_count(&events), before);
    }

    #[test]
    fn test_process_message_scheduled_controller_fires_after_render() {
        // Use a controller change (which fires events regardless of preset)
        // to verify that scheduled messages execute after render_audio advances time.
        //
        // render_audio processes the event queue at the START of each call (using the time
        // from the PREVIOUS render), so two render passes are needed:
        //   1st pass: advances time from 0.0 → 1.0 (event at 0.1 s is not yet processed)
        //   2nd pass: process_event_queue sees time=1.0 ≥ 0.1 → fires the scheduled CC
        let (mut proc, events) = make_processor();
        // Schedule CC 7 (volume) for time = 0.1 s
        let future_opts = SynthMethodOptions { time: 0.1 };
        proc.process_message(&[0xB0, 7, 80], 0, future_opts);

        // Verify the event hasn't fired yet
        let before = event_count(&events);

        // Render 1 second in maxBufferSize (128) chunks (rendering more than
        // maxBufferSize at once panics in 4.3.0).
        let chunk = 128;
        let mut out = vec![vec![0.0f32; chunk]; 2];
        for _ in 0..(44100 / chunk + 1) {
            proc.render_audio(&mut out, 0, chunk);
        }
        // One more chunk so the queue (processed at the start of a render) fires.
        proc.render_audio(&mut out, 0, chunk);

        let evs = events.lock().unwrap();
        let has_cc = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::ControllerChange(_))
        });
        assert!(has_cc, "Scheduled CC should fire after render advances time past 0.1s");
    }

    // -----------------------------------------------------------------------
    // render_audio — maxBufferSize
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "maxBufferSize")]
    fn test_render_audio_beyond_max_buffer_size_panics() {
        let (mut proc, _) = make_processor();
        let samples = 256; // default maxBufferSize is 128
        let mut out = vec![vec![0.0f32; samples]; 2];
        proc.render_audio(&mut out, 0, samples);
    }

    #[test]
    fn test_render_audio_with_larger_max_buffer_size() {
        let mut proc = SpessaSynthProcessor::new(
            44100.0,
            |_: SynthProcessorEvent| {},
            SynthProcessorOptions {
                max_buffer_size: 4096,
                ..Default::default()
            },
        );
        let samples = 4096;
        let mut out = vec![vec![0.0f32; samples]; 2];
        proc.render_audio(&mut out, 0, samples); // must not panic
    }

    // -----------------------------------------------------------------------
    // clear_cache
    // -----------------------------------------------------------------------

    #[test]
    fn test_clear_cache_no_panic() {
        let (mut proc, _) = make_processor();
        proc.clear_cache(); // Just ensure no panic
    }

    // -----------------------------------------------------------------------
    // note_off_channel / controller_change_channel (SynthesizerCore wrappers)
    // -----------------------------------------------------------------------

    #[test]
    fn test_note_off_channel_fires_event() {
        let events: Arc<Mutex<Vec<SynthProcessorEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let ev_clone = Arc::clone(&events);
        let mut core = SynthesizerCore::new(
            move |ev| ev_clone.lock().unwrap().push(ev),
            44100.0,
            SynthProcessorOptions::default(),
        );
        core.create_midi_channel(false);

        let before = events.lock().unwrap().len();
        core.note_off_channel(0, 60);
        let evs = events.lock().unwrap();
        assert!(
            evs.len() > before,
            "note_off_channel should emit at least one event"
        );
    }

    #[test]
    fn test_controller_change_channel_fires_event() {
        let events: Arc<Mutex<Vec<SynthProcessorEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let ev_clone = Arc::clone(&events);
        let mut core = SynthesizerCore::new(
            move |ev| ev_clone.lock().unwrap().push(ev),
            44100.0,
            SynthProcessorOptions::default(),
        );
        core.create_midi_channel(false);

        let before = events.lock().unwrap().len();
        core.controller_change_channel(0, 7, 100); // CC 7 = main volume
        let evs = events.lock().unwrap();
        let has_cc = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::ControllerChange(_))
        });
        assert!(has_cc);
    }

    #[test]
    fn test_pitch_wheel_channel_fires_event() {
        let events: Arc<Mutex<Vec<SynthProcessorEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let ev_clone = Arc::clone(&events);
        let mut core = SynthesizerCore::new(
            move |ev| ev_clone.lock().unwrap().push(ev),
            44100.0,
            SynthProcessorOptions::default(),
        );
        core.create_midi_channel(false);

        let before = events.lock().unwrap().len();
        core.pitch_wheel_channel(0, 8192, -1);
        let evs = events.lock().unwrap();
        let has_pw = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::PitchWheel(_))
        });
        assert!(has_pw);
    }
}
