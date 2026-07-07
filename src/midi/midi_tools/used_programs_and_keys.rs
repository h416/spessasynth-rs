/// used_programs_and_keys.rs
/// purpose: Scan a BasicMidi to find every (BasicPreset, note, velocity) combination that is
///          actually played, so callers can trim unused presets/instrument-zones/samples from a
///          sound bank via `BasicSoundBank::trim`.
/// Ported from: src/midi/midi_tools/used_programs_and_keys.ts (spessasynth_core 4.3.0)
/// (formerly `src/midi/midi_tools/used_keys_loaded.ts` in 4.2.0)
///
/// TS 4.3.0 rewrote this file almost entirely:
/// - Return type changed from `Map<BasicPreset, Set<string>>` ("note-velocity" strings) to
///   `PresetsWithKeyCombinations` = `Map<BasicPreset, Map<number, Set<number>>>` (midiNote ->
///   set of velocities). See `soundbank::types::PresetsWithKeyCombinations` for the Rust-specific
///   pointer-identity key scheme used in place of TS's `Map<BasicPreset, ...>`.
/// - `isXGOn`/`isGSOn`/`isGMOn`/`isGM2On`/`isGSDrumsOn`/`syxToChannel` (from the now-deleted
///   `utils/sysex_detector.ts`) are replaced by `MIDIUtils.analyzeSysEx`.
/// - Iterates `mid.timeline` directly instead of `mid.iterate(callback)`.
/// - Added RPN/NRPN tracking via `ParameterTracker` (RPN Coarse Tuning / GS-XG SysEx key-shift
///   are now honored when recording which MIDI note was actually played — drum channels ignore
///   key-shift; test case referenced upstream: th07_19_user_gm.mid), `CC 121` (Reset All
///   Controllers) handling, and `BankSelectHacks.getDefaultBank` for the per-system default bank
///   used when a system reset (XG/GM/GM2/GS) occurs mid-file.
use std::collections::{HashMap, HashSet};

use crate::midi::basic_midi::BasicMidi;
use crate::midi::enums::{midi_controllers, midi_message_types};
use crate::midi::midi_tools::midi_utils::{AnalyzedMidiMessage, MidiUtils};
use crate::midi::midi_tools::parameter_tracker::ParameterTracker;
use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
use crate::soundbank::basic_soundbank::preset_resolver::PresetResolver;
use crate::soundbank::types::{MIDISystem, PresetsWithKeyCombinations};
use crate::synthesizer::audio_engine::synth_constants::DEFAULT_PERCUSSION;
use crate::utils::loggin::SpessaLog;
use crate::utils::midi_hacks::BankSelectHacks;

// ─────────────────────────────────────────────────────────────────────────────
// Internal types
// ─────────────────────────────────────────────────────────────────────────────

/// Per-channel state tracked while scanning MIDI events.
/// Equivalent to: interface InternalChannelType
struct InternalChannelType {
    /// Stable identity of the currently-selected preset (pointer address), if any.
    preset_ptr: Option<usize>,
    bank_msb: u8,
    bank_lsb: u8,
    /// RPN/NRPN tracking (for RPN Coarse Tuning key-shift).
    param: ParameterTracker,
    is_drum: bool,
    /// Semitones, applied to note-on events on this channel (ignored on drum channels).
    key_shift: i32,
}

/// Resets a channel array back to its "system reset" state (GM/GM2/GS/XG On).
/// Equivalent to: the `reset` closure inside `getUsedProgramsAndKeys`
fn reset_channels(sys: MIDISystem, channels: &mut [InternalChannelType]) {
    for (i, ch) in channels.iter_mut().enumerate() {
        ch.is_drum = i % 16 == DEFAULT_PERCUSSION as usize;
        ch.bank_msb = BankSelectHacks::get_default_bank(sys);
        ch.bank_lsb = 0;
        ch.key_shift = 0;
        ch.param.reset();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Gets the used programs and keys for this MIDI file with a given sound bank.
///
/// `sound_bank` is queried via the [`PresetResolver`] trait, so this function works with both
/// `BasicSoundBank` and `SoundBankManager` without introducing a circular dependency.
///
/// Equivalent to: getUsedProgramsAndKeys(mid, soundBank)
pub fn get_used_programs_and_keys(
    midi: &BasicMidi,
    sound_bank: &dyn PresetResolver,
) -> PresetsWithKeyCombinations {
    SpessaLog::group_collapsed("Searching for all used programs and keys...");

    // Find every used preset and every key:velocity for each.
    // Make sure to care about ports and drums.
    let channels_amount = 16
        + midi
            .port_channel_offset_map
            .iter()
            .copied()
            .max()
            .unwrap_or(0) as usize;

    // Track channels and systems
    let mut system = MIDISystem::Gs;
    let mut master_key_shift: i32 = 0;

    let mut channels: Vec<InternalChannelType> = (0..channels_amount)
        .map(|i| {
            let is_drum = i % 16 == DEFAULT_PERCUSSION as usize;
            let preset = sound_bank.get_preset(
                MidiPatch {
                    bank_lsb: 0,
                    bank_msb: 0,
                    program: 0,
                    is_gm_gs_drum: is_drum,
                },
                system,
            );
            InternalChannelType {
                preset_ptr: preset.map(|p| p as *const BasicPreset as usize),
                bank_msb: 0,
                bank_lsb: 0,
                param: ParameterTracker::new(i as u8),
                is_drum,
                key_shift: 0,
            }
        })
        .collect();

    // Find all programs used and key-velocity combos in them.
    // bank:program each has a map of midiNote -> set of velocities.
    let mut used_programs_and_keys: PresetsWithKeyCombinations = HashMap::new();

    let mut ports: Vec<u32> = midi.tracks.iter().map(|t| t.port).collect();
    let offset_map = &midi.port_channel_offset_map;

    for te in &midi.timeline {
        let track_num = te.tr;
        let e = &midi.tracks[track_num].events[te.ev];

        // Do not assign ports to empty tracks.
        // Testcase: Cueshe - Bakit 1.mid
        if e.status_byte == midi_message_types::MIDI_PORT
            && !midi.tracks[track_num].channels.is_empty()
        {
            let mut port = e.data.first().copied().unwrap_or(0) as usize;
            if port >= offset_map.len() {
                SpessaLog::warn(&format!(
                    "Invalid port {} on track {}. (No offset found in the MIDI map.",
                    port, track_num
                ));
                port = 0;
            }
            ports[track_num] = port as u32;
            continue;
        }

        let status = e.status_byte & 0xf0;
        if status != midi_message_types::NOTE_ON
            && status != midi_message_types::CONTROLLER_CHANGE
            && status != midi_message_types::PROGRAM_CHANGE
            && status != midi_message_types::SYSTEM_EXCLUSIVE
        {
            continue;
        }

        let channel_offset = offset_map
            .get(ports[track_num] as usize)
            .copied()
            .unwrap_or(0) as usize;

        match status {
            s if s == midi_message_types::PROGRAM_CHANGE => {
                let channel = (e.status_byte & 0xf) as usize + channel_offset;
                if let Some(ch) = channels.get_mut(channel) {
                    let preset = sound_bank.get_preset(
                        MidiPatch {
                            bank_msb: ch.bank_msb,
                            bank_lsb: ch.bank_lsb,
                            program: e.data.first().copied().unwrap_or(0),
                            is_gm_gs_drum: ch.is_drum,
                        },
                        system,
                    );
                    ch.preset_ptr = preset.map(|p| p as *const BasicPreset as usize);
                }
            }

            s if s == midi_message_types::CONTROLLER_CHANGE => {
                let channel = (e.status_byte & 0xf) as usize + channel_offset;
                let cc = e.data.first().copied().unwrap_or(0);
                let value = e.data.get(1).copied().unwrap_or(0);

                match cc {
                    // Registered/non-registered param tracking
                    _ if cc == midi_controllers::REGISTERED_PARAMETER_MSB
                        || cc == midi_controllers::REGISTERED_PARAMETER_LSB
                        || cc == midi_controllers::NON_REGISTERED_PARAMETER_LSB
                        || cc == midi_controllers::NON_REGISTERED_PARAMETER_MSB =>
                    {
                        if let Some(ch) = channels.get_mut(channel) {
                            ch.param.controller_change(cc, value, track_num, te.ev);
                        }
                    }

                    _ if cc == midi_controllers::DATA_ENTRY_MSB
                        || cc == midi_controllers::DATA_ENTRY_LSB =>
                    {
                        if let Some(ch) = channels.get_mut(channel) {
                            let analyzed = ch.param.controller_change(cc, value, track_num, te.ev);
                            // RPN#02 Coarse Tune is key-shift according to GM2 section 3.4.3
                            if let Some(AnalyzedMidiMessage::KeyShift { value, .. }) = analyzed {
                                // Drum channels ignore key shift.
                                // Testcase: th07_19_user_gm.mid
                                ch.key_shift = if ch.is_drum { 0 } else { value };
                            }
                        }
                    }

                    _ if cc == midi_controllers::RESET_ALL_CONTROLLERS => {
                        if let Some(ch) = channels.get_mut(channel) {
                            ch.param.reset();
                        }
                    }

                    _ if cc == midi_controllers::BANK_SELECT => {
                        if let Some(ch) = channels.get_mut(channel) {
                            ch.bank_msb = value;
                        }
                    }

                    _ if cc == midi_controllers::BANK_SELECT_LSB => {
                        if let Some(ch) = channels.get_mut(channel) {
                            ch.bank_lsb = value;
                        }
                    }

                    _ => {}
                }
            }

            s if s == midi_message_types::NOTE_ON => {
                let channel = (e.status_byte & 0xf) as usize + channel_offset;
                // That's a note off.
                if e.data.get(1).copied().unwrap_or(0) == 0 {
                    continue;
                }
                let Some(ch) = channels.get(channel) else {
                    continue;
                };
                // If there's no preset, ignore.
                let Some(ptr) = ch.preset_ptr else {
                    continue;
                };

                let midi_note = e.data.first().copied().unwrap_or(0) as i32
                    + if ch.is_drum { 0 } else { master_key_shift }
                    + ch.key_shift;
                let velocity = e.data.get(1).copied().unwrap_or(0);

                used_programs_and_keys
                    .entry(ptr)
                    .or_default()
                    .entry(midi_note)
                    .or_default()
                    .insert(velocity);
            }

            s if s == midi_message_types::SYSTEM_EXCLUSIVE => {
                let syx = MidiUtils::analyze_sysex(&e.data);
                match syx {
                    AnalyzedMidiMessage::XgReset => {
                        system = MIDISystem::Xg;
                        master_key_shift = 0;
                        reset_channels(system, &mut channels);
                        SpessaLog::info("XG on detected!");
                    }

                    AnalyzedMidiMessage::Gm2On => {
                        system = MIDISystem::Gm2;
                        master_key_shift = 0;
                        reset_channels(system, &mut channels);
                        SpessaLog::info("GM2 on detected!");
                    }

                    AnalyzedMidiMessage::GmOn => {
                        system = MIDISystem::Gm;
                        master_key_shift = 0;
                        reset_channels(system, &mut channels);
                        SpessaLog::info("GM on detected!");
                    }

                    AnalyzedMidiMessage::GmOff | AnalyzedMidiMessage::GsReset => {
                        system = MIDISystem::Gs;
                        master_key_shift = 0;
                        reset_channels(system, &mut channels);
                        SpessaLog::info("GS on detected!");
                    }

                    AnalyzedMidiMessage::MasterKeyShift { value } => {
                        master_key_shift = value;
                    }

                    // Note: unlike "Drums On" / "Program Change" / "Controller Change" below,
                    // "Key Shift" is NOT offset by `channel_offset` here — this mirrors the
                    // TypeScript source exactly (which indexes `channels[syx.channel]` directly),
                    // even though it looks inconsistent with the other SysEx-channel cases.
                    AnalyzedMidiMessage::KeyShift { channel, value } => {
                        if let Some(ch) = channels.get_mut(channel as usize) {
                            // Drum channels ignore key shift.
                            // Testcase: th07_19_user_gm.mid
                            ch.key_shift = if ch.is_drum { 0 } else { value };
                        }
                    }

                    AnalyzedMidiMessage::DrumsOn { channel, is_drum } => {
                        let sysex_channel = channel as usize + channel_offset;
                        if let Some(ch) = channels.get_mut(sysex_channel) {
                            ch.is_drum = is_drum;
                        }
                    }

                    AnalyzedMidiMessage::ProgramChange { channel, value } => {
                        let sysex_channel = channel as usize + channel_offset;
                        if let Some(ch) = channels.get_mut(sysex_channel) {
                            let preset = sound_bank.get_preset(
                                MidiPatch {
                                    bank_msb: ch.bank_msb,
                                    bank_lsb: ch.bank_lsb,
                                    program: value,
                                    is_gm_gs_drum: ch.is_drum,
                                },
                                system,
                            );
                            ch.preset_ptr = preset.map(|p| p as *const BasicPreset as usize);
                        }
                    }

                    AnalyzedMidiMessage::ControllerChange {
                        channel,
                        controller,
                        value,
                    } => {
                        let sysex_channel = channel as usize + channel_offset;
                        if let Some(ch) = channels.get_mut(sysex_channel) {
                            if controller == midi_controllers::BANK_SELECT_LSB {
                                ch.bank_lsb = value;
                            } else if controller == midi_controllers::BANK_SELECT {
                                ch.bank_msb = value;
                            }
                        }
                    }

                    // Other classifications (Drum Setup, Reverb/Chorus/Delay/Variation/Insertion
                    // Param, Master Fine Tune, Fine Tune, Other) do not affect used-key tracking.
                    _ => {}
                }
            }

            _ => {}
        }
    }

    for (ptr, keys_for_preset) in used_programs_and_keys.iter() {
        if keys_for_preset.is_empty() {
            SpessaLog::info(&format!("Detected change but no keys for preset ptr {}", ptr));
        }
    }
    used_programs_and_keys.retain(|_, keys_for_preset| !keys_for_preset.is_empty());

    SpessaLog::group_end();
    used_programs_and_keys
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::basic_midi::BasicMidi;
    use crate::midi::enums::midi_message_types;
    use crate::midi::midi_message::MidiMessage;
    use crate::midi::midi_track::MidiTrack;
    use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
    use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
    use crate::soundbank::basic_soundbank::preset_resolver::PresetResolver;
    use crate::soundbank::types::MIDISystem;

    // ── Mock PresetResolver ───────────────────────────────────────────────────

    /// Minimal sound bank that always returns the same preset.
    struct OnePresetBank {
        preset: BasicPreset,
    }

    impl PresetResolver for OnePresetBank {
        fn get_preset(&self, _patch: MidiPatch, _system: MIDISystem) -> Option<&BasicPreset> {
            Some(&self.preset)
        }
    }

    /// Sound bank that returns one of two presets based on program number.
    struct TwoPresetBank {
        piano: BasicPreset,
        strings: BasicPreset,
    }

    impl PresetResolver for TwoPresetBank {
        fn get_preset(&self, patch: MidiPatch, _system: MIDISystem) -> Option<&BasicPreset> {
            if patch.program < 40 {
                Some(&self.piano)
            } else {
                Some(&self.strings)
            }
        }
    }

    /// Sound bank that always returns None.
    struct EmptyBank;
    impl PresetResolver for EmptyBank {
        fn get_preset(&self, _patch: MidiPatch, _system: MIDISystem) -> Option<&BasicPreset> {
            None
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn ev(ticks: u32, status: u8, data: Vec<u8>) -> MidiMessage {
        MidiMessage::new(ticks, status, data)
    }

    fn ptr_of(preset: &BasicPreset) -> usize {
        preset as *const BasicPreset as usize
    }

    fn simple_midi(events: Vec<MidiMessage>) -> BasicMidi {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        m.port_channel_offset_map = vec![0];
        let mut t = MidiTrack::new();
        for e in events {
            t.push_event(e);
        }
        m.tracks.push(t);
        m.flush(true);
        m
    }

    // ── Basic note detection ──────────────────────────────────────────────────

    #[test]
    fn test_single_note_on_registers_combo() {
        let bank = OnePresetBank {
            preset: BasicPreset::default(),
        };
        let midi = simple_midi(vec![
            ev(0, 0x90, vec![60, 100]), // note-on ch0, note=60, vel=100
        ]);

        let result = get_used_programs_and_keys(&midi, &bank);

        let key = ptr_of(&bank.preset);
        assert!(result.contains_key(&key), "preset should be in result");
        assert!(result[&key][&60].contains(&100));
    }

    #[test]
    fn test_note_on_vel0_not_registered() {
        let bank = OnePresetBank {
            preset: BasicPreset::default(),
        };
        let midi = simple_midi(vec![
            ev(0, 0x90, vec![60, 0]), // vel=0 = note-off, must not register
        ]);

        let result = get_used_programs_and_keys(&midi, &bank);
        assert!(result.is_empty());
    }

    #[test]
    fn test_multiple_notes_same_preset() {
        let bank = OnePresetBank {
            preset: BasicPreset::default(),
        };
        let midi = simple_midi(vec![
            ev(0, 0x90, vec![60, 100]),
            ev(10, 0x90, vec![64, 80]),
            ev(20, 0x90, vec![67, 90]),
        ]);

        let result = get_used_programs_and_keys(&midi, &bank);
        let combos = &result[&ptr_of(&bank.preset)];
        assert!(combos[&60].contains(&100));
        assert!(combos[&64].contains(&80));
        assert!(combos[&67].contains(&90));
        assert_eq!(combos.len(), 3);
    }

    #[test]
    fn test_duplicate_note_vel_combo_deduplicated() {
        let bank = OnePresetBank {
            preset: BasicPreset::default(),
        };
        let midi = simple_midi(vec![
            ev(0, 0x90, vec![60, 100]),
            ev(10, 0x90, vec![60, 100]), // exact duplicate
        ]);

        let result = get_used_programs_and_keys(&midi, &bank);
        assert_eq!(result[&ptr_of(&bank.preset)].len(), 1);
        assert_eq!(result[&ptr_of(&bank.preset)][&60].len(), 1);
    }

    // ── Program change ────────────────────────────────────────────────────────

    #[test]
    fn test_program_change_switches_preset() {
        let bank = TwoPresetBank {
            piano: BasicPreset::default(),
            strings: BasicPreset::default(),
        };
        let midi = simple_midi(vec![
            ev(0, 0x90, vec![60, 100]),  // played with initial preset (piano, program=0)
            ev(10, 0xC0, vec![40]),      // program change → program 40 (strings)
            ev(20, 0x90, vec![64, 80]),  // played with strings preset
        ]);

        let result = get_used_programs_and_keys(&midi, &bank);
        let piano_key = ptr_of(&bank.piano);
        let strings_key = ptr_of(&bank.strings);

        assert!(result.contains_key(&piano_key), "piano should be used");
        assert!(result[&piano_key][&60].contains(&100));
        assert!(result.contains_key(&strings_key), "strings should be used");
        assert!(result[&strings_key][&64].contains(&80));
    }

    // ── Bank select ───────────────────────────────────────────────────────────

    #[test]
    fn test_bank_select_msb_passed_to_get_preset() {
        struct BankCapture {
            preset: BasicPreset,
            captured_bank_msb: std::cell::Cell<u8>,
        }
        impl PresetResolver for BankCapture {
            fn get_preset(
                &self,
                patch: MidiPatch,
                _system: MIDISystem,
            ) -> Option<&BasicPreset> {
                self.captured_bank_msb.set(patch.bank_msb);
                Some(&self.preset)
            }
        }

        let bank = BankCapture {
            preset: BasicPreset::default(),
            captured_bank_msb: std::cell::Cell::new(0),
        };
        let midi = simple_midi(vec![
            ev(0, 0xB0, vec![0, 8]),  // bank select MSB = 8
            ev(5, 0xC0, vec![0]),     // program change triggers get_preset
            ev(10, 0x90, vec![60, 80]),
        ]);

        get_used_programs_and_keys(&midi, &bank);
        assert_eq!(bank.captured_bank_msb.get(), 8);
    }

    #[test]
    fn test_bank_select_lsb_passed_to_get_preset() {
        struct BankCapture {
            preset: BasicPreset,
            captured_bank_lsb: std::cell::Cell<u8>,
        }
        impl PresetResolver for BankCapture {
            fn get_preset(
                &self,
                patch: MidiPatch,
                _system: MIDISystem,
            ) -> Option<&BasicPreset> {
                self.captured_bank_lsb.set(patch.bank_lsb);
                Some(&self.preset)
            }
        }

        let bank = BankCapture {
            preset: BasicPreset::default(),
            captured_bank_lsb: std::cell::Cell::new(0),
        };
        let midi = simple_midi(vec![
            ev(0, 0xB0, vec![32, 5]), // bank select LSB = 5
            ev(5, 0xC0, vec![0]),
            ev(10, 0x90, vec![60, 80]),
        ]);

        get_used_programs_and_keys(&midi, &bank);
        assert_eq!(bank.captured_bank_lsb.get(), 5);
    }

    // ── Empty sound bank ──────────────────────────────────────────────────────

    #[test]
    fn test_empty_bank_no_preset_no_entry() {
        let bank = EmptyBank;
        let midi = simple_midi(vec![ev(0, 0x90, vec![60, 100])]);

        let result = get_used_programs_and_keys(&midi, &bank);
        assert!(result.is_empty());
    }

    // ── Multiple channels ─────────────────────────────────────────────────────

    #[test]
    fn test_notes_on_different_channels_both_registered() {
        let bank = OnePresetBank {
            preset: BasicPreset::default(),
        };
        let midi = simple_midi(vec![
            ev(0, 0x90, vec![60, 100]), // ch0
            ev(0, 0x91, vec![64, 80]),  // ch1
        ]);

        let result = get_used_programs_and_keys(&midi, &bank);
        let combos = &result[&ptr_of(&bank.preset)];
        assert!(combos[&60].contains(&100));
        assert!(combos[&64].contains(&80));
    }

    // ── Percussion channel (ch9) ──────────────────────────────────────────────

    #[test]
    fn test_percussion_channel_uses_drum_flag() {
        struct DrumCapture {
            preset: BasicPreset,
            got_drum: std::cell::Cell<bool>,
        }
        impl PresetResolver for DrumCapture {
            fn get_preset(
                &self,
                patch: MidiPatch,
                _system: MIDISystem,
            ) -> Option<&BasicPreset> {
                if patch.is_gm_gs_drum {
                    self.got_drum.set(true);
                }
                Some(&self.preset)
            }
        }

        let bank = DrumCapture {
            preset: BasicPreset::default(),
            got_drum: std::cell::Cell::new(false),
        };
        // ch9 = 0x99
        let midi = simple_midi(vec![ev(0, 0x99, vec![38, 100])]);

        get_used_programs_and_keys(&midi, &bank);
        assert!(bank.got_drum.get(), "ch9 should set is_gm_gs_drum=true");
    }

    // ── MIDI port meta event ──────────────────────────────────────────────────

    #[test]
    fn test_midi_port_event_updates_port() {
        // A track that declares a non-zero MIDI Port should still have its note-on registered
        // (under whatever channel offset `BasicMIDI::flush` assigns to that port), rather than
        // crashing or being silently dropped.
        let bank = OnePresetBank {
            preset: BasicPreset::default(),
        };
        let mut m = BasicMidi::new();
        m.time_division = 480;

        let mut t = MidiTrack::new();
        t.push_event(ev(0, midi_message_types::MIDI_PORT, vec![1])); // switch to port 1
        t.push_event(ev(10, 0x90, vec![60, 100]));
        m.tracks.push(t);
        m.flush(true);

        let result = get_used_programs_and_keys(&m, &bank);
        // Should have an entry (the note was registered under some channel)
        assert!(!result.is_empty());
    }

    // ── XG / GM system detection ──────────────────────────────────────────────

    #[test]
    fn test_xg_on_sysex_detected() {
        struct SystemCapture {
            preset: BasicPreset,
            system: std::cell::Cell<MIDISystem>,
        }
        impl PresetResolver for SystemCapture {
            fn get_preset(
                &self,
                _patch: MidiPatch,
                system: MIDISystem,
            ) -> Option<&BasicPreset> {
                self.system.set(system);
                Some(&self.preset)
            }
        }

        let bank = SystemCapture {
            preset: BasicPreset::default(),
            system: std::cell::Cell::new(MIDISystem::Gs),
        };

        // XG ON sysex: [Yamaha, ?, XG, ?, ?, 0x7e, 0x00]
        let xg_sysex = vec![0x43, 0x10, 0x4c, 0x00, 0x00, 0x7e, 0x00];
        let midi = simple_midi(vec![
            ev(0, midi_message_types::SYSTEM_EXCLUSIVE, xg_sysex),
            ev(10, 0xC0, vec![0]),      // program change triggers get_preset with new system
            ev(20, 0x90, vec![60, 80]),
        ]);

        get_used_programs_and_keys(&midi, &bank);
        assert_eq!(bank.system.get(), MIDISystem::Xg);
    }

    // ── Key-shift (RPN Coarse Tuning) ─────────────────────────────────────────

    #[test]
    fn test_rpn_coarse_tuning_shifts_recorded_note() {
        let bank = OnePresetBank {
            preset: BasicPreset::default(),
        };
        // RPN MSB=0, LSB=2 (coarse tuning) -> Data Entry MSB = 65 (+1 semitone)
        let midi = simple_midi(vec![
            ev(0, 0xB0, vec![midi_controllers::REGISTERED_PARAMETER_MSB, 0]),
            ev(0, 0xB0, vec![midi_controllers::REGISTERED_PARAMETER_LSB, 2]),
            ev(0, 0xB0, vec![midi_controllers::DATA_ENTRY_MSB, 65]),
            ev(10, 0x90, vec![60, 100]),
        ]);

        let result = get_used_programs_and_keys(&midi, &bank);
        let combos = &result[&ptr_of(&bank.preset)];
        // Note recorded at 60 + 1 = 61, not 60.
        assert!(combos.contains_key(&61));
        assert!(!combos.contains_key(&60));
    }

    #[test]
    fn test_rpn_coarse_tuning_ignored_on_drum_channel() {
        let bank = OnePresetBank {
            preset: BasicPreset::default(),
        };
        // Channel 9 (drum): same RPN coarse-tune messages should be ignored for note recording.
        let midi = simple_midi(vec![
            ev(0, 0xB9, vec![midi_controllers::REGISTERED_PARAMETER_MSB, 0]),
            ev(0, 0xB9, vec![midi_controllers::REGISTERED_PARAMETER_LSB, 2]),
            ev(0, 0xB9, vec![midi_controllers::DATA_ENTRY_MSB, 65]),
            ev(10, 0x99, vec![38, 100]),
        ]);

        let result = get_used_programs_and_keys(&midi, &bank);
        let combos = &result[&ptr_of(&bank.preset)];
        assert!(combos.contains_key(&38));
    }

    // ── No notes → empty result ───────────────────────────────────────────────

    #[test]
    fn test_no_note_events_returns_empty() {
        let bank = OnePresetBank {
            preset: BasicPreset::default(),
        };
        // Only controller change, no note-on
        let midi = simple_midi(vec![ev(0, 0xB0, vec![7, 100])]);
        let result = get_used_programs_and_keys(&midi, &bank);
        assert!(result.is_empty());
    }

    #[test]
    fn test_empty_midi_returns_empty() {
        let bank = OnePresetBank {
            preset: BasicPreset::default(),
        };
        let mut m = BasicMidi::new();
        m.flush(true);
        let result = get_used_programs_and_keys(&m, &bank);
        assert!(result.is_empty());
    }
}
