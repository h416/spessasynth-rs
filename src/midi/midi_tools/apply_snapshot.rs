/// apply_snapshot.rs
/// purpose: Modifies a MIDI sequence according to the locked presets and controllers in a
///          SynthesizerSnapshot, by translating the snapshot into `ModifyMidiOptions` and
///          delegating to `modify_midi_internal`.
/// Ported from: src/midi/midi_tools/apply_snapshot.ts (spessasynth_core 4.3.0)
/// (split out of `midi_editor.ts`'s `applySnapshotInternal` in 4.2.0)
///
/// # Snapshot type bridge
///
/// TS 4.3.0's `SynthesizerSnapshot` is fully ported (`system_parameters: GlobalSystemParameter`,
/// `midi_parameters: GlobalMIDIParameter`, effect processor snapshots stored as
/// `reverb_processor`/`chorus_processor`/`delay_processor`/`insertion_processor`), so the
/// snapshot-level reads here match TS directly:
///
/// - `snapshot.systemParameters.keyShift`/`fineTune` (global) -> `system_parameters.key_shift`
///   (semitones) / `system_parameters.fine_tune` (cents)
/// - `snapshot.systemParameters.drumLock`/`reverbLock`/`chorusLock`/`delayLock` -> the matching
///   `system_parameters.*_lock` flags, wired through to `ModifyMidiOptions`
/// - `snapshot.reverbProcessor`/`chorusProcessor`/`delayProcessor` -> the matching
///   `reverb_processor`/`chorus_processor`/`delay_processor` snapshots
///
/// `ChannelSnapshot` is still 4.2.0-shaped (flat fields, not the 4.3.0 `systemParameters`
/// sub-struct — that restructuring lands in Task 21), so the per-channel reads use the bridge:
///
/// - `channelSnapshot.systemParameters.isMuted` -> `ChannelSnapshot::is_muted`
/// - `channelSnapshot.systemParameters.presetLock` -> `ChannelSnapshot::lock_preset`
/// - `channelSnapshot.systemParameters.keyShift` (semitones) ->
///   `ChannelSnapshot::channel_transpose_key_shift`
/// - `channelSnapshot.systemParameters.fineTune` (cents) ->
///   `ChannelSnapshot::custom_controllers[CHANNEL_TRANSPOSE_FINE]`
///
/// The one remaining gap is `insertionParams`: TS passes `snapshot.insertionProcessor`
/// (`InsertionProcessorSnapshot`), but `ModifyMidiOptions.insertion_params` expects
/// `InsertionEffectParams`. Unifying those two effect types is deferred to Task 26 (effects);
/// it is not WAV-relevant, since the render path never applies snapshots.
use std::collections::HashMap;

use crate::midi::basic_midi::BasicMidi;
use crate::midi::enums::midi_controllers;
use crate::midi::midi_tools::modify_midi::{
    modify_midi_internal, ChannelModification, ClearableParameter, ModifyMidiOptions,
};
use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
use crate::synthesizer::audio_engine::channel::parameters::midi::CONTROLLER_TABLE_SIZE;
use crate::synthesizer::audio_engine::synthesizer_snapshot::SynthesizerSnapshot;
use crate::synthesizer::enums::custom_controllers;

/// Modifies the sequence according to the locked presets and controllers in the given snapshot.
/// Note that this ignores the MIDI parameters and only applies system parameter tuning.
///
/// Equivalent to: applySnapshotInternal(midi, snapshot)
pub fn apply_snapshot_internal(midi: &mut BasicMidi, snapshot: &SynthesizerSnapshot) {
    let mut channels: HashMap<u8, ClearableParameter<ChannelModification>> = HashMap::new();

    // TS 4.3.0: global key shift / fine tune come from the system parameters
    // (keyShift in semitones, fineTune in cents).
    let global_key_shift = snapshot.system_parameters.key_shift;
    let global_fine_tune = snapshot.system_parameters.fine_tune;

    for (channel_number, channel_snapshot) in snapshot.midi_channels.iter().enumerate() {
        if channel_snapshot.is_muted {
            channels.insert(channel_number as u8, ClearableParameter::Clear);
            continue;
        }

        let drum = channel_snapshot.drum_channel;
        let key_shift = channel_snapshot.channel_transpose_key_shift as f64
            + if drum { 0.0 } else { global_key_shift };
        let fine_tune = channel_snapshot.custom_controllers
            [custom_controllers::CHANNEL_TRANSPOSE_FINE as usize] as f64
            + if drum { 0.0 } else { global_fine_tune };

        let patch: Option<ClearableParameter<MidiPatch>> = if channel_snapshot.lock_preset {
            Some(ClearableParameter::Value(channel_snapshot.patch.patch))
        } else {
            None
        };

        let mut controllers: Vec<(u8, ClearableParameter<u8>)> = Vec::new();
        for cc_number in 0..CONTROLLER_TABLE_SIZE {
            if !channel_snapshot
                .locked_controllers
                .get(cc_number)
                .copied()
                .unwrap_or(false)
                || cc_number as u8 == midi_controllers::BANK_SELECT
                // TS 4.3.0 puts >127 entries in the map too, but they can never match a real
                // MIDI CC event (data bytes are 7-bit); Rust's `u8` controller key makes the
                // impossibility explicit by filtering them here.
                || cc_number > 127
            {
                continue;
            }
            // Channel controllers are stored as 14-bit values.
            let target_value = (channel_snapshot.midi_controllers[cc_number] >> 7) as u8;
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

    // TS 4.3.0: locked GS effect parameters are baked into the sequence when their
    // corresponding `*Lock` system parameter is set.
    let sys = &snapshot.system_parameters;
    let reverb_params = if sys.reverb_lock {
        Some(ClearableParameter::Value(snapshot.reverb_processor.clone()))
    } else {
        None
    };
    let chorus_params = if sys.chorus_lock {
        Some(ClearableParameter::Value(snapshot.chorus_processor.clone()))
    } else {
        None
    };
    let delay_params = if sys.delay_lock {
        Some(ClearableParameter::Value(snapshot.delay_processor.clone()))
    } else {
        None
    };

    modify_midi_internal(
        midi,
        &ModifyMidiOptions {
            channels: Some(channels),
            drum_setup_params_clear: sys.drum_lock,
            reverb_params,
            chorus_params,
            delay_params,
            // TODO(Task 26, effects): TS passes `snapshot.insertionProcessor`
            // (InsertionProcessorSnapshot) here, but `ModifyMidiOptions.insertion_params`
            // expects `InsertionEffectParams`. Wiring this requires unifying those two
            // effect types (not WAV-relevant: the render path never applies snapshots).
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
        ch(&mut snapshot, 0).is_muted = true;

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
        ch(&mut snapshot, 0).channel_transpose_key_shift = 2;

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
            c.lock_preset = true;
            c.patch.patch = MidiPatch {
                program: 25,
                bank_msb: 0,
                bank_lsb: 0,
                is_gm_gs_drum: false,
            };
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
