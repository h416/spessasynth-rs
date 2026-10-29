//! Per-key drum parameters (GS NRPN / XG drum setup / GS drum setup SysEx).
//! Ported from: src/midi/drum_parameters.ts
//!
//! The raw MIDI values (level, sends 0-127, coarse/fine pitch) are stored and converted in
//! note_on.

/// Default drum reverb send: 127 for every key, except the kicks (35, 36) which have none.
/// Equivalent to: DEFAULT_DRUM_REVERB (synthesizer/audio_engine/channel/reset.ts)
pub const DEFAULT_DRUM_REVERB: [u8; 128] = {
    let mut a = [127u8; 128];
    // Kicks have no reverb
    a[35] = 0;
    a[36] = 0;
    a
};

/// Parameters of a single drum key.
/// Equivalent to: interface DrumParameter (midi/types.ts)
#[derive(Clone, Debug, PartialEq)]
pub struct DrumParameter {
    /// Pitch offset in semitones (may be fractional: GS NRPN is 50 cents per step).
    pub pitch_coarse: f64,
    /// Pitch offset in cents.
    pub pitch_fine: f64,
    /// Level, 0-127. 120 is unity (gain = (level / 120)^2).
    pub level: u8,
    /// Exclusive class override (hi-hat, etc.). 0 = no override.
    pub assign_group: u8,
    /// Pan: 0 = random, 1-127 with 64 = center (adds to channel pan).
    pub pan: u8,
    /// Reverb send, 0-127.
    pub reverb_send: u8,
    /// Chorus send, 0-127.
    pub chorus_send: u8,
    /// GS delay / XG variation send, 0-127.
    pub variation_send: u8,
    /// Whether note-on is received.
    pub rx_note_on: bool,
    /// Whether note-off is received (kills voice instead of release).
    pub rx_note_off: bool,
}

/// Parameters of a single key of a GS User Drum Set.
/// Equivalent to: interface UserDrumSetParameter extends DrumParameter (midi/types.ts)
#[derive(Clone, Debug, PartialEq)]
pub struct UserDrumSetParameter {
    /// The per-key drum parameters (pitch, level, sends, ...).
    pub drum: DrumParameter,
    /// The key of the source drum set that this key plays.
    pub source_note_number: u8,
    /// The source drum set (bank LSB of the GS drum patch, i.e. the SC map).
    pub source_drum_set: u8,
    /// The program of the source drum set.
    pub program: u8,
}

/// A single `UserDrumSetParameter` field and its new value.
/// Rust equivalent of the TS `setUserDrumSetParam<K>(.., parameter: K, value)` pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UserDrumSetParameterChange {
    PitchCoarse(f64),
    Level(u8),
    AssignGroup(u8),
    Pan(u8),
    ReverbSend(u8),
    ChorusSend(u8),
    RxNoteOff(bool),
    RxNoteOn(bool),
    VariationSend(u8),
    SourceDrumSet(u8),
    Program(u8),
    SourceNoteNumber(u8),
}

impl UserDrumSetParameterChange {
    /// Sets the field. Returns false if it already had this value.
    pub fn apply(self, p: &mut UserDrumSetParameter) -> bool {
        use UserDrumSetParameterChange as C;
        macro_rules! set {
            ($field:expr, $v:expr) => {{
                if $field == $v {
                    return false;
                }
                $field = $v;
            }};
        }
        match self {
            C::PitchCoarse(v) => set!(p.drum.pitch_coarse, v),
            C::Level(v) => set!(p.drum.level, v),
            C::AssignGroup(v) => set!(p.drum.assign_group, v),
            C::Pan(v) => set!(p.drum.pan, v),
            C::ReverbSend(v) => set!(p.drum.reverb_send, v),
            C::ChorusSend(v) => set!(p.drum.chorus_send, v),
            C::RxNoteOff(v) => set!(p.drum.rx_note_off, v),
            C::RxNoteOn(v) => set!(p.drum.rx_note_on, v),
            C::VariationSend(v) => set!(p.drum.variation_send, v),
            C::SourceDrumSet(v) => set!(p.source_drum_set, v),
            C::Program(v) => set!(p.program, v),
            C::SourceNoteNumber(v) => set!(p.source_note_number, v),
        }
        true
    }
}

/// Equivalent to: class DrumParameterUtils
pub struct DrumParameterUtils;

impl DrumParameterUtils {
    /// The default parameters of a drum key.
    /// Equivalent to: DrumParameterUtils.DEFAULT_DATA\[midiNote\]
    pub fn default_data(midi_note: usize) -> DrumParameter {
        DrumParameter {
            pitch_coarse: 0.0,
            pitch_fine: 0.0,
            level: 120,
            assign_group: 0,
            pan: 64,
            reverb_send: DEFAULT_DRUM_REVERB[midi_note],
            chorus_send: 0,
            variation_send: 0,
            rx_note_on: true,
            rx_note_off: false,
        }
    }

    /// The default parameters of a GS User Drum Set key.
    /// Equivalent to: DrumParameterUtils.DEFAULT_USER_DATA\[midiNote\]
    pub fn default_user_data(midi_note: usize) -> UserDrumSetParameter {
        UserDrumSetParameter {
            drum: Self::default_data(midi_note),
            source_note_number: midi_note as u8,
            // Default to 2 which is SC-88 drum set (confirmed with SCVA)
            source_drum_set: 2,
            program: 0,
        }
    }

    /// Equivalent to: DrumParameterUtils.copyIntoUser(source, dest)
    pub fn copy_into_user(source: &UserDrumSetParameter, dest: &mut UserDrumSetParameter) {
        Self::copy_into(&source.drum, &mut dest.drum);
        dest.source_drum_set = source.source_drum_set;
        dest.source_note_number = source.source_note_number;
        dest.program = source.program;
    }

    /// Equivalent to: DrumParameterUtils.copyInto(source, dest)
    pub fn copy_into(source: &DrumParameter, dest: &mut DrumParameter) {
        dest.pitch_coarse = source.pitch_coarse;
        dest.pitch_fine = source.pitch_fine;
        dest.level = source.level;
        dest.assign_group = source.assign_group;
        dest.pan = source.pan;
        dest.reverb_send = source.reverb_send;
        dest.chorus_send = source.chorus_send;
        dest.variation_send = source.variation_send;
        dest.rx_note_off = source.rx_note_off;
        dest.rx_note_on = source.rx_note_on;
    }

    /// Equivalent to: DrumParameterUtils.isDefault(param, midiNote)
    pub fn is_default(param: &DrumParameter, midi_note: usize) -> bool {
        *param == Self::default_data(midi_note)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_reverb_skips_kicks() {
        assert_eq!(DEFAULT_DRUM_REVERB[34], 127);
        assert_eq!(DEFAULT_DRUM_REVERB[35], 0);
        assert_eq!(DEFAULT_DRUM_REVERB[36], 0);
        assert_eq!(DEFAULT_DRUM_REVERB[37], 127);
    }

    #[test]
    fn test_default_data_is_unity() {
        let d = DrumParameterUtils::default_data(40);
        assert_eq!(d.level, 120);
        assert_eq!(d.pan, 64);
        assert_eq!((d.chorus_send, d.variation_send), (0, 0));
        assert!(d.rx_note_on && !d.rx_note_off);
        assert!(DrumParameterUtils::is_default(&d, 40));
    }

    #[test]
    fn test_copy_into() {
        let mut src = DrumParameterUtils::default_data(0);
        src.level = 10;
        src.pitch_coarse = -1.5;
        let mut dst = DrumParameterUtils::default_data(0);
        DrumParameterUtils::copy_into(&src, &mut dst);
        assert_eq!(src, dst);
        assert!(!DrumParameterUtils::is_default(&dst, 0));
    }
}
