/// midi_editor.rs
/// purpose: MIDI sequence editing utilities (program changes, controller changes,
///          channel clearing, transposition, and snapshot application).
/// Ported from: src/midi/midi_tools/midi_editor.ts
use std::collections::{HashMap, HashSet};

use crate::midi::basic_midi::BasicMidi;
use crate::midi::enums::{midi_controllers, midi_message_types};
use crate::midi::midi_message::MidiMessage;
use crate::midi::midi_tools::get_gs_on::get_gs_on;
use crate::midi::types::{DesiredChannelTranspose, DesiredControllerChange, DesiredProgramChange};
use crate::soundbank::basic_soundbank::midi_patch;
use crate::synthesizer::audio_engine::engine_components::synth_constants::DEFAULT_PERCUSSION;
use crate::synthesizer::audio_engine::snapshot::synthesizer_snapshot::SynthesizerSnapshot;
use crate::synthesizer::enums::custom_controllers;
use crate::synthesizer::types::SynthSystem;
use crate::utils::loggin::{spessa_synth_group_collapsed, spessa_synth_group_end, spessa_synth_info};
use crate::utils::midi_hacks::BankSelectHacks;
use crate::utils::sysex_detector::{is_gm2_on, is_gm_on, is_gs_on, is_xg_on};

/// Creates a controller change MIDI message.
/// Equivalent to: getControllerChange(channel, cc, value, ticks)
fn get_controller_change(channel: u8, cc: u8, value: u8, ticks: u32) -> MidiMessage {
    MidiMessage::new(
        ticks,
        midi_message_types::CONTROLLER_CHANGE | (channel % 16),
        vec![cc, value],
    )
}

/// Creates a GS drum change SysEx message.
/// Equivalent to: getDrumChange(channel, ticks)
fn get_drum_change(channel: u8, ticks: u32) -> MidiMessage {
    let chan_address = 0x10
        | [1u8, 2, 3, 4, 5, 6, 7, 8, 0, 9, 10, 11, 12, 13, 14, 15][(channel % 16) as usize];
    // Excluding manufacturerID DeviceID and ModelID (and F7)
    let sysex_data: Vec<u8> = vec![
        0x41, // Roland
        0x10, // Device ID (defaults to 16 on Roland)
        0x42, // GS
        0x12, // Command ID (DT1)
        0x40, // System parameter   } Address
        chan_address, // Channel parameter } Address
        0x15, // Drum change        } Address
        0x01, // Is Drums           } Data
    ];
    // Calculate checksum
    // https://cdn.roland.com/assets/media/pdf/F-20_MIDI_Imple_e01_W.pdf section 4
    let sum = 0x40u16 + chan_address as u16 + 0x15 + 0x01;
    let checksum = (128 - (sum % 128)) as u8;
    let mut data = sysex_data;
    data.push(checksum);
    data.push(0xf7);
    MidiMessage::new(ticks, midi_message_types::SYSTEM_EXCLUSIVE, data)
}

/// Allows easy editing of the file by removing channels, changing programs,
/// changing controllers and transposing channels. Note that this modifies the MIDI in-place.
///
/// Equivalent to: modifyMIDIInternal(midi, desiredProgramChanges, desiredControllerChanges,
///                                    desiredChannelsToClear, desiredChannelsToTranspose)
pub fn modify_midi_internal(
    midi: &mut BasicMidi,
    desired_program_changes: &[DesiredProgramChange],
    desired_controller_changes: &[DesiredControllerChange],
    desired_channels_to_clear: &[u8],
    desired_channels_to_transpose: &[DesiredChannelTranspose],
) {
    spessa_synth_group_collapsed("Applying changes to the MIDI file...");

    spessa_synth_info(&format!(
        "Desired program changes: {:?}",
        desired_program_changes
    ));
    spessa_synth_info(&format!(
        "Desired CC changes: {:?}",
        desired_controller_changes
    ));
    spessa_synth_info(&format!(
        "Desired channels to clear: {:?}",
        desired_channels_to_clear
    ));
    spessa_synth_info(&format!(
        "Desired channels to transpose: {:?}",
        desired_channels_to_transpose
    ));

    let channels_to_change_program: HashSet<u8> =
        desired_program_changes.iter().map(|c| c.channel).collect();

    // Go through all events one by one
    let mut system: SynthSystem = SynthSystem::Gs;
    let mut added_gs = false;

    // It copies midiPorts everywhere else, but here 0 works so DO NOT CHANGE!
    // Midi port number for the corresponding track
    let mut midi_ports: Vec<u32> = midi.tracks.iter().map(|t| t.port).collect();
    // Midi port: channel offset
    let mut midi_port_channel_offsets: HashMap<u32, u32> = HashMap::new();
    let mut midi_port_channel_offset: u32 = 0;

    let assign_midi_port = |track_num: usize,
                            port: u32,
                            midi_ports: &mut Vec<u32>,
                            midi_port_channel_offsets: &mut HashMap<u32, u32>,
                            midi_port_channel_offset: &mut u32,
                            tracks: &[crate::midi::midi_track::MidiTrack]| {
        // Do not assign ports to empty tracks
        if tracks[track_num].channels.is_empty() {
            return;
        }

        // Assign new 16 channels if the port is not occupied yet
        if *midi_port_channel_offset == 0 {
            *midi_port_channel_offset += 16;
            midi_port_channel_offsets.insert(port, 0);
        }

        if let std::collections::hash_map::Entry::Vacant(e) = midi_port_channel_offsets.entry(port) {
            e.insert(*midi_port_channel_offset);
            *midi_port_channel_offset += 16;
        }

        midi_ports[track_num] = port;
    };

    // Assign port offsets
    let ports_snapshot: Vec<u32> = midi.tracks.iter().map(|t| t.port).collect();
    for (i, port) in ports_snapshot.iter().enumerate() {
        assign_midi_port(
            i,
            *port,
            &mut midi_ports,
            &mut midi_port_channel_offsets,
            &mut midi_port_channel_offset,
            &midi.tracks,
        );
    }

    let channels_amount = midi_port_channel_offset as usize;
    // Tracks if the channel already had its first note on
    let mut is_first_note_on = vec![true; channels_amount];
    // MIDI key transpose
    let mut coarse_transpose = vec![0i32; channels_amount];
    // RPN fine transpose
    let mut fine_transpose = vec![0.0f64; channels_amount];
    for transpose in desired_channels_to_transpose {
        let ch = transpose.channel as usize;
        if ch < channels_amount {
            let coarse = transpose.key_shift.trunc() as i32;
            let fine = transpose.key_shift - coarse as f64;
            coarse_transpose[ch] = coarse;
            fine_transpose[ch] = fine;
        }
    }

    // Iterate events in chronological order across all tracks
    let num_tracks = midi.tracks.len();
    let mut event_indexes = vec![0usize; num_tracks];
    let mut remaining_tracks = num_tracks;

    while remaining_tracks > 0 {
        // Find the track whose next event has the smallest tick
        let mut min_ticks = u32::MAX;
        let mut track_num = 0;
        for (i, track) in midi.tracks.iter().enumerate() {
            if event_indexes[i] >= track.events.len() {
                continue;
            }
            let tick = track.events[event_indexes[i]].ticks;
            if tick < min_ticks {
                track_num = i;
                min_ticks = tick;
            }
        }

        // If selected track is exhausted, count it as done
        if event_indexes[track_num] >= midi.tracks[track_num].events.len() {
            remaining_tracks -= 1;
            continue;
        }

        let index = event_indexes[track_num];
        let e_ticks = midi.tracks[track_num].events[index].ticks;
        let e_status_byte = midi.tracks[track_num].events[index].status_byte;
        let e_data = midi.tracks[track_num].events[index].data.clone();

        let port_offset = midi_port_channel_offsets
            .get(&midi_ports[track_num])
            .copied()
            .unwrap_or(0);

        if e_status_byte == midi_message_types::MIDI_PORT {
            if !e_data.is_empty() {
                assign_midi_port(
                    track_num,
                    e_data[0] as u32,
                    &mut midi_ports,
                    &mut midi_port_channel_offsets,
                    &mut midi_port_channel_offset,
                    &midi.tracks,
                );
            }
            event_indexes[track_num] += 1;
            continue;
        }

        // Don't clear meta events
        if (midi_message_types::SEQUENCE_NUMBER..=midi_message_types::SEQUENCE_SPECIFIC)
            .contains(&e_status_byte)
        {
            event_indexes[track_num] += 1;
            continue;
        }

        let status = e_status_byte & 0xF0;
        let midi_channel = e_status_byte & 0x0F;
        let channel = midi_channel as u32 + port_offset;
        let channel_u8 = channel as u8;

        // Clear channel?
        if desired_channels_to_clear.contains(&channel_u8) {
            midi.tracks[track_num].delete_event(index);
            // Don't increment index (next event slides into this position)
            continue;
        }

        match status {
            s if s == midi_message_types::NOTE_ON => {
                // Is it first?
                if (channel as usize) < channels_amount && is_first_note_on[channel as usize] {
                    is_first_note_on[channel as usize] = false;

                    // Add controllers first (because of insertion order, they end up before program changes)
                    let mut insert_offset = 0usize;
                    for change in desired_controller_changes
                        .iter()
                        .filter(|c| c.channel == channel_u8)
                    {
                        let cc_change = get_controller_change(
                            midi_channel,
                            change.controller_number,
                            change.controller_value,
                            e_ticks,
                        );
                        midi.tracks[track_num].add_event(cc_change, index + insert_offset);
                        event_indexes[track_num] += 1;
                        insert_offset += 1;
                    }

                    if (channel as usize) < channels_amount {
                        let fine_tune = fine_transpose[channel as usize];

                        if fine_tune != 0.0 {
                            // Add RPN for fine tuning
                            // 64 is the center, 96 = 50 cents up
                            let cents_coarse = (fine_tune * 64.0 + 64.0) as u8;
                            let rpn_coarse = get_controller_change(
                                midi_channel,
                                midi_controllers::REGISTERED_PARAMETER_MSB,
                                0,
                                e_ticks,
                            );
                            let rpn_fine = get_controller_change(
                                midi_channel,
                                midi_controllers::REGISTERED_PARAMETER_LSB,
                                1,
                                e_ticks,
                            );
                            let data_entry_coarse = get_controller_change(
                                midi_channel,
                                midi_controllers::DATA_ENTRY_MSB,
                                cents_coarse,
                                e_ticks,
                            );
                            let data_entry_fine = get_controller_change(
                                midi_channel,
                                midi_controllers::DATA_ENTRY_LSB,
                                0,
                                e_ticks,
                            );
                            // Note: added in reverse order (addEventBefore splices at current index)
                            midi.tracks[track_num]
                                .add_event(data_entry_fine, index + insert_offset);
                            event_indexes[track_num] += 1;
                            insert_offset += 1;
                            midi.tracks[track_num]
                                .add_event(data_entry_coarse, index + insert_offset);
                            event_indexes[track_num] += 1;
                            insert_offset += 1;
                            midi.tracks[track_num].add_event(rpn_fine, index + insert_offset);
                            event_indexes[track_num] += 1;
                            insert_offset += 1;
                            midi.tracks[track_num].add_event(rpn_coarse, index + insert_offset);
                            event_indexes[track_num] += 1;
                            insert_offset += 1;
                        }
                    }

                    if channels_to_change_program.contains(&channel_u8)
                        && let Some(change) = desired_program_changes
                            .iter()
                            .find(|c| c.channel == channel_u8)
                    {
                        spessa_synth_info(&format!(
                            "Setting {} to {}. Track num: {}",
                            change.channel,
                            midi_patch::to_midi_string(&change.patch),
                            track_num
                        ));

                        // Note: this is in reverse.
                        // The output event order is: drums -> lsb -> msb -> program change
                        let mut desired_bank_msb = change.patch.bank_msb;
                        let mut desired_bank_lsb = change.patch.bank_lsb;
                        let desired_program = change.patch.program;

                        // Add program change
                        let program_change = MidiMessage::new(
                            e_ticks,
                            midi_message_types::PROGRAM_CHANGE | midi_channel,
                            vec![desired_program],
                        );
                        midi.tracks[track_num]
                            .add_event(program_change, index + insert_offset);
                        event_indexes[track_num] += 1;
                        insert_offset += 1;

                        if BankSelectHacks::is_system_xg(system)
                            && change.patch.is_gm_gs_drum
                        {
                            // Best I can do is XG drums
                            spessa_synth_info(&format!(
                                "Adding XG Drum change on track {}",
                                track_num
                            ));
                            if let Some(drum_bank) = BankSelectHacks::get_drum_bank(system) {
                                desired_bank_msb = drum_bank;
                            }
                            desired_bank_lsb = 0;
                        }

                        // Add bank change (MSB)
                        let bank_msb_change = get_controller_change(
                            midi_channel,
                            midi_controllers::BANK_SELECT,
                            desired_bank_msb,
                            e_ticks,
                        );
                        midi.tracks[track_num]
                            .add_event(bank_msb_change, index + insert_offset);
                        event_indexes[track_num] += 1;
                        insert_offset += 1;

                        // Add bank change (LSB)
                        let bank_lsb_change = get_controller_change(
                            midi_channel,
                            midi_controllers::BANK_SELECT_LSB,
                            desired_bank_lsb,
                            e_ticks,
                        );
                        midi.tracks[track_num]
                            .add_event(bank_lsb_change, index + insert_offset);
                        event_indexes[track_num] += 1;
                        insert_offset += 1;

                        if change.patch.is_gm_gs_drum
                            && !BankSelectHacks::is_system_xg(system)
                            && midi_channel != DEFAULT_PERCUSSION
                        {
                            // Add GS drum change
                            spessa_synth_info(&format!(
                                "Adding GS Drum change on track {}",
                                track_num
                            ));
                            midi.tracks[track_num].add_event(
                                get_drum_change(midi_channel, e_ticks),
                                index + insert_offset,
                            );
                            event_indexes[track_num] += 1;
                            // insert_offset is not used after this, intentionally omitted
                        }
                    }
                }
                // Transpose key (for zero it won't change anyway)
                if (channel as usize) < channels_amount {
                    let current_index = event_indexes[track_num];
                    if !midi.tracks[track_num].events[current_index].data.is_empty() {
                        let new_val = midi.tracks[track_num].events[current_index].data[0] as i32
                            + coarse_transpose[channel as usize];
                        midi.tracks[track_num].events[current_index].data[0] =
                            new_val.clamp(0, 127) as u8;
                    }
                }
                event_indexes[track_num] += 1;
            }

            s if s == midi_message_types::NOTE_OFF => {
                // Transpose key
                if (channel as usize) < channels_amount
                    && !midi.tracks[track_num].events[index].data.is_empty()
                {
                    let new_val = midi.tracks[track_num].events[index].data[0] as i32
                        + coarse_transpose[channel as usize];
                    midi.tracks[track_num].events[index].data[0] =
                        new_val.clamp(0, 127) as u8;
                }
                event_indexes[track_num] += 1;
            }

            s if s == midi_message_types::PROGRAM_CHANGE => {
                // Do we delete it?
                if channels_to_change_program.contains(&channel_u8) {
                    // This channel has program change. BEGONE!
                    midi.tracks[track_num].delete_event(index);
                    // Don't increment
                    continue;
                }
                event_indexes[track_num] += 1;
            }

            s if s == midi_message_types::CONTROLLER_CHANGE => {
                if !e_data.is_empty() {
                    let cc_num = e_data[0];
                    let has_change = desired_controller_changes.iter().any(|c| {
                        c.channel == channel_u8 && cc_num == c.controller_number
                    });
                    if has_change {
                        // This controller is locked, BEGONE CHANGE!
                        midi.tracks[track_num].delete_event(index);
                        continue;
                    }
                    // Bank maybe?
                    if (cc_num == midi_controllers::BANK_SELECT
                        || cc_num == midi_controllers::BANK_SELECT_LSB)
                        && channels_to_change_program.contains(&channel_u8)
                    {
                        // BEGONE!
                        midi.tracks[track_num].delete_event(index);
                        continue;
                    }
                }
                event_indexes[track_num] += 1;
            }

            s if s == midi_message_types::SYSTEM_EXCLUSIVE => {
                let msg = &midi.tracks[track_num].events[index];
                // Check for XG on
                if is_xg_on(msg) {
                    spessa_synth_info("XG system on detected");
                    system = SynthSystem::Xg;
                    added_gs = true; // Flag as true so GS won't get added
                } else if msg.data.len() >= 6
                    && msg.data[0] == 0x43 // Yamaha
                    && msg.data[2] == 0x4c // XG
                    && msg.data[3] == 0x08 // Part parameter
                    && msg.data[5] == 0x03
                // Program change
                {
                    // Check for XG program change
                    // Do we delete it?
                    if msg.data.len() > 4
                        && channels_to_change_program
                            .contains(&(msg.data[4] + port_offset as u8))
                    {
                        // This channel has program change. BEGONE!
                        midi.tracks[track_num].delete_event(index);
                        continue;
                    }
                } else if is_gm2_on(msg) {
                    spessa_synth_info("GM2 system on detected");
                    system = SynthSystem::Gm2;
                    added_gs = true; // Flag as true so GS won't get added
                } else if is_gs_on(msg) {
                    // That's a GS on, we're done here
                    added_gs = true;
                    spessa_synth_info("GS on detected!");
                    // break in TS falls through to next event (no delete)
                } else if is_gm_on(msg) {
                    // That's a GM1 system change, remove it!
                    spessa_synth_info("GM on detected, removing!");
                    midi.tracks[track_num].delete_event(index);
                    added_gs = false;
                    continue;
                }
                event_indexes[track_num] += 1;
            }

            _ => {
                event_indexes[track_num] += 1;
            }
        }
    }

    // Check for GS
    if !added_gs && !desired_program_changes.is_empty() {
        // GS is not on, add it on the first track at index 0 (or 1 if track name is first)
        let mut insert_index = 0;
        if !midi.tracks.is_empty()
            && !midi.tracks[0].events.is_empty()
            && midi.tracks[0].events[0].status_byte == midi_message_types::TRACK_NAME
        {
            insert_index += 1;
        }
        if !midi.tracks.is_empty() {
            midi.tracks[0].add_event(get_gs_on(0), insert_index);
            spessa_synth_info("GS on not detected. Adding it.");
        }
    }
    midi.flush(true);
    spessa_synth_group_end();
}

/// Modifies the sequence according to the locked presets and controllers in the given snapshot.
/// Equivalent to: applySnapshotInternal(midi, snapshot)
pub fn apply_snapshot_internal(midi: &mut BasicMidi, snapshot: &SynthesizerSnapshot) {
    let mut channels_to_transpose: Vec<DesiredChannelTranspose> = Vec::new();
    let mut channels_to_clear: Vec<u8> = Vec::new();
    let mut program_changes: Vec<DesiredProgramChange> = Vec::new();
    let mut controller_changes: Vec<DesiredControllerChange> = Vec::new();

    for (channel_number, channel) in snapshot.channel_snapshots.iter().enumerate() {
        if channel.is_muted {
            channels_to_clear.push(channel_number as u8);
            continue;
        }
        let transpose_float = channel.channel_transpose_key_shift as f64
            + channel.custom_controllers[custom_controllers::CHANNEL_TRANSPOSE_FINE as usize]
                as f64
                / 100.0;
        if transpose_float != 0.0 {
            channels_to_transpose.push(DesiredChannelTranspose {
                channel: channel_number as u8,
                key_shift: transpose_float,
            });
        }
        if channel.lock_preset {
            program_changes.push(DesiredProgramChange {
                channel: channel_number as u8,
                patch: channel.patch.patch,
            });
        }
        // Check for locked controllers and change them appropriately
        for (cc_number, locked) in channel.locked_controllers.iter().enumerate() {
            if !locked
                || cc_number > 127
                || cc_number as u8 == midi_controllers::BANK_SELECT
            {
                continue;
            }
            // Channel controllers are stored as 14 bit values
            let target_value = (channel.midi_controllers[cc_number] >> 7) as u8;
            controller_changes.push(DesiredControllerChange {
                channel: channel_number as u8,
                controller_number: cc_number as u8,
                controller_value: target_value,
            });
        }
    }
    modify_midi_internal(
        midi,
        &program_changes,
        &controller_changes,
        &channels_to_clear,
        &channels_to_transpose,
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
    use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;

    fn make_msg(ticks: u32, status: u8, data: Vec<u8>) -> MidiMessage {
        MidiMessage::new(ticks, status, data)
    }

    fn make_track(events: Vec<MidiMessage>) -> MidiTrack {
        let mut t = MidiTrack::new();
        for e in &events {
            // Populate channels from voice messages (needed for port assignment)
            if e.status_byte >= 0x80 && e.status_byte < 0xF0 {
                t.channels.insert(e.status_byte & 0x0F);
            }
        }
        for e in events {
            t.push_event(e);
        }
        t
    }

    fn make_midi_with_track(track: MidiTrack) -> BasicMidi {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        m.tracks.push(track);
        m
    }

    // ── get_controller_change ────────────────────────────────────────────

    #[test]
    fn test_get_controller_change_status_byte() {
        let msg = get_controller_change(0, 7, 100, 0);
        assert_eq!(msg.status_byte, midi_message_types::CONTROLLER_CHANGE);
        assert_eq!(msg.data, vec![7, 100]);
        assert_eq!(msg.ticks, 0);
    }

    #[test]
    fn test_get_controller_change_channel_3() {
        let msg = get_controller_change(3, 10, 64, 480);
        assert_eq!(msg.status_byte, midi_message_types::CONTROLLER_CHANGE | 3);
        assert_eq!(msg.data, vec![10, 64]);
        assert_eq!(msg.ticks, 480);
    }

    #[test]
    fn test_get_controller_change_channel_wraps_at_16() {
        let msg = get_controller_change(17, 7, 100, 0);
        assert_eq!(msg.status_byte, midi_message_types::CONTROLLER_CHANGE | 1);
    }

    // ── get_drum_change ──────────────────────────────────────────────────

    #[test]
    fn test_get_drum_change_is_sysex() {
        let msg = get_drum_change(0, 0);
        assert_eq!(msg.status_byte, midi_message_types::SYSTEM_EXCLUSIVE);
    }

    #[test]
    fn test_get_drum_change_ends_with_f7() {
        let msg = get_drum_change(0, 0);
        assert_eq!(*msg.data.last().unwrap(), 0xF7);
    }

    #[test]
    fn test_get_drum_change_roland_header() {
        let msg = get_drum_change(0, 0);
        assert_eq!(msg.data[0], 0x41); // Roland
        assert_eq!(msg.data[1], 0x10); // Device ID
        assert_eq!(msg.data[2], 0x42); // GS
        assert_eq!(msg.data[3], 0x12); // DT1
    }

    #[test]
    fn test_get_drum_change_checksum() {
        let msg = get_drum_change(0, 0);
        // chan_address for channel 0: 0x10 | [1,2,3,4,5,6,7,8,0,9,10,11,12,13,14,15][0] = 0x10 | 1 = 0x11
        // sum = 0x40 + 0x11 + 0x15 + 0x01 = 0x67 = 103
        // checksum = 128 - (103 % 128) = 128 - 103 = 25
        let checksum = msg.data[msg.data.len() - 2];
        assert_eq!(checksum, 25);
    }

    #[test]
    fn test_get_drum_change_channel_9() {
        let msg = get_drum_change(9, 100);
        assert_eq!(msg.ticks, 100);
        // chan_address for channel 9: 0x10 | [1,2,...][9] = 0x10 | 9 = 0x19
        assert_eq!(msg.data[5], 0x19);
    }

    // ── modify_midi_internal: channel clearing ───────────────────────────

    #[test]
    fn test_clear_channel_removes_events() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),       // note on ch0
            make_msg(100, 0x91, vec![60, 100]),      // note on ch1
            make_msg(200, 0x80, vec![60, 0]),        // note off ch0
            make_msg(300, 0x81, vec![60, 0]),        // note off ch1
            make_msg(400, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(&mut midi, &[], &[], &[0], &[]);

        // Channel 0 events should be removed, channel 1 should remain
        let remaining_voice: Vec<u8> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte >= 0x80 && e.status_byte < 0xF0)
            .map(|e| e.status_byte & 0x0F)
            .collect();
        assert!(remaining_voice.iter().all(|ch| *ch == 1));
    }

    #[test]
    fn test_clear_channel_empty_list_keeps_all() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(100, 0x80, vec![60, 0]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(&mut midi, &[], &[], &[], &[]);

        // flush may add events (like track name), but voice events should be preserved
        let voice_events: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte >= 0x80 && e.status_byte < 0xF0)
            .collect();
        assert_eq!(voice_events.len(), 2);
    }

    // ── modify_midi_internal: transposition ──────────────────────────────

    #[test]
    fn test_transpose_note_on_up() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(100, 0x80, vec![60, 0]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(
            &mut midi,
            &[],
            &[],
            &[],
            &[DesiredChannelTranspose {
                channel: 0,
                key_shift: 5.0,
            }],
        );

        let note_on = midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte & 0xF0 == midi_message_types::NOTE_ON)
            .unwrap();
        assert_eq!(note_on.data[0], 65);

        let note_off = midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte & 0xF0 == midi_message_types::NOTE_OFF)
            .unwrap();
        assert_eq!(note_off.data[0], 65);
    }

    #[test]
    fn test_transpose_note_down() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(100, 0x80, vec![60, 0]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(
            &mut midi,
            &[],
            &[],
            &[],
            &[DesiredChannelTranspose {
                channel: 0,
                key_shift: -3.0,
            }],
        );

        let note_on = midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte & 0xF0 == midi_message_types::NOTE_ON)
            .unwrap();
        assert_eq!(note_on.data[0], 57);
    }

    #[test]
    fn test_transpose_clamps_to_valid_range() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![120, 100]),
            make_msg(100, 0x80, vec![120, 0]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(
            &mut midi,
            &[],
            &[],
            &[],
            &[DesiredChannelTranspose {
                channel: 0,
                key_shift: 20.0,
            }],
        );

        let note_on = midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte & 0xF0 == midi_message_types::NOTE_ON)
            .unwrap();
        assert!(note_on.data[0] <= 127);
    }

    // ── modify_midi_internal: program change insertion ────────────────────

    #[test]
    fn test_program_change_inserts_before_first_note() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);

        let patch = MidiPatch {
            program: 25,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        };
        modify_midi_internal(
            &mut midi,
            &[DesiredProgramChange { channel: 0, patch }],
            &[],
            &[],
            &[],
        );

        // Should have inserted GS ON + bank MSB + bank LSB + program change before the note-on
        let program_changes: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte & 0xF0 == midi_message_types::PROGRAM_CHANGE)
            .collect();
        assert_eq!(program_changes.len(), 1);
        assert_eq!(program_changes[0].data[0], 25);
    }

    #[test]
    fn test_program_change_removes_existing_program_changes() {
        let track = make_track(vec![
            make_msg(0, midi_message_types::PROGRAM_CHANGE, vec![10]),     // existing PC ch0
            make_msg(100, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);

        let patch = MidiPatch {
            program: 25,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        };
        modify_midi_internal(
            &mut midi,
            &[DesiredProgramChange { channel: 0, patch }],
            &[],
            &[],
            &[],
        );

        // Only the newly-inserted program change should remain (program 25)
        let program_changes: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte & 0xF0 == midi_message_types::PROGRAM_CHANGE)
            .collect();
        assert!(program_changes.iter().all(|pc| pc.data[0] == 25));
    }

    // ── modify_midi_internal: controller change insertion ─────────────────

    #[test]
    fn test_controller_change_inserts_before_first_note() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(
            &mut midi,
            &[],
            &[DesiredControllerChange {
                channel: 0,
                controller_number: 7,
                controller_value: 100,
            }],
            &[],
            &[],
        );

        // Should have inserted a CC7=100 before the note-on
        let cc_events: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| {
                e.status_byte & 0xF0 == midi_message_types::CONTROLLER_CHANGE
                    && e.data.len() >= 2
                    && e.data[0] == 7
            })
            .collect();
        assert_eq!(cc_events.len(), 1);
        assert_eq!(cc_events[0].data[1], 100);
    }

    #[test]
    fn test_locked_controller_removes_existing_cc() {
        let track = make_track(vec![
            make_msg(0, midi_message_types::CONTROLLER_CHANGE, vec![7, 80]),   // CC7 ch0
            make_msg(100, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(
            &mut midi,
            &[],
            &[DesiredControllerChange {
                channel: 0,
                controller_number: 7,
                controller_value: 100,
            }],
            &[],
            &[],
        );

        // The original CC7=80 should be removed, only the new CC7=100 remains
        let cc7_events: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| {
                e.status_byte & 0xF0 == midi_message_types::CONTROLLER_CHANGE
                    && e.data.len() >= 2
                    && e.data[0] == 7
            })
            .collect();
        assert_eq!(cc7_events.len(), 1);
        assert_eq!(cc7_events[0].data[1], 100);
    }

    // ── modify_midi_internal: bank select removal ────────────────────────

    #[test]
    fn test_bank_select_removed_when_program_change_set() {
        let track = make_track(vec![
            make_msg(0, midi_message_types::CONTROLLER_CHANGE, vec![0, 5]),    // Bank MSB
            make_msg(0, midi_message_types::CONTROLLER_CHANGE | 0, vec![32, 3]), // Bank LSB
            make_msg(0, midi_message_types::PROGRAM_CHANGE, vec![10]),
            make_msg(100, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);

        let patch = MidiPatch {
            program: 25,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        };
        modify_midi_internal(
            &mut midi,
            &[DesiredProgramChange { channel: 0, patch }],
            &[],
            &[],
            &[],
        );

        // Original bank select events should be gone
        let bank_selects: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| {
                e.status_byte & 0xF0 == midi_message_types::CONTROLLER_CHANGE
                    && e.data.len() >= 2
                    && (e.data[0] == midi_controllers::BANK_SELECT
                        || e.data[0] == midi_controllers::BANK_SELECT_LSB)
            })
            .collect();
        // Only the newly inserted bank changes should exist (bank_msb=0, bank_lsb=0)
        for bs in &bank_selects {
            assert_eq!(bs.data[1], 0);
        }
    }

    // ── modify_midi_internal: GS system detection ────────────────────────

    #[test]
    fn test_adds_gs_on_when_not_detected() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);

        let patch = MidiPatch {
            program: 25,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        };
        modify_midi_internal(
            &mut midi,
            &[DesiredProgramChange { channel: 0, patch }],
            &[],
            &[],
            &[],
        );

        // Should have a GS ON SysEx
        let sysex_events: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE)
            .collect();
        assert!(!sysex_events.is_empty());
        // Check it's a GS ON (Roland 0x41, GS 0x42, mode set 0x7F)
        let gs_on = sysex_events
            .iter()
            .find(|e| e.data.len() >= 7 && e.data[0] == 0x41 && e.data[2] == 0x42 && e.data[6] == 0x7f);
        assert!(gs_on.is_some());
    }

    #[test]
    fn test_does_not_add_gs_on_when_already_present() {
        let gs_on_msg = get_gs_on(0);
        let track = make_track(vec![
            gs_on_msg,
            make_msg(100, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);

        let patch = MidiPatch {
            program: 25,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: false,
        };
        modify_midi_internal(
            &mut midi,
            &[DesiredProgramChange { channel: 0, patch }],
            &[],
            &[],
            &[],
        );

        // Should have exactly one GS ON SysEx (no duplicate added)
        let gs_on_count = midi.tracks[0]
            .events
            .iter()
            .filter(|e| {
                e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE
                    && e.data.len() >= 7
                    && e.data[0] == 0x41
                    && e.data[2] == 0x42
                    && e.data[6] == 0x7f
            })
            .count();
        assert_eq!(gs_on_count, 1);
    }

    #[test]
    fn test_gm_on_removed() {
        // GM ON SysEx: 0x7E 0x7F 0x09 0x01
        let gm_on = MidiMessage::new(
            0,
            midi_message_types::SYSTEM_EXCLUSIVE,
            vec![0x7e, 0x7f, 0x09, 0x01, 0xf7],
        );
        let track = make_track(vec![
            gm_on,
            make_msg(100, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(&mut midi, &[], &[], &[], &[]);

        // GM ON should be removed
        let gm_on_events: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| {
                e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE
                    && e.data.len() >= 4
                    && e.data[0] == 0x7e
                    && e.data[2] == 0x09
                    && e.data[3] == 0x01
            })
            .collect();
        assert!(gm_on_events.is_empty());
    }

    // ── modify_midi_internal: meta events preserved ──────────────────────

    #[test]
    fn test_meta_events_not_cleared() {
        let track = make_track(vec![
            make_msg(0, midi_message_types::TRACK_NAME, b"Test".to_vec()),
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(&mut midi, &[], &[], &[0], &[]);

        // Track name should still be there even though channel 0 is cleared
        let has_track_name = midi.tracks[0]
            .events
            .iter()
            .any(|e| e.status_byte == midi_message_types::TRACK_NAME);
        assert!(has_track_name);
    }

    // ── modify_midi_internal: drum program changes ───────────────────────

    #[test]
    fn test_drum_program_change_adds_gs_drum_sysex() {
        let gs_on_msg = get_gs_on(0);
        let track = make_track(vec![
            gs_on_msg,
            make_msg(100, 0x90, vec![60, 100]),   // note on ch0
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);

        let drum_patch = MidiPatch {
            program: 0,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: true,
        };
        modify_midi_internal(
            &mut midi,
            &[DesiredProgramChange {
                channel: 0,
                patch: drum_patch,
            }],
            &[],
            &[],
            &[],
        );

        // Should have a GS drum SysEx for channel 0 (not channel 9, which is already drums)
        let drum_sysex: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| {
                e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE
                    && e.data.len() >= 8
                    && e.data[0] == 0x41
                    && e.data[2] == 0x42
                    && e.data[3] == 0x12
                    && e.data[6] == 0x15
                    && e.data[7] == 0x01
            })
            .collect();
        assert!(!drum_sysex.is_empty());
    }

    // ── modify_midi_internal: no changes = no crash ──────────────────────

    #[test]
    fn test_no_changes_no_crash() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(100, 0x80, vec![60, 0]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(&mut midi, &[], &[], &[], &[]);
        // Just verify it doesn't panic
        assert!(!midi.tracks.is_empty());
    }

    #[test]
    fn test_empty_midi_no_crash() {
        let mut midi = BasicMidi::new();
        modify_midi_internal(&mut midi, &[], &[], &[], &[]);
        assert!(midi.tracks.is_empty());
    }

    // ── modify_midi_internal: fine tuning with transpose ─────────────────

    #[test]
    fn test_fine_transpose_adds_rpn_events() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(
            &mut midi,
            &[],
            &[],
            &[],
            &[DesiredChannelTranspose {
                channel: 0,
                key_shift: 2.5, // 2 semitones + 0.5 fine
            }],
        );

        // Should have RPN events (MSB=101, LSB=100) for fine tuning
        let rpn_events: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| {
                e.status_byte & 0xF0 == midi_message_types::CONTROLLER_CHANGE
                    && e.data.len() >= 2
                    && (e.data[0] == midi_controllers::REGISTERED_PARAMETER_MSB
                        || e.data[0] == midi_controllers::REGISTERED_PARAMETER_LSB)
            })
            .collect();
        assert!(!rpn_events.is_empty());
    }

    // ── modify_midi_internal: multi-track ────────────────────────────────

    #[test]
    fn test_multi_track_clear_channel() {
        let track0 = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),       // ch0
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let track1 = make_track(vec![
            make_msg(100, 0x91, vec![64, 100]),      // ch1
            make_msg(300, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = BasicMidi::new();
        midi.time_division = 480;
        midi.tracks.push(track0);
        midi.tracks.push(track1);
        modify_midi_internal(&mut midi, &[], &[], &[0], &[]);

        // Channel 0 events should be gone from track 0
        let ch0_voice_events: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte >= 0x80 && e.status_byte < 0xF0 && e.status_byte & 0x0F == 0)
            .collect();
        assert!(ch0_voice_events.is_empty());

        // Channel 1 events should remain in track 1
        let ch1_voice_events: Vec<_> = midi.tracks[1]
            .events
            .iter()
            .filter(|e| e.status_byte >= 0x80 && e.status_byte < 0xF0 && e.status_byte & 0x0F == 1)
            .collect();
        assert!(!ch1_voice_events.is_empty());
    }

    // ── XG system detection ──────────────────────────────────────────────

    #[test]
    fn test_xg_on_detected() {
        // XG ON: 0x43 dev 0x4C 0x00 0x00 0x7E 0x00
        let xg_on = MidiMessage::new(
            0,
            midi_message_types::SYSTEM_EXCLUSIVE,
            vec![0x43, 0x10, 0x4c, 0x00, 0x00, 0x7e, 0x00, 0xf7],
        );
        let track = make_track(vec![
            xg_on,
            make_msg(100, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);

        let drum_patch = MidiPatch {
            program: 0,
            bank_msb: 0,
            bank_lsb: 0,
            is_gm_gs_drum: true,
        };
        modify_midi_internal(
            &mut midi,
            &[DesiredProgramChange {
                channel: 0,
                patch: drum_patch,
            }],
            &[],
            &[],
            &[],
        );

        // When XG is detected, drum changes use XG drum bank (127) instead of GS drum SysEx
        // No GS drum SysEx should be present
        let gs_drum_sysex: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| {
                e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE
                    && e.data.len() >= 8
                    && e.data[0] == 0x41
                    && e.data[6] == 0x15
                    && e.data[7] == 0x01
            })
            .collect();
        assert!(gs_drum_sysex.is_empty());
    }
}
