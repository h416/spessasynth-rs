/// get_note_times.rs
/// purpose: Compute the absolute start time and duration (in seconds) for every
///          note in a BasicMidi, taking tempo changes into account.
/// Ported from: src/midi/midi_tools/get_note_times.ts
use crate::midi::basic_midi::BasicMidi;
use crate::midi::types::NoteTime;
use crate::synthesizer::audio_engine::engine_components::synth_constants::DEFAULT_PERCUSSION;
use crate::utils::big_endian::read_big_endian;

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Closes an open note for `channel` with key `midi_note`.
/// Sets its `length` in `note_times`, removes it from `unfinished_notes`,
/// and decrements the `unfinished` counter (even when the note is not found,
/// mirroring the TypeScript behaviour).
fn note_off(
    note_times: &mut [Vec<NoteTime>],
    unfinished_notes: &mut [Vec<(u8, usize)>],
    unfinished: &mut i32,
    midi_note: u8,
    channel: usize,
    elapsed_time: f64,
    min_drum_length: f64,
) {
    if let Some(pos) = unfinished_notes[channel]
        .iter()
        .position(|(n, _)| *n == midi_note)
    {
        let (_, idx) = unfinished_notes[channel][pos];
        let time = elapsed_time - note_times[channel][idx].start;
        note_times[channel][idx].length = if channel == DEFAULT_PERCUSSION as usize {
            f64::max(time, min_drum_length)
        } else {
            time
        };
        unfinished_notes[channel].remove(pos);
    }
    *unfinished -= 1;
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Calculates all note times in seconds.
///
/// Returns an array of 16 channel slots, each containing [`NoteTime`] entries
/// with key number, velocity, absolute start time, and duration.
///
/// `min_drum_length`: minimum duration (seconds) forced on percussion notes
/// (channel 9).  Pass `0.0` to apply no minimum.
///
/// Equivalent to: getNoteTimesInternal(midi, minDrumLength)
pub fn get_note_times_internal(midi: &BasicMidi, min_drum_length: f64) -> Vec<Vec<NoteTime>> {
    let mut note_times: Vec<Vec<NoteTime>> = vec![Vec::new(); 16];

    // Flatten all track events and sort by tick (stable, matches TypeScript Array.sort).
    let mut events: Vec<&crate::midi::midi_message::MidiMessage> =
        midi.tracks.iter().flat_map(|t| t.events.iter()).collect();
    events.sort_by_key(|e| e.ticks);

    let mut elapsed_time = 0.0f64;
    // Default: 120 BPM
    let mut one_tick_to_seconds = 60.0 / (120.0 * midi.time_division as f64);
    let mut unfinished: i32 = 0;

    // unfinished_notes[channel] = list of (midi_note, index_into_note_times[channel])
    let mut unfinished_notes: Vec<Vec<(u8, usize)>> = vec![Vec::new(); 16];

    let n_events = events.len();
    let mut event_index = 0usize;

    while event_index < n_events {
        let event = events[event_index];
        let status_nibble = event.status_byte >> 4;
        let channel = (event.status_byte & 0x0F) as usize;

        if status_nibble == 0x8 {
            // Note off
            let midi_note = event.data[0];
            note_off(
                &mut note_times,
                &mut unfinished_notes,
                &mut unfinished,
                midi_note,
                channel,
                elapsed_time,
                min_drum_length,
            );
        } else if status_nibble == 0x9 {
            // Note on (vel=0 treated as note-off)
            let midi_note = event.data[0];
            let velocity = event.data[1];
            if velocity == 0 {
                note_off(
                    &mut note_times,
                    &mut unfinished_notes,
                    &mut unfinished,
                    midi_note,
                    channel,
                    elapsed_time,
                    min_drum_length,
                );
            } else {
                // Stop any previous note with the same key first.
                note_off(
                    &mut note_times,
                    &mut unfinished_notes,
                    &mut unfinished,
                    midi_note,
                    channel,
                    elapsed_time,
                    min_drum_length,
                );

                // Open the new note.
                let note_idx = note_times[channel].len();
                note_times[channel].push(NoteTime {
                    midi_note,
                    start: elapsed_time,
                    length: -1.0,
                    velocity,
                });
                unfinished_notes[channel].push((midi_note, note_idx));
                unfinished += 1;
            }
        } else if event.status_byte == 0x51 {
            // Set Tempo meta event: 3-byte big-endian microseconds/beat
            let tempo_us = read_big_endian(&event.data, 3, 0);
            let tempo_bpm = 60_000_000.0 / tempo_us as f64;
            one_tick_to_seconds = 60.0 / (tempo_bpm * midi.time_division as f64);
        }

        event_index += 1;
        if event_index >= n_events {
            break;
        }

        // Advance elapsed time by the tick difference to the next event.
        elapsed_time += one_tick_to_seconds
            * (events[event_index].ticks as f64 - events[event_index - 1].ticks as f64);
    }

    // Close any notes that never received a note-off.
    if unfinished > 0 {
        for channel in 0..16usize {
            for &(_, idx) in &unfinished_notes[channel] {
                let time = elapsed_time - note_times[channel][idx].start;
                note_times[channel][idx].length = if channel == DEFAULT_PERCUSSION as usize {
                    f64::max(time, min_drum_length)
                } else {
                    time
                };
            }
        }
    }

    note_times
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::basic_midi::BasicMidi;
    use crate::midi::enums::midi_message_types;
    use crate::midi::midi_message::MidiMessage;
    use crate::midi::midi_track::MidiTrack;

    // Helper: push an event into a track.
    fn push(track: &mut MidiTrack, ticks: u32, status: u8, data: Vec<u8>) {
        track.push_event(MidiMessage::new(ticks, status, data));
    }

    // Helper: build a minimal BasicMidi with one track.
    fn one_track_midi(time_division: u32) -> (BasicMidi, MidiTrack) {
        let mut m = BasicMidi::new();
        m.time_division = time_division;
        let t = MidiTrack::new();
        (m, t)
    }

    // ── Basic note timing ─────────────────────────────────────────────────────

    #[test]
    fn test_single_note_default_tempo() {
        // 120 BPM, 480 ticks/beat → 1 beat = 0.5 s
        // note-on at tick 0, note-off at tick 480 → length 0.5 s
        let (mut m, mut t) = one_track_midi(480);
        push(&mut t, 0, 0x90, vec![60, 100]);
        push(&mut t, 480, 0x80, vec![60, 0]);
        m.tracks.push(t);

        let result = get_note_times_internal(&m, 0.0);
        assert_eq!(result[0].len(), 1);
        let note = &result[0][0];
        assert_eq!(note.midi_note, 60);
        assert_eq!(note.velocity, 100);
        assert!((note.start - 0.0).abs() < 1e-9);
        assert!((note.length - 0.5).abs() < 1e-6, "length = {}", note.length);
    }

    #[test]
    fn test_single_note_velocity_stored_as_u8() {
        let (mut m, mut t) = one_track_midi(480);
        push(&mut t, 0, 0x90, vec![64, 80]);
        push(&mut t, 480, 0x80, vec![64, 0]);
        m.tracks.push(t);

        let result = get_note_times_internal(&m, 0.0);
        assert_eq!(result[0][0].velocity, 80);
    }

    #[test]
    fn test_note_on_vel0_treated_as_note_off() {
        // Note-on with vel=0 should close the note.
        let (mut m, mut t) = one_track_midi(480);
        push(&mut t, 0, 0x90, vec![60, 100]);
        push(&mut t, 480, 0x90, vec![60, 0]); // vel=0 → note-off
        m.tracks.push(t);

        let result = get_note_times_internal(&m, 0.0);
        assert_eq!(result[0].len(), 1);
        assert!((result[0][0].length - 0.5).abs() < 1e-6);
    }

    // ── Tempo change ──────────────────────────────────────────────────────────

    #[test]
    fn test_tempo_change_affects_subsequent_notes() {
        // 480 ticks/beat.
        // Tick 0: SET_TEMPO 120 BPM (500000 µs/beat)
        // Tick 0: note-on C4
        // Tick 480: SET_TEMPO 240 BPM (250000 µs/beat) → 1 beat = 0.25 s
        // Tick 960: note-off C4 → elapsed = 0.5 + 0.25 = 0.75 s, length = 0.75 s
        let (mut m, mut t) = one_track_midi(480);
        push(&mut t, 0, midi_message_types::SET_TEMPO, vec![0x07, 0xA1, 0x20]); // 120 BPM
        push(&mut t, 0, 0x90, vec![60, 100]);
        push(&mut t, 480, midi_message_types::SET_TEMPO, vec![0x03, 0xD0, 0x90]); // 240 BPM
        push(&mut t, 960, 0x80, vec![60, 0]);
        m.tracks.push(t);

        let result = get_note_times_internal(&m, 0.0);
        assert_eq!(result[0].len(), 1);
        // 480 ticks @ 120 BPM → 0.5 s, then 480 ticks @ 240 BPM → 0.25 s
        assert!((result[0][0].length - 0.75).abs() < 1e-6, "length = {}", result[0][0].length);
    }

    #[test]
    fn test_initial_tempo_defaults_to_120_bpm() {
        // Without an explicit SET_TEMPO, 120 BPM should be assumed.
        let (mut m, mut t) = one_track_midi(480);
        push(&mut t, 0, 0x90, vec![60, 100]);
        push(&mut t, 480, 0x80, vec![60, 0]);
        m.tracks.push(t);

        let result = get_note_times_internal(&m, 0.0);
        assert!((result[0][0].length - 0.5).abs() < 1e-6);
    }

    // ── Percussion minimum length ─────────────────────────────────────────────

    #[test]
    fn test_drum_channel_min_length_applied() {
        // Drum note on ch9, note-off immediately (same tick) → natural length = 0.
        // min_drum_length = 0.1 → forced length = 0.1.
        let (mut m, mut t) = one_track_midi(480);
        push(&mut t, 0, 0x99, vec![38, 100]); // note-on ch9
        push(&mut t, 0, 0x89, vec![38, 0]); // note-off ch9, same tick
        m.tracks.push(t);

        let result = get_note_times_internal(&m, 0.1);
        assert_eq!(result[9].len(), 1);
        assert!(
            (result[9][0].length - 0.1).abs() < 1e-9,
            "length = {}",
            result[9][0].length
        );
    }

    #[test]
    fn test_non_drum_channel_no_min_length() {
        // Non-drum note with zero-length should stay zero, not gain min_drum_length.
        let (mut m, mut t) = one_track_midi(480);
        push(&mut t, 0, 0x90, vec![60, 100]); // ch0
        push(&mut t, 0, 0x80, vec![60, 0]); // note-off same tick
        m.tracks.push(t);

        let result = get_note_times_internal(&m, 0.1);
        assert_eq!(result[0].len(), 1);
        assert!((result[0][0].length - 0.0).abs() < 1e-9);
    }

    // ── Unfinished notes ──────────────────────────────────────────────────────

    #[test]
    fn test_note_without_note_off_stays_minus_one() {
        // When a note-on fires with no prior note on the same key, the TypeScript
        // "stop previous" call decrements `unfinished` even though no note was found,
        // leaving the counter at 0 after the push.  The fix-up (unfinished > 0) never
        // runs, so the note retains its sentinel length of -1.
        let (mut m, mut t) = one_track_midi(480);
        push(&mut t, 0, 0x90, vec![60, 100]); // note-on, no note-off
        m.tracks.push(t);

        let result = get_note_times_internal(&m, 0.0);
        assert_eq!(result[0].len(), 1);
        assert_eq!(result[0][0].start, 0.0);
        // length stays -1.0: unfinished counter = 0, fix-up does not run.
        assert_eq!(result[0][0].length, -1.0);
    }

    // ── Multiple channels ─────────────────────────────────────────────────────

    #[test]
    fn test_notes_on_different_channels() {
        let (mut m, mut t) = one_track_midi(480);
        push(&mut t, 0, 0x90, vec![60, 100]); // ch0
        push(&mut t, 0, 0x91, vec![64, 80]); // ch1
        push(&mut t, 480, 0x80, vec![60, 0]); // ch0 note-off
        push(&mut t, 480, 0x81, vec![64, 0]); // ch1 note-off
        m.tracks.push(t);

        let result = get_note_times_internal(&m, 0.0);
        assert_eq!(result[0].len(), 1);
        assert_eq!(result[1].len(), 1);
        assert_eq!(result[0][0].midi_note, 60);
        assert_eq!(result[1][0].midi_note, 64);
        assert!((result[0][0].length - 0.5).abs() < 1e-6);
        assert!((result[1][0].length - 0.5).abs() < 1e-6);
    }

    // ── Multi-track ───────────────────────────────────────────────────────────

    #[test]
    fn test_events_from_multiple_tracks_merged() {
        // Two tracks; events interleaved after merge.
        let mut m = BasicMidi::new();
        m.time_division = 480;

        let mut t0 = MidiTrack::new();
        push(&mut t0, 0, 0x90, vec![60, 100]);
        push(&mut t0, 480, 0x80, vec![60, 0]);
        m.tracks.push(t0);

        let mut t1 = MidiTrack::new();
        push(&mut t1, 0, 0x91, vec![64, 80]);
        push(&mut t1, 480, 0x81, vec![64, 0]);
        m.tracks.push(t1);

        let result = get_note_times_internal(&m, 0.0);
        assert_eq!(result[0].len(), 1);
        assert_eq!(result[1].len(), 1);
    }

    // ── Retrigger (note-on while same key still active) ───────────────────────

    #[test]
    fn test_retrigger_closes_previous_note() {
        // Second note-on with the same key should close the first.
        let (mut m, mut t) = one_track_midi(480);
        push(&mut t, 0, 0x90, vec![60, 100]); // first note-on
        push(&mut t, 240, 0x90, vec![60, 90]); // retrigger: closes first, opens second
        push(&mut t, 480, 0x80, vec![60, 0]); // note-off for second
        m.tracks.push(t);

        let result = get_note_times_internal(&m, 0.0);
        // Two NoteTime entries for ch0
        assert_eq!(result[0].len(), 2);
        let first = &result[0][0];
        let second = &result[0][1];
        // First note: start=0.0, length=0.25 s (240 ticks)
        assert!((first.length - 0.25).abs() < 1e-6, "first length = {}", first.length);
        // Second note: start=0.25, length=0.25 s
        assert!((second.start - 0.25).abs() < 1e-6);
        assert!((second.length - 0.25).abs() < 1e-6, "second length = {}", second.length);
    }

    // ── Empty midi ────────────────────────────────────────────────────────────

    #[test]
    fn test_empty_midi_returns_16_empty_channels() {
        let m = BasicMidi::new();
        let result = get_note_times_internal(&m, 0.0);
        assert_eq!(result.len(), 16);
        for ch in &result {
            assert!(ch.is_empty());
        }
    }

    // ── Note start times ──────────────────────────────────────────────────────

    #[test]
    fn test_second_note_start_time_reflects_elapsed() {
        // elapsed_time advances between events based on tick deltas.
        // note-A at tick 0, note-B at tick 960 (= 1.0 s at 120 BPM, 480 ticks/beat).
        // note-A off at tick 960 (same time as note-B on),
        // note-B off at tick 1440 (= 0.5 s after note-B on).
        let (mut m, mut t) = one_track_midi(480);
        push(&mut t, 0, 0x90, vec![60, 100]); // note-A on
        push(&mut t, 960, 0x80, vec![60, 0]); // note-A off at 1.0 s
        push(&mut t, 960, 0x91, vec![64, 80]); // note-B on ch1 at 1.0 s
        push(&mut t, 1440, 0x81, vec![64, 0]); // note-B off ch1 at 1.5 s
        m.tracks.push(t);

        let result = get_note_times_internal(&m, 0.0);
        assert_eq!(result[0].len(), 1);
        assert_eq!(result[1].len(), 1);
        // note-A: start=0, length=1.0 s
        assert!((result[0][0].start - 0.0).abs() < 1e-9);
        assert!((result[0][0].length - 1.0).abs() < 1e-6, "A length = {}", result[0][0].length);
        // note-B: start=1.0 s, length=0.5 s
        assert!((result[1][0].start - 1.0).abs() < 1e-6, "B start = {}", result[1][0].start);
        assert!((result[1][0].length - 0.5).abs() < 1e-6, "B length = {}", result[1][0].length);
    }
}
