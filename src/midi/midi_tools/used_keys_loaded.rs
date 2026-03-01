/// used_keys_loaded.rs
/// purpose: Scan a BasicMidi to find every (BasicPreset, note-velocity) combination
///          that is actually played, so callers can trim unused presets from the
///          sound bank.
/// Ported from: src/midi/midi_tools/used_keys_loaded.ts
use std::collections::{HashMap, HashSet};

use crate::midi::basic_midi::BasicMidi;
use crate::midi::enums::{midi_controllers, midi_message_types};
use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
use crate::soundbank::basic_soundbank::preset_resolver::PresetResolver;
use crate::synthesizer::types::SynthSystem;
use crate::synthesizer::audio_engine::engine_components::synth_constants::DEFAULT_PERCUSSION;
use crate::utils::loggin::{spessa_synth_group, spessa_synth_group_end, spessa_synth_info};
use crate::utils::sysex_detector::{is_gm2_on, is_gm_on, is_gs_drums_on, is_gs_on, is_xg_on};

// ─────────────────────────────────────────────────────────────────────────────
// Internal types
// ─────────────────────────────────────────────────────────────────────────────

/// Per-channel state tracked while scanning MIDI events.
/// Equivalent to: InternalChannelType
struct InternalChannelType {
    /// Stable identity of the currently-selected preset (pointer address).
    preset_ptr: Option<usize>,
    /// Name of the preset (stored for logging).
    preset_name: Option<String>,
    bank_msb: u8,
    bank_lsb: u8,
    is_drum: bool,
}

/// GS part-number → MIDI channel mapping.
/// Part 0 → channel 9 (default percussion), parts 1-8 → channels 0-8,
/// parts 9-15 → channels 10-15.
const GS_PART_TO_CHANNEL: [usize; 16] =
    [9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15];

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Result type: maps a preset (by its stable pointer address) to the set of
/// "note-velocity" strings that were played with that preset.
///
/// The `usize` key equals `preset as *const BasicPreset as usize`.  Callers
/// that iterate over a `BasicSoundBank`'s presets can compute the same key to
/// check membership:
///
/// ```rust,ignore
/// let key = preset as *const BasicPreset as usize;
/// if used.contains_key(&key) { /* preset is used */ }
/// ```
pub type UsedPresetKeys = HashMap<usize, HashSet<String>>;

/// Scans `midi` and returns every preset referenced by a note-on event,
/// together with the set of `"note-velocity"` combinations played through it.
///
/// `sound_bank` is queried via the [`PresetResolver`] trait, so this function
/// works with both `BasicSoundBank` and `SoundBankManager` without introducing
/// a circular dependency.
///
/// Equivalent to: getUsedProgramsAndKeys(mid, soundBank)
pub fn get_used_programs_and_keys(
    midi: &BasicMidi,
    sound_bank: &dyn PresetResolver,
) -> UsedPresetKeys {
    spessa_synth_group("Searching for all used programs and keys...");

    // Compute total channel slots needed across all ports.
    let max_offset = midi
        .port_channel_offset_map
        .iter()
        .copied()
        .max()
        .unwrap_or(0) as usize;
    let channels_amount = 16 + max_offset;

    let mut system = SynthSystem::Gs;

    // Initialise per-channel state.
    let mut channel_presets: Vec<InternalChannelType> = (0..channels_amount)
        .map(|i| {
            let is_drum = i % 16 == DEFAULT_PERCUSSION as usize;
            let preset = sound_bank.get_preset(
                MidiPatch {
                    bank_msb: 0,
                    bank_lsb: 0,
                    is_gm_gs_drum: is_drum,
                    program: 0,
                },
                system,
            );
            InternalChannelType {
                preset_ptr: preset.map(|p| p as *const BasicPreset as usize),
                preset_name: preset.map(|p| p.name.clone()),
                bank_msb: 0,
                bank_lsb: 0,
                is_drum,
            }
        })
        .collect();

    // Result: preset_ptr → set of "note-velocity" strings.
    let mut used_programs_and_keys: UsedPresetKeys = HashMap::new();
    // Names stored separately to enable logging without raw-pointer dereferences.
    let mut preset_names: HashMap<usize, String> = HashMap::new();

    // Per-track current port (starts with each track's declared port, updated by
    // MIDI Port meta events).
    let mut ports: Vec<u32> = midi.tracks.iter().map(|t| t.port).collect();

    // Borrow the offset map before calling iterate so the closure can access it
    // without conflicting with the immutable &self borrow inside iterate.
    let port_map = &midi.port_channel_offset_map;

    midi.iterate(|event, track_num| {
        // ── MIDI Port meta event ─────────────────────────────────────────────
        if event.status_byte == midi_message_types::MIDI_PORT {
            if let Some(&port) = event.data.first() {
                ports[track_num] = port as u32;
            }
            return;
        }

        // Only process: note-on, controller change, program change, sysex.
        let status = event.status_byte & 0xF0;
        if status != midi_message_types::NOTE_ON
            && status != midi_message_types::CONTROLLER_CHANGE
            && status != midi_message_types::PROGRAM_CHANGE
            && event.status_byte != midi_message_types::SYSTEM_EXCLUSIVE
        {
            return;
        }

        // Compute the logical channel (MIDI channel + port offset).
        let port_idx = ports[track_num] as usize;
        let offset = port_map.get(port_idx).copied().unwrap_or(0) as usize;
        let ch_in_event = (event.status_byte & 0x0F) as usize;
        let channel = ch_in_event + offset;

        match status {
            // ── Program change ───────────────────────────────────────────────
            s if s == midi_message_types::PROGRAM_CHANGE => {
                if let Some(ch) = channel_presets.get_mut(channel) {
                    let preset = sound_bank.get_preset(
                        MidiPatch {
                            bank_msb: ch.bank_msb,
                            bank_lsb: ch.bank_lsb,
                            program: event.data.first().copied().unwrap_or(0),
                            is_gm_gs_drum: ch.is_drum,
                        },
                        system,
                    );
                    ch.preset_ptr = preset.map(|p| p as *const BasicPreset as usize);
                    ch.preset_name = preset.map(|p| p.name.clone());
                }
            }

            // ── Controller change (bank select only) ─────────────────────────
            s if s == midi_message_types::CONTROLLER_CHANGE => {
                if let Some(ch) = channel_presets.get_mut(channel) {
                    match event.data.first().copied().unwrap_or(0) {
                        cc if cc == midi_controllers::BANK_SELECT_LSB => {
                            ch.bank_lsb = event.data.get(1).copied().unwrap_or(0);
                        }
                        cc if cc == midi_controllers::BANK_SELECT => {
                            ch.bank_msb = event.data.get(1).copied().unwrap_or(0);
                        }
                        _ => {}, // other controllers are irrelevant
                    }
                }
            }

            // ── Note on ──────────────────────────────────────────────────────
            s if s == midi_message_types::NOTE_ON => {
                if event.data.get(1).copied().unwrap_or(0) == 0 {
                    return; // velocity 0 = note off
                }
                if let Some(ch) = channel_presets.get(channel)
                    && let Some(ptr) = ch.preset_ptr
                {
                    let combo = format!(
                        "{}-{}",
                        event.data.first().copied().unwrap_or(0),
                        event.data.get(1).copied().unwrap_or(0)
                    );
                    used_programs_and_keys.entry(ptr).or_default().insert(combo);
                    if let Some(name) = &ch.preset_name {
                        preset_names.entry(ptr).or_insert_with(|| name.clone());
                    }
                }
            }

            // ── System exclusive ─────────────────────────────────────────────
            _ => {
                if event.status_byte != midi_message_types::SYSTEM_EXCLUSIVE {
                    return;
                }

                if !is_gs_drums_on(event) {
                    // Update the active MIDI system mode.
                    if is_xg_on(event) {
                        system = SynthSystem::Xg;
                        spessa_synth_info("XG on detected!");
                    } else if is_gm2_on(event) {
                        system = SynthSystem::Gm2;
                        spessa_synth_info("GM2 on detected!");
                    } else if is_gm_on(event) {
                        system = SynthSystem::Gm;
                        spessa_synth_info("GM on detected!");
                    } else if is_gs_on(event) {
                        system = SynthSystem::Gs;
                        spessa_synth_info("GS on detected!");
                    }
                    return;
                }

                // GS drum-part sysex: update the is_drum flag for the addressed channel.
                let part = (event.data.get(5).copied().unwrap_or(0) & 0x0F) as usize;
                let sysex_channel = GS_PART_TO_CHANNEL[part] + offset;
                let is_drum = event.data.get(7).copied().unwrap_or(0) > 0
                    && (event.data.get(5).copied().unwrap_or(0) >> 4) > 0;
                if let Some(ch) = channel_presets.get_mut(sysex_channel) {
                    ch.is_drum = is_drum;
                }
            }
        }
    });

    // Remove presets that were selected (program change) but never received any
    // note-on events — they don't need to be kept in the sound bank.
    used_programs_and_keys.retain(|ptr, combos| {
        if combos.is_empty() {
            if let Some(name) = preset_names.get(ptr) {
                spessa_synth_info(&format!("Detected change but no keys for {}", name));
            }
            false
        } else {
            true
        }
    });

    spessa_synth_group_end();
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
    use crate::synthesizer::types::SynthSystem;

    // ── Mock PresetResolver ───────────────────────────────────────────────────

    /// Minimal sound bank that always returns the same preset.
    struct OnePresetBank {
        preset: BasicPreset,
    }

    impl PresetResolver for OnePresetBank {
        fn get_preset(&self, _patch: MidiPatch, _system: SynthSystem) -> Option<&BasicPreset> {
            Some(&self.preset)
        }
    }

    /// Sound bank that returns one of two presets based on program number.
    struct TwoPresetBank {
        piano: BasicPreset,
        strings: BasicPreset,
    }

    impl PresetResolver for TwoPresetBank {
        fn get_preset(&self, patch: MidiPatch, _system: SynthSystem) -> Option<&BasicPreset> {
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
        fn get_preset(&self, _patch: MidiPatch, _system: SynthSystem) -> Option<&BasicPreset> {
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
        assert!(result[&key].contains("60-100"));
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
        assert!(combos.contains("60-100"));
        assert!(combos.contains("64-80"));
        assert!(combos.contains("67-90"));
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
        assert!(result[&piano_key].contains("60-100"));
        assert!(result.contains_key(&strings_key), "strings should be used");
        assert!(result[&strings_key].contains("64-80"));
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
                _system: SynthSystem,
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
                _system: SynthSystem,
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
        assert!(combos.contains("60-100"));
        assert!(combos.contains("64-80"));
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
                _system: SynthSystem,
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
        // Set up a MIDI with two ports (offset 0 and 16).
        let bank = OnePresetBank {
            preset: BasicPreset::default(),
        };
        let mut m = BasicMidi::new();
        m.time_division = 480;
        m.port_channel_offset_map = vec![0, 16]; // port 0 → offset 0, port 1 → offset 16

        let mut t = MidiTrack::new();
        t.push_event(ev(0, midi_message_types::MIDI_PORT, vec![1])); // switch to port 1
        t.push_event(ev(10, 0x90, vec![60, 100])); // now on ch0 + offset 16 = ch16
        m.tracks.push(t);

        let result = get_used_programs_and_keys(&m, &bank);
        // Should have an entry (the note was registered under some channel)
        assert!(!result.is_empty());
    }

    // ── XG / GM system detection ──────────────────────────────────────────────

    #[test]
    fn test_xg_on_sysex_detected() {
        struct SystemCapture {
            preset: BasicPreset,
            system: std::cell::Cell<SynthSystem>,
        }
        impl PresetResolver for SystemCapture {
            fn get_preset(
                &self,
                _patch: MidiPatch,
                system: SynthSystem,
            ) -> Option<&BasicPreset> {
                self.system.set(system);
                Some(&self.preset)
            }
        }

        let bank = SystemCapture {
            preset: BasicPreset::default(),
            system: std::cell::Cell::new(SynthSystem::Gs),
        };

        // XG ON sysex: [Yamaha, ?, XG, ?, ?, 0x7e, 0x00]
        let xg_sysex = vec![0x43, 0x10, 0x4c, 0x00, 0x00, 0x7e, 0x00];
        let midi = simple_midi(vec![
            ev(0, midi_message_types::SYSTEM_EXCLUSIVE, xg_sysex),
            ev(10, 0xC0, vec![0]),      // program change triggers get_preset with new system
            ev(20, 0x90, vec![60, 80]),
        ]);

        get_used_programs_and_keys(&midi, &bank);
        assert_eq!(bank.system.get(), SynthSystem::Xg);
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
        let m = BasicMidi::new();
        let result = get_used_programs_and_keys(&m, &bank);
        assert!(result.is_empty());
    }
}
