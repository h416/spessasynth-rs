//! midi.rs
//! purpose: The channel MIDI parameters (state driven by MIDI messages / SysEx).
//! Ported from: src/synthesizer/audio_engine/channel/parameters/midi.ts
use crate::midi::enums::MidiController;

/// Per-channel MIDI parameters (state driven by MIDI messages / SysEx).
/// Ported from: src/synthesizer/audio_engine/channel/parameters/midi.ts
/// Equivalent to: interface ChannelMIDIParameter
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelMidiParameter {
    /// The current pressure (aftertouch) of this channel.
    /// Equivalent to: pressure
    pub pressure: i32,

    /// The current pitch wheel value (0-16,383) of this channel.
    /// Equivalent to: pitchWheel
    pub pitch_wheel: i32,

    /// The current pitch wheel range, in semitones.
    /// Equivalent to: pitchWheelRange
    pub pitch_wheel_range: f64,

    /// The modulation depth in cents.
    /// This is internally converted to a multiplier by dividing by 50.
    /// The MIDI spec assumes the default modulation depth is 50 cents, but it
    /// may vary for different sound banks.
    /// Equivalent to: modulationDepth
    pub modulation_depth: f64,

    /// The channel's receiving number (0-based index), -1 means OFF.
    /// Only used when customChannelNumbers is enabled.
    /// Equivalent to: rxChannel
    pub rx_channel: i32,

    /// If the channel is in the poly mode.
    /// - `true` - POLY ON - regular playback.
    /// - `false` - MONO ON - one note per channel, others are killed on Note On.
    /// Equivalent to: polyMode
    pub poly_mode: bool,

    /// The key shift of the channel (in semitones). Drum channels ignore this.
    /// Equivalent to: keyShift
    pub key_shift: f64,

    /// Cents, RPN/SysEx for fine-tuning. Drum channels ignore this value.
    /// Equivalent to: fineTune
    pub fine_tune: f64,

    /// Enables random panning for every note played on this channel.
    /// Equivalent to: randomPan
    pub random_pan: bool,

    /// Assign mode for the channel (voice assignment behavior on overlap).
    /// 0 = Single, 1 = LimitedMulti, 2 = FullMulti.
    /// Equivalent to: assignMode
    pub assign_mode: u8,

    /// Indicates whether this channel uses the insertion EFX processor.
    /// Equivalent to: efxAssign
    pub efx_assign: bool,

    /// CC1 for GS controller matrix (arbitrary MIDI controller). Default 16.
    /// Equivalent to: cc1
    pub cc1: MidiController,

    /// CC2 for GS controller matrix (arbitrary MIDI controller). Default 17.
    /// Equivalent to: cc2
    pub cc2: MidiController,

    /// Drum map for GS system exclusive tracking (0 melodic, 1 or 2 drum).
    /// Equivalent to: drumMap
    pub drum_map: u8,

    /// The relation between the input and the actual velocity.
    ///
    /// If Velo Depth is increased, small differences in playing dynamics make a large
    /// difference in the loudness of the sound. If Velo Depth is decreased, even large
    /// differences in playing dynamics make only a small difference in the loudness.
    ///
    /// Examples (with offset set to normal):
    /// - 64 is normal.
    /// - 32 is half velocity at max volume.
    /// - 127 is max velocity at half volume.
    ///
    /// Equivalent to: velocitySenseDepth
    pub velocity_sense_depth: u8,

    /// The offset to add to the input velocity.
    ///
    /// If Velo Offset is set higher than 64, even softly played notes (low velocity) will
    /// be sounded loudly. If Velo Offset is set lower than 64, even strongly played notes
    /// (high velocity) will be sounded softly.
    ///
    /// Examples (with depth set to normal):
    /// - 64 is normal.
    /// - 32 is silent until half velocity, max velocity is half volume.
    /// - 96 starts at half volume and reaches max volume at half velocity.
    /// - 127 always forces velocity to max.
    ///
    /// Equivalent to: velocitySenseOffset
    pub velocity_sense_offset: u8,

    /// The dry (direct) level of the channel, 0-127.
    /// Only active in XG with the variation connection set to SYSTEM.
    /// Equivalent to: dryLevel
    pub dry_level: u8,
}

/// A typed (parameter, value) pair for `MidiChannel::set_midi_parameter`.
/// Rust equivalent of the TS generic `setMIDIParameter<P>(parameter, value)`.
#[derive(Clone, Debug, PartialEq)]
pub enum ChannelMidiParameterValue {
    Pressure(i32),
    PitchWheel(i32),
    PitchWheelRange(f64),
    ModulationDepth(f64),
    RxChannel(i32),
    PolyMode(bool),
    KeyShift(f64),
    FineTune(f64),
    RandomPan(bool),
    AssignMode(u8),
    EfxAssign(bool),
    Cc1(MidiController),
    Cc2(MidiController),
    DrumMap(u8),
    VelocitySenseDepth(u8),
    VelocitySenseOffset(u8),
    DryLevel(u8),
}

/// The default MIDI parameters of a channel.
/// Equivalent to: DEFAULT_CHANNEL_MIDI_PARAMETERS
pub const DEFAULT_CHANNEL_MIDI_PARAMETERS: ChannelMidiParameter = ChannelMidiParameter {
    pressure: 0,
    pitch_wheel: 8192,
    pitch_wheel_range: 2.0,
    modulation_depth: 50.0,
    rx_channel: 0,
    poly_mode: true,
    key_shift: 0.0,
    fine_tune: 0.0,
    random_pan: false,
    assign_mode: 2,
    efx_assign: false,
    cc1: 0x10,
    cc2: 0x11,
    drum_map: 0,
    velocity_sense_depth: 64,
    velocity_sense_offset: 64,
    dry_level: 127,
};

/// The keys of [`ChannelMidiParameter`], used to lock a parameter.
/// Equivalent to: keyof ChannelMIDIParameter
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelMidiParameterKey {
    Pressure,
    PitchWheel,
    PitchWheelRange,
    ModulationDepth,
    RxChannel,
    PolyMode,
    KeyShift,
    FineTune,
    RandomPan,
    AssignMode,
    EfxAssign,
    Cc1,
    Cc2,
    DrumMap,
    VelocitySenseDepth,
    VelocitySenseOffset,
    DryLevel,
}

/// The number of [`ChannelMidiParameterKey`]s.
pub const CHANNEL_MIDI_PARAMETER_COUNT: usize = 17;

impl ChannelMidiParameterValue {
    /// The parameter this value is for.
    pub fn key(&self) -> ChannelMidiParameterKey {
        use ChannelMidiParameterKey as K;
        use ChannelMidiParameterValue as V;
        match self {
            V::Pressure(_) => K::Pressure,
            V::PitchWheel(_) => K::PitchWheel,
            V::PitchWheelRange(_) => K::PitchWheelRange,
            V::ModulationDepth(_) => K::ModulationDepth,
            V::RxChannel(_) => K::RxChannel,
            V::PolyMode(_) => K::PolyMode,
            V::KeyShift(_) => K::KeyShift,
            V::FineTune(_) => K::FineTune,
            V::RandomPan(_) => K::RandomPan,
            V::AssignMode(_) => K::AssignMode,
            V::EfxAssign(_) => K::EfxAssign,
            V::Cc1(_) => K::Cc1,
            V::Cc2(_) => K::Cc2,
            V::DrumMap(_) => K::DrumMap,
            V::VelocitySenseDepth(_) => K::VelocitySenseDepth,
            V::VelocitySenseOffset(_) => K::VelocitySenseOffset,
            V::DryLevel(_) => K::DryLevel,
        }
    }
}

impl ChannelMidiParameter {
    /// All parameters with their values, in the key order of
    /// `DEFAULT_CHANNEL_MIDI_PARAMETERS`.
    /// Equivalent to: Object.entries(midiParameters)
    pub fn entries(&self) -> [ChannelMidiParameterValue; CHANNEL_MIDI_PARAMETER_COUNT] {
        use ChannelMidiParameterValue as V;
        [
            V::Pressure(self.pressure),
            V::PitchWheel(self.pitch_wheel),
            V::PitchWheelRange(self.pitch_wheel_range),
            V::ModulationDepth(self.modulation_depth),
            V::RxChannel(self.rx_channel),
            V::PolyMode(self.poly_mode),
            V::KeyShift(self.key_shift),
            V::FineTune(self.fine_tune),
            V::RandomPan(self.random_pan),
            V::AssignMode(self.assign_mode),
            V::EfxAssign(self.efx_assign),
            V::Cc1(self.cc1),
            V::Cc2(self.cc2),
            V::DrumMap(self.drum_map),
            V::VelocitySenseDepth(self.velocity_sense_depth),
            V::VelocitySenseOffset(self.velocity_sense_offset),
            V::DryLevel(self.dry_level),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameter_keys_are_distinct_and_in_range() {
        let values = DEFAULT_CHANNEL_MIDI_PARAMETERS.entries();
        let mut seen = [false; CHANNEL_MIDI_PARAMETER_COUNT];
        for (i, v) in values.iter().enumerate() {
            let k = v.key() as usize;
            // Object.entries order matches the key order
            assert_eq!(k, i);
            assert!(!seen[k]);
            seen[k] = true;
        }
    }
}
