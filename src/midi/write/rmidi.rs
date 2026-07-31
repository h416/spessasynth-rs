/// rmidi_writer.rs
/// purpose: Write RMIDI (RIFF + MIDI + SoundFont) files.
/// Ported from: src/midi/write/rmidi.ts (spessasynth_core 4.3.0)
///
/// TS 4.3.0 replaced the direct `isXGOn`/`isGSOn`/`isGMOn`/`isGM2On`/`isGSDrumsOn`/`syxToChannel`
/// SysEx-detector calls (from `utils/sysex_detector.ts`, deleted in 4.3.0) with a single
/// `MIDIUtils.analyzeSysEx` call whose result is dispatched on `syx.type`. This also broadens
/// SysEx support: GS/XG SysEx-encoded bank-select/program-change/mono-poly messages are now
/// recognized and replaced with equivalent regular Controller Change / Program Change events
/// in-place (see the `AnalyzedMidiMessage::ControllerChange`/`ProgramChange` arms below), so the
/// downstream bank-select tracking logic picks them up too. `getGsOn` (from the now-removed
/// `get_gs_on.ts`) became `MIDIUtils.gsReset`. `readRIFFChunk`/`writeRIFFChunkRaw`/
/// `writeRIFFChunkParts` free functions became `RIFFChunk::read`/`write`/`write_parts` static
/// methods (migrated below). `correctBankOffsetInternal` also gained a trailing `mid.flush()`
/// call to keep derived state (loop points, timeline, etc.) consistent after in-place event
/// mutation/removal.
///
/// Note: this Rust file uses its own manual tick-sorted `(ticks, track_idx, event_idx)` scan
/// instead of `BasicMidi::iterate`'s callback (unlike the TS `mid.iterate((e, trackNum,
/// eventIndexes) => {...})`), since it already provides equivalent (track_idx, event_idx)
/// addressing for in-place event replacement without needing `iterate`'s `eventIndexes` plumbing.
///
/// Quirk faithfully preserved from upstream: TS captures `const status = e.statusByte & 0xf0;`
/// *before* the SysEx branch, then reassigns the *local* `e` to the replacement event for the
/// `"Controller Change"`/`"Program Change"` SysEx cases (`break`, falling through) — but leaves
/// `status` unchanged. So the post-SysEx `if (status === MIDIMessageTypes.programChange)` block
/// (proper patch/preset lookup) never runs for a SysEx-derived Program Change; it falls straight
/// to the bank-select detector, which only inspects `e.data[0]` (the program number) against
/// `bankSelect`/`bankSelectLSB`. This looks like an upstream oversight, but per this repo's port
/// policy we replicate it exactly rather than "fixing" it. See `event_status` (kept stale, used
/// for the PROGRAM_CHANGE check) vs the fresh `ch_num` computation (uses the *updated* status
/// byte) below.
use crate::midi::basic_midi::BasicMidi;
use crate::midi::enums::{midi_controllers, midi_message_types};
use crate::midi::midi_message::MidiMessage;
use crate::midi::midi_tools::midi_utils::{AnalyzedMidiMessage, MidiUtils};
use crate::midi::write::midi::write_midi_internal;
use crate::midi::types::{RMIDInfoDataPartial, RMIDIWriteOptions};
use crate::soundbank::basic_soundbank::midi_patch::MidiPatch;
use crate::soundbank::basic_soundbank::preset_resolver::PresetResolver;
use crate::synthesizer::audio_engine::synth_constants::DEFAULT_PERCUSSION;
use crate::soundbank::types::MIDISystem;
use crate::utils::loggin::SpessaLog;
use crate::utils::midi_hacks::BankSelectHacks;
use crate::utils::riff_chunk::RIFFChunk;

/// Default copyright notice embedded in RMIDI files.
/// Equivalent to: DEFAULT_COPYRIGHT
pub const DEFAULT_COPYRIGHT: &str = "Created using SpessaSynth";

/// Per-channel state tracked while scanning events in `correct_bank_offset_internal`.
struct ChannelInfo {
    program: u8,
    drums: bool,
    /// (track_idx, event_idx) of the most-recent bank-select MSB event on this channel.
    last_bank_idx: Option<(usize, usize)>,
    /// (track_idx, event_idx) of the most-recent bank-select LSB event on this channel.
    last_bank_lsb_idx: Option<(usize, usize)>,
    has_bank_select: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Encodes a string as UTF-8 bytes followed by a null terminator.
fn encode_string_bytes(s: &str) -> Vec<u8> {
    let mut bytes = s.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

/// Returns the decoded name of the MIDI file.
/// Equivalent to: BasicMIDI.getName() (UTF-8 only; no multi-byte encoding support)
fn get_name(mid: &BasicMidi) -> String {
    // 1. Check rmidi_info["name"] (null-terminated UTF-8 bytes)
    if let Some(bytes) = mid.rmidi_info.get("name") {
        let trimmed = if bytes.last() == Some(&0) {
            &bytes[..bytes.len() - 1]
        } else {
            bytes.as_slice()
        };
        if let Ok(s) = std::str::from_utf8(trimmed) {
            let s = s.trim();
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    // 2. Fall back to binary_name (UTF-8)
    if let Some(bin_name) = &mid.binary_name
        && let Ok(s) = std::str::from_utf8(bin_name)
    {
        let s = s.trim();
        if !s.is_empty() {
            return s.to_string();
        }
    }
    // 3. Fall back to file_name
    mid.file_name.clone().unwrap_or_default()
}

// ─────────────────────────────────────────────────────────────────────────────
// correctBankOffsetInternal
// ─────────────────────────────────────────────────────────────────────────────

/// Fixes bank-select and program-change events so that the MIDI plays correctly
/// with the embedded sound bank at the given `bank_offset`.
/// Modifies `mid` in-place.
/// Equivalent to: correctBankOffsetInternal
fn correct_bank_offset_internal(
    mid: &mut BasicMidi,
    bank_offset: u8,
    sound_bank: &dyn PresetResolver,
) {
    let mut system = MIDISystem::Gm;

    // ports[track_idx] = the current MIDI port for that track (default 0).
    // Note: the TypeScript comment says "here 0 works so DO NOT CHANGE!" for the
    // initial fill, matching the portChannelOffsetMap[0] = 0 convention.
    let mut ports = vec![0u8; mid.tracks.len()];

    // channels_amount = 16 + max(port_channel_offset_map)
    let max_offset = mid
        .port_channel_offset_map
        .iter()
        .copied()
        .max()
        .unwrap_or(0) as usize;
    let channels_amount = 16 + max_offset;

    let mut channels_info: Vec<ChannelInfo> = (0..channels_amount)
        .map(|i| ChannelInfo {
            program: 0,
            drums: i % 16 == DEFAULT_PERCUSSION as usize,
            last_bank_idx: None,
            last_bank_lsb_idx: None,
            has_bank_select: false,
        })
        .collect();

    // Build a tick-sorted event list: (ticks, track_idx, event_idx).
    // Stable sort: within the same (tick, track_idx) the original event order is preserved.
    let mut event_list: Vec<(u32, usize, usize)> = mid
        .tracks
        .iter()
        .enumerate()
        .flat_map(|(ti, track)| {
            track
                .events
                .iter()
                .enumerate()
                .map(move |(ei, e)| (e.ticks, ti, ei))
        })
        .collect();
    event_list.sort_by_key(|&(ticks, ti, _)| (ticks, ti));

    // ── Scan pass ────────────────────────────────────────────────────────────
    // Note: like TS 4.3.0, all event mutations (program number and bank-select data bytes) are
    // applied *immediately* during the scan, not batched — so a later program change on the
    // same channel reads the already-rewritten value of a shared bank-select event, exactly as
    // `ch.lastBank.data[1] = ...` behaves in rmidi.ts.
    for &(_, track_idx, event_idx) in &event_list {
        // `event_status` is mutable: it is updated in-place when a SysEx event is replaced with
        // a regular Controller Change / Program Change below (mirrors TS reassigning its local
        // `e` to `newEvent` so that `e.statusByte` reflects the replacement).
        let mut event_status = mid.tracks[track_idx].events[event_idx].status_byte;
        // Clone data to avoid borrow conflicts inside the loop body. Also updated in-place on
        // SysEx replacement (mirrors TS's `e.data`).
        let mut event_data = mid.tracks[track_idx].events[event_idx].data.clone();

        let port_offset = mid
            .port_channel_offset_map
            .get(ports[track_idx] as usize)
            .copied()
            .unwrap_or(0) as usize;

        // MIDI_PORT: update port for this track
        if event_status == midi_message_types::MIDI_PORT {
            if let Some(&p) = event_data.first() {
                ports[track_idx] = p;
            }
            continue;
        }

        // `status` is captured once, from the *original* event, and is intentionally never
        // updated after a SysEx→CC/PC in-place replacement below (see the module doc comment
        // for the upstream quirk this faithfully preserves: this makes the `PROGRAM_CHANGE`
        // branch below unreachable for SysEx-derived program changes).
        let status = event_status & 0xf0;
        if status != midi_message_types::CONTROLLER_CHANGE
            && status != midi_message_types::PROGRAM_CHANGE
            && event_status != midi_message_types::SYSTEM_EXCLUSIVE
        {
            continue;
        }

        // ── SysEx ────────────────────────────────────────────────────────────
        if event_status == midi_message_types::SYSTEM_EXCLUSIVE {
            match MidiUtils::analyze_sysex(&event_data) {
                // Check for drum sysex
                AnalyzedMidiMessage::DrumsOn { channel, is_drum } => {
                    let sysex_channel = channel as usize + port_offset;
                    if sysex_channel < channels_info.len() {
                        channels_info[sysex_channel].drums = is_drum;
                    }
                    continue;
                }
                // Check for XG
                AnalyzedMidiMessage::XgReset => {
                    system = MIDISystem::Xg;
                    continue;
                }
                AnalyzedMidiMessage::GsReset => {
                    system = MIDISystem::Gs;
                    continue;
                }
                AnalyzedMidiMessage::GmOn | AnalyzedMidiMessage::GmOff => {
                    // We do not want GM1; this event is removed later if system stays GM.
                    system = MIDISystem::Gm;
                    continue;
                }
                AnalyzedMidiMessage::Gm2On => {
                    system = MIDISystem::Gm2;
                    continue;
                }
                AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller,
                    value,
                } => {
                    // Replace the system exclusive with a regular controller change.
                    let ticks = mid.tracks[track_idx].events[event_idx].ticks;
                    let new_status = midi_message_types::CONTROLLER_CHANGE | channel;
                    let new_data = vec![controller, value];
                    mid.tracks[track_idx].events[event_idx] =
                        MidiMessage::new(ticks, new_status, new_data.clone());
                    event_status = new_status;
                    event_data = new_data;
                    SpessaLog::info("Replaced a system exclusive with controller change!");
                    // Do not `continue`; keep parsing (matches upstream TS's `break`).
                }
                AnalyzedMidiMessage::ProgramChange { channel, value } => {
                    // Replace the system exclusive with a regular program change.
                    let ticks = mid.tracks[track_idx].events[event_idx].ticks;
                    let new_status = midi_message_types::PROGRAM_CHANGE | channel;
                    let new_data = vec![value];
                    mid.tracks[track_idx].events[event_idx] =
                        MidiMessage::new(ticks, new_status, new_data.clone());
                    event_status = new_status;
                    event_data = new_data;
                    SpessaLog::info("Replaced a system exclusive with program change!");
                    // Do not `continue`; keep parsing (matches upstream TS's `break`).
                }
                AnalyzedMidiMessage::Other
                | AnalyzedMidiMessage::ReverbParam
                | AnalyzedMidiMessage::ChorusParam
                | AnalyzedMidiMessage::DelayParam
                | AnalyzedMidiMessage::VariationParam
                | AnalyzedMidiMessage::InsertionParam
                | AnalyzedMidiMessage::DrumSetup
                | AnalyzedMidiMessage::MasterKeyShift { .. }
                | AnalyzedMidiMessage::KeyShift { .. }
                | AnalyzedMidiMessage::MasterFineTune { .. }
                | AnalyzedMidiMessage::FineTune { .. }
                | AnalyzedMidiMessage::RandomPan { .. } => {
                    continue;
                }
            }
        }

        // ── Channel message ───────────────────────────────────────────────────
        // Uses the (possibly just-replaced) fresh status byte, matching TS's `e.statusByte`.
        let ch_num = (event_status & 0xf) as usize + port_offset;
        if ch_num >= channels_info.len() {
            continue;
        }

        if status == midi_message_types::PROGRAM_CHANGE {
            let sent_program = event_data.first().copied().unwrap_or(0);

            // Read the bank bytes from the previously seen bank-select events.
            let last_bank_data: u8 =
                if let Some((ti, ei)) = channels_info[ch_num].last_bank_idx {
                    mid.tracks[ti].events[ei].data.get(1).copied().unwrap_or(0)
                } else {
                    0
                };
            let last_bank_lsb_data: u8 =
                if let Some((ti, ei)) = channels_info[ch_num].last_bank_lsb_idx {
                    mid.tracks[ti].events[ei].data.get(1).copied().unwrap_or(0)
                } else {
                    0
                };

            let patch = MidiPatch {
                program: sent_program,
                bank_lsb: last_bank_lsb_data,
                // Equivalent to: subtractBankOffset(ch.lastBank?.data?.[1] ?? 0,
                //   mid.bankOffset, system === "xg")
                bank_msb: BankSelectHacks::subtract_bank_offset(
                    last_bank_data,
                    mid.bank_offset as u8,
                    system == MIDISystem::Xg,
                ),
                is_gm_gs_drum: channels_info[ch_num].drums,
            };

            let Some(target_preset) = sound_bank.get_preset(patch, system) else {
                continue;
            };

            // Copy the fields we need out of the preset (which borrows `sound_bank`, not
            // `mid`) so `mid` can be mutated below.
            let target_program = target_preset.program;
            let target_bank_msb = target_preset.bank_msb;
            let target_bank_lsb = target_preset.bank_lsb;
            let target_is_gm_gs_drum = target_preset.is_gm_gs_drum;

            // Set the program number (immediately, like TS's `e.data[0] = ...`).
            if let Some(b) = mid.tracks[track_idx].events[event_idx].data.get_mut(0) {
                *b = target_program;
            }

            // If GM/GS drums are returned in an XG context, leave bank selects as-is.
            if target_is_gm_gs_drum && BankSelectHacks::is_system_xg(system) {
                continue;
            }

            if let Some((lb_ti, lb_ei)) = channels_info[ch_num].last_bank_idx {
                // Equivalent to: ch.lastBank.data[1] =
                //   addBankOffset(targetPreset.bankMSB, bankOffset, system === "xg")
                let new_bank = BankSelectHacks::add_bank_offset(
                    target_bank_msb,
                    bank_offset,
                    system == MIDISystem::Xg,
                );
                if let Some(b) = mid.tracks[lb_ti].events[lb_ei].data.get_mut(1) {
                    *b = new_bank;
                }
            } else {
                // TS: `if (ch.lastBank === undefined) return;` — no MSB means the LSB is not
                // touched either.
                continue;
            }
            if let Some((lbl_ti, lbl_ei)) = channels_info[ch_num].last_bank_lsb_idx {
                // Equivalent to: ch.lastBankLSB.data[1] = targetPreset.bankLSB
                if let Some(b) = mid.tracks[lbl_ti].events[lbl_ei].data.get_mut(1) {
                    *b = target_bank_lsb;
                }
            }
            continue;
        }

        // ── Controller change: only care about bank-select MSB/LSB ─────────
        let cc_num = event_data.first().copied().unwrap_or(0xff);
        let is_lsb = cc_num == midi_controllers::BANK_SELECT_LSB;
        let is_msb = cc_num == midi_controllers::BANK_SELECT;
        if !is_msb && !is_lsb {
            continue;
        }
        channels_info[ch_num].has_bank_select = true;
        if is_lsb {
            channels_info[ch_num].last_bank_lsb_idx = Some((track_idx, event_idx));
        } else {
            channels_info[ch_num].last_bank_idx = Some((track_idx, event_idx));
        }
    }

    // ── Add missing bank selects ──────────────────────────────────────────────
    for (ch, ch_info) in channels_info.iter().enumerate() {
        if ch_info.has_bank_select {
            continue;
        }
        let midi_channel = (ch % 16) as u8;
        let port_offset = (ch / 16) * 16;

        // Map offset back to port number.
        let port = match mid
            .port_channel_offset_map
            .iter()
            .position(|&o| o as usize == port_offset)
        {
            Some(p) => p as u32,
            None => continue,
        };

        // Find a track that uses this port and this MIDI channel.
        let track_idx = match mid
            .tracks
            .iter()
            .position(|t| t.port == port && t.channels.contains(&midi_channel))
        {
            Some(ti) => ti,
            None => continue,
        };

        let pc_status = midi_message_types::PROGRAM_CHANGE | midi_channel;

        // Find the index of the first program-change event for this channel.
        let mut index_to_add = mid.tracks[track_idx]
            .events
            .iter()
            .position(|e| e.status_byte == pc_status);

        if index_to_add.is_none() {
            // No program change — find the first voice event for this channel.
            let voice_idx = mid.tracks[track_idx].events.iter().position(|e| {
                e.status_byte > 0x80
                    && e.status_byte < 0xf0
                    && (e.status_byte & 0xf) == midi_channel
            });
            let voice_idx = match voice_idx {
                Some(vi) => vi,
                None => continue,
            };
            let program_ticks = mid.tracks[track_idx].events[voice_idx].ticks;
            let target_program = sound_bank
                .get_preset(
                    MidiPatch {
                        bank_msb: 0,
                        bank_lsb: 0,
                        program: 0,
                        is_gm_gs_drum: false,
                    },
                    system,
                )
                .map(|p| p.program)
                .unwrap_or(0);
            let pc_event = MidiMessage::new(program_ticks, pc_status, vec![target_program]);
            mid.tracks[track_idx].add_event(pc_event, voice_idx);
            index_to_add = Some(voice_idx);
        }

        let idx = index_to_add.unwrap();
        let ticks = mid.tracks[track_idx].events[idx].ticks;

        let target_preset = match sound_bank.get_preset(
            MidiPatch {
                bank_lsb: 0,
                bank_msb: 0,
                program: ch_info.program,
                is_gm_gs_drum: ch_info.drums,
            },
            system,
        ) {
            Some(p) => p,
            None => continue,
        };

        // Equivalent to: addBankOffset(targetPreset.bankMSB, bankOffset, system === "xg")
        let target_bank = BankSelectHacks::add_bank_offset(
            target_preset.bank_msb,
            bank_offset,
            system == MIDISystem::Xg,
        );

        let bs_event = MidiMessage::new(
            ticks,
            midi_message_types::CONTROLLER_CHANGE | midi_channel,
            vec![midi_controllers::BANK_SELECT, target_bank],
        );
        mid.tracks[track_idx].add_event(bs_event, idx);
    }

    // ── GM → GS switch ────────────────────────────────────────────────────────
    // If no GS/XG/GM2 mode was detected, replace all GM ON/OFF sysex events with
    // a Roland GS ON message at the beginning of track 0.
    if system == MIDISystem::Gm {
        for track in mid.tracks.iter_mut() {
            track.events.retain(|e| {
                !(e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE
                    && matches!(
                        MidiUtils::analyze_sysex(&e.data),
                        AnalyzedMidiMessage::GmOn | AnalyzedMidiMessage::GmOff
                    ))
            });
        }
        if !mid.tracks.is_empty() {
            let index = if !mid.tracks[0].events.is_empty()
                && mid.tracks[0].events[0].status_byte == midi_message_types::TRACK_NAME
            {
                1
            } else {
                0
            };
            mid.tracks[0].add_event(MidiUtils::gs_reset(0), index);
        }
    }
    mid.flush(true);
}

// ─────────────────────────────────────────────────────────────────────────────
// writeRMIDIInternal
// ─────────────────────────────────────────────────────────────────────────────

/// Writes an RMIDI file.
///
/// This method modifies `mid` in-place (applies bank corrections, stores metadata
/// in `mid.rmidi_info`).
///
/// # Parameters
/// - `mid`:               The MIDI data to embed.
/// - `sound_bank_binary`: Raw SF2/DLS bytes to append to the RIFF structure.
/// - `options`:           Write options (bank offset, metadata, correction flag).
/// - `sound_bank`:        Required if `options.correct_bank_offset` is `true`.
///
/// # Returns
/// The raw bytes of the RMIDI file on success, or an error string.
///
/// Equivalent to: writeRMIDIInternal
pub fn write_rmidi_internal(
    mid: &mut BasicMidi,
    sound_bank_binary: &[u8],
    options: RMIDIWriteOptions,
    sound_bank: Option<&dyn PresetResolver>,
) -> Result<Vec<u8>, String> {
    let bank_offset = options.bank_offset;

    if options.correct_bank_offset {
        let sb = sound_bank.ok_or_else(|| {
            "Sound bank must be provided if correcting bank offset.".to_string()
        })?;
        correct_bank_offset_internal(mid, bank_offset, sb);
    }

    // Serialize the (possibly corrected) MIDI to bytes before touching rmidi_info.
    let midi_bytes = write_midi_internal(mid);

    // Destructure metadata options.
    let RMIDInfoDataPartial {
        name,
        engineer,
        artist,
        album,
        genre,
        picture,
        comment,
        creation_date,
        copyright,
        midi_encoding,
        software,
        subject,
    } = options.metadata;

    // Apply defaults (matches TypeScript's `metadata.xxx ??= ...`).
    let name = name.unwrap_or_else(|| get_name(mid));
    let copyright = copyright.unwrap_or_else(|| DEFAULT_COPYRIGHT.to_string());
    let software = software.unwrap_or_else(|| "SpessaSynth".to_string());

    // Always set info encoding to UTF-8 when metadata is written.
    mid.rmidi_info
        .insert("infoEncoding".to_string(), encode_string_bytes("utf-8"));

    // Apply string metadata fields.
    if !name.is_empty() {
        mid.rmidi_info
            .insert("name".to_string(), encode_string_bytes(&name));
    }
    if !copyright.is_empty() {
        mid.rmidi_info
            .insert("copyright".to_string(), encode_string_bytes(&copyright));
    }
    if !software.is_empty() {
        mid.rmidi_info
            .insert("software".to_string(), encode_string_bytes(&software));
    }

    // Creation date: use provided value or current UTC time.
    let date_str = if let Some(dt) = creation_date {
        dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
    } else {
        chrono::Utc::now()
            .naive_utc()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string()
    };
    mid.rmidi_info
        .insert("creationDate".to_string(), encode_string_bytes(&date_str));

    // Apply optional metadata fields.
    if let Some(v) = artist && !v.is_empty() {
        mid.rmidi_info
            .insert("artist".to_string(), encode_string_bytes(&v));
    }
    if let Some(v) = album && !v.is_empty() {
        mid.rmidi_info
            .insert("album".to_string(), encode_string_bytes(&v));
    }
    if let Some(v) = genre && !v.is_empty() {
        mid.rmidi_info
            .insert("genre".to_string(), encode_string_bytes(&v));
    }
    if let Some(v) = picture && !v.is_empty() {
        mid.rmidi_info.insert("picture".to_string(), v);
    }
    if let Some(v) = comment && !v.is_empty() {
        mid.rmidi_info
            .insert("comment".to_string(), encode_string_bytes(&v));
    }
    if let Some(v) = engineer && !v.is_empty() {
        mid.rmidi_info
            .insert("engineer".to_string(), encode_string_bytes(&v));
    }
    if let Some(v) = subject && !v.is_empty() {
        mid.rmidi_info
            .insert("subject".to_string(), encode_string_bytes(&v));
    }
    if let Some(v) = midi_encoding && !v.is_empty() {
        mid.rmidi_info
            .insert("midiEncoding".to_string(), encode_string_bytes(&v));
    }

    // Build INFO sub-chunks from rmidi_info.
    // Sort keys for deterministic output order.
    let mut keys: Vec<String> = mid.rmidi_info.keys().cloned().collect();
    keys.sort();

    let mut info_content: Vec<Vec<u8>> = Vec::new();
    for key in &keys {
        let data = mid.rmidi_info[key].as_slice();
        match key.as_str() {
            "album" => {
                info_content.push(RIFFChunk::write("IALB", data, false, false).to_vec());
                info_content.push(RIFFChunk::write("IPRD", data, false, false).to_vec());
            }
            "software" => {
                info_content.push(RIFFChunk::write("ISFT", data, false, false).to_vec());
            }
            "infoEncoding" => {
                info_content.push(RIFFChunk::write("IENC", data, false, false).to_vec());
            }
            "creationDate" => {
                info_content.push(RIFFChunk::write("ICRD", data, false, false).to_vec());
            }
            "picture" => {
                info_content.push(RIFFChunk::write("IPIC", data, false, false).to_vec());
            }
            "name" => {
                info_content.push(RIFFChunk::write("INAM", data, false, false).to_vec());
            }
            "artist" => {
                info_content.push(RIFFChunk::write("IART", data, false, false).to_vec());
            }
            "genre" => {
                info_content.push(RIFFChunk::write("IGNR", data, false, false).to_vec());
            }
            "copyright" => {
                info_content.push(RIFFChunk::write("ICOP", data, false, false).to_vec());
            }
            "comment" => {
                info_content.push(RIFFChunk::write("ICMT", data, false, false).to_vec());
            }
            "engineer" => {
                info_content.push(RIFFChunk::write("IENG", data, false, false).to_vec());
            }
            "subject" => {
                info_content.push(RIFFChunk::write("ISBJ", data, false, false).to_vec());
            }
            "midiEncoding" => {
                info_content.push(RIFFChunk::write("MENC", data, false, false).to_vec());
            }
            _ => {}
        }
    }

    // DBNK chunk: bank offset as 2-byte little-endian value.
    let dbnk_bytes = (bank_offset as u16).to_le_bytes();
    info_content.push(RIFFChunk::write("DBNK", &dbnk_bytes, false, false).to_vec());

    // Assemble: RIFF { "RMID" + data_chunk + LIST/INFO_chunk + sound_bank_binary }
    let data_chunk = RIFFChunk::write("data", &midi_bytes, false, false);
    let info_refs: Vec<&[u8]> = info_content.iter().map(|v| v.as_slice()).collect();
    let info_chunk = RIFFChunk::write_parts("INFO", &info_refs, true);
    let parts: &[&[u8]] = &[
        b"RMID",
        &data_chunk,
        &info_chunk,
        sound_bank_binary,
    ];
    let result = RIFFChunk::write_parts("RIFF", parts, false);

    Ok(result.to_vec())
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
    use crate::midi::types::RMIDIWriteOptions;
    use crate::soundbank::basic_soundbank::basic_preset::BasicPreset;
    use crate::soundbank::basic_soundbank::preset_resolver::PresetResolver;
    use crate::soundbank::types::MIDISystem;

    // ── Test helpers ──────────────────────────────────────────────────────────

    struct MockBank {
        preset: BasicPreset,
    }

    impl MockBank {
        fn new() -> Self {
            Self {
                preset: BasicPreset::default(),
            }
        }
        fn with_program(program: u8) -> Self {
            let mut preset = BasicPreset::default();
            preset.program = program;
            Self { preset }
        }
    }

    impl PresetResolver for MockBank {
        fn get_preset(&self, _patch: MidiPatch, _system: MIDISystem) -> Option<&BasicPreset> {
            Some(&self.preset)
        }
    }

    /// Like `MockBank`, but records every `MidiPatch` it is queried with, so tests can assert
    /// what bank/program values `correct_bank_offset_internal` actually looked up.
    struct RecordingBank {
        preset: BasicPreset,
        queries: std::cell::RefCell<Vec<MidiPatch>>,
    }

    impl RecordingBank {
        fn new(preset: BasicPreset) -> Self {
            Self {
                preset,
                queries: std::cell::RefCell::new(Vec::new()),
            }
        }
    }

    impl PresetResolver for RecordingBank {
        fn get_preset(&self, patch: MidiPatch, _system: MIDISystem) -> Option<&BasicPreset> {
            self.queries.borrow_mut().push(patch);
            Some(&self.preset)
        }
    }

    fn make_msg(ticks: u32, status: u8, data: Vec<u8>) -> MidiMessage {
        MidiMessage::new(ticks, status, data)
    }

    fn make_simple_midi() -> BasicMidi {
        let mut mid = BasicMidi::new();
        mid.time_division = 480;
        let mut track = MidiTrack::new();
        track.push_event(make_msg(0, midi_message_types::TRACK_NAME, b"Test".to_vec()));
        track.push_event(make_msg(0, midi_message_types::SET_TEMPO, vec![0x07, 0xA1, 0x20]));
        track.push_event(make_msg(0, 0x90, vec![60, 100])); // note-on ch 0
        track.push_event(make_msg(480, 0x80, vec![60, 0])); // note-off ch 0
        track.push_event(make_msg(960, midi_message_types::END_OF_TRACK, vec![]));
        mid.tracks.push(track);
        mid.flush(false);
        mid
    }

    fn find_fourcc(data: &[u8], tag: &[u8; 4]) -> Option<usize> {
        data.windows(4).position(|w| w == tag)
    }

    // ── get_name ──────────────────────────────────────────────────────────────

    #[test]
    fn test_get_name_from_rmidi_info() {
        let mut mid = BasicMidi::new();
        mid.rmidi_info
            .insert("name".to_string(), encode_string_bytes("Hello Song"));
        assert_eq!(get_name(&mid), "Hello Song");
    }

    #[test]
    fn test_get_name_from_rmidi_info_strips_null_terminator() {
        let mut mid = BasicMidi::new();
        // The null byte should be stripped
        mid.rmidi_info.insert("name".to_string(), b"My Song\0".to_vec());
        assert_eq!(get_name(&mid), "My Song");
    }

    #[test]
    fn test_get_name_from_binary_name() {
        let mut mid = BasicMidi::new();
        mid.binary_name = Some(b"Binary Name".to_vec());
        assert_eq!(get_name(&mid), "Binary Name");
    }

    #[test]
    fn test_get_name_from_file_name() {
        let mut mid = BasicMidi::new();
        mid.file_name = Some("song.mid".to_string());
        assert_eq!(get_name(&mid), "song.mid");
    }

    #[test]
    fn test_get_name_rmidi_info_has_priority_over_binary_name() {
        let mut mid = BasicMidi::new();
        mid.rmidi_info
            .insert("name".to_string(), encode_string_bytes("RMIDI Name"));
        mid.binary_name = Some(b"Binary Name".to_vec());
        assert_eq!(get_name(&mid), "RMIDI Name");
    }

    #[test]
    fn test_get_name_binary_name_has_priority_over_file_name() {
        let mut mid = BasicMidi::new();
        mid.binary_name = Some(b"Binary".to_vec());
        mid.file_name = Some("file.mid".to_string());
        assert_eq!(get_name(&mid), "Binary");
    }

    #[test]
    fn test_get_name_empty_if_all_missing() {
        let mid = BasicMidi::new();
        assert_eq!(get_name(&mid), "");
    }

    #[test]
    fn test_get_name_whitespace_only_falls_through() {
        let mut mid = BasicMidi::new();
        mid.binary_name = Some(b"   ".to_vec());
        mid.file_name = Some("fallback.mid".to_string());
        assert_eq!(get_name(&mid), "fallback.mid");
    }

    // ── encode_string_bytes ────────────────────────────────────────────────────

    #[test]
    fn test_encode_string_bytes_adds_null() {
        let b = encode_string_bytes("abc");
        assert_eq!(b, b"abc\0");
    }

    #[test]
    fn test_encode_string_bytes_empty_string() {
        let b = encode_string_bytes("");
        assert_eq!(b, b"\0");
    }

    // ── write_rmidi_internal ──────────────────────────────────────────────────

    #[test]
    fn test_write_rmidi_starts_with_riff() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions::default();
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        assert_eq!(&out[0..4], b"RIFF", "expected RIFF header");
    }

    #[test]
    fn test_write_rmidi_rmid_type_at_offset_8() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions::default();
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        assert_eq!(&out[8..12], b"RMID", "expected RMID type at offset 8");
    }

    #[test]
    fn test_write_rmidi_contains_data_chunk() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions::default();
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        assert!(
            find_fourcc(&out, b"data").is_some(),
            "expected 'data' chunk"
        );
    }

    #[test]
    fn test_write_rmidi_contains_list_chunk() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions::default();
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        // LIST/INFO chunk
        assert!(find_fourcc(&out, b"LIST").is_some(), "expected LIST chunk");
        assert!(find_fourcc(&out, b"INFO").is_some(), "expected INFO type");
    }

    #[test]
    fn test_write_rmidi_contains_inam_chunk() {
        let mut mid = make_simple_midi();
        let mut opts = RMIDIWriteOptions::default();
        opts.metadata.name = Some("My Track".to_string());
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        assert!(find_fourcc(&out, b"INAM").is_some(), "expected INAM chunk");
    }

    #[test]
    fn test_write_rmidi_inam_contains_name_bytes() {
        let mut mid = make_simple_midi();
        let mut opts = RMIDIWriteOptions::default();
        opts.metadata.name = Some("MySong".to_string());
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        let pos = out.windows(6).position(|w| w == b"MySong").unwrap();
        assert!(pos > 0);
    }

    #[test]
    fn test_write_rmidi_contains_icop_chunk_with_default_copyright() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions::default();
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        assert!(find_fourcc(&out, b"ICOP").is_some(), "expected ICOP chunk");
        // Default copyright text should appear in the output
        let copyright_bytes = DEFAULT_COPYRIGHT.as_bytes();
        assert!(
            out.windows(copyright_bytes.len())
                .any(|w| w == copyright_bytes),
            "expected default copyright bytes"
        );
    }

    #[test]
    fn test_write_rmidi_contains_isft_chunk() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions::default();
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        assert!(find_fourcc(&out, b"ISFT").is_some(), "expected ISFT chunk");
    }

    #[test]
    fn test_write_rmidi_contains_icrd_chunk() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions::default();
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        assert!(find_fourcc(&out, b"ICRD").is_some(), "expected ICRD chunk");
    }

    #[test]
    fn test_write_rmidi_contains_ienc_chunk() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions::default();
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        assert!(find_fourcc(&out, b"IENC").is_some(), "expected IENC chunk");
    }

    #[test]
    fn test_write_rmidi_contains_dbnk_chunk() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions::default();
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        assert!(find_fourcc(&out, b"DBNK").is_some(), "expected DBNK chunk");
    }

    #[test]
    fn test_write_rmidi_dbnk_value_zero_by_default() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions::default();
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        let dbnk_pos = find_fourcc(&out, b"DBNK").unwrap();
        // DBNK chunk: [D][B][N][K][size LE 4B][data 2B]
        let data_start = dbnk_pos + 8;
        assert_eq!(out[data_start], 0, "bank offset LSB should be 0");
        assert_eq!(out[data_start + 1], 0, "bank offset MSB should be 0");
    }

    #[test]
    fn test_write_rmidi_dbnk_value_nonzero() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions {
            bank_offset: 5,
            ..Default::default()
        };
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        let dbnk_pos = find_fourcc(&out, b"DBNK").unwrap();
        let data_start = dbnk_pos + 8;
        assert_eq!(out[data_start], 5, "bank offset LSB should be 5");
        assert_eq!(out[data_start + 1], 0, "bank offset MSB should be 0");
    }

    #[test]
    fn test_write_rmidi_correct_bank_false_no_soundbank_ok() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions {
            correct_bank_offset: false,
            ..Default::default()
        };
        // Should succeed without a sound bank
        assert!(write_rmidi_internal(&mut mid, b"", opts, None).is_ok());
    }

    #[test]
    fn test_write_rmidi_correct_bank_true_no_soundbank_err() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions {
            correct_bank_offset: true,
            ..Default::default()
        };
        let result = write_rmidi_internal(&mut mid, b"", opts, None);
        assert!(result.is_err(), "expected error when soundbank missing");
    }

    #[test]
    fn test_write_rmidi_sound_bank_binary_appended() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions::default();
        let sb_data = b"SFDATA_SENTINEL";
        let out = write_rmidi_internal(&mut mid, sb_data, opts, None).unwrap();
        // The sound bank binary should appear near the end
        let pos = out
            .windows(sb_data.len())
            .position(|w| w == sb_data.as_slice());
        assert!(pos.is_some(), "sound bank binary should appear in output");
    }

    #[test]
    fn test_write_rmidi_custom_metadata_artist() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions {
            metadata: RMIDInfoDataPartial {
                artist: Some("Test Artist".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        assert!(find_fourcc(&out, b"IART").is_some(), "expected IART chunk");
        let artist_bytes = b"Test Artist";
        assert!(
            out.windows(artist_bytes.len()).any(|w| w == artist_bytes),
            "expected artist bytes"
        );
    }

    #[test]
    fn test_write_rmidi_custom_metadata_album_writes_ialb_and_iprd() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions {
            metadata: RMIDInfoDataPartial {
                album: Some("My Album".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        // Album generates both IALB and IPRD chunks
        assert!(find_fourcc(&out, b"IALB").is_some(), "expected IALB chunk");
        assert!(find_fourcc(&out, b"IPRD").is_some(), "expected IPRD chunk");
    }

    #[test]
    fn test_write_rmidi_data_chunk_contains_midi_header() {
        let mut mid = make_simple_midi();
        let opts = RMIDIWriteOptions::default();
        let out = write_rmidi_internal(&mut mid, b"", opts, None).unwrap();
        // The MIDI bytes (starting with "MThd") should appear inside the data chunk
        let mthd_pos = out.windows(4).position(|w| w == b"MThd");
        assert!(mthd_pos.is_some(), "MIDI MThd header should be present");
    }

    // ── correct_bank_offset_internal ─────────────────────────────────────────

    #[test]
    fn test_correct_bank_offset_gm_adds_gs_on_at_start() {
        // A MIDI with a GM ON sysex — should be replaced with GS ON.
        let gm_on_sysex = MidiMessage::new(
            0,
            midi_message_types::SYSTEM_EXCLUSIVE,
            vec![0x7e, 0x7f, 0x09, 0x01, 0xf7], // GM1 ON
        );

        let mut mid = BasicMidi::new();
        mid.time_division = 480;
        let mut track = MidiTrack::new();
        track.push_event(make_msg(0, midi_message_types::TRACK_NAME, b"".to_vec()));
        track.push_event(gm_on_sysex);
        track.push_event(make_msg(0, 0x90, vec![60, 100]));
        track.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        mid.tracks.push(track);
        mid.flush(false);

        let bank = MockBank::new();
        correct_bank_offset_internal(&mut mid, 0, &bank);

        // GM ON sysex should be gone
        let has_gm_on = mid.tracks[0].events.iter().any(|e| {
            e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE
                && MidiUtils::analyze_sysex(&e.data) == AnalyzedMidiMessage::GmOn
        });
        assert!(!has_gm_on, "GM ON sysex should have been removed");

        // GS ON sysex should be present
        let has_gs_on = mid.tracks[0].events.iter().any(|e| {
            e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE && e.data.len() >= 7
                && e.data[0] == 0x41 // Roland
                && e.data[2] == 0x42 // GS
        });
        assert!(has_gs_on, "GS ON sysex should have been added");
    }

    #[test]
    fn test_correct_bank_offset_gs_system_keeps_gs_no_replacement() {
        // If GS ON is present, system is "gs" and no GM → GS replacement happens.
        let gs_on = MidiMessage::new(
            0,
            midi_message_types::SYSTEM_EXCLUSIVE,
            vec![0x41, 0x10, 0x42, 0x12, 0x40, 0x00, 0x7f, 0x00, 0x41, 0xf7],
        );
        let mut mid = BasicMidi::new();
        mid.time_division = 480;
        let mut track = MidiTrack::new();
        track.push_event(make_msg(0, midi_message_types::TRACK_NAME, b"".to_vec()));
        track.push_event(gs_on);
        track.push_event(make_msg(0, 0x90, vec![60, 100]));
        track.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        mid.tracks.push(track);
        mid.flush(false);

        let initial_gs_count = mid.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE)
            .count();

        let bank = MockBank::new();
        correct_bank_offset_internal(&mut mid, 0, &bank);

        let final_gs_count = mid.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE)
            .count();

        // GS ON should remain; no additional sysex should be inserted
        assert_eq!(
            initial_gs_count, final_gs_count,
            "GS system: sysex count should not change"
        );
    }

    #[test]
    fn test_correct_bank_offset_program_change_updated() {
        // MockBank always returns program=7; verify program change is updated.
        let mut mid = BasicMidi::new();
        mid.time_division = 480;
        let mut track = MidiTrack::new();
        track.push_event(make_msg(0, midi_message_types::TRACK_NAME, b"".to_vec()));
        // Bank select CC0 on ch 0: value 0
        track.push_event(make_msg(0, 0xB0, vec![0x00, 0x00]));
        // Program change ch 0: program 10
        track.push_event(make_msg(0, 0xC0, vec![10]));
        track.push_event(make_msg(0, 0x90, vec![60, 100]));
        track.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        mid.tracks.push(track);
        mid.flush(false);

        let bank = MockBank::with_program(7);
        correct_bank_offset_internal(&mut mid, 0, &bank);

        // Find program change event
        let pc_event = mid.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == 0xC0);
        assert!(pc_event.is_some(), "program change event should exist");
        assert_eq!(
            pc_event.unwrap().data[0],
            7,
            "program should be updated to MockBank's program (7)"
        );
    }

    #[test]
    fn test_correct_bank_offset_bank_msb_updated() {
        // MockBank returns preset with bank_msb=3; bank_offset=1 → written as 4.
        let mut preset = BasicPreset::default();
        preset.program = 0;
        preset.bank_msb = 3;
        let bank = MockBank { preset };

        let mut mid = BasicMidi::new();
        mid.time_division = 480;
        let mut track = MidiTrack::new();
        track.push_event(make_msg(0, midi_message_types::TRACK_NAME, b"".to_vec()));
        track.push_event(make_msg(0, 0xB0, vec![0x00, 0x00])); // CC0 (bank MSB)
        track.push_event(make_msg(0, 0xC0, vec![0])); // Program change
        track.push_event(make_msg(0, 0x90, vec![60, 100]));
        track.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        mid.tracks.push(track);
        mid.flush(false);

        correct_bank_offset_internal(&mut mid, 1, &bank);

        // Find the CC0 event and check its value
        let cc0 = mid.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == 0xB0 && e.data.first() == Some(&0x00));
        assert!(cc0.is_some(), "bank select CC0 should exist");
        assert_eq!(
            cc0.unwrap().data[1],
            4, // bankMSB(3) + bankOffset(1) = 4
            "bank MSB should be preset.bank_msb + bank_offset"
        );
    }

    #[test]
    fn test_correct_bank_offset_no_bank_select_adds_one() {
        // Channel 0 has a voice event but no bank select — one should be inserted.
        let mut mid = BasicMidi::new();
        mid.time_division = 480;
        let mut track = MidiTrack::new();
        track.push_event(make_msg(0, midi_message_types::TRACK_NAME, b"".to_vec()));
        track.push_event(make_msg(0, 0xC0, vec![5])); // Program change ch 0
        track.push_event(make_msg(0, 0x90, vec![60, 100])); // Note-on ch 0
        track.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        mid.tracks.push(track);
        mid.flush(false);

        let bank = MockBank::new();
        correct_bank_offset_internal(&mut mid, 0, &bank);

        // A bank select CC0 event should now be present on ch 0
        let has_bank_select = mid.tracks[0]
            .events
            .iter()
            .any(|e| e.status_byte == 0xB0 && e.data.first() == Some(&0x00));
        assert!(has_bank_select, "bank select should have been inserted");
    }

    // ── 4.3.0: SysEx→CC/PC replacement (MIDIUtils.analyzeSysEx) ─────────────

    #[test]
    fn test_correct_bank_offset_xg_sysex_bank_select_replaced_with_cc() {
        // XG SysEx-encoded Bank Select MSB (a1=0x08 multi-part, channel 0, a3=0x01) should be
        // replaced in-place with a regular Controller Change (bank select) event — new in 4.3.0.
        let xg_bank_select = MidiMessage::new(
            0,
            midi_message_types::SYSTEM_EXCLUSIVE,
            vec![0x43, 0x10, 0x4c, 0x08, 0x00, 0x01, 5], // channel 0, value 5
        );
        let mut mid = BasicMidi::new();
        mid.time_division = 480;
        let mut track = MidiTrack::new();
        track.push_event(make_msg(0, midi_message_types::TRACK_NAME, b"".to_vec()));
        track.push_event(xg_bank_select);
        // Deliberately no other program change on this channel: a subsequent PC on the same
        // channel would re-derive the bank MSB from the sound bank's preset and overwrite this
        // event's value again, which isn't what this test is checking.
        track.push_event(make_msg(0, 0x90, vec![60, 100]));
        track.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        mid.tracks.push(track);
        mid.flush(false);

        let bank = MockBank::new();
        correct_bank_offset_internal(&mut mid, 0, &bank);

        // The XG bank-select SysEx should be gone (note: since no GS/XG/GM2 reset was seen, a
        // GS Reset SysEx gets auto-inserted per the "GM → GS switch" logic below — that's an
        // unrelated, separate SysEx, not the one we're replacing here).
        let has_xg_cc_sysex = mid.tracks[0].events.iter().any(|e| {
            e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE
                && matches!(
                    MidiUtils::analyze_sysex(&e.data),
                    AnalyzedMidiMessage::ControllerChange { .. }
                )
        });
        assert!(!has_xg_cc_sysex, "XG SysEx should have been replaced");
        let replaced = mid.tracks[0].events.iter().find(|e| {
            e.status_byte == midi_message_types::CONTROLLER_CHANGE
                && e.data.first() == Some(&midi_controllers::BANK_SELECT)
        });
        assert!(
            replaced.is_some(),
            "expected a bank-select CC in place of the XG SysEx"
        );
        assert_eq!(replaced.unwrap().data[1], 5);
    }

    #[test]
    fn test_correct_bank_offset_gs_sysex_program_change_replaced_in_place() {
        // GS SysEx-encoded tone number (program change) on part 1 (→ channel 0) should be
        // replaced in-place with a regular Program Change event — new in 4.3.0.
        let gs_program_change = MidiMessage::new(
            0,
            midi_message_types::SYSTEM_EXCLUSIVE,
            vec![0x41, 0x10, 0x42, 0x12, 0x40, 0x11, 0x00, 42, 0x00, 0xf7],
        );
        let mut mid = BasicMidi::new();
        mid.time_division = 480;
        let mut track = MidiTrack::new();
        track.push_event(make_msg(0, midi_message_types::TRACK_NAME, b"".to_vec()));
        track.push_event(gs_program_change);
        track.push_event(make_msg(0, 0x90, vec![60, 100]));
        track.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        mid.tracks.push(track);
        mid.flush(false);

        let bank = MockBank::new();
        correct_bank_offset_internal(&mut mid, 0, &bank);

        // (A GS Reset SysEx gets auto-inserted separately since no GS/XG/GM2 reset was seen —
        // see the "GM → GS switch" note in the XG test above.)
        let has_gs_program_change_sysex = mid.tracks[0].events.iter().any(|e| {
            e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE
                && matches!(
                    MidiUtils::analyze_sysex(&e.data),
                    AnalyzedMidiMessage::ProgramChange { .. }
                )
        });
        assert!(
            !has_gs_program_change_sysex,
            "GS SysEx should have been replaced"
        );
        // Per the upstream quirk (see module doc comment), the replaced Program Change is NOT
        // re-resolved through the sound bank's preset lookup — it keeps the raw value (42) since
        // the post-SysEx `status === PROGRAM_CHANGE` branch never runs for it.
        let replaced = mid.tracks[0]
            .events
            .iter()
            .find(|e| e.status_byte == midi_message_types::PROGRAM_CHANGE);
        assert!(replaced.is_some(), "expected a program change event");
    }

    #[test]
    fn test_correct_bank_offset_gm_off_sysex_removed_when_system_stays_gm() {
        // "GM Off" SysEx now also flags the system as GM and gets removed if it stays GM
        // (previously only "GM On" was recognized at all; 4.3.0 added GM Off detection too).
        let gm_off_sysex = MidiMessage::new(
            0,
            midi_message_types::SYSTEM_EXCLUSIVE,
            vec![0x7e, 0x7f, 0x09, 0x02, 0xf7],
        );
        let mut mid = BasicMidi::new();
        mid.time_division = 480;
        let mut track = MidiTrack::new();
        track.push_event(make_msg(0, midi_message_types::TRACK_NAME, b"".to_vec()));
        track.push_event(gm_off_sysex);
        track.push_event(make_msg(0, 0x90, vec![60, 100]));
        track.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        mid.tracks.push(track);
        mid.flush(false);

        let bank = MockBank::new();
        correct_bank_offset_internal(&mut mid, 0, &bank);

        let has_gm_off = mid.tracks[0].events.iter().any(|e| {
            e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE
                && MidiUtils::analyze_sysex(&e.data) == AnalyzedMidiMessage::GmOff
        });
        assert!(!has_gm_off, "GM Off sysex should have been removed");
        // A GS ON should have been inserted since the system stayed GM.
        let has_gs_on = mid.tracks[0].events.iter().any(|e| {
            e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE
                && MidiUtils::analyze_sysex(&e.data) == AnalyzedMidiMessage::GsReset
        });
        assert!(has_gs_on, "GS ON sysex should have been added");
    }

    #[test]
    fn test_correct_bank_offset_gs_drums_on_does_not_panic_and_is_left_in_place() {
        // GS "Drums On" SysEx for part 0 (→ channel 9) marks that channel as a drum channel (an
        // internal side effect used for subsequent program-change patch resolution) but — unlike
        // the ControllerChange/ProgramChange SysEx cases — is neither replaced nor removed from
        // the track; it is only inspected.
        let gs_drums_on = MidiMessage::new(
            0,
            midi_message_types::SYSTEM_EXCLUSIVE,
            vec![0x41, 0x10, 0x42, 0x12, 0x40, 0x10, 0x15, 0x01, 0x00, 0xf7],
        );
        let mut mid = BasicMidi::new();
        mid.time_division = 480;
        let mut track = MidiTrack::new();
        track.push_event(make_msg(0, midi_message_types::TRACK_NAME, b"".to_vec()));
        track.push_event(gs_drums_on);
        track.push_event(make_msg(0, 0xC9, vec![5])); // Program change ch 9
        track.push_event(make_msg(0, 0x99, vec![60, 100])); // Note-on ch 9
        track.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        mid.tracks.push(track);
        mid.flush(false);

        let bank = MockBank::new();
        correct_bank_offset_internal(&mut mid, 0, &bank);
        let has_drums_on_sysex = mid.tracks[0].events.iter().any(|e| {
            e.status_byte == midi_message_types::SYSTEM_EXCLUSIVE
                && matches!(
                    MidiUtils::analyze_sysex(&e.data),
                    AnalyzedMidiMessage::DrumsOn { .. }
                )
        });
        assert!(
            has_drums_on_sysex,
            "GS Drums On sysex should be left in the track untouched"
        );
    }

    // ── XG bank-offset handling (subtractBankOffset's isXG flag) ────────────

    #[test]
    fn test_correct_bank_offset_xg_system_preserves_drum_bank_127_on_lookup() {
        // In an XG file (XG Reset seen), a bank-select MSB of 127 (XG drum bank) followed by a
        // program change must be looked up with bankMSB=127 preserved — subtractBankOffset must
        // receive isXG=true (TS: `system === "xg"`), so the file's bank offset (1 here) is NOT
        // subtracted from the XG drum bank. With the old `false` flag this would become 126.
        let xg_reset = MidiMessage::new(
            0,
            midi_message_types::SYSTEM_EXCLUSIVE,
            vec![0x43, 0x10, 0x4c, 0x00, 0x00, 0x7e, 0x00],
        );
        let mut mid = BasicMidi::new();
        mid.time_division = 480;
        let mut track = MidiTrack::new();
        track.push_event(make_msg(0, midi_message_types::TRACK_NAME, b"".to_vec()));
        track.push_event(xg_reset);
        // Bank select MSB = 127 (XG drums) on ch 0
        track.push_event(make_msg(0, 0xB0, vec![0x00, 127]));
        // Program change ch 0
        track.push_event(make_msg(0, 0xC0, vec![0]));
        track.push_event(make_msg(0, 0x90, vec![60, 100]));
        track.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        mid.tracks.push(track);
        mid.flush(false);
        // Simulate an RMIDI whose events have a bank offset of 1 baked in.
        mid.bank_offset = 1;

        let bank = RecordingBank::new(BasicPreset::default());
        correct_bank_offset_internal(&mut mid, 0, &bank);

        let queries = bank.queries.borrow();
        assert_eq!(queries.len(), 1, "expected exactly one preset lookup");
        assert_eq!(
            queries[0].bank_msb, 127,
            "XG drum bank 127 must be preserved (isXG=true), not offset-subtracted to 126"
        );
    }

    // ── Immediate write-back semantics (TS parity) ──────────────────────────

    #[test]
    fn test_correct_bank_offset_second_pc_reads_rewritten_bank_value() {
        // TS rewrites `ch.lastBank.data[1]` synchronously during the scan (rmidi.ts:198-205),
        // so when ONE bank-select is followed by TWO program changes on the same channel, the
        // second program change's lookup reads the already-rewritten bank value (the preset's
        // bankMSB), not the original file value. Verify the Rust port does the same.
        let mut preset = BasicPreset::default();
        preset.program = 0;
        preset.bank_msb = 5;
        let bank = RecordingBank::new(preset);

        let mut mid = BasicMidi::new();
        mid.time_division = 480;
        let mut track = MidiTrack::new();
        track.push_event(make_msg(0, midi_message_types::TRACK_NAME, b"".to_vec()));
        // One bank select MSB = 3 on ch 0, shared by both program changes below.
        track.push_event(make_msg(0, 0xB0, vec![0x00, 3]));
        track.push_event(make_msg(0, 0xC0, vec![10])); // PC #1
        track.push_event(make_msg(0, 0x90, vec![60, 100]));
        track.push_event(make_msg(240, 0xC0, vec![20])); // PC #2 (same channel, later tick)
        track.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        mid.tracks.push(track);
        mid.flush(false);

        correct_bank_offset_internal(&mut mid, 0, &bank);

        let queries = bank.queries.borrow();
        assert_eq!(queries.len(), 2, "expected two preset lookups");
        // PC #1 sees the original file bank value (3).
        assert_eq!(queries[0].bank_msb, 3);
        // PC #1's lookup rewrote the shared bank-select to preset.bank_msb (5, offset 0), so
        // PC #2 must see 5 — matching TS's synchronous `ch.lastBank.data[1] = ...` write.
        assert_eq!(
            queries[1].bank_msb, 5,
            "second PC should read the bank value rewritten by the first PC"
        );
    }
}
