/// Per-drum-note parameters for drum channels.
/// Each MidiChannel has 128 of these (one per MIDI note).
#[derive(Clone, Debug)]
pub struct DrumParameters {
    /// Pitch offset in cents.
    pub pitch: f64,
    /// Gain multiplier (linear amplitude). Default 1.0.
    pub gain: f64,
    /// Exclusive class override (hi-hat, etc.). 0 = no override.
    pub exclusive_class: u8,
    /// Pan value: 0 = random, 1-127 with 64 = center (adds to channel pan).
    pub pan: u8,
    /// Reverb send multiplier (0.0-1.0).
    pub reverb_gain: f64,
    /// Chorus send multiplier (0.0-1.0).
    pub chorus_gain: f64,
    /// Delay send multiplier (0.0-1.0).
    pub delay_gain: f64,
    /// Whether note-on is received.
    pub rx_note_on: bool,
    /// Whether note-off is received (kills voice instead of release).
    pub rx_note_off: bool,
}

impl Default for DrumParameters {
    fn default() -> Self {
        Self {
            pitch: 0.0,
            gain: 1.0,
            exclusive_class: 0,
            pan: 64,
            reverb_gain: 1.0,
            chorus_gain: 0.0,
            delay_gain: 0.0,
            rx_note_on: true,
            rx_note_off: false,
        }
    }
}

/// Drum reverb reset values per note (SC-88 standard).
/// Most drums get reverb 127, except kick drums (35, 36) which get 0.
pub fn drum_reverb_reset_value(note: usize) -> u8 {
    match note {
        35 | 36 => 0,
        _ => 127,
    }
}

/// Resets a DrumParameters array to defaults.
pub fn reset_drum_params(params: &mut [DrumParameters]) {
    for (i, p) in params.iter_mut().enumerate() {
        p.pitch = 0.0;
        p.gain = 1.0;
        p.exclusive_class = 0;
        p.pan = 64;
        p.reverb_gain = drum_reverb_reset_value(i) as f64 / 127.0;
        p.chorus_gain = 0.0;
        p.delay_gain = 0.0;
        p.rx_note_on = true;
        p.rx_note_off = false;
    }
}
