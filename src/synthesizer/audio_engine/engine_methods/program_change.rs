/// program_change.rs
/// purpose: MIDI program change handler for MidiChannel.
/// Ported from: src/synthesizer/audio_engine/engine_methods/program_change.ts
use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
use crate::synthesizer::audio_engine::engine_components::sound_bank_manager::SoundBankManager;
use crate::synthesizer::audio_engine::synthesizer_core::MidiChannel;
use crate::synthesizer::types::{
    ChannelPropertyChangeCallback, ProgramChangeCallback, SynthProcessorEvent, SynthSystem,
};

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
        current_system: SynthSystem,
        enable_event_system: bool,
    ) -> Vec<SynthProcessorEvent> {
        if self.lock_preset {
            return Vec::new();
        }

        self.patch.program = program;
        let channel_system = self.channel_system(current_system);

        let result = sound_bank_manager.get_preset_and_bank_idx(self.patch, channel_system);
        let Some((preset, bank_idx)) = result else {
            return Vec::new();
        };

        let bank = &sound_bank_manager.sound_bank_list[bank_idx].sound_bank;
        let is_any_drums = preset.is_any_drums(bank.is_xg_bank());
        let preset_clone = preset.clone();
        self.preset = Some(preset_clone);
        self.preset_bank_idx = Some(bank_idx);

        let mut events = Vec::new();

        // Update drum flag if it changed
        if is_any_drums != self.drum_channel
            && let Some(ev) = self.set_drum_flag(is_any_drums)
        {
            events.push(ev);
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
