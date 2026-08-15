/// basic_midi.rs
/// purpose: Central MIDI data structure with parse, iterate, and tick-to-seconds utilities.
/// Ported from: src/midi/basic_midi.ts (spessasynth_core 4.3.0)
///
/// Key 4.3.0 changes ported here:
/// - `BasicMIDI.fromArrayBuffer` now performs the RIFF ("RMID") / "XMF_" / plain-SMF format
///   dispatch itself (calling `parseRMIDIInternal`/`loadXMF`/`parseSMFInternal` directly) instead
///   of delegating to a single `loadMIDIFromArrayBufferInternal` — the dispatch used to live in
///   `read/midi.ts` (4.2.0's `midi_loader.ts`). This Rust port's previous (phase-1) structure had
///   the dispatch in `read/midi.rs`; it is moved here to `from_array_buffer` to match.
/// - `timeDivision`'s default changed from 0 to 480.
/// - A new `timeline: readonly Readonly<TimelineEvent>[]` field: a flattened, time-sorted list
///   of all events (as `{tr: trackNum, ev: eventIndexInTrack}` pairs), rebuilt at the end of
///   `parseInternal` via `iterate`. `iterate`'s callback signature grew a third `eventIndexes`
///   parameter to support this (and to let callers like `write/rmidi.ts` replace events in-place
///   by index).
/// - Touhou loop false-positive fix ("Touhou loop 誤検出の修正" in the 4.3.0 release notes): CC2
///   (Touhou)/CC111 (RPG Maker) loop-start markers, and CC4 (Touhou) loop-end markers, now
///   additionally require the CC value to be 0 (CC116/CC117, the dedicated EMIDI/XMI loop
///   markers, remain unconditional). A duplicate CC4/CC117 loop-end marker now also resets
///   `loopType` back to "hard" (previously only `loopEnd` was reset to 0, leaving a stale "soft"
///   loop type from the first, now-invalidated, hit).
/// - `getUsedProgramsAndKeys`'s return type changed shape (`Map<preset, Set<"key-velocity">>` →
///   `Map<preset, Map<midiNote, Set<velocity>>>`) and `modify()`/`applySnapshot()`/
///   `preloadSynth()` were also touched — none of these BasicMIDI *methods* are wired up in this
///   Rust port yet (pre-existing gap predating 4.3.0, unrelated to this diff; the underlying
///   `midi_tools` functions exist but are not exposed as `BasicMidi` methods), so they are not
///   addressed here.
use std::collections::{HashMap, HashSet};

use crate::midi::enums::midi_message_types;
use crate::midi::midi_message::MidiMessage;
use crate::midi::midi_track::MidiTrack;
use crate::midi::read::midi::parse_smf_internal;
use crate::midi::read::rmidi::parse_rmidi_internal;
use crate::midi::types::{MidiFormat, MidiLoop, MidiLoopType, TempoChange, TimelineEvent};
use crate::soundbank::types::GenericRange;
use crate::utils::byte_functions::big_endian::read_big_endian;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::loggin::SpessaLog;
use crate::utils::byte_functions::string::read_binary_string;

// ─────────────────────────────────────────────────────────────────────────────
// Helper: tick-to-seconds conversion (standalone, usable from parse_internal)
// ─────────────────────────────────────────────────────────────────────────────

/// Converts a tick value to seconds using the given tempo map and time division.
/// `tempo_changes` must be in **reverse** tick order (last change at index 0).
fn compute_ticks_to_seconds(
    mut ticks: u32,
    tempo_changes: &[TempoChange],
    time_division: u32,
) -> f64 {
    if time_division == 0 || tempo_changes.is_empty() {
        return 0.0;
    }
    // Find the first entry whose tick is <= the requested tick (reverse-sorted list).
    let tempo_idx = tempo_changes
        .iter()
        .position(|v| v.ticks <= ticks)
        .unwrap_or(tempo_changes.len().saturating_sub(1));

    let mut total_seconds = 0.0f64;
    let mut i = tempo_idx;
    while i < tempo_changes.len() {
        let tc = &tempo_changes[i];
        let ticks_since = ticks as f64 - tc.ticks as f64;
        total_seconds += (ticks_since * 60.0) / (tc.tempo * time_division as f64);
        ticks = tc.ticks;
        i += 1;
    }
    total_seconds
}

// ─────────────────────────────────────────────────────────────────────────────
// BasicMidi struct
// ─────────────────────────────────────────────────────────────────────────────

/// The complete parsed MIDI file.
/// Equivalent to: class BasicMIDI
pub struct BasicMidi {
    /// The tracks in the sequence.
    pub tracks: Vec<MidiTrack>,
    /// A flattened, time-sorted list of all events in the MIDI sequence. The order between the
    /// tracks is preserved. Each entry points to the event's track number and its index within
    /// that track. This is the recommended way of iterating over the MIDI sequence's events.
    ///
    /// Do not mutate this outside of `parse_internal` (TS: `readonly Readonly<TimelineEvent>[]`;
    /// Rust has no equivalent compile-time enforcement for a `Vec` field).
    /// Equivalent to: BasicMIDI.timeline (TS 4.3.0)
    pub timeline: Vec<TimelineEvent>,
    /// MIDI ticks per beat (time division).
    pub time_division: u32,
    /// Total duration of the sequence in seconds.
    pub duration: f64,
    /// Tempo changes in reverse tick order (last change first; tick 0 always last).
    pub tempo_changes: Vec<TempoChange>,
    /// Extra metadata events (copyright, non-voice track names, etc.).
    pub extra_metadata: Vec<MidiMessage>,
    /// Lyric events sorted by tick.
    pub lyrics: Vec<MidiMessage>,
    /// Tick of the first note-on event in the sequence.
    pub first_note_on: u32,
    /// Min/max MIDI note range used in the sequence.
    pub key_range: GenericRange,
    /// Tick of the last voice event.
    pub last_voice_event_tick: u32,
    /// Channel-offset per MIDI port (index = port number, value = first channel index).
    pub port_channel_offset_map: Vec<u32>,
    /// Loop region (start tick, end tick, type).
    pub midi_loop: MidiLoop,
    /// Optional file name provided at load time.
    pub file_name: Option<String>,
    /// MIDI file format (0, 1, or 2).
    pub format: MidiFormat,
    /// RMIDI metadata chunks stored as raw bytes, keyed by TypeScript field name
    /// (e.g. "name", "album", "infoEncoding", "midiEncoding", "creationDate", …).
    pub rmidi_info: HashMap<String, Vec<u8>>,
    /// Bank offset used for RMIDI files.
    pub bank_offset: u32,
    /// True if this is a Soft Karaoke (.kar) file.
    pub is_karaoke_file: bool,
    /// True if the file uses multiple MIDI ports.
    pub is_multi_port: bool,
    /// True if this RMIDI file contains a DLS sound bank.
    pub is_dls_rmidi: bool,
    /// Embedded sound-bank binary (SF2 or DLS), if present.
    pub embedded_sound_bank: Option<Vec<u8>>,
    /// Raw bytes of the MIDI name for multi-byte-encoding support.
    pub binary_name: Option<Vec<u8>>,
}

impl Default for BasicMidi {
    fn default() -> Self {
        Self::new()
    }
}

impl BasicMidi {
    /// Creates a new, empty BasicMidi with default field values.
    /// Equivalent to: new BasicMIDI()
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            timeline: Vec::new(),
            // TS 4.3.0 changed the default from 0 to 480.
            time_division: 480,
            duration: 0.0,
            tempo_changes: vec![TempoChange {
                ticks: 0,
                tempo: 120.0,
            }],
            extra_metadata: Vec::new(),
            lyrics: Vec::new(),
            first_note_on: 0,
            key_range: GenericRange {
                min: 0.0,
                max: 127.0,
            },
            last_voice_event_tick: 0,
            port_channel_offset_map: vec![0],
            midi_loop: MidiLoop {
                start: 0,
                end: 0,
                loop_type: MidiLoopType::Hard,
            },
            file_name: None,
            format: MidiFormat::SingleTrack,
            rmidi_info: HashMap::new(),
            bank_offset: 0,
            is_karaoke_file: false,
            is_multi_port: false,
            is_dls_rmidi: false,
            embedded_sound_bank: None,
            binary_name: None,
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Public API
    // ─────────────────────────────────────────────────────────────────────

    /// Creates a BasicMidi from raw MIDI file bytes.
    ///
    /// Reads the MIDI file format, extracts the header and track chunks, and populates the
    /// BasicMidi instance with the parsed data. Supports Standard MIDI Files (SMF) and RIFF MIDI
    /// (RMIDI); Extensible Music Format (XMF) is not supported (out of scope for this port; the
    /// XMF branch below panics via `unimplemented!`). RMIDI files' embedded sound bank is stored
    /// in `embedded_sound_bank`.
    ///
    /// Equivalent to: BasicMIDI.fromArrayBuffer(arrayBuffer, fileName)
    pub fn from_array_buffer(data: &[u8], file_name: &str) -> Result<Self, String> {
        let mut midi = BasicMidi::new();
        let mut binary_data = IndexedByteArray::from_slice(data);
        // Peek at the first 4 bytes without advancing the cursor.
        let initial_string = read_binary_string(&binary_data, 4, 0);
        match initial_string.as_str() {
            "RIFF" => {
                // Possibly an RMID file (https://github.com/spessasus/sf2-rmidi-specification#readme)
                parse_rmidi_internal(&mut midi, &mut binary_data, file_name)?;
            }
            "XMF_" => {
                // Extensible Music Format — not supported (XMF out of scope for this port).
                unimplemented!("XMF not supported");
            }
            _ => {
                // Assume Standard MIDI File.
                parse_smf_internal(&mut midi, &mut binary_data, file_name)?;
            }
        }
        Ok(midi)
    }

    /// Converts MIDI ticks to time in seconds using the current tempo map.
    /// Equivalent to: midiTicksToSeconds(ticks)
    pub fn midi_ticks_to_seconds(&self, ticks: u32) -> f64 {
        compute_ticks_to_seconds(ticks, &self.tempo_changes, self.time_division)
    }

    /// Sorts track events (if `sort_events` is true) then rebuilds all internal metadata.
    /// Equivalent to: flush(sortEvents = true)
    pub fn flush(&mut self, sort_events: bool) {
        if sort_events {
            for track in self.tracks.iter_mut() {
                track.events.sort_by_key(|e| e.ticks);
            }
        }
        self.parse_internal();
    }

    /// Iterates over all MIDI events in chronological (tick) order.
    /// `callback(event, track_index, event_indexes)` is called for each event, where
    /// `event_indexes` is the full per-track cursor array at the time of the call (so
    /// `event_indexes[track_index]` is this event's index within its track — useful for
    /// in-place replacement, see `write/rmidi.rs`).
    ///
    /// You probably should use the `timeline` field instead if you're not mutating the MIDI
    /// during the iteration loop.
    ///
    /// Equivalent to: iterate(callback)
    pub fn iterate<F>(&self, mut callback: F)
    where
        F: FnMut(&MidiMessage, usize, &[usize]),
    {
        let n = self.tracks.len();
        if n == 0 {
            return;
        }
        let mut event_indexes = vec![0usize; n];
        let mut remaining_tracks = n;

        while remaining_tracks > 0 {
            // Find the track whose next event has the smallest tick.
            let mut min_ticks = u32::MAX;
            let mut track_num = 0;
            for (i, track) in self.tracks.iter().enumerate() {
                if event_indexes[i] >= track.events.len() {
                    continue;
                }
                let tick = track.events[event_indexes[i]].ticks;
                if tick < min_ticks {
                    track_num = i;
                    min_ticks = tick;
                }
            }

            // If selected track is exhausted, count it as done.
            if event_indexes[track_num] >= self.tracks[track_num].events.len() {
                remaining_tracks -= 1;
                continue;
            }

            let event = &self.tracks[track_num].events[event_indexes[track_num]];
            callback(event, track_num, &event_indexes);
            event_indexes[track_num] += 1;
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Internal
    // ─────────────────────────────────────────────────────────────────────

    /// Parses all track events to extract tempo changes, loop points, port map,
    /// MIDI name, karaoke state, key range, etc.
    /// Equivalent to: parseInternal()
    fn parse_internal(&mut self) {
        SpessaLog::group("Interpreting MIDI events...");

        // --- Local accumulators (assigned to self at the end) ---
        let mut karaoke_has_title = false;
        let mut tempo_changes = vec![TempoChange {
            ticks: 0,
            tempo: 120.0,
        }];
        let mut extra_metadata: Vec<MidiMessage> = Vec::new();
        let mut lyrics: Vec<MidiMessage> = Vec::new();
        let mut key_range = GenericRange {
            min: 127.0,
            max: 0.0,
        };
        let mut last_voice_event_tick = 0u32;
        let mut loop_start: Option<u32> = None;
        let mut loop_end: Option<u32> = None;
        let mut loop_type = MidiLoopType::Hard;
        let mut is_karaoke_file = false;
        let mut binary_name: Option<Vec<u8>> = None;
        let mut name_detected = self.rmidi_info.contains_key("name");

        // Reset shared state that is NOT accumulated into local vars
        self.midi_loop = MidiLoop {
            start: 0,
            end: 0,
            loop_type: MidiLoopType::Hard,
        };
        self.is_karaoke_file = false;
        self.is_multi_port = false;

        let num_tracks = self.tracks.len();
        let mut track_used_channels: Vec<HashSet<u8>> =
            (0..num_tracks).map(|_| HashSet::new()).collect();
        let mut track_has_voice: Vec<bool> = vec![false; num_tracks];

        // ────────────────────────────────────────────────────
        // First pass: iterate tracks and process events
        // ────────────────────────────────────────────────────
        for track_idx in 0..num_tracks {
            // Temporarily take ownership of the events to allow mutable access
            // while freely accessing other self fields.
            let mut events = std::mem::take(&mut self.tracks[track_idx].events);
            let mut i = 0;
            while i < events.len() {
                let status = events[i].status_byte;
                let ticks = events[i].ticks;
                let data = events[i].data.clone();
                let event_text = read_binary_string(&data, data.len(), 0);

                // ── Voice message (0x80–0xEF) ──────────────────────────
                if (0x80..0xF0).contains(&status) {
                    track_has_voice[track_idx] = true;
                    if ticks > last_voice_event_tick {
                        last_voice_event_tick = ticks;
                    }

                    match status & 0xF0 {
                        x if x == midi_message_types::CONTROLLER_CHANGE => {
                            if !data.is_empty() {
                                match data[0] {
                                    // Touhou / RPG Maker loop start.
                                    // For Touhou and RPG Maker, the data value must be 0.
                                    2 | 111 => {
                                        if data.get(1).copied().unwrap_or(0) == 0 {
                                            loop_start = Some(ticks);
                                        }
                                    }
                                    // EMIDI/XMI loop start (cc116) and EMIDI global
                                    // loop start (cc118), both unconditional.
                                    116 | 118 => {
                                        loop_start = Some(ticks);
                                    }
                                    // Touhou (cc4) / EMIDI (cc117) / EMIDI global (cc119)
                                    // loop end. For Touhou loops, the data value must be 0.
                                    4 | 117 | 119 => {
                                        let cc_num = data[0];
                                        let value = data.get(1).copied().unwrap_or(0);
                                        if loop_end.is_none()
                                            && (cc_num != 4 || value == 0)
                                        {
                                            loop_type = MidiLoopType::Soft;
                                            loop_end = Some(ticks);
                                        } else {
                                            // Appeared more than once (or a CC4 with a nonzero
                                            // value) → not a real loop marker.
                                            loop_end = Some(0);
                                            loop_type = MidiLoopType::Hard;
                                        }
                                    }
                                    // Bank select MSB – DLS RMIDI bank offset detection
                                    0 => {
                                        if self.is_dls_rmidi
                                            && data.len() > 1
                                            && data[1] != 0
                                            && data[1] != 127
                                        {
                                            SpessaLog::info(
                                                "DLS RMIDI with offset 1 detected!",
                                            );
                                            self.bank_offset = 1;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        x if x == midi_message_types::NOTE_ON => {
                            let ch = status & 0x0F;
                            track_used_channels[track_idx].insert(ch);
                            if !data.is_empty() {
                                let note = data[0] as f64;
                                if note < key_range.min {
                                    key_range.min = note;
                                }
                                if note > key_range.max {
                                    key_range.max = note;
                                }
                            }
                        }
                        _ => {}
                    }
                }

                // ── Meta/System events ────────────────────────────────
                match status {
                    x if x == midi_message_types::END_OF_TRACK => {
                        if i != events.len() - 1 {
                            events.remove(i);
                            SpessaLog::warn("Unexpected EndOfTrack. Removing!");
                            continue; // don't increment i
                        }
                    }

                    x if x == midi_message_types::SET_TEMPO => {
                        if data.len() >= 3 {
                            let us_per_beat = read_big_endian(&data, 3, 0);
                            if us_per_beat > 0 {
                                tempo_changes.push(TempoChange {
                                    ticks,
                                    tempo: 60_000_000.0 / us_per_beat as f64,
                                });
                            }
                        }
                    }

                    x if x == midi_message_types::MARKER => {
                        let text = event_text.trim().to_lowercase();
                        match text.as_str() {
                            "start" | "loopstart" => {
                                loop_start = Some(ticks);
                            }
                            "loopend" => {
                                loop_end = Some(ticks);
                            }
                            _ => {}
                        }
                    }

                    x if x == midi_message_types::COPYRIGHT => {
                        extra_metadata.push(events[i].clone());
                    }

                    // LYRIC: check for karaoke, then fall through to TEXT logic.
                    x if x == midi_message_types::LYRIC => {
                        if event_text.trim().starts_with("@KMIDI KARAOKE FILE") {
                            is_karaoke_file = true;
                            SpessaLog::info("Karaoke MIDI detected!");
                        }
                        if is_karaoke_file {
                            // Replace status byte so downstream consumers see TEXT
                            events[i].status_byte = midi_message_types::TEXT;
                        } else {
                            lyrics.push(events[i].clone());
                        }
                        // Intentional fallthrough: run TEXT logic on this event too.
                        let checked = event_text.trim();
                        if checked.starts_with("@KMIDI KARAOKE FILE") {
                            is_karaoke_file = true;
                        } else if is_karaoke_file {
                            if checked.starts_with("@T") || checked.starts_with("@A") {
                                if karaoke_has_title {
                                    extra_metadata.push(events[i].clone());
                                } else {
                                    let skip = 2.min(data.len());
                                    binary_name = Some(data[skip..].to_vec());
                                    karaoke_has_title = true;
                                    name_detected = true;
                                }
                            } else if !checked.starts_with('@') {
                                lyrics.push(events[i].clone());
                            }
                        }
                    }

                    // TEXT: karaoke title / lyrics detection.
                    x if x == midi_message_types::TEXT => {
                        let checked = event_text.trim();
                        if checked.starts_with("@KMIDI KARAOKE FILE") {
                            is_karaoke_file = true;
                            SpessaLog::info("Karaoke MIDI detected!");
                        } else if is_karaoke_file {
                            if checked.starts_with("@T") || checked.starts_with("@A") {
                                if karaoke_has_title {
                                    extra_metadata.push(events[i].clone());
                                } else {
                                    let skip = 2.min(data.len());
                                    binary_name = Some(data[skip..].to_vec());
                                    karaoke_has_title = true;
                                    name_detected = true;
                                }
                            } else if !checked.starts_with('@') {
                                lyrics.push(events[i].clone());
                            }
                        }
                    }

                    _ => {}
                }

                i += 1;
            }

            // Track name: set after inner loop (index > 0 only; track 0 name = the MIDI name).
            self.tracks[track_idx].name = String::new();
            if track_idx > 0
                && let Some(name_msg) = events
                    .iter()
                    .find(|e| e.status_byte == midi_message_types::TRACK_NAME)
                    .cloned()
            {
                let track_name =
                    read_binary_string(&name_msg.data, name_msg.data.len(), 0);
                self.tracks[track_idx].name = track_name.clone();
                if !track_has_voice[track_idx]
                    && !track_name.to_lowercase().contains("setup")
                {
                    extra_metadata.push(name_msg);
                }
            }

            // Restore events to track
            self.tracks[track_idx].events = events;
        }

        // Apply accumulated channels
        for (track_idx, used_channels) in track_used_channels.iter_mut().enumerate() {
            self.tracks[track_idx].channels = std::mem::take(used_channels);
        }

        // Reverse tempo_changes: last change first, tick-0 always last.
        tempo_changes.reverse();

        SpessaLog::info("Correcting loops, ports and detecting notes...");

        // First note-on tick
        let first_note_on = self
            .tracks
            .iter()
            .filter_map(|t| {
                t.events
                    .iter()
                    .find(|e| (e.status_byte & 0xF0) == midi_message_types::NOTE_ON)
                    .map(|e| e.ticks)
            })
            .min()
            .unwrap_or(0);

        SpessaLog::info(&format!(
            "First note-on detected at: {} ticks!",
            first_note_on
        ));

        // Loop region
        let ls = loop_start.unwrap_or(first_note_on);
        let le = match loop_end {
            None | Some(0) => last_voice_event_tick,
            Some(v) => v,
        };
        let midi_loop = MidiLoop {
            start: ls,
            end: le,
            loop_type,
        };
        // Loop fix: if loop end extends past last voice event, update it.
        let last_voice_event_tick = last_voice_event_tick.max(midi_loop.end);

        SpessaLog::info(&format!(
            "Loop points: start: {} end: {}",
            midi_loop.start, midi_loop.end
        ));

        // ── Port detection ────────────────────────────────────────────
        let mut port_offset = 0u32;
        let mut port_map_opt: Vec<Option<u32>> = Vec::new();
        let mut track_ports: Vec<Option<u32>> = vec![None; num_tracks];

        for (track_idx, track) in self.tracks.iter().enumerate() {
            if track.channels.is_empty() {
                continue;
            }
            for event in &track.events {
                if event.status_byte != midi_message_types::MIDI_PORT {
                    continue;
                }
                if event.data.is_empty() {
                    continue;
                }
                let port = event.data[0] as usize;
                track_ports[track_idx] = Some(port as u32);
                while port_map_opt.len() <= port {
                    port_map_opt.push(None);
                }
                if port_map_opt[port].is_none() {
                    port_map_opt[port] = Some(port_offset);
                    port_offset += 16;
                }
            }
        }

        // Convert sparse Option slots to 0
        let mut port_channel_offset_map: Vec<u32> =
            port_map_opt.into_iter().map(|o| o.unwrap_or(0)).collect();

        // Default port for tracks without a port assignment
        let default_port = track_ports
            .iter()
            .filter_map(|p| *p)
            .min()
            .unwrap_or(0);

        for (track_idx, &tp) in track_ports.iter().enumerate() {
            self.tracks[track_idx].port = tp.unwrap_or(default_port);
        }

        // Ensure port map has at least one entry
        if port_channel_offset_map.is_empty() {
            port_channel_offset_map = vec![0];
        }
        if port_channel_offset_map.len() < 2 {
            SpessaLog::info("No additional MIDI Ports detected.");
        } else {
            self.is_multi_port = true;
            SpessaLog::info("MIDI Ports detected!");
        }

        // ── MIDI name detection ───────────────────────────────────────
        if !name_detected {
            if num_tracks > 1 {
                // Multi-track: if track 0 has no note-on events, its track name is the MIDI name.
                let track0_has_notes = self.tracks[0].events.iter().any(|e| {
                    e.status_byte >= midi_message_types::NOTE_ON
                        && e.status_byte < midi_message_types::POLY_PRESSURE
                });
                if !track0_has_notes
                    && let Some(name_msg) = self.tracks[0]
                        .events
                        .iter()
                        .find(|e| e.status_byte == midi_message_types::TRACK_NAME)
                {
                    binary_name = Some(name_msg.data.clone());
                }
            } else if num_tracks == 1
                && let Some(name_msg) = self.tracks[0]
                    .events
                    .iter()
                    .find(|e| e.status_byte == midi_message_types::TRACK_NAME)
            {
                binary_name = Some(name_msg.data.clone());
            }
        }

        // Remove metadata events with empty data
        extra_metadata.retain(|m| !m.data.is_empty());

        // Sort lyrics by tick
        lyrics.sort_by_key(|e| e.ticks);

        // Ensure track 0 has an event at tick 0; if not, insert a track-name event.
        let any_starts_at_zero = self
            .tracks
            .iter()
            .any(|t| t.events.first().is_some_and(|e| e.ticks == 0));
        if !any_starts_at_zero && !self.tracks.is_empty() {
            let name_data = binary_name.as_deref().unwrap_or(&[]).to_vec();
            let stub = MidiMessage::new(0, midi_message_types::TRACK_NAME, name_data);
            self.tracks[0].events.insert(0, stub);
        }

        // Compute final duration using the freshly built tempo map
        let duration =
            compute_ticks_to_seconds(last_voice_event_tick, &tempo_changes, self.time_division);

        // Get sorted events (timeline). `self.tracks` is already in its final state at this
        // point (updated in-place per track above), so `self.iterate` can safely be called here
        // even though the rest of `self`'s derived fields haven't been committed yet.
        // Equivalent to: (this.timeline as TimelineEvent[]).length = 0; this.iterate(...)
        let mut timeline: Vec<TimelineEvent> = Vec::new();
        self.iterate(|_, tr, event_indexes| {
            timeline.push(TimelineEvent {
                tr,
                ev: event_indexes[tr],
            });
        });

        // Invalidate empty binary_name
        if binary_name.as_ref().is_some_and(|n| n.is_empty()) {
            binary_name = None;
        }

        SpessaLog::info(&format!(
            "MIDI file parsed. Total tick time: {}, total seconds time: {:.2}",
            last_voice_event_tick, duration
        ));
        SpessaLog::group_end();

        // ── Commit to self ────────────────────────────────────────────
        self.tempo_changes = tempo_changes;
        self.extra_metadata = extra_metadata;
        self.lyrics = lyrics;
        self.first_note_on = first_note_on;
        self.key_range = key_range;
        self.last_voice_event_tick = last_voice_event_tick;
        self.port_channel_offset_map = port_channel_offset_map;
        self.midi_loop = midi_loop;
        self.is_karaoke_file = is_karaoke_file;
        self.binary_name = binary_name;
        self.duration = duration;
        self.timeline = timeline;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::midi_track::MidiTrack;

    fn make_msg(ticks: u32, status: u8, data: Vec<u8>) -> MidiMessage {
        MidiMessage::new(ticks, status, data)
    }

    fn make_track(events: Vec<MidiMessage>) -> MidiTrack {
        let mut t = MidiTrack::new();
        for e in events {
            t.push_event(e);
        }
        t
    }

    // ── BasicMidi::new ────────────────────────────────────────────────

    #[test]
    fn test_new_default_fields() {
        let m = BasicMidi::new();
        assert_eq!(m.tracks.len(), 0);
        // TS 4.3.0 changed the default from 0 to 480.
        assert_eq!(m.time_division, 480);
        assert_eq!(m.duration, 0.0);
        assert!(m.timeline.is_empty());
        assert_eq!(m.tempo_changes.len(), 1);
        assert_eq!(m.tempo_changes[0].ticks, 0);
        assert_eq!(m.tempo_changes[0].tempo, 120.0);
        assert_eq!(m.bank_offset, 0);
        assert!(!m.is_karaoke_file);
        assert!(!m.is_multi_port);
        assert!(!m.is_dls_rmidi);
        assert!(m.embedded_sound_bank.is_none());
        assert!(m.binary_name.is_none());
    }

    // ── midi_ticks_to_seconds ─────────────────────────────────────────

    #[test]
    fn test_ticks_to_seconds_simple_120bpm() {
        // 120 BPM, 480 ticks/beat → 1 beat = 0.5 s → 480 ticks = 0.5 s
        let mut m = BasicMidi::new();
        m.time_division = 480;
        m.tempo_changes = vec![TempoChange {
            ticks: 0,
            tempo: 120.0,
        }];
        let s = m.midi_ticks_to_seconds(480);
        assert!((s - 0.5).abs() < 1e-9, "got {s}");
    }

    #[test]
    fn test_ticks_to_seconds_zero() {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        m.tempo_changes = vec![TempoChange {
            ticks: 0,
            tempo: 120.0,
        }];
        assert_eq!(m.midi_ticks_to_seconds(0), 0.0);
    }

    #[test]
    fn test_ticks_to_seconds_zero_time_division_returns_zero() {
        let mut m = BasicMidi::new();
        m.time_division = 0;
        assert_eq!(m.midi_ticks_to_seconds(480), 0.0);
    }

    #[test]
    fn test_ticks_to_seconds_two_tempo_changes() {
        // ticks 0–480: 120 BPM (480 ticks/beat → 0.5 s)
        // ticks 480–960: 60 BPM (480 ticks/beat → 1.0 s)
        // total 960 ticks → 1.5 s
        let mut m = BasicMidi::new();
        m.time_division = 480;
        // reversed: last change first
        m.tempo_changes = vec![
            TempoChange {
                ticks: 480,
                tempo: 60.0,
            },
            TempoChange {
                ticks: 0,
                tempo: 120.0,
            },
        ];
        let s = m.midi_ticks_to_seconds(960);
        assert!((s - 1.5).abs() < 1e-9, "got {s}");
    }

    // ── iterate ───────────────────────────────────────────────────────

    #[test]
    fn test_iterate_single_track() {
        let mut m = BasicMidi::new();
        m.tracks.push(make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(100, 0x80, vec![60, 0]),
        ]));
        let mut collected: Vec<(u32, usize)> = Vec::new();
        m.iterate(|e, t, _event_indexes| collected.push((e.ticks, t)));
        assert_eq!(collected, vec![(0, 0), (100, 0)]);
    }

    #[test]
    fn test_iterate_event_indexes_reflect_per_track_cursor() {
        let mut m = BasicMidi::new();
        m.tracks.push(make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, 0x80, vec![60, 0]),
        ]));
        m.tracks.push(make_track(vec![make_msg(100, 0x90, vec![64, 100])]));
        let mut collected: Vec<(usize, usize)> = Vec::new();
        m.iterate(|_, t, event_indexes| collected.push((t, event_indexes[t])));
        // track 0 event 0 (tick 0), track 1 event 0 (tick 100), track 0 event 1 (tick 200)
        assert_eq!(collected, vec![(0, 0), (1, 0), (0, 1)]);
    }

    #[test]
    fn test_iterate_two_tracks_interleaved() {
        let mut m = BasicMidi::new();
        m.tracks.push(make_track(vec![
            make_msg(0, 0x90, vec![60, 100]),
            make_msg(200, 0x80, vec![60, 0]),
        ]));
        m.tracks.push(make_track(vec![
            make_msg(100, 0x90, vec![64, 100]),
            make_msg(300, 0x80, vec![64, 0]),
        ]));
        let mut ticks: Vec<u32> = Vec::new();
        m.iterate(|e, _t, _event_indexes| ticks.push(e.ticks));
        assert_eq!(ticks, vec![0, 100, 200, 300]);
    }

    #[test]
    fn test_iterate_empty_midi() {
        let m = BasicMidi::new();
        let mut count = 0;
        m.iterate(|_, _, _| count += 1);
        assert_eq!(count, 0);
    }

    // ── flush / parse_internal ────────────────────────────────────────

    #[test]
    fn test_flush_computes_tempo_change() {
        // One track: set-tempo (500000 µs/beat = 120 BPM) + note-on + end-of-track
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        // Set tempo: 500000 µs/beat = 120 BPM
        t.push_event(make_msg(0, midi_message_types::SET_TEMPO, vec![0x07, 0xA1, 0x20]));
        t.push_event(make_msg(480, 0x90, vec![60, 100]));
        t.push_event(make_msg(960, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        // tempo_changes is reversed; last element is the tick-0 entry
        let last = m.tempo_changes.last().unwrap();
        assert_eq!(last.ticks, 0);
        assert!((last.tempo - 120.0).abs() < 1.0, "tempo = {}", last.tempo);
    }

    #[test]
    fn test_flush_detects_first_note_on() {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, midi_message_types::SET_TEMPO, vec![0x07, 0xA1, 0x20]));
        t.push_event(make_msg(100, 0x90, vec![60, 100]));
        t.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        assert_eq!(m.first_note_on, 100);
    }

    #[test]
    fn test_flush_computes_duration() {
        // 120 BPM, 480 ticks/beat → 480 ticks = 0.5 s
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, midi_message_types::SET_TEMPO, vec![0x07, 0xA1, 0x20]));
        t.push_event(make_msg(480, 0x90, vec![60, 100]));
        t.push_event(make_msg(960, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        // last voice event is at tick 480 → 0.5 s at 120 BPM
        assert!((m.duration - 0.5).abs() < 0.01, "duration = {}", m.duration);
    }

    #[test]
    fn test_flush_sort_events_reorders() {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        // Intentionally out of order
        t.push_event(make_msg(200, midi_message_types::END_OF_TRACK, vec![]));
        t.push_event(make_msg(100, 0x90, vec![60, 100]));
        t.push_event(make_msg(0, midi_message_types::SET_TEMPO, vec![0x07, 0xA1, 0x20]));
        m.tracks.push(t);
        m.flush(true);
        assert_eq!(m.tracks[0].events[0].ticks, 0);
        assert_eq!(m.tracks[0].events[1].ticks, 100);
        assert_eq!(m.tracks[0].events[2].ticks, 200);
    }

    #[test]
    fn test_flush_removes_misplaced_end_of_track() {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        // EndOfTrack NOT at last position
        t.push_event(make_msg(0, midi_message_types::END_OF_TRACK, vec![]));
        t.push_event(make_msg(100, 0x90, vec![60, 100]));
        t.push_event(make_msg(200, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        // The misplaced one at tick 0 should have been removed
        let eot_count = m.tracks[0]
            .events
            .iter()
            .filter(|e| e.status_byte == midi_message_types::END_OF_TRACK)
            .count();
        assert_eq!(eot_count, 1);
        assert_eq!(
            m.tracks[0].events.last().unwrap().status_byte,
            midi_message_types::END_OF_TRACK
        );
    }

    #[test]
    fn test_flush_key_range() {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![40, 100])); // note 40
        t.push_event(make_msg(100, 0x90, vec![80, 100])); // note 80
        t.push_event(make_msg(200, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        assert_eq!(m.key_range.min, 40.0);
        assert_eq!(m.key_range.max, 80.0);
    }

    #[test]
    fn test_flush_loop_marker_start_end() {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        // CC 116 = loop start
        t.push_event(make_msg(100, 0xB0, vec![116, 0]));
        // CC 117 = loop end
        t.push_event(make_msg(400, 0xB0, vec![117, 0]));
        t.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        assert_eq!(m.midi_loop.start, 100);
        assert_eq!(m.midi_loop.end, 400);
        assert_eq!(m.midi_loop.loop_type, MidiLoopType::Soft);
    }

    // ── Touhou loop false-positive fix (4.3.0) ─────────────────────────

    #[test]
    fn test_flush_touhou_cc4_nonzero_value_is_not_a_loop_end() {
        // CC4 (Touhou loop end marker) with a NONZERO value must NOT be treated as a loop
        // end (this is the 4.3.0 false-positive fix — CC4 is also the legitimate "Foot
        // Controller" controller, so a real foot-controller CC4 must not be misdetected).
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        // CC 4 with nonzero value: NOT a loop marker.
        t.push_event(make_msg(200, 0xB0, vec![4, 64]));
        t.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        // No soft loop end detected → falls back to last voice event tick, loop type hard.
        assert_eq!(m.midi_loop.loop_type, MidiLoopType::Hard);
        assert_eq!(m.midi_loop.end, m.last_voice_event_tick);
    }

    #[test]
    fn test_flush_touhou_cc4_zero_value_is_a_loop_end() {
        // CC4 with value 0 IS a legitimate Touhou loop end marker.
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        t.push_event(make_msg(200, 0xB0, vec![4, 0]));
        t.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        assert_eq!(m.midi_loop.end, 200);
        assert_eq!(m.midi_loop.loop_type, MidiLoopType::Soft);
    }

    #[test]
    fn test_flush_touhou_cc2_requires_zero_value_for_loop_start() {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        // CC2 with nonzero value: not a loop start; first note-on becomes loop start instead.
        t.push_event(make_msg(50, 0xB0, vec![2, 5]));
        t.push_event(make_msg(100, 0x90, vec![60, 100]));
        t.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        assert_eq!(m.midi_loop.start, 100); // firstNoteOn, not the CC2 tick
    }

    #[test]
    fn test_flush_touhou_cc2_zero_value_sets_loop_start() {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        t.push_event(make_msg(50, 0xB0, vec![2, 0]));
        t.push_event(make_msg(100, 0x90, vec![60, 100]));
        t.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        assert_eq!(m.midi_loop.start, 50);
    }

    #[test]
    fn test_flush_cc117_loop_end_unconditional_on_value() {
        // CC117 (EMIDI/XMI) loop end is unconditional regardless of value.
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        t.push_event(make_msg(300, 0xB0, vec![117, 42]));
        t.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        assert_eq!(m.midi_loop.end, 300);
        assert_eq!(m.midi_loop.loop_type, MidiLoopType::Soft);
    }

    #[test]
    fn test_flush_duplicate_loop_end_resets_loop_type_to_hard() {
        // A second loop-end marker resets loopType back to Hard (4.3.0 fix — previously
        // only loopEnd was reset to 0, leaving a stale "soft" loop type).
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        t.push_event(make_msg(200, 0xB0, vec![117, 0]));
        t.push_event(make_msg(300, 0xB0, vec![117, 0]));
        t.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        assert_eq!(m.midi_loop.loop_type, MidiLoopType::Hard);
    }

    // ── EMIDI global loop markers (4.3.18) ─────────────────────────────

    #[test]
    fn test_flush_cc118_global_loop_start_unconditional_on_value() {
        // CC118 (EMIDI global loop start) behaves like CC116: unconditional.
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        t.push_event(make_msg(120, 0xB0, vec![118, 42]));
        t.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        assert_eq!(m.midi_loop.start, 120);
    }

    #[test]
    fn test_flush_cc119_global_loop_end_unconditional_on_value() {
        // CC119 (EMIDI global loop end) behaves like CC117: unconditional.
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        t.push_event(make_msg(300, 0xB0, vec![119, 42]));
        t.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        assert_eq!(m.midi_loop.end, 300);
        assert_eq!(m.midi_loop.loop_type, MidiLoopType::Soft);
    }

    #[test]
    fn test_flush_cc118_cc119_pair_sets_both_loop_points() {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        t.push_event(make_msg(100, 0xB0, vec![118, 0]));
        t.push_event(make_msg(400, 0xB0, vec![119, 0]));
        t.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        assert_eq!(m.midi_loop.start, 100);
        assert_eq!(m.midi_loop.end, 400);
        assert_eq!(m.midi_loop.loop_type, MidiLoopType::Soft);
    }

    #[test]
    fn test_flush_cc119_duplicate_loop_end_resets_loop_type_to_hard() {
        // CC119 shares the loop-end arm with CC4/CC117, so a duplicate invalidates it.
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        t.push_event(make_msg(0, 0x90, vec![60, 100]));
        t.push_event(make_msg(200, 0xB0, vec![119, 0]));
        t.push_event(make_msg(300, 0xB0, vec![119, 0]));
        t.push_event(make_msg(480, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        assert_eq!(m.midi_loop.loop_type, MidiLoopType::Hard);
    }

    // ── timeline (4.3.0) ────────────────────────────────────────────────

    #[test]
    fn test_flush_builds_timeline() {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t0 = MidiTrack::new();
        t0.push_event(make_msg(0, 0x90, vec![60, 100]));
        t0.push_event(make_msg(200, midi_message_types::END_OF_TRACK, vec![]));
        let mut t1 = MidiTrack::new();
        t1.push_event(make_msg(100, 0x90, vec![64, 100]));
        t1.push_event(make_msg(150, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t0);
        m.tracks.push(t1);
        m.flush(false);

        // Timeline should mirror `iterate`'s order exactly: total event count and tick order.
        let total_events: usize = m.tracks.iter().map(|t| t.events.len()).sum();
        assert_eq!(m.timeline.len(), total_events);
        let mut ticks: Vec<u32> = Vec::new();
        for te in &m.timeline {
            ticks.push(m.tracks[te.tr].events[te.ev].ticks);
        }
        let mut sorted_ticks = ticks.clone();
        sorted_ticks.sort();
        assert_eq!(ticks, sorted_ticks, "timeline should be tick-ordered");
    }

    #[test]
    fn test_flush_tempo_changes_reversed() {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        let mut t = MidiTrack::new();
        // tempo at tick 0: 120 BPM (500000 us)
        t.push_event(make_msg(0, midi_message_types::SET_TEMPO, vec![0x07, 0xA1, 0x20]));
        // tempo at tick 480: 60 BPM (1000000 us)
        t.push_event(make_msg(480, midi_message_types::SET_TEMPO, vec![0x0F, 0x42, 0x40]));
        t.push_event(make_msg(960, 0x90, vec![60, 100]));
        t.push_event(make_msg(1440, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t);
        m.flush(false);
        // After reversing: index 0 = last change (tick 480), last = tick 0
        assert_eq!(m.tempo_changes[0].ticks, 480);
        assert_eq!(m.tempo_changes.last().unwrap().ticks, 0);
    }

    #[test]
    fn test_flush_port_detection() {
        let mut m = BasicMidi::new();
        m.time_division = 480;
        // Track 0: no channels (conductor), no port event
        let mut t0 = MidiTrack::new();
        t0.push_event(make_msg(0, midi_message_types::END_OF_TRACK, vec![]));
        // Track 1: has note-on + port event (port 1)
        let mut t1 = MidiTrack::new();
        t1.push_event(make_msg(0, midi_message_types::MIDI_PORT, vec![1]));
        t1.push_event(make_msg(10, 0x90, vec![60, 100]));
        t1.push_event(make_msg(100, midi_message_types::END_OF_TRACK, vec![]));
        m.tracks.push(t0);
        m.tracks.push(t1);
        m.flush(false);
        assert!(m.is_multi_port);
        assert_eq!(m.tracks[1].port, 1);
    }
}
