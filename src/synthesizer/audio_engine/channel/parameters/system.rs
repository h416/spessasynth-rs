/// system.rs
/// purpose: Per-channel system parameters (API-controlled, not MIDI-controlled).
/// Ported from: src/synthesizer/audio_engine/channel/parameters/system.ts
use crate::synthesizer::enums::InterpolationType;

/// The system parameters of the channel.
/// These can only be changed via the API.
/// Equivalent to: interface ChannelSystemParameter
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelSystemParameter {
    // Channel exclusive
    /// If the preset is locked, preventing any program changes from being sent.
    /// Equivalent to: presetLock
    pub preset_lock: bool,

    /// If the channel should not produce any sound
    /// and ignore incoming Note On messages.
    /// Equivalent to: isMuted
    pub is_muted: bool,

    // Shared with synth
    /// The gain for the channel. From 0 to any number. 1 is 100% volume.
    /// Equivalent to: gain
    pub gain: f64,

    /// The panning of the channel. -1 (left) to 1 (right). 0 is center.
    /// Equivalent to: pan
    pub pan: f64,

    /// The channel key shift in semitones.
    /// Drum channels DO NOT ignore this value.
    /// Equivalent to: keyShift
    pub key_shift: f64,

    /// The channel tuning in cents.
    /// Drum channels DO NOT ignore this value.
    /// Equivalent to: fineTune
    pub fine_tune: f64,

    /// The interpolation type used for sample playback.
    /// Overrides the global parameter if set (None = use global).
    /// Equivalent to: interpolationType: InterpolationType | null
    pub interpolation_type: Option<InterpolationType>,

    /// If the channel should prevent changing any parameters via NRPN.
    /// Overrides the global parameter if set (None = use global).
    /// Equivalent to: nrpnParamLock: boolean | null
    pub nrpn_param_lock: Option<bool>,

    /// Indicates whether the channel is in monophonic retrigger mode.
    /// Emulates the behavior of Microsoft GS Wavetable Synth, where a new note
    /// kills the previous one if it is still playing.
    /// Overrides the global parameter if set (None = use global).
    /// Equivalent to: monophonicRetrigger: boolean | null
    pub monophonic_retrigger: Option<bool>,
}

/// A typed (parameter, value) pair for `MidiChannel::set_system_parameter`.
/// Rust equivalent of the TS generic `setSystemParameterInternal<P>(parameter, value)`.
#[derive(Clone, Debug, PartialEq)]
pub enum ChannelSystemParameterValue {
    PresetLock(bool),
    IsMuted(bool),
    Gain(f64),
    Pan(f64),
    KeyShift(f64),
    FineTune(f64),
    InterpolationType(Option<InterpolationType>),
    NrpnParamLock(Option<bool>),
    MonophonicRetrigger(Option<bool>),
}

/// The default system parameters of a channel.
/// Equivalent to: DEFAULT_CHANNEL_SYSTEM_PARAMETERS
pub const DEFAULT_CHANNEL_SYSTEM_PARAMETERS: ChannelSystemParameter = ChannelSystemParameter {
    // Channel exclusive
    preset_lock: false,
    is_muted: false,

    // Shared with synth
    gain: 1.0,
    pan: 0.0,
    key_shift: 0.0,
    fine_tune: 0.0,

    interpolation_type: None,
    nrpn_param_lock: None,
    monophonic_retrigger: None,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_system_parameters() {
        let p = DEFAULT_CHANNEL_SYSTEM_PARAMETERS;
        assert!(!p.preset_lock);
        assert!(!p.is_muted);
        assert_eq!(p.gain, 1.0);
        assert_eq!(p.pan, 0.0);
        assert_eq!(p.key_shift, 0.0);
        assert_eq!(p.fine_tune, 0.0);
        assert_eq!(p.interpolation_type, None);
        assert_eq!(p.nrpn_param_lock, None);
        assert_eq!(p.monophonic_retrigger, None);
    }
}
