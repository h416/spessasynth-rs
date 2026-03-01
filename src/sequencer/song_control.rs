/// song_control.rs
/// purpose: MIDI port assignment and sequence loading.
/// Ported from: src/sequencer/song_control.ts
use crate::sequencer::sequencer::SpessaSynthSequencer;
use crate::sequencer::types::{SequencerEvent, SongChangeEventData};
use crate::utils::loggin::{spessa_synth_info, spessa_synth_warn};
use crate::utils::other::format_time;

impl SpessaSynthSequencer {
    /// Assigns a MIDI port channel offset to a track.
    /// Equivalent to: assignMIDIPortInternal(trackNum, port)
    pub(crate) fn assign_midi_port(&mut self, track_num: usize, port: u32) {
        let song_idx = match self.current_song_index {
            Some(i) => i,
            None => return,
        };

        // Do not assign ports to empty tracks
        if self.songs[song_idx].tracks[track_num].channels.is_empty() {
            return;
        }

        // Assign new 16 channels if the port is not occupied yet
        if self.midi_port_channel_offset == 0 {
            self.midi_port_channel_offset += 16;
            self.midi_port_channel_offsets.insert(port, 0);
        }

        if !self.midi_port_channel_offsets.contains_key(&port) {
            if self.synth.synth_core.midi_channels.len() < self.midi_port_channel_offset + 15 {
                self.add_new_midi_port();
            }
            self.midi_port_channel_offsets
                .insert(port, self.midi_port_channel_offset);
            self.midi_port_channel_offset += 16;
        }

        self.current_midi_ports[track_num] = port;
    }

    /// Loads a new sequence internally.
    /// Equivalent to: loadNewSequenceInternal(parsedMidi)
    ///
    /// Takes a song index into self.songs instead of a reference, to avoid borrow conflicts.
    pub(crate) fn load_new_sequence(&mut self, song_index: usize) {
        let tracks_len = self.songs[song_index].tracks.len();
        if tracks_len == 0 {
            spessa_synth_warn("This MIDI has no tracks!");
            return;
        }

        let duration = self.songs[song_index].duration;
        if duration == 0.0 {
            spessa_synth_warn("This MIDI file has a duration of exactly 0 seconds.");
            self.paused_time = Some(0.0);
            self.is_finished = true;
            return;
        }

        let time_division = self.songs[song_index].time_division;
        self.one_tick_to_seconds = 60.0 / (120.0 * time_division as f64);
        self.current_song_index = Some(song_index);
        self.is_finished = false;

        // Clear old embedded bank if exists
        self.synth.clear_embedded_bank();

        // Check for embedded soundfont
        if self.songs[song_index].embedded_sound_bank.is_some() {
            spessa_synth_info("Embedded soundbank detected! Using it.");
            let bank_data = self.songs[song_index]
                .embedded_sound_bank
                .clone()
                .unwrap();
            let bank_offset = self.songs[song_index].bank_offset as u8;
            self.synth.set_embedded_sound_bank(bank_data, bank_offset);
            // TODO: preloadSynth for embedded sound bank
        }

        // Copy over the port data
        self.current_midi_ports = self.songs[song_index]
            .tracks
            .iter()
            .map(|t| t.port)
            .collect();

        // Clear last port data
        self.midi_port_channel_offset = 0;
        self.midi_port_channel_offsets.clear();

        // Assign port offsets
        let ports: Vec<(usize, u32)> = self.songs[song_index]
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (i, t.port))
            .collect();
        for (track_index, port) in ports {
            self.assign_midi_port(track_index, port);
        }

        let first_note_on = self.songs[song_index].first_note_on;
        self.first_note_time = self.songs[song_index].midi_ticks_to_seconds(first_note_on);

        let duration_str = format_time(duration.ceil()).time;
        spessa_synth_info(&format!("Total song time: {}", duration_str));

        self.call_event(SequencerEvent::SongChange(SongChangeEventData {
            song_index: self.song_index,
        }));

        if duration <= 0.2 {
            let short_str = format_time(duration.round()).time;
            spessa_synth_warn(&format!(
                "Very short song: ({}). Disabling loop!",
                short_str
            ));
            self.loop_count = 0;
        }

        // Reset the time
        self.set_current_time(0.0);
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
    use crate::synthesizer::processor::SpessaSynthProcessor;

    fn make_processor() -> SpessaSynthProcessor {
        SpessaSynthProcessor::new(44100.0, |_: SynthProcessorEvent| {}, SynthProcessorOptions::default())
    }

    fn make_simple_midi() -> BasicMidi {
        let mut midi = BasicMidi::new();
        midi.time_division = 480;
        midi.duration = 2.0;
        midi.first_note_on = 0;
        midi.last_voice_event_tick = 960;
        midi.tempo_changes = vec![TempoChange { ticks: 0, tempo: 120.0 }];

        let mut track = MidiTrack::new();
        track.channels.insert(0);
        track.push_event(MidiMessage::new(0, 0x90, vec![60, 100]));
        track.push_event(MidiMessage::new(480, 0x80, vec![60, 0]));
        track.push_event(MidiMessage::new(960, 0x2F, vec![]));
        midi.tracks.push(track);
        midi
    }

    fn make_multi_port_midi() -> BasicMidi {
        let mut midi = BasicMidi::new();
        midi.time_division = 480;
        midi.duration = 2.0;
        midi.first_note_on = 0;
        midi.last_voice_event_tick = 960;
        midi.is_multi_port = true;
        midi.tempo_changes = vec![TempoChange { ticks: 0, tempo: 120.0 }];

        // Track 0: port 0
        let mut t0 = MidiTrack::new();
        t0.port = 0;
        t0.channels.insert(0);
        t0.push_event(MidiMessage::new(0, 0x90, vec![60, 100]));
        midi.tracks.push(t0);

        // Track 1: port 1
        let mut t1 = MidiTrack::new();
        t1.port = 1;
        t1.channels.insert(0);
        t1.push_event(MidiMessage::new(0, 0x90, vec![62, 80]));
        midi.tracks.push(t1);

        midi
    }

    // -- load_new_sequence --

    #[test]
    fn test_load_new_sequence_sets_current_song() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        seq.songs.push(make_simple_midi());
        seq.load_new_sequence(0);
        assert_eq!(seq.current_song_index, Some(0));
    }

    #[test]
    fn test_load_new_sequence_sets_first_note_time() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        seq.songs.push(make_simple_midi());
        seq.load_new_sequence(0);
        // first_note_on = 0 → first_note_time = 0.0
        assert!((seq.first_note_time - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_load_new_sequence_zero_duration_pauses() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        let mut midi = BasicMidi::new();
        midi.time_division = 480;
        midi.duration = 0.0;
        midi.tracks.push(MidiTrack::new());
        seq.songs.push(midi);
        seq.load_new_sequence(0);
        assert!(seq.is_finished);
        assert_eq!(seq.paused_time, Some(0.0));
    }

    // -- assign_midi_port --

    #[test]
    fn test_assign_midi_port_basic() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        seq.songs.push(make_simple_midi());
        seq.current_song_index = Some(0);
        seq.current_midi_ports = vec![0];
        seq.assign_midi_port(0, 0);
        assert_eq!(*seq.midi_port_channel_offsets.get(&0).unwrap(), 0);
        assert_eq!(seq.midi_port_channel_offset, 16);
    }

    #[test]
    fn test_assign_midi_port_multi_port() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        seq.songs.push(make_multi_port_midi());
        seq.load_new_sequence(0);
        // Port 0 offset = 0, port 1 offset = 16
        assert_eq!(*seq.midi_port_channel_offsets.get(&0).unwrap(), 0);
        assert_eq!(*seq.midi_port_channel_offsets.get(&1).unwrap(), 16);
    }

    #[test]
    fn test_assign_midi_port_skips_empty_track() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        let mut midi = make_simple_midi();
        // Add an empty track (no channels)
        midi.tracks.push(MidiTrack::new());
        seq.songs.push(midi);
        seq.current_song_index = Some(0);
        seq.current_midi_ports = vec![0, 0];
        // Track 1 has no channels, should not assign port
        let before = seq.midi_port_channel_offset;
        seq.assign_midi_port(1, 5);
        assert_eq!(seq.midi_port_channel_offset, before);
    }

    // -- load via load_new_song_list --

    #[test]
    fn test_load_new_song_list_multi_port_creates_channels() {
        let mut seq = SpessaSynthSequencer::new(make_processor());
        let before_channels = seq.synth.synth_core.midi_channels.len();
        seq.load_new_song_list(vec![make_multi_port_midi()]);
        // Should have created additional channels for port 1
        assert!(seq.synth.synth_core.midi_channels.len() > before_channels);
    }
}
