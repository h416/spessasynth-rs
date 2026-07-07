/// preset_resolver.rs
/// purpose: Trait for types that can resolve a MIDI patch to a BasicPreset.
///
/// This trait breaks the circular dependency between `basic_midi.rs` and
/// `basic_soundbank.rs`:
///
/// - `BasicMIDI::get_used_programs_and_keys` accepts `&dyn PresetResolver`
///   instead of `&BasicSoundBank`, so `basic_midi.rs` no longer depends on
///   `basic_soundbank.rs`.
/// - `BasicSoundBank::trim` (TS 4.3.0's rework of `trimSoundBank`) takes an
///   already-computed `PresetsWithKeyCombinations` instead of a `&BasicMIDI`, so
///   `basic_soundbank.rs` still doesn't need to depend on `basic_midi.rs`; the free function
///   `get_used_programs_and_keys` (in `midi_tools::used_programs_and_keys`) is the one that
///   depends on this trait to stay decoupled from `basic_soundbank.rs`.
///
/// Implementors:
/// - `BasicSoundBank`    (implemented when `basic_soundbank.rs` is ported)
/// - `SoundBankManager`  (implemented when `sound_bank_manager.rs` is ported)
use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
use crate::soundbank::types::MIDISystem;

/// Resolves a MIDI patch (program + bank + system) to the most appropriate
/// `BasicPreset` in a sound bank.
///
/// The resolution uses a fallback chain: exact match → relaxed bank match →
/// program-only match → first preset. See `select_patch` in
/// `midi_patch.rs` for the full algorithm.
///
/// Returns `None` only when the implementor has no presets at all.
pub trait PresetResolver {
    fn get_preset(&self, patch: MidiPatch, system: MIDISystem) -> Option<&BasicPreset>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
    use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
    use crate::soundbank::types::MIDISystem;

    // Minimal implementor for testing the trait object machinery.
    struct MockBank {
        preset: BasicPreset,
    }

    impl PresetResolver for MockBank {
        fn get_preset(&self, _patch: MidiPatch, _system: MIDISystem) -> Option<&BasicPreset> {
            Some(&self.preset)
        }
    }

    struct EmptyBank;

    impl PresetResolver for EmptyBank {
        fn get_preset(&self, _patch: MidiPatch, _system: MIDISystem) -> Option<&BasicPreset> {
            None
        }
    }

    fn any_patch() -> MidiPatch {
        MidiPatch {
            program: 0,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        }
    }

    #[test]
    fn test_mock_bank_returns_some() {
        let bank = MockBank {
            preset: BasicPreset::default(),
        };
        assert!(bank.get_preset(any_patch(), MIDISystem::Gs).is_some());
    }

    #[test]
    fn test_empty_bank_returns_none() {
        let bank = EmptyBank;
        assert!(bank.get_preset(any_patch(), MIDISystem::Gs).is_none());
    }

    #[test]
    fn test_trait_object_dyn_dispatch() {
        let bank: Box<dyn PresetResolver> = Box::new(MockBank {
            preset: BasicPreset::default(),
        });
        assert!(bank.get_preset(any_patch(), MIDISystem::Gs).is_some());
    }

    #[test]
    fn test_trait_object_empty_returns_none() {
        let bank: Box<dyn PresetResolver> = Box::new(EmptyBank);
        assert!(bank.get_preset(any_patch(), MIDISystem::Gs).is_none());
    }

    #[test]
    fn test_drum_patch_forwarded() {
        struct DrumChecker {
            preset: BasicPreset,
        }
        impl PresetResolver for DrumChecker {
            fn get_preset(&self, patch: MidiPatch, _system: MIDISystem) -> Option<&BasicPreset> {
                if patch.is_gm_gs_drum {
                    Some(&self.preset)
                } else {
                    None
                }
            }
        }

        let bank = DrumChecker {
            preset: BasicPreset::default(),
        };
        let drum = MidiPatch {
            is_gm_gs_drum: true,
            ..any_patch()
        };
        let melodic = MidiPatch {
            is_gm_gs_drum: false,
            ..any_patch()
        };
        assert!(bank.get_preset(drum, MIDISystem::Gs).is_some());
        assert!(bank.get_preset(melodic, MIDISystem::Gs).is_none());
    }

    #[test]
    fn test_system_forwarded() {
        struct SystemChecker {
            preset: BasicPreset,
        }
        impl PresetResolver for SystemChecker {
            fn get_preset(&self, _patch: MidiPatch, system: MIDISystem) -> Option<&BasicPreset> {
                if system == MIDISystem::Xg {
                    Some(&self.preset)
                } else {
                    None
                }
            }
        }

        let bank = SystemChecker {
            preset: BasicPreset::default(),
        };
        assert!(bank.get_preset(any_patch(), MIDISystem::Xg).is_some());
        assert!(bank.get_preset(any_patch(), MIDISystem::Gs).is_none());
    }
}
