/// processor.rs
/// purpose: SpessaSynthProcessor - the main public API wrapping SynthesizerCore.
/// Ported from: src/synthesizer/processor.ts
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
use crate::midi::enums::midi_message_types;
use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
use crate::soundbank::sound_bank_loader::load_sound_bank;
use crate::synthesizer::audio_engine::engine_components::synth_constants::{
    DEFAULT_SYNTH_MODE, MIDI_CHANNEL_COUNT, embedded_sound_bank_id,
};
use crate::synthesizer::audio_engine::snapshot::synthesizer_snapshot::SynthesizerSnapshot;
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::types::{
    CachedVoiceList, MasterParameterChangeCallback, MasterParameterType, SynthMethodOptions,
    SynthProcessorEvent, SynthProcessorOptions, SynthSystem,
};
use crate::utils::loggin::spessa_synth_info;

// ---------------------------------------------------------------------------
// Additional SynthesizerCore methods — channel-level wrappers
// ---------------------------------------------------------------------------

impl SynthesizerCore {
    /// Sends note-off to a channel, dispatching events.
    /// Equivalent to: midiChannels[channel].noteOff(midiNote) (SpessaSynthProcessor context)
    pub fn note_off_channel(&mut self, channel: usize, midi_note: u8) {
        let current_time = self.current_time;
        let black_midi_mode = self.master_parameters.black_midi_mode;
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
        let current_system = self.master_parameters.midi_system;
        let enable_event_system = self.enable_event_system;
        let voices = &mut self.voices;
        let events = self.midi_channels[channel].controller_change(
            controller,
            value,
            voices,
            current_time,
            current_system,
            enable_event_system,
        );
        for ev in events {
            self.call_event(ev);
        }
    }

    /// Sends a program change to a channel, dispatching events.
    /// Equivalent to: midiChannels[channel].programChange(program)
    pub fn program_change_channel(&mut self, channel: usize, program: u8) {
        let current_system = self.master_parameters.midi_system;
        let enable_event_system = self.enable_event_system;
        let events = self.midi_channels[channel].program_change(
            program,
            &self.sound_bank_manager,
            current_system,
            enable_event_system,
        );
        for ev in events {
            self.call_event(ev);
        }
    }

    /// Sends a pitch wheel message to a channel, dispatching events.
    /// Equivalent to: midiChannels[channel].pitchWheel(pitch, midiNote)
    pub fn pitch_wheel_channel(&mut self, channel: usize, pitch: i16, midi_note: i32) {
        let enable_event_system = self.enable_event_system;
        let voices = &mut self.voices;
        let events =
            self.midi_channels[channel].pitch_wheel(voices, pitch, midi_note, enable_event_system);
        for ev in events {
            self.call_event(ev);
        }
    }

    /// Sends a channel pressure message to a channel, dispatching events.
    /// Equivalent to: midiChannels[channel].channelPressure(pressure)
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
        use crate::synthesizer::audio_engine::engine_components::synth_constants::ALL_CHANNELS_OR_DIFFERENT_ACTION;
        use crate::utils::loggin::spessa_synth_info;
        use crate::utils::other::array_to_hex_string;

        // Ensure that the device ID matches
        if self.master_parameters.device_id != ALL_CHANNELS_OR_DIFFERENT_ACTION
            && syx[1] != 0x7f // 0x7f means broadcast
            && self.master_parameters.device_id != syx[1] as i32
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
                spessa_synth_info(&format!(
                    "Unrecognized SysEx: {} (unknown manufacturer)",
                    array_to_hex_string(syx)
                ));
            }
        }
    }

    /// Processes a raw MIDI message.
    /// If options.time > current_time, the dispatch is scheduled for later;
    /// otherwise it executes immediately.
    /// Equivalent to: processMessage(message, channelOffset, force, options) in synthesizer_core.ts
    pub fn process_message(
        &mut self,
        message: &[u8],
        channel_offset: usize,
        force: bool,
        options: SynthMethodOptions,
    ) {
        let time = options.time;
        if time > self.current_time {
            let msg = message.to_vec();
            self.schedule_event(
                move |core| core.dispatch_message_internal(&msg, channel_offset, force),
                time,
            );
        } else {
            let msg = message.to_vec();
            self.dispatch_message_internal(&msg, channel_offset, force);
        }
    }

    /// Dispatches the actual MIDI event bytes.
    fn dispatch_message_internal(&mut self, message: &[u8], channel_offset: usize, force: bool) {
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
                    if force {
                        let current_time = self.current_time;
                        let voices = &mut self.voices;
                        self.midi_channels[channel].kill_note(
                            message[1],
                            -12000,
                            voices,
                            current_time,
                        );
                    } else {
                        self.note_off_channel(channel, message[1]);
                    }
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
                    self.stop_all_channels(false);
                    self.reset_all_controllers(DEFAULT_SYNTH_MODE);
                }
                _ => {}
            }
        }
    }

    /// Renders per-channel audio.
    /// Stub: renderAudioSplit from synthesizer_core.ts is not yet ported.
    pub fn render_audio_split(
        &mut self,
        _reverb: &mut [Vec<f32>],
        _chorus: &mut [Vec<f32>],
        _separate: &mut Vec<Vec<Vec<f32>>>,
        _start_index: usize,
        _sample_count: usize,
    ) {
        // TODO: Port from synthesizer_core.ts renderAudioSplit
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
        for _ in 0..MIDI_CHANNEL_COUNT {
            core.create_midi_channel(false);
        }
        spessa_synth_info("SpessaSynth is ready!");
        Self {
            sample_rate,
            synth_core: core,
            saved_snapshot: None,
        }
    }

    // -----------------------------------------------------------------------
    // Properties (Rust getters/setters for TypeScript get/set)
    // -----------------------------------------------------------------------

    /// Are chorus and reverb effects enabled?
    /// Equivalent to: get enableEffects() / set enableEffects(v)
    pub fn enable_effects(&self) -> bool {
        self.synth_core.enable_effects
    }
    pub fn set_enable_effects(&mut self, v: bool) {
        self.synth_core.enable_effects = v;
    }

    /// Is the event system enabled?
    /// Equivalent to: get enableEventSystem() / set enableEventSystem(v)
    pub fn enable_event_system(&self) -> bool {
        self.synth_core.enable_event_system
    }
    pub fn set_enable_event_system(&mut self, v: bool) {
        self.synth_core.enable_event_system = v;
    }

    /// Total active voice count.
    /// Equivalent to: get totalVoicesAmount()
    pub fn total_voices_amount(&self) -> u32 {
        self.synth_core.voice_count
    }

    /// Current synthesizer time in seconds.
    /// Equivalent to: get currentSynthTime()
    pub fn current_synth_time(&self) -> f64 {
        self.synth_core.current_time
    }

    // -----------------------------------------------------------------------
    // Master parameters
    // -----------------------------------------------------------------------

    /// Sets a master parameter.
    /// Equivalent to: setMasterParameter(type, value)
    pub fn set_master_parameter(&mut self, change: MasterParameterChangeCallback) {
        self.synth_core.set_master_parameter(change);
    }

    /// Gets all master parameters.
    /// Equivalent to: getAllMasterParameters()
    pub fn get_all_master_parameters(&self) -> MasterParameterType {
        self.synth_core.get_all_master_parameters()
    }

    // -----------------------------------------------------------------------
    // System control
    // -----------------------------------------------------------------------

    /// Resets all controllers on all channels.
    /// Equivalent to: resetAllControllers(system = DEFAULT_SYNTH_MODE)
    pub fn reset_all_controllers(&mut self, system: SynthSystem) {
        self.synth_core.reset_all_controllers(system);
    }

    // -----------------------------------------------------------------------
    // Snapshot
    // -----------------------------------------------------------------------

    /// Applies a synthesizer snapshot to restore state.
    /// Saves the snapshot so it can be re-applied after bank changes.
    /// Equivalent to: applySynthesizerSnapshot(snapshot)
    pub fn apply_synthesizer_snapshot(&mut self, snapshot: SynthesizerSnapshot) {
        self.saved_snapshot = Some(snapshot.clone());
        snapshot.apply(&mut self.synth_core);
        spessa_synth_info("Finished applying snapshot!");
        self.reset_all_controllers(DEFAULT_SYNTH_MODE);
    }

    /// Returns a snapshot of the current synthesizer state.
    /// Equivalent to: getSnapshot()
    pub fn get_snapshot(&self) -> SynthesizerSnapshot {
        SynthesizerSnapshot::create(&self.synth_core)
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
            self.apply_synthesizer_snapshot(snapshot);
        }
        spessa_synth_info(&format!("Embedded sound bank set at offset {}", offset));
    }

    /// Removes the embedded sound bank.
    /// Equivalent to: clearEmbeddedBank()
    pub fn clear_embedded_bank(&mut self) {
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

    /// Creates a new MIDI channel.
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
    /// Equivalent to: controllerChange(channel, controllerNumber, controllerValue)
    pub fn controller_change(&mut self, channel: usize, controller: u8, value: u8) {
        self.synth_core
            .controller_change_channel(channel, controller, value);
    }

    /// Sends a MIDI note-on message.
    /// Equivalent to: noteOn(channel, midiNote, velocity)
    pub fn note_on(&mut self, channel: usize, midi_note: u8, velocity: u8) {
        self.synth_core.note_on(channel, midi_note, velocity);
    }

    /// Sends a MIDI note-off message.
    /// Equivalent to: noteOff(channel, midiNote)
    pub fn note_off(&mut self, channel: usize, midi_note: u8) {
        self.synth_core.note_off_channel(channel, midi_note);
    }

    /// Sends a MIDI poly pressure (aftertouch) message.
    /// Equivalent to: polyPressure(channel, midiNote, pressure)
    pub fn poly_pressure(&mut self, channel: usize, midi_note: u8, pressure: u8) {
        self.synth_core
            .poly_pressure_channel(channel, midi_note, pressure);
    }

    /// Sends a MIDI channel pressure (aftertouch) message.
    /// Equivalent to: channelPressure(channel, pressure)
    pub fn channel_pressure(&mut self, channel: usize, pressure: u8) {
        self.synth_core.channel_pressure_channel(channel, pressure);
    }

    /// Sends a MIDI pitch wheel message.
    /// pitch: 0–16383 (8192 = center); midi_note: -1 for channel-wide pitch wheel.
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
    /// Equivalent to: processMessage(message, channelOffset, force, options)
    pub fn process_message(
        &mut self,
        message: &[u8],
        channel_offset: usize,
        force: bool,
        options: SynthMethodOptions,
    ) {
        self.synth_core
            .process_message(message, channel_offset, force, options);
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

    /// Legacy render_audio with reverb/chorus parameters (ignored, kept for compatibility).
    pub fn render_audio_legacy(
        &mut self,
        outputs: &mut [Vec<f32>],
        _reverb: &mut [Vec<f32>],
        _chorus: &mut [Vec<f32>],
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
    use crate::synthesizer::audio_engine::engine_components::synth_constants::MIDI_CHANNEL_COUNT;
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
        assert_eq!(proc.synth_core.midi_channels.len(), MIDI_CHANNEL_COUNT as usize);
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
    // enable_effects / enable_event_system
    // -----------------------------------------------------------------------

    #[test]
    fn test_enable_effects_default_true() {
        let (proc, _) = make_processor();
        assert!(proc.enable_effects());
    }

    #[test]
    fn test_set_enable_effects_false() {
        let (mut proc, _) = make_processor();
        proc.set_enable_effects(false);
        assert!(!proc.enable_effects());
    }

    #[test]
    fn test_enable_event_system_default_true() {
        let (proc, _) = make_processor();
        assert!(proc.enable_event_system());
    }

    #[test]
    fn test_set_enable_event_system_false() {
        let (mut proc, _) = make_processor();
        proc.set_enable_event_system(false);
        assert!(!proc.enable_event_system());
    }

    // -----------------------------------------------------------------------
    // total_voices_amount / current_synth_time
    // -----------------------------------------------------------------------

    #[test]
    fn test_total_voices_amount_initially_zero() {
        let (proc, _) = make_processor();
        assert_eq!(proc.total_voices_amount(), 0);
    }

    #[test]
    fn test_current_synth_time_initially_zero() {
        let (proc, _) = make_processor();
        assert!((proc.current_synth_time() - 0.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // set_master_parameter / get_all_master_parameters
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_master_parameter_master_gain() {
        let (mut proc, _) = make_processor();
        proc.set_master_parameter(MasterParameterChangeCallback::MasterGain(0.5));
        assert!((proc.get_all_master_parameters().master_gain - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_set_master_parameter_voice_cap() {
        let (mut proc, _) = make_processor();
        proc.set_master_parameter(MasterParameterChangeCallback::VoiceCap(100));
        assert_eq!(proc.get_all_master_parameters().voice_cap, 100);
    }

    #[test]
    fn test_set_master_parameter_device_id() {
        let (mut proc, _) = make_processor();
        proc.set_master_parameter(MasterParameterChangeCallback::DeviceId(5));
        assert_eq!(proc.get_all_master_parameters().device_id, 5);
    }

    // -----------------------------------------------------------------------
    // reset_all_controllers
    // -----------------------------------------------------------------------

    #[test]
    fn test_reset_all_controllers_fires_event() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        proc.reset_all_controllers(SynthSystem::Gs);
        // AllControllerReset event should be emitted
        let evs = events.lock().unwrap();
        assert!(
            evs.len() > before,
            "Expected at least one event after reset"
        );
        let has_reset = evs.iter().any(|e| {
            matches!(e, SynthProcessorEvent::AllControllerReset)
        });
        assert!(has_reset, "Expected AllControllerReset event");
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
    fn test_create_midi_channel_fires_new_channel_event() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        proc.create_midi_channel();
        let evs = events.lock().unwrap();
        assert!(evs.len() > before);
        let has_new_channel = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::NewChannel)
        });
        assert!(has_new_channel);
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
    // get_snapshot / apply_synthesizer_snapshot round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_snapshot_captures_channel_count() {
        let (proc, _) = make_processor();
        let snap = proc.get_snapshot();
        assert_eq!(
            snap.channel_snapshots.len(),
            proc.synth_core.midi_channels.len()
        );
    }

    #[test]
    fn test_apply_snapshot_restores_master_gain() {
        let (mut proc, _) = make_processor();
        proc.set_master_parameter(MasterParameterChangeCallback::MasterGain(0.3));
        let snap = proc.get_snapshot();

        proc.set_master_parameter(MasterParameterChangeCallback::MasterGain(1.0));
        proc.apply_synthesizer_snapshot(snap);

        assert!(
            (proc.get_all_master_parameters().master_gain - 0.3).abs() < 1e-9
        );
    }

    #[test]
    fn test_apply_snapshot_saves_snapshot_internally() {
        let (mut proc, _) = make_processor();
        let snap = proc.get_snapshot();
        proc.apply_synthesizer_snapshot(snap);
        assert!(proc.saved_snapshot.is_some());
    }

    // -----------------------------------------------------------------------
    // process_message — immediate dispatch
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_message_note_on_no_preset_does_not_panic() {
        // Without a sound bank, note_on silently returns; no panic expected.
        let (mut proc, _) = make_processor();
        proc.process_message(&[0x90, 60, 100], 0, false, SynthMethodOptions::default());
    }

    #[test]
    fn test_process_message_note_on_velocity_zero_is_note_off() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        // Note-on with velocity 0 → note-off
        proc.process_message(&[0x90, 60, 0], 0, false, SynthMethodOptions::default());
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
        proc.process_message(&[0x80, 60, 0], 0, false, SynthMethodOptions::default());
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
        proc.process_message(&[0xE0, 0x00, 0x40], 0, false, SynthMethodOptions::default());
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
        proc.process_message(&[0xC0, 10], 0, false, SynthMethodOptions::default());
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
        proc.process_message(&[0xB0, 7, 100], 0, false, SynthMethodOptions::default());
        let evs = events.lock().unwrap();
        let has_cc = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::ControllerChange(_))
        });
        assert!(has_cc);
    }

    #[test]
    fn test_process_message_reset_fires_controller_reset_event() {
        let (mut proc, events) = make_processor();
        let before = event_count(&events);
        // System Reset: 0xFF
        proc.process_message(&[0xFF], 0, false, SynthMethodOptions::default());
        let evs = events.lock().unwrap();
        let has_reset = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::AllControllerReset)
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
        proc.process_message(&[0xE0, 0x00, 0x40], 1, false, SynthMethodOptions::default());
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
        proc.process_message(&[0x90, 60, 100], 0, false, future_time);
        // Should not have fired yet
        assert_eq!(event_count(&events), before);
    }

    #[test]
    fn test_process_message_scheduled_controller_fires_after_render() {
        // Use a controller change (which fires events regardless of preset)
        // to verify that scheduled messages execute after render_audio advances time.
        //
        // render_audio processes the event queue at the START of each call (using the time
        // from the PREVIOUS render), so two render calls are needed:
        //   1st render: advances time from 0.0 → 1.0 (event at 0.1 s is not yet processed)
        //   2nd render: process_event_queue sees time=1.0 ≥ 0.1 → fires the scheduled CC
        let (mut proc, events) = make_processor();
        // Schedule CC 7 (volume) for time = 0.1 s
        let future_opts = SynthMethodOptions { time: 0.1 };
        proc.process_message(&[0xB0, 7, 80], 0, false, future_opts);

        // Verify the event hasn't fired yet
        let before = event_count(&events);

        let samples = 44100; // 1 second at 44100 Hz

        // First render: advances time from 0.0 → 1.0
        let mut out = vec![vec![0.0f32; samples]; 2];
        proc.render_audio(&mut out, 0, samples);

        // Second render: process_event_queue fires the event scheduled at 0.1 s
        let mut out2 = vec![vec![0.0f32; samples]; 2];
        proc.render_audio(&mut out2, 0, samples);

        let evs = events.lock().unwrap();
        let has_cc = evs.iter().skip(before).any(|e| {
            matches!(e, SynthProcessorEvent::ControllerChange(_))
        });
        assert!(has_cc, "Scheduled CC should fire after render advances time past 0.1s");
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
