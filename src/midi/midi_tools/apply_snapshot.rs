//! apply_snapshot.rs
//! purpose: Modifies a MIDI sequence according to the locked presets and controllers in a
//!          SynthesizerSnapshot, by translating the snapshot into `ModifyMidiOptions` and
//!          delegating to `modify_midi_internal`.
//! Ported from: src/midi/midi_tools/apply_snapshot.ts
//!
//! The snapshot side follows the current TS. The MIDI editor it hands the changes to
//! (`modify_midi.rs`) is the TS 4.3.0 `modifyMIDIInternal`, not the current `MIDIEditor`
//! (midi_editor.ts, outside the MIDI-to-WAV scope), so only the changes that editor accepts
//! are passed on.
//! Not ported (MIDI editing only, no effect on the MIDI-to-WAV output): the per-channel
//! `midiParams` (locked Channel MIDI Parameters), the global `midiParams`, `userDrumParams`,
//! `gsInsertionParams` and the `xg*Params` blocks of `ModifyMIDIOptions`.
use std::collections::HashMap;

use crate::midi::basic_midi::BasicMidi;
use crate::midi::enums::midi_controllers;
use crate::midi::midi_tools::modify_midi::{
    modify_midi_internal, ChannelModification, ClearableParameter, ModifyMidiOptions,
};
use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
use crate::synthesizer::audio_engine::synth_constants::CONTROLLER_TABLE_SIZE;
use crate::synthesizer::audio_engine::synthesizer_snapshot::SynthesizerSnapshot;

/// Modifies the sequence *in-place* according to the locked presets and controllers in the
/// given snapshot.
///
/// Note that System Parameters `fineTune` and `keyShift` are passed to the relative tuning
/// parameters of the channels.
/// Only locked MIDI parameters and controllers are applied.
///
/// Equivalent to: applySnapshotInternal(midi, snapshot)
pub fn apply_snapshot_internal(midi: &mut BasicMidi, snapshot: &SynthesizerSnapshot) {
    let mut channels: HashMap<u8, ClearableParameter<ChannelModification>> = HashMap::new();
    let global_key_shift = snapshot.system_parameters.key_shift;
    let global_fine_tune = snapshot.system_parameters.fine_tune;
    for (channel_number, channel_snapshot) in snapshot.midi_channels.iter().enumerate() {
        if channel_snapshot.system_parameters.is_muted {
            channels.insert(channel_number as u8, ClearableParameter::Clear);
            continue;
        }
        let drum = channel_snapshot.drum_channel;
        let key_shift =
            channel_snapshot.system_parameters.key_shift + if drum { 0.0 } else { global_key_shift };
        let fine_tune =
            channel_snapshot.system_parameters.fine_tune + if drum { 0.0 } else { global_fine_tune };
        let mut patch: Option<ClearableParameter<MidiPatch>> = None;
        if channel_snapshot.system_parameters.preset_lock
            && let Some(p) = &channel_snapshot.patch
        {
            patch = Some(ClearableParameter::Value(p.patch));
        }

        let mut controllers: Vec<(u8, ClearableParameter<u8>)> = Vec::new();
        for cc_number in 0..CONTROLLER_TABLE_SIZE {
            if !channel_snapshot.locked_controllers[cc_number]
                || cc_number as u8 == midi_controllers::BANK_SELECT
            {
                continue;
            }
            let target_value = (channel_snapshot.midi_controllers[cc_number] >> 7) as u8; // Channel controllers are stored as 14 bit values
            controllers.push((cc_number as u8, ClearableParameter::Value(target_value)));
        }

        channels.insert(
            channel_number as u8,
            ClearableParameter::Value(ChannelModification {
                key_shift,
                fine_tune,
                patch,
                controllers: Some(controllers),
            }),
        );
    }

    let sys = &snapshot.system_parameters;
    modify_midi_internal(
        midi,
        &ModifyMidiOptions {
            channels: Some(channels),
            // Drum lock clears all drum edits for every channel
            drum_setup_params_clear: sys.drum_lock,
            reverb_params: sys
                .gs_reverb_lock
                .then(|| ClearableParameter::Value(snapshot.gs_reverb_processor.clone())),
            chorus_params: sys
                .gs_chorus_lock
                .then(|| ClearableParameter::Value(snapshot.gs_chorus_processor.clone())),
            delay_params: sys
                .gs_delay_lock
                .then(|| ClearableParameter::Value(snapshot.gs_delay_processor.clone())),
            // Not ported (see the module doc)
            insertion_params: None,
        },
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::enums::midi_message_types;
    use crate::midi::midi_message::MidiMessage;
    use crate::midi::midi_track::MidiTrack;
    use crate::soundbank::basic_soundbank::midi_patch::MidiPatchFull;
    use crate::synthesizer::audio_engine::channel::channel_snapshot::ChannelSnapshot;
    use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
    use crate::synthesizer::audio_engine::synthesizer_snapshot::get_synthesizer_snapshot;
    use crate::synthesizer::types::{SynthProcessorEvent, SynthProcessorOptions};
    use std::sync::{Arc, Mutex};

    fn make_core_with_channels(n: usize) -> SynthesizerCore {
        let events: Arc<Mutex<Vec<SynthProcessorEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let ev_clone = Arc::clone(&events);
        let mut core = SynthesizerCore::new(
            move |ev| {
                ev_clone.lock().unwrap().push(ev);
            },
            44100.0,
            SynthProcessorOptions {
                events_enabled: true,
                ..Default::default()
            },
        );
        for _ in 0..n {
            core.create_midi_channel(false);
        }
        core
    }

    fn make_snapshot(channels: usize) -> SynthesizerSnapshot {
        let core = make_core_with_channels(channels);
        get_synthesizer_snapshot(&core)
    }

    fn make_msg(ticks: u32, status: u8, data: Vec<u8>) -> MidiMessage {
        MidiMessage::new(ticks, status, data)
    }

    fn simple_midi(events: Vec<MidiMessage>) -> BasicMidi {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        for e in &events {
            if e.status_byte >= 0x80 && e.status_byte < 0xF0 {
                t.channels.insert(e.status_byte & 0x0F);
            }
        }
        for e in events {
            t.push_event(e);
        }
        m.tracks.push(t);
        m
    }

    fn ch(snapshot: &mut SynthesizerSnapshot, i: usize) -> &mut ChannelSnapshot {
        &mut snapshot.midi_channels[i]
    }

    #[test]
    fn test_muted_channel_cleared() {
        let mut snapshot = make_snapshot(1);
        ch(&mut snapshot, 0).system_parameters.is_muted = true;

        let mut midi = simple_midi(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(100, 0x80, vec![60, 0]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        apply_snapshot_internal(&mut midi, &snapshot);

        let voice_events: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte >= 0x80 && e.status_byte < 0xF0)
            .collect();
        assert!(voice_events.is_empty());
    }

    #[test]
    fn test_channel_key_shift_transposes_notes() {
        let mut snapshot = make_snapshot(1);
        ch(&mut snapshot, 0).system_parameters.key_shift = 2.0;

        let mut midi = simple_midi(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(100, 0x80, vec![60, 0]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        apply_snapshot_internal(&mut midi, &snapshot);

        let note_on = midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte & 0xF0 == midi_message_types::NOTE_ON)
            .unwrap();
        assert_eq!(note_on.data[0], 62);
    }

    #[test]
    fn test_locked_controller_inserted_before_first_note() {
        let mut snapshot = make_snapshot(1);
        {
            let c = ch(&mut snapshot, 0);
            c.locked_controllers[7] = true;
            c.midi_controllers[7] = 100 << 7; // 14-bit storage
        }

        let mut midi = simple_midi(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        apply_snapshot_internal(&mut midi, &snapshot);

        let cc7 = midi.tracks[0].events.iter().find(|e| {
            e.status_byte & 0xF0 == midi_message_types::CONTROLLER_CHANGE
                && e.data.len() >= 2
                && e.data[0] == 7
        });
        assert!(cc7.is_some());
        assert_eq!(cc7.unwrap().data[1], 100);
    }

    #[test]
    fn test_bank_select_lock_ignored() {
        // Locking CC 0 (bank select) must NOT generate a controller change.
        let mut snapshot = make_snapshot(1);
        {
            let c = ch(&mut snapshot, 0);
            c.locked_controllers[0] = true;
            c.midi_controllers[0] = 5 << 7;
        }

        let mut midi = simple_midi(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        apply_snapshot_internal(&mut midi, &snapshot);

        let bank_cc = midi.tracks[0].events.iter().find(|e| {
            e.status_byte & 0xF0 == midi_message_types::CONTROLLER_CHANGE
                && e.data.len() >= 2
                && e.data[0] == 0
        });
        assert!(bank_cc.is_none());
    }

    #[test]
    fn test_locked_preset_adds_program_change() {
        let mut snapshot = make_snapshot(1);
        {
            let c = ch(&mut snapshot, 0);
            c.system_parameters.preset_lock = true;
            c.patch = Some(MidiPatchFull {
                patch: MidiPatch {
                    program: 25,
                    bank_msb: 0,
                    bank_lsb: 0,
                    is_gm_gs_drum: false,
                },
                name: String::new(),
                is_drum: false,
            });
        }

        let mut midi = simple_midi(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        apply_snapshot_internal(&mut midi, &snapshot);

        let pc = midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte & 0xF0 == midi_message_types::PROGRAM_CHANGE);
        assert!(pc.is_some());
        assert_eq!(pc.unwrap().data[0], 25);
    }

    #[test]
    fn test_global_transposition_skipped_on_drum_channel() {
        let mut snapshot = make_snapshot(10);
        snapshot.system_parameters.key_shift = 2.0;
        ch(&mut snapshot, 9).drum_channel = true;

        let mut midi = simple_midi(vec![
            make_msg(0, 0x99, vec![38, 100]), // drum note ch9
            make_msg(0, 0x90, vec![60, 100]), // melodic note ch0
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        apply_snapshot_internal(&mut midi, &snapshot);

        let drum_note = midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == 0x99)
            .unwrap();
        assert_eq!(drum_note.data[0], 38); // unshifted

        let melodic_note = midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == 0x90)
            .unwrap();
        assert_eq!(melodic_note.data[0], 62); // +2 semitones
    }

    #[test]
    fn test_empty_snapshot_no_crash() {
        let snapshot = make_snapshot(0);
        let mut midi = simple_midi(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        apply_snapshot_internal(&mut midi, &snapshot);
        assert!(!midi.tracks.is_empty());
    }
}
