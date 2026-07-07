/// modify_midi.rs
/// purpose: MIDI sequence editing utilities (program changes, controller changes, channel
///          clearing, transposition, RPN/NRPN-aware deletion, and GS effect parameter writing).
/// Ported from: src/midi/midi_tools/modify_midi.ts (spessasynth_core 4.3.0)
/// (formerly `src/midi/midi_tools/midi_editor.ts` in 4.2.0)
///
/// TS 4.3.0 rewrote this file almost entirely, replacing the flat
/// `modifyMIDIInternal(midi, desiredProgramChanges, desiredControllerChanges,
/// desiredChannelsToClear, desiredChannelsToTranspose)` signature with a single
/// `modifyMIDIInternal(midi, opts: ModifyMIDIOptions)`, where `opts.channels` is a
/// `Map<channel, ClearableParameter<ChannelModification>>` (channel -> "clear everything" or a
/// per-channel modification). New capabilities: RPN/NRPN-aware deletion via `ParameterTracker`
/// (so locking/clearing a controller that's actually driven by an NRPN, e.g. GS vibrato depth,
/// now works), and writing GS reverb/chorus/delay/insertion-effect System Exclusive parameters
/// (`reverbParams`/`chorusParams`/`delayParams`/`insertionParams`), replacing/augmenting
/// `applySnapshotInternal`'s previous "controllers + program changes only" scope (now split out
/// into `apply_snapshot.rs`).
///
/// `isXGOn`/`isGSOn`/`isGMOn`/`isGM2On` (from the now-deleted `utils/sysex_detector.ts`) are
/// replaced by `MIDIUtils.analyzeSysEx`; `getGsOn` is replaced by `MIDIUtils.gsReset`.
///
/// # Faithfully-reproduced upstream quirks
///
/// - `addEventBefore` (here: `add_event_before`) always inserts at the *original*, frozen event
///   index captured at the start of processing the current note-on/off — it never advances that
///   index between repeated calls. Since each insertion happens at the same array position, later
///   calls end up *before* earlier ones in the final MIDI: e.g. the program-change block calls
///   `addEventBefore` for program-change, then bank MSB, then bank LSB, then (optionally) GS drum
///   change, producing the final on-track order `drums -> lsb -> msb -> program change -> (note
///   on)` — exactly as the upstream code comment says. The per-channel controller-lock loop and
///   the fine-tune RPN block rely on the same reversal.
/// - The fine-tune RPN block's Data Entry MSB event is built with `getControllerChange(channel,
///   ...)` (the port-offset-inclusive absolute channel number) while every other call in that
///   block uses `midiChannel` (0-15, no port offset) — almost certainly an upstream typo, kept
///   as-is.
/// - The post-loop "insertion effect enabled per channel" writer calls `targetTrack.addEvents(
///   targetTicks, ...)`, i.e. it (mis)uses a *tick count* as the splice array index instead of
///   `targetIndex`. `Vec::insert` panics on an out-of-bounds index where JS's `Array.splice`
///   silently clamps, so the Rust port's `splice_insert` helper clamps explicitly — this is the
///   one deviation from "port bugs as-is", and only prevents a panic; it does not change where
///   the event ends up when the tick count *is* in-bounds.
use std::collections::{HashMap, HashSet};

use crate::midi::basic_midi::BasicMidi;
use crate::midi::enums::{midi_controllers, midi_message_types};
use crate::midi::midi_message::MidiMessage;
use crate::midi::midi_tools::midi_utils::{AnalyzedMidiMessage, MidiUtils};
use crate::midi::midi_tools::parameter_tracker::ParameterTracker;
use crate::midi::midi_track::MidiTrack;
use crate::soundbank::basic_soundbank::midi_patch::{self, MidiPatch};
use crate::soundbank::types::MIDISystem;
use crate::synthesizer::audio_engine::effects::chorus::ChorusSnapshot;
use crate::synthesizer::audio_engine::effects::delay::DelaySnapshot;
use crate::synthesizer::audio_engine::effects::reverb::ReverbSnapshot;
use crate::synthesizer::audio_engine::synth_constants::DEFAULT_PERCUSSION;
use crate::utils::loggin::SpessaLog;
use crate::utils::midi_hacks::BankSelectHacks;

// ─────────────────────────────────────────────────────────────────────────────
// Address maps (GS SysEx parameter addresses, keyed by the same field names as the
// value types themselves — mirrors the TS source's reuse of `ReverbProcessorSnapshot`/
// `ChorusProcessorSnapshot`/`DelayProcessorSnapshot` for both the address map *and* the
// parameter-value payload).
// ─────────────────────────────────────────────────────────────────────────────

const REVERB_ADDRESS_MAP: ReverbSnapshot = ReverbSnapshot {
    character: 0x31,
    pre_lowpass: 0x32,
    level: 0x33,
    time: 0x34,
    delay_feedback: 0x35,
    pre_delay_time: 0x37,
};

const CHORUS_ADDRESS_MAP: ChorusSnapshot = ChorusSnapshot {
    pre_lowpass: 0x39,
    level: 0x3a,
    feedback: 0x3b,
    delay: 0x3c,
    rate: 0x3d,
    depth: 0x3e,
    send_level_to_reverb: 0x3f,
    send_level_to_delay: 0x40,
};

const DELAY_ADDRESS_MAP: DelaySnapshot = DelaySnapshot {
    pre_lowpass: 0x51,
    time_center: 0x52,
    time_ratio_left: 0x53,
    time_ratio_right: 0x54,
    level_center: 0x55,
    level_left: 0x56,
    level_right: 0x57,
    level: 0x58,
    feedback: 0x59,
    send_level_to_reverb: 0x5a,
};

// ─────────────────────────────────────────────────────────────────────────────
// Public options types
// ─────────────────────────────────────────────────────────────────────────────

/// Represents a value that means "clear this parameter" instead of "replace this parameter
/// with". Essentially:
/// - absent (`None` wrapping this, at the field level) - no change.
/// - `Clear` - clear all changes of this parameter from the MIDI file.
/// - `Value(T)` - clear all changes of this parameter from the MIDI file and add `T`.
///
/// Equivalent to: `ClearableParameter<T> = T | "clear"`
#[derive(Clone, Debug)]
pub enum ClearableParameter<T> {
    Clear,
    Value(T),
}

/// A single channel's requested modifications.
/// Equivalent to: interface ChannelModification
#[derive(Clone, Debug, Default)]
pub struct ChannelModification {
    /// All controllers that should be modified for this channel, in the order they should be
    /// considered (this order is user-visible: see the "faithfully-reproduced upstream quirks"
    /// module doc — a `Vec` is used instead of a `HashMap` specifically to preserve this order,
    /// mirroring JavaScript's insertion-ordered `Map`).
    /// - `Clear` - all controller changes for this controller are removed.
    /// - `Value(v)` - clear + sets the new controller at the start of the song, effectively
    ///   locking it to `v`.
    pub controllers: Option<Vec<(u8, ClearableParameter<u8>)>>,

    /// The new program of this channel.
    /// - `Clear` - all program changes for this channel are removed.
    /// - `Value(patch)` - clear + sets the new patch according to the MIDI system at the start
    ///   of the sequence.
    pub patch: Option<ClearableParameter<MidiPatch>>,

    /// The channel key shift in semitones. Note on/off numbers are shifted.
    pub key_shift: f64,

    /// The channel tuning in cents. Tuned using RPN Fine Tune. Range is `[-100; 99.986]` cents.
    pub fine_tune: f64,
}

/// A local reshaping of TS 4.3.0's `InsertionProcessorSnapshot` (`type`/`params`/`channels`),
/// used only by `modify_midi`/`apply_snapshot`.
///
/// This intentionally does *not* reuse `synthesizer_snapshot::InsertionSnapshot` (the current
/// Rust engine-state type, 4.2.0-shaped): that type has no `channels: Vec<bool>` (per-channel
/// insertion-enabled flags — tracked per-`ChannelSnapshot` instead in the current Rust engine)
/// and packs only 20 raw parameter bytes, whereas TS 4.3.0's `params` is 23 bytes wide (20
/// parameters + 3 trailing effect-send slots, indices 20-22). Reconciling the two engine-state
/// shapes is left to Task 20; `apply_snapshot.rs` bridges between them with a documented,
/// best-effort field mapping.
#[derive(Clone, Debug)]
pub struct InsertionEffectParams {
    /// The EFX type of this processor, stored as `MSB << 8 | LSB`.
    pub efx_type: u16,
    /// 20 parameters for the effect (255 = "no change") + 3 effect sends (indices 20, 21, 22).
    pub params: [u8; 23],
    /// A boolean list for channels that have the insertion effect enabled.
    pub channels: Vec<bool>,
}

/// Options for [`modify_midi_internal`].
/// Equivalent to: interface ModifyMIDIOptions
#[derive(Default)]
pub struct ModifyMidiOptions {
    /// The channel changes.
    /// - `Clear` - all MIDI messages for this channel, such as Note On, are removed.
    /// - `Value(m)` - modifies the channel according to `m`.
    pub channels: Option<HashMap<u8, ClearableParameter<ChannelModification>>>,
    /// The drum parameter changes. Only `true` ("clear") is currently meaningful — TypeScript's
    /// `ClearableParameter<never>` can, by construction, never carry a `Value`.
    pub drum_setup_params_clear: bool,
    /// The desired GS reverb parameters.
    /// - `Clear` - all existing parameter-change MIDI messages are removed.
    /// - `Value(p)` - clear + the new parameters are set via System Exclusive messages.
    pub reverb_params: Option<ClearableParameter<ReverbSnapshot>>,
    /// The GS chorus parameters (same `Clear`/`Value` semantics as `reverb_params`).
    pub chorus_params: Option<ClearableParameter<ChorusSnapshot>>,
    /// The GS delay parameters (same `Clear`/`Value` semantics as `reverb_params`).
    pub delay_params: Option<ClearableParameter<DelaySnapshot>>,
    /// The GS Insertion Effect parameters (same `Clear`/`Value` semantics as `reverb_params`).
    pub insertion_params: Option<ClearableParameter<InsertionEffectParams>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Creates a Controller Change MIDI message.
/// Equivalent to: getControllerChange(channel, cc, value, ticks)
fn get_controller_change(channel: u8, cc: u8, value: u8, ticks: u32) -> MidiMessage {
    MidiMessage::new(
        ticks,
        midi_message_types::CONTROLLER_CHANGE | (channel % 16),
        vec![cc, value],
    )
}

/// Emulates JavaScript's `Uint8Array` element-assignment coercion (`ToUint8`): truncate toward
/// zero, then wrap into `0..256` (non-negative modulo), matching `IndexedByteArray`'s behavior
/// when TS does e.g. `e.data[0] += channelStatus.keyShift`. Used for key-shift/transpose, which
/// (unlike the pre-4.3.0 Rust port) is no longer clamped to `0..=127`.
fn to_uint8(x: f64) -> u8 {
    let n = x.trunc() as i64;
    (n.rem_euclid(256)) as u8
}

/// Inserts `msg` at position `index` (clamped to the track's current length, mirroring
/// JavaScript's `Array.prototype.splice` clamping behavior — unlike `Vec::insert`, which panics
/// on an out-of-bounds index).
fn splice_insert(track: &mut MidiTrack, index: usize, msg: MidiMessage) {
    let at = index.min(track.events.len());
    track.add_event(msg, at);
}

/// Inserts `msg` at the frozen `index` within `track_num` and bumps that track's event-index
/// cursor. Called repeatedly with the *same* `index` value reverses relative insertion order —
/// see the module-level doc comment.
///
/// Equivalent to: `addEventBefore(e)` (the closure inside `modifyMIDIInternal`)
fn add_event_before(
    midi: &mut BasicMidi,
    event_indexes: &mut [i64],
    track_num: usize,
    index: usize,
    msg: MidiMessage,
) {
    splice_insert(&mut midi.tracks[track_num], index, msg);
    event_indexes[track_num] += 1;
}

/// Deletes the event at `(track_num, index)` and decrements that track's event-index cursor so
/// the outer loop's unconditional `+= 1` leaves it net-unchanged (the next event slides into the
/// same position).
/// Equivalent to: `deleteThisEvent()` (the closure inside `modifyMIDIInternal`)
fn delete_this_event(midi: &mut BasicMidi, event_indexes: &mut [i64], track_num: usize, index: usize) {
    midi.tracks[track_num].delete_event(index);
    event_indexes[track_num] -= 1;
}

/// Assigns a MIDI port to a track and (on first sight of a new port) allocates that port its own
/// 16-channel block. Ports on tracks with no channels in use are ignored.
/// Equivalent to: `assignMIDIPort(trackNum, port)` (the closure inside `modifyMIDIInternal`)
fn assign_midi_port(
    track_num: usize,
    port: u32,
    midi_ports: &mut [u32],
    midi_port_channel_offsets: &mut HashMap<u32, u32>,
    midi_port_channel_offset: &mut u32,
    tracks: &[MidiTrack],
) {
    if tracks[track_num].channels.is_empty() {
        return;
    }
    if *midi_port_channel_offset == 0 {
        *midi_port_channel_offset += 16;
        midi_port_channel_offsets.insert(port, 0);
    }
    if let std::collections::hash_map::Entry::Vacant(e) = midi_port_channel_offsets.entry(port) {
        e.insert(*midi_port_channel_offset);
        *midi_port_channel_offset += 16;
    }
    midi_ports[track_num] = port;
}

/// Per-channel state tracked while scanning MIDI events.
/// Equivalent to: interface ChannelStatus
struct ChannelStatus {
    /// Tracks if the channel already had its first Note On.
    is_first_note_on: bool,
    /// RPN/NRPN tracking.
    param: ParameterTracker,
    /// Whether the parameter selection (MSB, LSB) and data entry were cleared. Some MIDI files
    /// send a parameter-number MSB once and then set the value via LSB-only messages afterward
    /// (technically invalid MIDI 1.0, but real files do this), so each of the three components
    /// is tracked independently.
    cleared_params: ClearedParams,
    /// Semitones (defaults to 0 when the channel has no requested modification).
    key_shift: f64,
    /// Cents (defaults to 0 when the channel has no requested modification).
    fine_tune: f64,
}

#[derive(Clone, Copy)]
struct ClearedParams {
    /// Param LSB.
    p_lsb: bool,
    /// Param MSB.
    p_msb: bool,
    /// Data (any).
    data: bool,
}

/// Deletes the parameter-selection pair (RPN/NRPN MSB + LSB) and the Data Entry event currently
/// being processed, tracking (via `ClearedParams`) what has already been deleted so repeated
/// Data-Entry-only messages targeting the same parameter don't try to delete the selector events
/// twice.
///
/// Equivalent to: `deleteParameter(channel)` (the closure inside `modifyMIDIInternal`)
fn delete_parameter(
    midi: &mut BasicMidi,
    event_indexes: &mut [i64],
    channel_status: &mut ChannelStatus,
    track_num: usize,
    index: usize,
) {
    SpessaLog::info(&format!(
        "Clearing Non/Registered Parameter. Clear MSB: {}, clear LSB: {}, clear data: {}.",
        channel_status.cleared_params.p_msb,
        channel_status.cleared_params.p_lsb,
        channel_status.cleared_params.data,
    ));

    if !channel_status.cleared_params.data {
        // Delete the current data-entry event first. This is safe because it's the event
        // currently being processed in the loop, meaning its index is always higher than or
        // equal to the cached MSB/LSB (on a possibly-different track).
        delete_this_event(midi, event_indexes, track_num, index);

        let (msb, lsb) = channel_status.param.param_msb_lsb_mut();
        // Shift the cached events down if they are on the same track (very likely).
        if track_num == msb.track && index < msb.event {
            msb.event -= 1;
        }
        if track_num == lsb.track && index < lsb.event {
            lsb.event -= 1;
        }
        channel_status.cleared_params.data = true;
    }

    if !channel_status.cleared_params.p_msb {
        let (msb_track, msb_event, shift_lsb) = {
            let (msb, lsb) = channel_status.param.param_msb_lsb_mut();
            let shift_lsb = msb.track == lsb.track && msb.event < lsb.event;
            (msb.track, msb.event, shift_lsb)
        };
        midi.tracks[msb_track].delete_event(msb_event);
        event_indexes[msb_track] -= 1;
        if shift_lsb {
            channel_status.param.param_lsb_mut().event -= 1;
        }
        channel_status.cleared_params.p_msb = true;
    }

    if !channel_status.cleared_params.p_lsb {
        let lsb = channel_status.param.param_lsb_mut();
        let (lsb_track, lsb_event) = (lsb.track, lsb.event);
        midi.tracks[lsb_track].delete_event(lsb_event);
        event_indexes[lsb_track] -= 1;
        channel_status.cleared_params.p_lsb = true;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Allows easy editing of the file by removing channels, changing programs, changing
/// controllers, transposing channels, and writing GS reverb/chorus/delay/insertion-effect
/// parameters. Note that this modifies the MIDI in-place.
///
/// Equivalent to: modifyMIDIInternal(midi, opts)
pub fn modify_midi_internal(midi: &mut BasicMidi, opts: &ModifyMidiOptions) {
    SpessaLog::group_collapsed("Applying changes to the MIDI file...");
    SpessaLog::info(&format!("Desired channel changes: {:?}", opts.channels));
    SpessaLog::info(&format!(
        "Desired reverb parameters present: {}",
        opts.reverb_params.is_some()
    ));
    SpessaLog::info(&format!(
        "Desired chorus parameters present: {}",
        opts.chorus_params.is_some()
    ));
    SpessaLog::info(&format!(
        "Desired delay parameters present: {}",
        opts.delay_params.is_some()
    ));
    SpessaLog::info(&format!(
        "Desired insertion parameters present: {}",
        opts.insertion_params.is_some()
    ));

    // Optimizations
    let clear_drum_params = opts.drum_setup_params_clear;
    // Track only channels to clear
    let mut cleared_channels: HashSet<u8> = HashSet::new();
    // Track only channels to change here
    let mut channel_changes: HashMap<u8, ChannelModification> = HashMap::new();
    if let Some(channels) = &opts.channels {
        for (&channel, ch) in channels {
            match ch {
                ClearableParameter::Clear => {
                    cleared_channels.insert(channel);
                }
                ClearableParameter::Value(m) => {
                    channel_changes.insert(channel, m.clone());
                }
            }
        }
    }

    // Go through all events one by one
    let mut system = MIDISystem::Gs;
    let mut added_gs = false;
    // Track reset position to insert effects right after
    let mut reset_track = 0usize;
    let mut reset_index = 0usize;

    // It copies midiPorts everywhere else, but here 0 works so DO NOT CHANGE!
    // MIDI port number for the corresponding track
    let mut midi_ports: Vec<u32> = midi.tracks.iter().map(|t| t.port).collect();
    // MIDI port: channel offset
    let mut midi_port_channel_offsets: HashMap<u32, u32> = HashMap::new();
    let mut midi_port_channel_offset: u32 = 0;

    // Assign port offsets
    let ports_snapshot: Vec<u32> = midi.tracks.iter().map(|t| t.port).collect();
    for (i, &port) in ports_snapshot.iter().enumerate() {
        assign_midi_port(
            i,
            port,
            &mut midi_ports,
            &mut midi_port_channel_offsets,
            &mut midi_port_channel_offset,
            &midi.tracks,
        );
    }

    let channels_amount = midi_port_channel_offset as usize;
    let mut channel_statuses: Vec<ChannelStatus> = (0..channels_amount)
        .map(|i| {
            let cm = channel_changes.get(&(i as u8));
            ChannelStatus {
                is_first_note_on: true,
                param: ParameterTracker::new(i as u8),
                cleared_params: ClearedParams {
                    p_lsb: true,
                    p_msb: true,
                    data: true,
                },
                key_shift: cm.map(|c| c.key_shift).unwrap_or(0.0),
                fine_tune: cm.map(|c| c.fine_tune).unwrap_or(0.0),
            }
        })
        .collect();

    // Manual re-implementation of `BasicMIDI::iterate`, using `i64` cursors: unlike the shared
    // `iterate` helper, this loop needs to delete/insert events on tracks *other* than the one
    // currently being visited (see `delete_parameter`), which can transiently push a cursor
    // below zero within a single event's processing before the trailing `+= 1` (for the
    // currently-visited track only) brings it back. Cursors are clamped to zero when used as an
    // index/exhaustion check, which only matters for pathological inputs that would already
    // crash the upstream TypeScript (indexing a JS array at `-1` yields `undefined`, and `.ticks`
    // on `undefined` throws) — for any well-formed MIDI file this clamp is never actually hit.
    //
    // The `'process` labeled block below plays the role of the TS callback body: TS's `return`
    // statements map to `break 'process`, and the unconditional `event_indexes[track_num] += 1`
    // after the block mirrors `iterate`'s post-callback `eventIndexes[trackNum]++` (which is what
    // makes `deleteThisEvent`'s `--` net out to "cursor stays, next event slides in").
    let num_tracks = midi.tracks.len();
    let mut event_indexes: Vec<i64> = vec![0; num_tracks];
    let mut remaining_tracks = num_tracks;

    while remaining_tracks > 0 {
        let mut min_ticks = u32::MAX;
        let mut track_num = 0usize;
        for i in 0..num_tracks {
            let idx = event_indexes[i].max(0) as usize;
            if idx >= midi.tracks[i].events.len() {
                continue;
            }
            let tick = midi.tracks[i].events[idx].ticks;
            if tick < min_ticks {
                track_num = i;
                min_ticks = tick;
            }
        }

        let index = event_indexes[track_num].max(0) as usize;
        if index >= midi.tracks[track_num].events.len() {
            remaining_tracks -= 1;
            continue;
        }

        'process: {
        let e_ticks = midi.tracks[track_num].events[index].ticks;
        let e_status_byte = midi.tracks[track_num].events[index].status_byte;
        let e_data = midi.tracks[track_num].events[index].data.clone();

        let port_offset = midi_port_channel_offsets
            .get(&midi_ports[track_num])
            .copied()
            .unwrap_or(0);

        if e_status_byte == midi_message_types::MIDI_PORT {
            if let Some(&port) = e_data.first() {
                assign_midi_port(
                    track_num,
                    port as u32,
                    &mut midi_ports,
                    &mut midi_port_channel_offsets,
                    &mut midi_port_channel_offset,
                    &midi.tracks,
                );
            }
            break 'process;
        }

        // Only process voice + System Exclusive messages.
        if e_status_byte < midi_message_types::NOTE_OFF
            || e_status_byte > midi_message_types::SYSTEM_EXCLUSIVE
        {
            break 'process;
        }

        let status = e_status_byte & 0xf0;
        let midi_channel = e_status_byte & 0xf;
        let channel = midi_channel as u32 + port_offset;
        let channel_u8 = channel as u8;

        // Clear channel?
        if e_status_byte != midi_message_types::SYSTEM_EXCLUSIVE
            && cleared_channels.contains(&channel_u8)
        {
            delete_this_event(midi, &mut event_indexes, track_num, index);
            break 'process;
        }

        let channel_idx = channel as usize;

        match status {
            s if s == midi_message_types::NOTE_ON => {
                // Make sure that we want to modify this channel at all.
                let Some(channel_change) = channel_changes.get(&channel_u8).cloned() else {
                    break 'process;
                };

                if channel_statuses[channel_idx].is_first_note_on {
                    channel_statuses[channel_idx].is_first_note_on = false;

                    // First: controllers. Because FSMP does not like program changes after CC
                    // changes in embedded MIDIs, and since insertion reverses order (see module
                    // doc), controllers get added first in the source, then programs end up
                    // before them in the output.
                    if let Some(controllers) = &channel_change.controllers {
                        for (cc, value) in controllers {
                            if let ClearableParameter::Value(v) = value {
                                let cc_change =
                                    get_controller_change(midi_channel, *cc, *v, e_ticks);
                                add_event_before(
                                    midi,
                                    &mut event_indexes,
                                    track_num,
                                    index,
                                    cc_change,
                                );
                            }
                        }
                    }

                    // Tuning
                    let fine_tune = channel_statuses[channel_idx].fine_tune;
                    if fine_tune != 0.0 {
                        // 64 is the center, 96 = 50 cents up.
                        let data = (fine_tune * 81.92).floor() as i32 + 8192;
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
                        // NOTE: uses `channel` (port-offset-inclusive), not `midi_channel`, as
                        // upstream does — see module doc "faithfully-reproduced upstream quirks".
                        let data_entry_coarse = get_controller_change(
                            channel_u8,
                            midi_controllers::DATA_ENTRY_MSB,
                            ((data >> 7) & 0x7f) as u8,
                            e_ticks,
                        );
                        let data_entry_fine = get_controller_change(
                            midi_channel,
                            midi_controllers::DATA_ENTRY_LSB,
                            (data & 0x7f) as u8,
                            e_ticks,
                        );
                        add_event_before(midi, &mut event_indexes, track_num, index, data_entry_fine);
                        add_event_before(midi, &mut event_indexes, track_num, index, data_entry_coarse);
                        add_event_before(midi, &mut event_indexes, track_num, index, rpn_fine);
                        add_event_before(midi, &mut event_indexes, track_num, index, rpn_coarse);
                    }

                    // Program change
                    if let Some(ClearableParameter::Value(patch)) = &channel_change.patch {
                        SpessaLog::info(&format!(
                            "Setting {} to {}. Track num: {}",
                            channel, midi_patch::to_midi_string(patch), track_num
                        ));

                        // Note: this is in reverse. The output event order is:
                        // drums -> lsb -> msb -> program change.
                        let mut desired_bank_msb = patch.bank_msb;
                        let mut desired_bank_lsb = patch.bank_lsb;
                        let desired_program = patch.program;

                        let program_change = MidiMessage::new(
                            e_ticks,
                            midi_message_types::PROGRAM_CHANGE | midi_channel,
                            vec![desired_program],
                        );
                        add_event_before(midi, &mut event_indexes, track_num, index, program_change);

                        if BankSelectHacks::is_system_xg(system) && patch.is_gm_gs_drum {
                            // Best I can do is XG drums.
                            SpessaLog::info(&format!("Adding XG Drum change on track {}", track_num));
                            if let Some(drum_bank) = BankSelectHacks::get_drum_bank(system) {
                                desired_bank_msb = drum_bank;
                            }
                            desired_bank_lsb = 0;
                        }

                        let bank_msb_change = get_controller_change(
                            midi_channel,
                            midi_controllers::BANK_SELECT,
                            desired_bank_msb,
                            e_ticks,
                        );
                        add_event_before(midi, &mut event_indexes, track_num, index, bank_msb_change);

                        let bank_lsb_change = get_controller_change(
                            midi_channel,
                            midi_controllers::BANK_SELECT_LSB,
                            desired_bank_lsb,
                            e_ticks,
                        );
                        add_event_before(midi, &mut event_indexes, track_num, index, bank_lsb_change);

                        if patch.is_gm_gs_drum
                            && !BankSelectHacks::is_system_xg(system)
                            && midi_channel != DEFAULT_PERCUSSION
                        {
                            SpessaLog::info(&format!("Adding GS Drum change on track {}", track_num));
                            add_event_before(
                                midi,
                                &mut event_indexes,
                                track_num,
                                index,
                                MidiUtils::gs_drum_change(e_ticks, midi_channel, 1),
                            );
                        }
                    }
                }

                // Transpose key (for zero it won't change anyway). Matches JS `Uint8Array`
                // wraparound (no clamping) rather than the pre-4.3.0 clamp-to-0..=127 behavior.
                // The cursor (bumped once per event inserted above) points at the current
                // note-on's shifted position, mirroring TS's mutation of the same `e` object.
                let key_shift = channel_statuses[channel_idx].key_shift;
                let cur_index = event_indexes[track_num].max(0) as usize;
                if !midi.tracks[track_num].events[cur_index].data.is_empty() {
                    let new_val = midi.tracks[track_num].events[cur_index].data[0] as f64 + key_shift;
                    midi.tracks[track_num].events[cur_index].data[0] = to_uint8(new_val);
                }
            }

            s if s == midi_message_types::NOTE_OFF => {
                if !channel_changes.contains_key(&channel_u8) {
                    break 'process;
                }
                let key_shift = channel_statuses[channel_idx].key_shift;
                if !midi.tracks[track_num].events[index].data.is_empty() {
                    let new_val = midi.tracks[track_num].events[index].data[0] as f64 + key_shift;
                    midi.tracks[track_num].events[index].data[0] = to_uint8(new_val);
                }
            }

            s if s == midi_message_types::PROGRAM_CHANGE => {
                if channel_changes
                    .get(&channel_u8)
                    .is_some_and(|c| c.patch.is_some())
                {
                    // This channel has a program change. BEGONE!
                    delete_this_event(midi, &mut event_indexes, track_num, index);
                    break 'process;
                }
            }

            s if s == midi_message_types::CONTROLLER_CHANGE => {
                let cc_num = e_data.first().copied().unwrap_or(0);
                let value = e_data.get(1).copied().unwrap_or(0);
                let channel_change = channel_changes.get(&channel_u8);

                let locked = channel_change
                    .and_then(|c| c.controllers.as_ref())
                    .and_then(|controllers| controllers.iter().find(|(cc, _)| *cc == cc_num));
                if locked.is_some() {
                    // This controller is locked, BEGONE CHANGE!
                    delete_this_event(midi, &mut event_indexes, track_num, index);
                    break 'process;
                }

                match cc_num {
                    _ if cc_num == midi_controllers::BANK_SELECT
                        || cc_num == midi_controllers::BANK_SELECT_LSB =>
                    {
                        // TS returns here whether or not the event was deleted.
                        if channel_change.is_some_and(|c| c.patch.is_some()) {
                            // BEGONE!
                            delete_this_event(midi, &mut event_indexes, track_num, index);
                        }
                        break 'process;
                    }

                    _ if cc_num == midi_controllers::REGISTERED_PARAMETER_LSB
                        || cc_num == midi_controllers::REGISTERED_PARAMETER_MSB
                        || cc_num == midi_controllers::NON_REGISTERED_PARAMETER_MSB
                        || cc_num == midi_controllers::NON_REGISTERED_PARAMETER_LSB =>
                    {
                        // Flag the parameter as not cleaned.
                        if cc_num == midi_controllers::NON_REGISTERED_PARAMETER_LSB
                            || cc_num == midi_controllers::REGISTERED_PARAMETER_LSB
                        {
                            channel_statuses[channel_idx].cleared_params.p_lsb = false;
                        } else {
                            channel_statuses[channel_idx].cleared_params.p_msb = false;
                        }
                        channel_statuses[channel_idx]
                            .param
                            .controller_change(cc_num, value, track_num, index);
                        break 'process;
                    }

                    _ if cc_num == midi_controllers::DATA_ENTRY_MSB
                        || cc_num == midi_controllers::DATA_ENTRY_LSB =>
                    {
                        channel_statuses[channel_idx].cleared_params.data = false;
                        let data = channel_statuses[channel_idx]
                            .param
                            .controller_change(cc_num, value, track_num, index);

                        let Some(data) = data else {
                            break 'process;
                        };

                        let mut consumed = false;
                        match data {
                            AnalyzedMidiMessage::DrumSetup => {
                                if clear_drum_params {
                                    delete_parameter(
                                        midi,
                                        &mut event_indexes,
                                        &mut channel_statuses[channel_idx],
                                        track_num,
                                        index,
                                    );
                                }
                                consumed = true;
                            }

                            AnalyzedMidiMessage::ControllerChange {
                                channel: data_channel,
                                controller: data_cc,
                                ..
                            } => {
                                // NRPN can change controllers too!
                                let locked = channel_changes
                                    .get(&channel_u8)
                                    .and_then(|c| c.controllers.as_ref())
                                    .and_then(|controllers| {
                                        controllers.iter().find(|(cc, _)| *cc == data_cc)
                                    });
                                if locked.is_some() {
                                    delete_parameter(
                                        midi,
                                        &mut event_indexes,
                                        &mut channel_statuses[data_channel as usize],
                                        track_num,
                                        index,
                                    );
                                    consumed = true;
                                } else if (data_cc == midi_controllers::BANK_SELECT
                                    || data_cc == midi_controllers::BANK_SELECT_LSB)
                                    && channel_changes
                                        .get(&channel_u8)
                                        .is_some_and(|c| c.patch.is_some())
                                {
                                    delete_parameter(
                                        midi,
                                        &mut event_indexes,
                                        &mut channel_statuses[data_channel as usize],
                                        track_num,
                                        index,
                                    );
                                }
                            }

                            AnalyzedMidiMessage::FineTune { value: cents, .. } => {
                                if channel_statuses[channel_idx].fine_tune != 0.0 {
                                    if channel_statuses[channel_idx].is_first_note_on {
                                        // No note-on yet. Then use it as relative!
                                        let new_tune =
                                            channel_statuses[channel_idx].fine_tune + cents;
                                        channel_statuses[channel_idx].key_shift +=
                                            (new_tune / 100.0).trunc();
                                        channel_statuses[channel_idx].fine_tune = new_tune % 100.0;
                                    }
                                    // We're tuning it ourselves, BEGONE!
                                    delete_parameter(
                                        midi,
                                        &mut event_indexes,
                                        &mut channel_statuses[channel_idx],
                                        track_num,
                                        index,
                                    );
                                }
                                consumed = true;
                            }

                            _ => {}
                        }

                        if consumed {
                            break 'process;
                        }

                        // Some MIDIs send parameter MSB once and then set via LSB only
                        // afterward; mark both as "cleaned" so future LSB-only entries won't
                        // try to delete them again.
                        channel_statuses[channel_idx].cleared_params.p_lsb = true;
                        channel_statuses[channel_idx].cleared_params.p_msb = true;
                        break 'process;
                    }

                    _ => {
                        break 'process;
                    }
                }
            }

            s if s == midi_message_types::SYSTEM_EXCLUSIVE => {
                let syx = MidiUtils::analyze_sysex(&e_data);
                match syx {
                    AnalyzedMidiMessage::XgReset => {
                        SpessaLog::info("XG system on detected");
                        system = MIDISystem::Xg;
                        added_gs = true; // Flag as true so GS won't get added.
                        reset_track = track_num;
                        reset_index = index;
                        for ch in channel_statuses.iter_mut() {
                            ch.param.reset();
                            ch.cleared_params = ClearedParams {
                                p_lsb: true,
                                p_msb: true,
                                data: true,
                            };
                        }
                    }

                    AnalyzedMidiMessage::Gm2On => {
                        SpessaLog::info("GM2 system on detected");
                        system = MIDISystem::Gm2;
                        added_gs = true;
                        reset_track = track_num;
                        reset_index = index;
                        for ch in channel_statuses.iter_mut() {
                            ch.param.reset();
                            ch.cleared_params = ClearedParams {
                                p_lsb: true,
                                p_msb: true,
                                data: true,
                            };
                        }
                    }

                    AnalyzedMidiMessage::GsReset => {
                        SpessaLog::info("GS on detected!");
                        added_gs = true;
                        reset_track = track_num;
                        reset_index = index;
                        for ch in channel_statuses.iter_mut() {
                            ch.param.reset();
                            ch.cleared_params = ClearedParams {
                                p_lsb: true,
                                p_msb: true,
                                data: true,
                            };
                        }
                    }

                    AnalyzedMidiMessage::GmOff | AnalyzedMidiMessage::GmOn => {
                        SpessaLog::info("GM on detected, removing!");
                        delete_this_event(midi, &mut event_indexes, track_num, index);
                        added_gs = false;
                        break 'process;
                    }

                    AnalyzedMidiMessage::DrumSetup => {
                        if clear_drum_params {
                            delete_this_event(midi, &mut event_indexes, track_num, index);
                            break 'process;
                        }
                    }

                    AnalyzedMidiMessage::ReverbParam => {
                        if opts.reverb_params.is_some() {
                            delete_this_event(midi, &mut event_indexes, track_num, index);
                            break 'process;
                        }
                    }

                    AnalyzedMidiMessage::ChorusParam => {
                        if opts.chorus_params.is_some() {
                            delete_this_event(midi, &mut event_indexes, track_num, index);
                            break 'process;
                        }
                    }

                    AnalyzedMidiMessage::DelayParam => {
                        if opts.delay_params.is_some() {
                            delete_this_event(midi, &mut event_indexes, track_num, index);
                            break 'process;
                        }
                    }

                    AnalyzedMidiMessage::InsertionParam => {
                        if opts.insertion_params.is_some() {
                            delete_this_event(midi, &mut event_indexes, track_num, index);
                            break 'process;
                        }
                    }

                    AnalyzedMidiMessage::ProgramChange {
                        channel: syx_channel,
                        ..
                    } => {
                        let target = syx_channel as u32 + port_offset;
                        if channel_changes
                            .get(&(target as u8))
                            .is_some_and(|c| c.patch.is_some())
                        {
                            delete_this_event(midi, &mut event_indexes, track_num, index);
                            break 'process;
                        }
                    }

                    AnalyzedMidiMessage::FineTune {
                        channel: syx_channel,
                        value: cents,
                    } => {
                        let target = (syx_channel as u32 + port_offset) as u8;
                        let has_change = channel_changes.contains_key(&target);
                        if let Some(syx_status) = channel_statuses.get_mut(target as usize) {
                            if syx_status.is_first_note_on && has_change {
                                // No note-on yet. Then use it as relative!
                                let new_tune = syx_status.fine_tune + cents;
                                syx_status.key_shift += (new_tune / 100.0).trunc();
                                syx_status.fine_tune = new_tune % 100.0;
                                delete_this_event(midi, &mut event_indexes, track_num, index);
                                break 'process;
                            }
                        }
                    }

                    AnalyzedMidiMessage::ControllerChange {
                        channel: syx_channel,
                        controller: cc_num,
                        ..
                    } => {
                        let target = syx_channel as u32 + port_offset;
                        let syx_change = channel_changes.get(&(target as u8));
                        let locked = syx_change
                            .and_then(|c| c.controllers.as_ref())
                            .and_then(|controllers| controllers.iter().find(|(cc, _)| *cc == cc_num));
                        if locked.is_some() {
                            delete_this_event(midi, &mut event_indexes, track_num, index);
                            break 'process;
                        }
                        if (cc_num == midi_controllers::BANK_SELECT
                            || cc_num == midi_controllers::BANK_SELECT_LSB)
                            && syx_change.is_some_and(|c| c.patch.is_some())
                        {
                            delete_this_event(midi, &mut event_indexes, track_num, index);
                            break 'process;
                        }
                    }

                    _ => {}
                }
            }

            _ => {}
        }
        } // end of 'process

        event_indexes[track_num] += 1;
    }

    // Check for GS reset and insert it to ensure that a reset always exists.
    let any_patch_change = channel_changes.values().any(|c| c.patch.is_some());
    if !added_gs && any_patch_change && !midi.tracks.is_empty() {
        // GS is not on, add it on the first track at index 0 (or 1 if track name is first).
        let mut insert_index = 0;
        if !midi.tracks[0].events.is_empty()
            && midi.tracks[0].events[0].status_byte == midi_message_types::TRACK_NAME
        {
            insert_index += 1;
        }
        splice_insert(&mut midi.tracks[0], insert_index, MidiUtils::gs_reset(0));
        reset_track = 0;
        reset_index = insert_index;
        SpessaLog::info("GS on not detected. Adding it.");
    }

    // Add effects.
    // `Math.max(0, midi.firstNoteOn)` in TS is redundant here: `first_note_on` is a `u32` and can
    // never be negative.
    let target_ticks = midi.first_note_on;
    let target_index = reset_index + 1;

    if let Some(ClearableParameter::Value(p)) = &opts.reverb_params {
        let m = &REVERB_ADDRESS_MAP;
        let mut at = target_index.min(midi.tracks[reset_track].events.len());
        for (addr, value) in [
            (m.level, p.level),
            (m.pre_lowpass, p.pre_lowpass),
            (m.character, p.character),
            (m.time, p.time),
            (m.delay_feedback, p.delay_feedback),
            (m.pre_delay_time, p.pre_delay_time),
        ] {
            midi.tracks[reset_track]
                .add_event(MidiUtils::gs_message(target_ticks, 0x40, 0x01, addr, &[value]), at);
            at += 1;
        }
    }

    if let Some(ClearableParameter::Value(p)) = &opts.chorus_params {
        let m = &CHORUS_ADDRESS_MAP;
        let mut at = target_index.min(midi.tracks[reset_track].events.len());
        for (addr, value) in [
            (m.level, p.level),
            (m.pre_lowpass, p.pre_lowpass),
            (m.feedback, p.feedback),
            (m.delay, p.delay),
            (m.rate, p.rate),
            (m.depth, p.depth),
            (m.send_level_to_reverb, p.send_level_to_reverb),
            (m.send_level_to_delay, p.send_level_to_delay),
        ] {
            midi.tracks[reset_track]
                .add_event(MidiUtils::gs_message(target_ticks, 0x40, 0x01, addr, &[value]), at);
            at += 1;
        }
    }

    if let Some(ClearableParameter::Value(p)) = &opts.delay_params {
        let m = &DELAY_ADDRESS_MAP;
        let mut at = target_index.min(midi.tracks[reset_track].events.len());
        for (addr, value) in [
            (m.level, p.level),
            (m.pre_lowpass, p.pre_lowpass),
            (m.time_center, p.time_center),
            (m.time_ratio_left, p.time_ratio_left),
            (m.time_ratio_right, p.time_ratio_right),
            (m.level_center, p.level_center),
            (m.level_left, p.level_left),
            (m.level_right, p.level_right),
            (m.feedback, p.feedback),
            (m.send_level_to_reverb, p.send_level_to_reverb),
        ] {
            midi.tracks[reset_track]
                .add_event(MidiUtils::gs_message(target_ticks, 0x40, 0x01, addr, &[value]), at);
            at += 1;
        }
    }

    if let Some(ClearableParameter::Value(p)) = &opts.insertion_params {
        // NOTE: uses `target_ticks` (a tick count) as the splice index, not `target_index` —
        // faithfully reproduced upstream quirk, see module doc. `splice_insert` clamps instead
        // of panicking when `target_ticks` (as `usize`) exceeds the track's length.
        for (channel, &enabled) in p.channels.iter().enumerate() {
            if enabled {
                splice_insert(
                    &mut midi.tracks[reset_track],
                    target_ticks as usize,
                    MidiUtils::gs_message(
                        target_ticks,
                        0x40,
                        0x40 | MidiUtils::channel_to_syx(channel as u8),
                        0x22,
                        &[1],
                    ),
                );
            }
        }

        // Params and sends.
        for (param, &value) in p.params.iter().enumerate() {
            if value == 255 {
                continue;
            }
            splice_insert(
                &mut midi.tracks[reset_track],
                target_index,
                MidiUtils::gs_message(target_ticks, 0x40, 0x03, (param + 3) as u8, &[value]),
            );
        }

        // Last means that it will be first, so the order is: Type / Params and sends / Channels.
        splice_insert(
            &mut midi.tracks[reset_track],
            target_index,
            MidiUtils::gs_message(
                target_ticks,
                0x40,
                0x03,
                0x00,
                &[(p.efx_type >> 8) as u8, (p.efx_type & 0x7f) as u8],
            ),
        );
    }

    midi.flush(true);
    SpessaLog::group_end();
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

    fn make_msg(ticks: u32, status: u8, data: Vec<u8>) -> MidiMessage {
        MidiMessage::new(ticks, status, data)
    }

    fn make_track(events: Vec<MidiMessage>) -> MidiTrack {
        let mut t = MidiTrack::new();
        for e in &events {
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

    fn opts_with_channel(channel: u8, modif: ChannelModification) -> ModifyMidiOptions {
        let mut channels = HashMap::new();
        channels.insert(channel, ClearableParameter::Value(modif));
        ModifyMidiOptions {
            channels: Some(channels),
            ..Default::default()
        }
    }

    fn opts_clear_channel(channel: u8) -> ModifyMidiOptions {
        let mut channels = HashMap::new();
        channels.insert(channel, ClearableParameter::Clear);
        ModifyMidiOptions {
            channels: Some(channels),
            ..Default::default()
        }
    }

    // ── channel clearing ─────────────────────────────────────────────────────

    #[test]
    fn test_clear_channel_removes_events() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(100, 0x91, vec![60, 100]),
            make_msg(200, 0x80, vec![60, 0]),
            make_msg(300, 0x81, vec![60, 0]),
            make_msg(400, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(&mut midi, &opts_clear_channel(0));

        let remaining_voice: Vec<u8> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte >= 0x80 && e.status_byte < 0xF0)
            .map(|e| e.status_byte & 0x0F)
            .collect();
        assert!(remaining_voice.iter().all(|ch| *ch == 1));
    }

    #[test]
    fn test_no_options_keeps_all_events() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(100, 0x80, vec![60, 0]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(&mut midi, &ModifyMidiOptions::default());

        let voice_events: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte >= 0x80 && e.status_byte < 0xF0)
            .collect();
        assert_eq!(voice_events.len(), 2);
    }

    // ── transposition ─────────────────────────────────────────────────────────

    #[test]
    fn test_transpose_note_on_and_off_up() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(100, 0x80, vec![60, 0]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(
            &mut midi,
            &opts_with_channel(
                0,
                ChannelModification {
                    key_shift: 5.0,
                    ..Default::default()
                },
            ),
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
            &opts_with_channel(
                0,
                ChannelModification {
                    key_shift: -3.0,
                    ..Default::default()
                },
            ),
        );

        let note_on = midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte & 0xF0 == midi_message_types::NOTE_ON)
            .unwrap();
        assert_eq!(note_on.data[0], 57);
    }

    #[test]
    fn test_transpose_without_channel_modification_leaves_note_unchanged() {
        // Channel 1 has no requested modification: notes must be left untouched even though
        // options.channels is Some (for a different channel).
        let track = make_track(vec![
            make_msg(0, 0x91, vec![60, 100]),
            make_msg(100, 0x81, vec![60, 0]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(
            &mut midi,
            &opts_with_channel(
                0,
                ChannelModification {
                    key_shift: 12.0,
                    ..Default::default()
                },
            ),
        );

        let note_on = midi.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte & 0xF0 == midi_message_types::NOTE_ON)
            .unwrap();
        assert_eq!(note_on.data[0], 60);
    }

    // ── program change ────────────────────────────────────────────────────────

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
            &opts_with_channel(
                0,
                ChannelModification {
                    patch: Some(ClearableParameter::Value(patch)),
                    ..Default::default()
                },
            ),
        );

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
            &opts_with_channel(
                0,
                ChannelModification {
                    patch: Some(ClearableParameter::Value(patch)),
                    ..Default::default()
                },
            ),
        );

        let program_changes: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte & 0xF0 == midi_message_types::PROGRAM_CHANGE)
            .collect();
        assert!(program_changes.iter().all(|pc| pc.data[0] == 25));
    }

    #[test]
    fn test_patch_clear_removes_program_change_without_adding_new_one() {
        let track = make_track(vec![
            make_msg(0, midi_message_types::PROGRAM_CHANGE, vec![10]),
            make_msg(100, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(
            &mut midi,
            &opts_with_channel(
                0,
                ChannelModification {
                    patch: Some(ClearableParameter::Clear),
                    ..Default::default()
                },
            ),
        );

        let program_changes: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte & 0xF0 == midi_message_types::PROGRAM_CHANGE)
            .collect();
        assert!(program_changes.is_empty());
    }

    // ── controller lock ───────────────────────────────────────────────────────

    #[test]
    fn test_controller_change_inserts_before_first_note() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        let mut controllers = Vec::new();
        controllers.push((7u8, ClearableParameter::Value(100u8)));
        modify_midi_internal(
            &mut midi,
            &opts_with_channel(
                0,
                ChannelModification {
                    controllers: Some(controllers),
                    ..Default::default()
                },
            ),
        );

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
            make_msg(0, midi_message_types::CONTROLLER_CHANGE, vec![7, 80]),
            make_msg(100, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        let controllers = vec![(7u8, ClearableParameter::Value(100u8))];
        modify_midi_internal(
            &mut midi,
            &opts_with_channel(
                0,
                ChannelModification {
                    controllers: Some(controllers),
                    ..Default::default()
                },
            ),
        );

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

    #[test]
    fn test_bank_select_removed_when_program_change_set() {
        let track = make_track(vec![
            make_msg(0, midi_message_types::CONTROLLER_CHANGE, vec![0, 5]),
            make_msg(0, midi_message_types::CONTROLLER_CHANGE, vec![32, 3]),
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
            &opts_with_channel(
                0,
                ChannelModification {
                    patch: Some(ClearableParameter::Value(patch)),
                    ..Default::default()
                },
            ),
        );

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
        for bs in &bank_selects {
            assert_eq!(bs.data[1], 0);
        }
    }

    // ── GS system detection ───────────────────────────────────────────────────

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
            &opts_with_channel(
                0,
                ChannelModification {
                    patch: Some(ClearableParameter::Value(patch)),
                    ..Default::default()
                },
            ),
        );

        let gs_on = midi.tracks[0].events.iter().find(|e| {
            e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE
                && e.data.len() >= 7
                && e.data[0] == 0x41
                && e.data[2] == 0x42
                && e.data[6] == 0x7f
        });
        assert!(gs_on.is_some());
    }

    #[test]
    fn test_does_not_add_gs_on_when_already_present() {
        let gs_on_msg = MidiUtils::gs_reset(0);
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
            &opts_with_channel(
                0,
                ChannelModification {
                    patch: Some(ClearableParameter::Value(patch)),
                    ..Default::default()
                },
            ),
        );

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
        modify_midi_internal(&mut midi, &ModifyMidiOptions::default());

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

    // ── drum program changes ──────────────────────────────────────────────────

    #[test]
    fn test_drum_program_change_adds_gs_drum_sysex() {
        let gs_on_msg = MidiUtils::gs_reset(0);
        let track = make_track(vec![
            gs_on_msg,
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
            &opts_with_channel(
                0,
                ChannelModification {
                    patch: Some(ClearableParameter::Value(drum_patch)),
                    ..Default::default()
                },
            ),
        );

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

    #[test]
    fn test_xg_on_detected_uses_xg_drum_bank_instead_of_gs_sysex() {
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
            &opts_with_channel(
                0,
                ChannelModification {
                    patch: Some(ClearableParameter::Value(drum_patch)),
                    ..Default::default()
                },
            ),
        );

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

    // ── meta events preserved ─────────────────────────────────────────────────

    #[test]
    fn test_meta_events_not_cleared() {
        let track = make_track(vec![
            make_msg(0, midi_message_types::TRACK_NAME, b"Test".to_vec()),
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(&mut midi, &opts_clear_channel(0));

        let has_track_name = midi.tracks[0]
            .events
            .iter()
            .any(|e| e.status_byte == midi_message_types::TRACK_NAME);
        assert!(has_track_name);
    }

    // ── fine tuning ───────────────────────────────────────────────────────────

    #[test]
    fn test_fine_tune_adds_rpn_events() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(
            &mut midi,
            &opts_with_channel(
                0,
                ChannelModification {
                    fine_tune: 50.0,
                    ..Default::default()
                },
            ),
        );

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

    // ── RPN/NRPN-driven controller locking (deleteParameter) ─────────────────

    #[test]
    fn test_locked_nrpn_controller_deletes_selector_and_data_entry() {
        // GS/XG NRPN: part parameter (msb=1) + TVF cutoff frequency (lsb=0x20) maps to CC 74
        // (brightness). Locking CC 74 should delete the whole NRPN sequence.
        let track = make_track(vec![
            make_msg(
                0,
                midi_message_types::CONTROLLER_CHANGE,
                vec![midi_controllers::NON_REGISTERED_PARAMETER_MSB, 1],
            ),
            make_msg(
                0,
                midi_message_types::CONTROLLER_CHANGE,
                vec![midi_controllers::NON_REGISTERED_PARAMETER_LSB, 0x20],
            ),
            make_msg(
                0,
                midi_message_types::CONTROLLER_CHANGE,
                vec![midi_controllers::DATA_ENTRY_MSB, 90],
            ),
            make_msg(100, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        let controllers = vec![(
            midi_controllers::BRIGHTNESS,
            ClearableParameter::Value(64u8),
        )];
        modify_midi_internal(
            &mut midi,
            &opts_with_channel(
                0,
                ChannelModification {
                    controllers: Some(controllers),
                    ..Default::default()
                },
            ),
        );

        // Original NRPN MSB/LSB/DataEntry should all be gone.
        let nrpn_remnants = midi.tracks[0]
            .events
            .iter()
            .filter(|e| {
                e.status_byte & 0xF0 == midi_message_types::CONTROLLER_CHANGE
                    && e.data.len() >= 2
                    && (e.data[0] == midi_controllers::NON_REGISTERED_PARAMETER_MSB
                        || e.data[0] == midi_controllers::NON_REGISTERED_PARAMETER_LSB
                        || e.data[0] == midi_controllers::DATA_ENTRY_MSB)
            })
            .count();
        assert_eq!(nrpn_remnants, 0);

        // The new locked CC74=64 should be present.
        let locked = midi.tracks[0].events.iter().any(|e| {
            e.status_byte & 0xF0 == midi_message_types::CONTROLLER_CHANGE
                && e.data.len() >= 2
                && e.data[0] == midi_controllers::BRIGHTNESS
                && e.data[1] == 64
        });
        assert!(locked);
    }

    // ── no-op / empty inputs ──────────────────────────────────────────────────

    #[test]
    fn test_no_changes_no_crash() {
        let track = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(100, 0x80, vec![60, 0]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = make_midi_with_track(track);
        modify_midi_internal(&mut midi, &ModifyMidiOptions::default());
        assert!(!midi.tracks.is_empty());
    }

    #[test]
    fn test_empty_midi_no_crash() {
        let mut midi = BasicMidi::new();
        modify_midi_internal(&mut midi, &ModifyMidiOptions::default());
        assert!(midi.tracks.is_empty());
    }

    // ── multi-track ───────────────────────────────────────────────────────────

    #[test]
    fn test_multi_track_clear_channel() {
        let track0 = make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let track1 = make_track(vec![
            make_msg(100, 0x91, vec![64, 100]),
            make_msg(300, midi_message_types::END_OF_TRACK, vec![]),
        ]);
        let mut midi = BasicMidi::new();
        midi.time_division = 480;
        midi.tracks.push(track0);
        midi.tracks.push(track1);
        modify_midi_internal(&mut midi, &opts_clear_channel(0));

        let ch0_voice_events: Vec<_> = midi.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte >= 0x80 && e.status_byte < 0xF0 && e.status_byte & 0x0F == 0)
            .collect();
        assert!(ch0_voice_events.is_empty());

        let ch1_voice_events: Vec<_> = midi.tracks[1]
            .events
            .iter()
            .filter(|e| e.status_byte >= 0x80 && e.status_byte < 0xF0 && e.status_byte & 0x0F == 1)
            .collect();
        assert!(!ch1_voice_events.is_empty());
    }
}
