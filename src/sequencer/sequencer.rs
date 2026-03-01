/// sequencer.rs
/// purpose: SpessaSynthSequencer struct and core methods.
/// Ported from: src/sequencer/sequencer.ts
use std::collections::HashMap;

use crate::midi::basic_midi::BasicMidi;
use crate::midi::enums::midi_controllers;
use crate::sequencer::types::SequencerEvent;
use crate::synthesizer::audio_engine::engine_components::synth_constants::DEFAULT_SYNTH_MODE;
use crate::synthesizer::processor::SpessaSynthProcessor;
use crate::utils::loggin::spessa_synth_warn;

// ---------------------------------------------------------------------------
// PlayingNote
// ---------------------------------------------------------------------------

/// A note currently being played (for pause/resume).
#[derive(Clone, Debug)]
pub struct PlayingNote {
    pub midi_note: u8,
    pub channel: usize,
    pub velocity: u8,
}

// ---------------------------------------------------------------------------
// SpessaSynthSequencer
// ---------------------------------------------------------------------------

/// MIDI sequencer that drives a SpessaSynthProcessor.
/// Equivalent to: class SpessaSynthSequencer
pub struct SpessaSynthSequencer {
    // -- public --
    /// Song list.
    /// Equivalent to: songs
    pub songs: Vec<BasicMidi>,

    /// The synthesizer connected to the sequencer.
    /// Equivalent to: synth (readonly)
    pub synth: SpessaSynthProcessor,

    /// If the notes that were playing when paused should be re-triggered on resume.
    /// Equivalent to: retriggerPausedNotes
    pub retrigger_paused_notes: bool,

    /// Loop count. 0 = disabled, u32::MAX = infinite.
    /// Equivalent to: loopCount
    pub loop_count: u32,

    /// Skip to the first note-on event.
    /// Equivalent to: skipToFirstNoteOn
    pub skip_to_first_note_on: bool,

    /// Whether the sequencer has finished playing.
    /// Equivalent to: isFinished
    pub is_finished: bool,

    /// Whether to preload voices for newly loaded sequences.
    /// Equivalent to: preload
    pub preload: bool,

    /// Event callback.
    /// Equivalent to: onEventCall
    pub on_event_call: Option<Box<dyn FnMut(SequencerEvent)>>,

    // -- internal --
    /// Index into `songs` for the currently loaded song. None if no song loaded.
    pub(crate) current_song_index: Option<usize>,

    /// Time of the first note in seconds.
    /// Equivalent to: firstNoteTime
    pub(crate) first_note_time: f64,

    /// Duration of one MIDI tick in seconds.
    /// Equivalent to: oneTickToSeconds
    pub(crate) one_tick_to_seconds: f64,

    /// Current event index per track.
    /// Equivalent to: eventIndexes
    pub(crate) event_indexes: Vec<usize>,

    /// Time that has been played in the current song.
    /// Equivalent to: playedTime
    pub(crate) played_time: f64,

    /// Paused time. None = playing, Some = paused at this time.
    /// Equivalent to: pausedTime (undefined = playing)
    pub(crate) paused_time: Option<f64>,

    /// Absolute start time (synth time base).
    /// Equivalent to: absoluteStartTime
    pub(crate) absolute_start_time: f64,

    /// Currently playing notes (for pause/resume).
    /// Equivalent to: playingNotes
    pub(crate) playing_notes: Vec<PlayingNote>,

    /// MIDI port number for each track.
    /// Equivalent to: currentMIDIPorts
    pub(crate) current_midi_ports: Vec<u32>,

    /// Next channel offset to assign to a new MIDI port.
    /// Equivalent to: midiPortChannelOffset
    pub(crate) midi_port_channel_offset: usize,

    /// Channel offset per MIDI port. Record<port, offset>.
    /// Equivalent to: midiPortChannelOffsets
    pub(crate) midi_port_channel_offsets: HashMap<u32, usize>,

    /// Current song index in the song list.
    /// Equivalent to: _songIndex
    pub(crate) song_index: usize,

    /// Shuffle mode enabled.
    /// Equivalent to: _shuffleMode
    pub(crate) shuffle_mode: bool,

    /// Shuffled song indexes.
    /// Equivalent to: shuffledSongIndexes
    pub(crate) shuffled_song_indexes: Vec<usize>,

    /// Playback rate.
    /// Equivalent to: _playbackRate
    pub(crate) playback_rate: f64,
}

impl SpessaSynthSequencer {
    /// Creates a new sequencer without any songs loaded.
    /// Equivalent to: constructor(spessasynthProcessor)
    pub fn new(synth: SpessaSynthProcessor) -> Self {
        let absolute_start_time = synth.current_synth_time();
        Self {
            songs: Vec::new(),
            synth,
            retrigger_paused_notes: true,
            loop_count: 0,
            skip_to_first_note_on: true,
            is_finished: false,
            preload: true,
            on_event_call: None,
            current_song_index: None,
            first_note_time: 0.0,
            one_tick_to_seconds: 0.0,
            event_indexes: Vec::new(),
            played_time: 0.0,
            paused_time: Some(0.0),
            absolute_start_time,
            playing_notes: Vec::new(),
            current_midi_ports: Vec::new(),
            midi_port_channel_offset: 0,
            midi_port_channel_offsets: HashMap::new(),
            song_index: 0,
            shuffle_mode: false,
            shuffled_song_indexes: Vec::new(),
            playback_rate: 1.0,
        }
    }

    // -----------------------------------------------------------------------
    // Getters / setters
    // -----------------------------------------------------------------------

    /// Returns the currently loaded MIDI data, if any.
    /// Equivalent to: get midiData()
    pub fn midi_data(&self) -> Option<&BasicMidi> {
        self.current_song_index.and_then(|i| self.songs.get(i))
    }

    /// Returns the duration of the current sequence in seconds.
    /// Equivalent to: get duration()
    pub fn duration(&self) -> f64 {
        self.midi_data().map_or(0.0, |m| m.duration)
    }

    /// Returns the current song index.
    /// Equivalent to: get songIndex()
    pub fn get_song_index(&self) -> usize {
        self.song_index
    }

    /// Sets the song index and loads the song.
    /// Equivalent to: set songIndex(value)
    pub fn set_song_index(&mut self, value: usize) {
        if self.songs.is_empty() {
            return;
        }
        self.song_index = value.max(0) % self.songs.len();
        self.load_current_song();
    }

    /// Returns shuffle mode state.
    /// Equivalent to: get shuffleMode()
    pub fn get_shuffle_mode(&self) -> bool {
        self.shuffle_mode
    }

    /// Sets shuffle mode.
    /// Equivalent to: set shuffleMode(on)
    pub fn set_shuffle_mode(&mut self, on: bool) {
        self.shuffle_mode = on;
        if on {
            self.shuffle_song_indexes();
            self.song_index = 0;
            self.load_current_song();
        } else if !self.shuffled_song_indexes.is_empty() {
            self.song_index = self.shuffled_song_indexes[self.song_index];
        }
    }

    /// Returns the playback rate.
    /// Equivalent to: get playbackRate()
    pub fn get_playback_rate(&self) -> f64 {
        self.playback_rate
    }

    /// Sets the playback rate.
    /// Equivalent to: set playbackRate(value)
    pub fn set_playback_rate(&mut self, value: f64) {
        let t = self.current_time();
        self.playback_rate = value;
        self.recalculate_start_time(t);
    }

    /// Returns the current playback time in seconds.
    /// Equivalent to: get currentTime()
    pub fn current_time(&self) -> f64 {
        if let Some(paused) = self.paused_time {
            return paused;
        }
        (self.synth.current_synth_time() - self.absolute_start_time) * self.playback_rate
    }

    /// Sets the current playback time.
    /// Equivalent to: set currentTime(time)
    pub fn set_current_time(&mut self, time: f64) {
        let song_idx = match self.current_song_index {
            Some(i) => i,
            None => return,
        };
        if self.paused() {
            self.paused_time = Some(time);
        }
        let duration = self.songs[song_idx].duration;
        let first_note_on = self.songs[song_idx].first_note_on;
        if time > duration || time < 0.0 {
            if self.skip_to_first_note_on {
                self.set_time_ticks(first_note_on.saturating_sub(1));
            } else {
                self.set_time_ticks(0);
            }
        } else if self.skip_to_first_note_on && time < self.first_note_time {
            self.set_time_ticks(first_note_on.saturating_sub(1));
        } else {
            self.playing_notes.clear();
            self.call_event(SequencerEvent::TimeChange(
                crate::sequencer::types::TimeChangeEventData { new_time: time },
            ));
            self.set_time_to(time, None);
            self.recalculate_start_time(time);
        }
    }

    /// Returns true if paused.
    /// Equivalent to: get paused()
    pub fn paused(&self) -> bool {
        self.paused_time.is_some()
    }

    // -----------------------------------------------------------------------
    // Playback control
    // -----------------------------------------------------------------------

    /// Starts or resumes playback.
    /// Equivalent to: play()
    pub fn play(&mut self) {
        if self.current_song_index.is_none() {
            spessa_synth_warn("No songs loaded in the sequencer. Ignoring the play call.");
            return;
        }
        // Reset the time if at end
        if self.current_time() >= self.duration() {
            self.set_current_time(0.0);
        }
        // Unpause if paused
        if self.paused() {
            self.recalculate_start_time(self.paused_time.unwrap_or(0.0));
        }
        if self.retrigger_paused_notes {
            let notes: Vec<PlayingNote> = self.playing_notes.clone();
            for n in &notes {
                self.synth.note_on(n.channel, n.midi_note, n.velocity);
            }
        }
        self.paused_time = None;
    }

    /// Pauses the playback.
    /// Equivalent to: pause()
    pub fn pause(&mut self) {
        self.pause_internal(false);
    }

    /// Loads a new song list into the sequencer.
    /// Equivalent to: loadNewSongList(midiBuffers)
    pub fn load_new_song_list(&mut self, midi_buffers: Vec<BasicMidi>) {
        self.songs = midi_buffers;
        if self.songs.is_empty() {
            return;
        }
        self.song_index = 0;
        self.shuffle_song_indexes();
        // TODO: preloadSynth for songs without embedded sound bank
        self.load_current_song();
    }

    // -----------------------------------------------------------------------
    // Internal methods
    // -----------------------------------------------------------------------

    /// Fires an event through the callback.
    /// Equivalent to: callEvent(type, data)
    pub(crate) fn call_event(&mut self, event: SequencerEvent) {
        if let Some(ref mut callback) = self.on_event_call {
            callback(event);
        }
    }

    /// Internal pause implementation.
    /// Equivalent to: pauseInternal(isFinished)
    pub(crate) fn pause_internal(&mut self, is_finished: bool) {
        if self.paused() {
            return;
        }
        self.stop();
        self.call_event(SequencerEvent::Pause(
            crate::sequencer::types::PauseEventData { is_finished },
        ));
        if is_finished {
            self.call_event(SequencerEvent::SongEnded);
        }
    }

    /// Called when the current song finishes playing.
    /// Equivalent to: songIsFinished()
    pub(crate) fn song_is_finished(&mut self) {
        self.is_finished = true;
        if self.songs.len() == 1 {
            self.pause_internal(true);
            return;
        }
        self.song_index += 1;
        self.song_index %= self.songs.len();
        self.load_current_song();
    }

    /// Stops the playback.
    /// Equivalent to: stop()
    pub(crate) fn stop(&mut self) {
        let t = self.current_time();
        self.paused_time = Some(t);
        self.send_midi_all_off();
    }

    /// Returns the track number of the next closest event.
    /// Equivalent to: findFirstEventIndex()
    pub(crate) fn find_first_event_index(&self) -> usize {
        let song_idx = match self.current_song_index {
            Some(i) => i,
            None => return 0,
        };
        let tracks = &self.songs[song_idx].tracks;
        let mut index = 0;
        let mut ticks = u32::MAX;
        for (i, track) in tracks.iter().enumerate() {
            if self.event_indexes[i] >= track.events.len() {
                continue;
            }
            let event = &track.events[self.event_indexes[i]];
            if event.ticks < ticks {
                index = i;
                ticks = event.ticks;
            }
        }
        index
    }

    /// Adds a new MIDI port (16 channels) to the synth.
    /// Equivalent to: addNewMIDIPort()
    pub(crate) fn add_new_midi_port(&mut self) {
        for _ in 0..16 {
            self.synth.create_midi_channel();
        }
    }

    /// Sends all-off to all channels.
    /// Equivalent to: sendMIDIAllOff() (non-external path only)
    pub(crate) fn send_midi_all_off(&mut self) {
        // Disable sustain on first 16 channels
        for i in 0..16 {
            self.synth
                .controller_change(i, midi_controllers::SUSTAIN_PEDAL, 0);
        }
        self.synth.stop_all_channels(false);
    }

    /// Resets all controllers.
    /// Equivalent to: sendMIDIReset() (non-external path only)
    pub(crate) fn send_midi_reset(&mut self) {
        self.send_midi_all_off();
        self.synth.reset_all_controllers(DEFAULT_SYNTH_MODE);
    }

    /// Loads the current song from the song list.
    /// Equivalent to: loadCurrentSong()
    pub(crate) fn load_current_song(&mut self) {
        let index = if self.shuffle_mode {
            self.shuffled_song_indexes[self.song_index]
        } else {
            self.song_index
        };
        self.load_new_sequence(index);
    }

    /// Shuffles the song indexes.
    /// Equivalent to: shuffleSongIndexes()
    pub(crate) fn shuffle_song_indexes(&mut self) {
        let mut indexes: Vec<usize> = (0..self.songs.len()).collect();
        self.shuffled_song_indexes = Vec::with_capacity(indexes.len());
        // Simple Fisher-Yates style shuffle using a basic deterministic approach
        // (No random needed for WAV generation; kept for API compatibility)
        while !indexes.is_empty() {
            let idx = indexes.len() - 1;
            self.shuffled_song_indexes.push(indexes[idx]);
            indexes.remove(idx);
        }
    }

    /// Sets the time in MIDI ticks.
    /// Equivalent to: setTimeTicks(ticks)
    pub(crate) fn set_time_ticks(&mut self, ticks: u32) {
        if self.current_song_index.is_none() {
            return;
        }
        self.playing_notes.clear();
        let song_idx = self.current_song_index.unwrap();
        let seconds = self.songs[song_idx].midi_ticks_to_seconds(ticks);
        self.call_event(SequencerEvent::TimeChange(
            crate::sequencer::types::TimeChangeEventData { new_time: seconds },
        ));
        self.set_time_to(0.0, Some(ticks));
        self.recalculate_start_time(self.played_time);
    }

    /// Recalculates the absolute start time.
    /// Equivalent to: recalculateStartTime(time)
    pub(crate) fn recalculate_start_time(&mut self, time: f64) {
        self.absolute_start_time = self.synth.current_synth_time() - time / self.playback_rate;
    }

    /// Jumps to a MIDI tick without processing controllers (soft-loop).
    /// Equivalent to: jumpToTick(targetTicks)
    pub(crate) fn jump_to_tick(&mut self, target_ticks: u32) {
        let song_idx = match self.current_song_index {
            Some(i) => i,
            None => return,
        };

        self.send_midi_all_off();
        let seconds = self.songs[song_idx].midi_ticks_to_seconds(target_ticks);
        self.call_event(SequencerEvent::TimeChange(
            crate::sequencer::types::TimeChangeEventData { new_time: seconds },
        ));

        // Recalculate time and reset indexes
        self.recalculate_start_time(seconds);
        self.played_time = seconds;
        self.event_indexes.clear();
        for track in &self.songs[song_idx].tracks {
            let idx = track
                .events
                .iter()
                .position(|e| e.ticks >= target_ticks)
                .unwrap_or(track.events.len());
            self.event_indexes.push(idx);
        }

        // Correct tempo
        let time_division = self.songs[song_idx].time_division;
        let target_tempo = self.songs[song_idx]
            .tempo_changes
            .iter()
            .find(|t| t.ticks <= target_ticks);
        if let Some(tc) = target_tempo {
            self.one_tick_to_seconds = 60.0 / (tc.tempo * time_division as f64);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::basic_midi::BasicMidi;
    use crate::midi::midi_message::MidiMessage;
    use crate::midi::midi_track::MidiTrack;
    use crate::midi::types::TempoChange;
    use crate::synthesizer::types::{SynthProcessorEvent, SynthProcessorOptions};

    fn make_processor() -> SpessaSynthProcessor {
        SpessaSynthProcessor::new(44100.0, |_: SynthProcessorEvent| {}, SynthProcessorOptions::default())
    }

    fn make_sequencer() -> SpessaSynthSequencer {
        SpessaSynthSequencer::new(make_processor())
    }

    fn make_simple_midi() -> BasicMidi {
        let mut midi = BasicMidi::new();
        midi.time_division = 480;
        midi.duration = 2.0;
        midi.first_note_on = 0;
        midi.last_voice_event_tick = 960;
        midi.tempo_changes = vec![TempoChange {
            ticks: 0,
            tempo: 120.0,
        }];
        let mut track = MidiTrack::new();
        track.channels.insert(0);
        track.push_event(MidiMessage::new(0, 0x90, vec![60, 100]));
        track.push_event(MidiMessage::new(480, 0x80, vec![60, 0]));
        track.push_event(MidiMessage::new(960, 0x2F, vec![]));
        midi.tracks.push(track);
        midi
    }

    // -- constructor --

    #[test]
    fn test_new_default_fields() {
        let seq = make_sequencer();
        assert!(seq.songs.is_empty());
        assert!(seq.paused());
        assert!(seq.current_song_index.is_none());
        assert_eq!(seq.playback_rate, 1.0);
        assert!(seq.retrigger_paused_notes);
        assert!(seq.skip_to_first_note_on);
        assert_eq!(seq.loop_count, 0);
    }

    // -- midi_data --

    #[test]
    fn test_midi_data_none_before_load() {
        let seq = make_sequencer();
        assert!(seq.midi_data().is_none());
    }

    #[test]
    fn test_midi_data_some_after_load() {
        let mut seq = make_sequencer();
        seq.load_new_song_list(vec![make_simple_midi()]);
        assert!(seq.midi_data().is_some());
    }

    // -- duration --

    #[test]
    fn test_duration_zero_when_no_song() {
        let seq = make_sequencer();
        assert_eq!(seq.duration(), 0.0);
    }

    #[test]
    fn test_duration_matches_midi() {
        let mut seq = make_sequencer();
        seq.load_new_song_list(vec![make_simple_midi()]);
        assert!((seq.duration() - 2.0).abs() < f64::EPSILON);
    }

    // -- paused --

    #[test]
    fn test_initially_paused() {
        let seq = make_sequencer();
        assert!(seq.paused());
    }

    #[test]
    fn test_play_unpauses() {
        let mut seq = make_sequencer();
        seq.load_new_song_list(vec![make_simple_midi()]);
        seq.play();
        assert!(!seq.paused());
    }

    #[test]
    fn test_pause_pauses() {
        let mut seq = make_sequencer();
        seq.load_new_song_list(vec![make_simple_midi()]);
        seq.play();
        seq.pause();
        assert!(seq.paused());
    }

    // -- playback_rate --

    #[test]
    fn test_playback_rate_default() {
        let seq = make_sequencer();
        assert_eq!(seq.get_playback_rate(), 1.0);
    }

    #[test]
    fn test_set_playback_rate() {
        let mut seq = make_sequencer();
        seq.set_playback_rate(2.0);
        assert_eq!(seq.get_playback_rate(), 2.0);
    }

    // -- find_first_event_index --

    #[test]
    fn test_find_first_event_index_single_track() {
        let mut seq = make_sequencer();
        seq.load_new_song_list(vec![make_simple_midi()]);
        seq.play();
        assert_eq!(seq.find_first_event_index(), 0);
    }

    #[test]
    fn test_find_first_event_index_multi_track() {
        let mut seq = make_sequencer();
        let mut midi = BasicMidi::new();
        midi.time_division = 480;
        midi.duration = 2.0;
        midi.first_note_on = 0;
        midi.last_voice_event_tick = 960;
        midi.tempo_changes = vec![TempoChange { ticks: 0, tempo: 120.0 }];

        // Track 0: first event at tick 100
        let mut t0 = MidiTrack::new();
        t0.channels.insert(0);
        t0.push_event(MidiMessage::new(100, 0x90, vec![60, 100]));
        midi.tracks.push(t0);

        // Track 1: first event at tick 50
        let mut t1 = MidiTrack::new();
        t1.channels.insert(1);
        t1.push_event(MidiMessage::new(50, 0x90, vec![62, 80]));
        midi.tracks.push(t1);

        seq.load_new_song_list(vec![midi]);
        seq.play();
        // Track 1 has the earlier event
        assert_eq!(seq.find_first_event_index(), 1);
    }

    // -- load_new_song_list --

    #[test]
    fn test_load_new_song_list_sets_song() {
        let mut seq = make_sequencer();
        seq.load_new_song_list(vec![make_simple_midi()]);
        assert_eq!(seq.songs.len(), 1);
        assert!(seq.current_song_index.is_some());
    }

    #[test]
    fn test_load_empty_song_list() {
        let mut seq = make_sequencer();
        seq.load_new_song_list(vec![]);
        assert!(seq.songs.is_empty());
        assert!(seq.current_song_index.is_none());
    }

    // -- recalculate_start_time --

    #[test]
    fn test_recalculate_start_time_at_zero() {
        let mut seq = make_sequencer();
        seq.recalculate_start_time(0.0);
        let synth_time = seq.synth.current_synth_time();
        assert!((seq.absolute_start_time - synth_time).abs() < 1e-9);
    }

    // -- send_midi_all_off --

    #[test]
    fn test_send_midi_all_off_no_panic() {
        let mut seq = make_sequencer();
        seq.send_midi_all_off();
    }

    // -- send_midi_reset --

    #[test]
    fn test_send_midi_reset_no_panic() {
        let mut seq = make_sequencer();
        seq.send_midi_reset();
    }

    // -- is_finished --

    #[test]
    fn test_is_finished_default_false() {
        let seq = make_sequencer();
        assert!(!seq.is_finished);
    }

    // -- song_is_finished with single song --

    #[test]
    fn test_song_is_finished_single_song() {
        let mut seq = make_sequencer();
        seq.load_new_song_list(vec![make_simple_midi()]);
        seq.play();
        seq.song_is_finished();
        assert!(seq.is_finished);
        assert!(seq.paused());
    }

    // -- shuffle --

    #[test]
    fn test_shuffle_song_indexes_covers_all() {
        let mut seq = make_sequencer();
        seq.load_new_song_list(vec![make_simple_midi(), make_simple_midi(), make_simple_midi()]);
        let mut sorted = seq.shuffled_song_indexes.clone();
        sorted.sort();
        assert_eq!(sorted, vec![0, 1, 2]);
    }
}
