//! SysEx address maps and drum map numbers.
//! Ported from: src/midi/midi_tools/sysex_data.ts (new in 4.4.0)
//!
//! Only the drum map numbers are on the WAV rendering path; the address maps are used by
//! the MIDI editor (not ported).

/// In GS, 0 is melodic and 1 is the first drum map.
/// Equivalent to: DEFAULT_GS_DRUM_MAP
pub const DEFAULT_GS_DRUM_MAP: u8 = 1;

/// In XG, 0 is melodic, 1 is a non-editable drum and 2 is the first drum map.
/// Equivalent to: DEFAULT_XG_DRUM_MAP
pub const DEFAULT_XG_DRUM_MAP: u8 = 2;

/// The drum map number meaning melodic (not a drum channel).
/// Shared by GS and XG.
/// Equivalent to: MELODIC_MAP
pub const MELODIC_MAP: u8 = 0;
