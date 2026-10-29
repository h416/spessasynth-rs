//! GS User Drum Sets (programs 64 and 65).
//! Ported from: src/synthesizer/audio_engine/user_drum_set.ts (new in 4.4.0)
//!
//! A user drum set maps each MIDI key to a key of another GS drum set
//! (`sourceDrumSet` = bank LSB, `program`, `sourceNoteNumber`).
//! In TS it is a `SynthesizerPatch` that the sound bank manager lists next to the real
//! presets; Rust lists a placeholder `BasicPreset` for it with a sentinel bank index
//! (see `SoundBankManager::user_drum_bank_idx`), and note_on resolves the voices
//! through `SynthesizerCore::get_user_drum_voices`.
use std::cell::Cell;

use crate::midi::drum_parameters::{DrumParameterUtils, UserDrumSetParameter};
use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;

/// The patch used when the bound patch does not resolve to a real drum set.
/// Equivalent to: FALLBACK_PATCH
pub const FALLBACK_PATCH: MidiPatch = MidiPatch {
    bank_msb: 0,
    bank_lsb: 0,
    is_gm_gs_drum: true,
    program: 0,
};

/// Equivalent to: class UserDrumSet
pub struct UserDrumSet {
    pub program: u8,
    pub name: String,
    /// The key parameters for this drum set.
    /// Index is the MIDI key, value are the parameters for this key.
    pub key_params: Vec<UserDrumSetParameter>,
    /// Bumped when changes are committed (program change to this set).
    /// TS purges the cached voices of the patch at that point
    /// (`synthCore.purgeCachedPatch(preset)`); Rust tags the cached voices of a user drum
    /// set with the generation they were built in and rebuilds stale ones, which is
    /// observably the same. A `Cell` because the commit happens in `MidiChannel::program_change`,
    /// which only has shared access to the sound bank manager.
    pub generation: Cell<u32>,
}

impl UserDrumSet {
    /// Creates a new custom drum set.
    pub fn new(program: u8, name: &str) -> Self {
        let mut s = Self {
            program,
            name: name.to_string(),
            key_params: (0..128).map(DrumParameterUtils::default_user_data).collect(),
            generation: Cell::new(0),
        };
        // Correct init
        s.reset();
        s
    }

    /// Resets the drum set.
    pub fn reset(&mut self) {
        // Initialize all 128 keys to the default drum patch
        for i in 0..128 {
            DrumParameterUtils::copy_into_user(
                &DrumParameterUtils::default_user_data(i),
                &mut self.key_params[i],
            );
        }
    }

    /// Gets a snapshot of this User Drum Set instance.
    pub fn get_snapshot(&self) -> Vec<UserDrumSetParameter> {
        self.key_params.clone()
    }

    /// The patch a key is bound to.
    /// Equivalent to: the `tempPatch` setup in getVoiceParameters
    pub fn bound_patch(&self, midi_note: usize) -> MidiPatch {
        let binding = &self.key_params[midi_note];
        MidiPatch {
            bank_lsb: binding.source_drum_set,
            bank_msb: 0,
            program: binding.program,
            is_gm_gs_drum: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_bind_to_sc88_kit_same_key() {
        let s = UserDrumSet::new(64, "User Drum Set 1");
        assert_eq!(s.key_params.len(), 128);
        let p = s.bound_patch(38);
        assert_eq!((p.bank_lsb, p.program, p.is_gm_gs_drum), (2, 0, true));
        assert_eq!(s.key_params[38].source_note_number, 38);
    }

    #[test]
    fn reset_restores_defaults() {
        let mut s = UserDrumSet::new(65, "User Drum Set 2");
        s.key_params[40].drum.level = 1;
        s.key_params[40].source_note_number = 10;
        s.reset();
        assert_eq!(s.key_params[40], DrumParameterUtils::default_user_data(40));
    }
}
