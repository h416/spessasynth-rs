//! program_change.rs
//! purpose: MIDI program change handler for MidiChannel.
//! Ported from: src/synthesizer/audio_engine/engine_methods/program_change.ts

use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
use crate::synthesizer::audio_engine::sound_bank_manager::SoundBankManager;
use crate::synthesizer::audio_engine::channel::midi_channel::{DrumLocks, MidiChannel};
use crate::synthesizer::types::{ChannelPropertyChangeCallback, ProgramChangeCallback, SynthProcessorEvent};
use crate::soundbank::types::MIDISystem;
use crate::midi::drum_parameters::DrumParameterUtils;
use crate::synthesizer::audio_engine::synth_constants::{GS_USER_DRUM_1, GS_USER_DRUM_2};
use crate::utils::loggin::{SpessaLog, spessa_synth_info};

impl MidiChannel {
    /// Changes the program (preset) of this channel.
    ///
    /// Looks up the preset from the sound bank manager using the channel's current patch
    /// (bank MSB/LSB + new program number). Fires a `ProgramChange` event and a
    /// `ChannelPropertyChange` event if the event system is enabled.
    ///
    /// If no preset is found, returns an empty event list (no-op).
    ///
    /// Equivalent to: programChange(program: number)
    pub fn program_change(
        &mut self,
        program: u8,
        sound_bank_manager: &SoundBankManager,
        current_system: MIDISystem,
        locks: DrumLocks,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        if self.system_parameters.preset_lock {
            return Vec::new();
        }

        self.patch.program = program;
        let channel_system = self.channel_system(current_system);

        let result = sound_bank_manager.get_preset_and_bank_idx(self.patch, channel_system);
        let Some((preset, bank_idx)) = result else {
            return Vec::new();
        };

        let is_drum = sound_bank_manager.is_drum(preset, bank_idx);
        let preset_clone = preset.clone();
        self.preset = Some(preset_clone);
        self.preset_bank_idx = Some(bank_idx);

        let mut events = Vec::new();

        // Drums first
        // SC resets drum params on program change
        if is_drum != self.drum_channel {
            self.set_drum_flag(is_drum);
        }

        self.reset_drum_params(locks.drum, current_system);

        // Commit changes made to user drums.
        // SCVA does not play drum sounds until the change is sent, even if this patch was selected before then.
        // See the corresponding test in MIDI tests.
        let preset_ref = self.preset.as_ref().unwrap();
        if preset_ref.is_gm_gs_drum
            && (preset_ref.program == GS_USER_DRUM_1 || preset_ref.program == GS_USER_DRUM_2)
            && !locks.user_drum
        {
            spessa_synth_info(&format!(
                "Committing changes to User Drum Set {}!",
                preset_ref.program as i32 - 63
            ));
            match SoundBankManager::user_drum_set_of(bank_idx) {
                Some(drum_set) => {
                    let set = &sound_bank_manager.user_drum_sets[drum_set];
                    // Purge cache for this preset to cache the new drum voice data
                    set.generation.set(set.generation.get().wrapping_add(1));
                    // Copy drum param data
                    for (i, binding) in set.key_params.iter().enumerate() {
                        DrumParameterUtils::copy_into(&binding.drum, &mut self.drum_params[i]);
                        // SC-55 uses 100 cents, SC-88 and above is 50
                        // Refer to source binding and convert the copy to semitones
                        self.drum_params[i].pitch_coarse *=
                            if binding.source_drum_set == 1 { 1.0 } else { 0.5 };
                    }
                }
                None => SpessaLog::warn(&format!(
                    "Current patch should be GS User Drum! Instead found {}.",
                    preset_ref.name
                )),
            }
        }

        // Fire program change event
        let preset_ref = self.preset.as_ref().unwrap();
        events.push(SynthProcessorEvent::ProgramChange(ProgramChangeCallback {
            patch: MidiPatch {
                program: preset_ref.program,
                bank_msb: preset_ref.bank_msb,
                bank_lsb: preset_ref.bank_lsb,
                is_gm_gs_drum: preset_ref.is_gm_gs_drum,
            },
            channel: self.channel,
        }));

        // Fire channel property event
        if let Some(ev) = self.build_channel_property_event(enable_event_system) {
            events.push(ev);
        }

        events
    }
}
