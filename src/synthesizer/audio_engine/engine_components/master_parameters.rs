/// master_parameters.rs
/// purpose: Default master parameters for the synthesizer.
/// Ported from: src/synthesizer/audio_engine/engine_components/master_parameters.ts
use crate::synthesizer::audio_engine::engine_components::synth_constants::{
    ALL_CHANNELS_OR_DIFFERENT_ACTION, DEFAULT_SYNTH_MODE, SYNTHESIZER_GAIN, VOICE_CAP,
};
use crate::synthesizer::enums::interpolation_types;
use crate::synthesizer::types::MasterParameterType;

/// Default master parameters for the synthesizer.
/// Equivalent to: DEFAULT_MASTER_PARAMETERS
pub const DEFAULT_MASTER_PARAMETERS: MasterParameterType = MasterParameterType {
    master_gain: SYNTHESIZER_GAIN,
    master_pan: 0.0,
    voice_cap: VOICE_CAP,
    interpolation_type: interpolation_types::HERMITE,
    midi_system: DEFAULT_SYNTH_MODE,
    monophonic_retrigger_mode: false,
    reverb_gain: 1.0,
    chorus_gain: 1.0,
    black_midi_mode: false,
    transposition: 0.0,
    device_id: ALL_CHANNELS_OR_DIFFERENT_ACTION,
    delay_gain: 1.0,
};

impl Default for MasterParameterType {
    fn default() -> Self {
        DEFAULT_MASTER_PARAMETERS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthesizer::audio_engine::engine_components::synth_constants::{
        ALL_CHANNELS_OR_DIFFERENT_ACTION, SYNTHESIZER_GAIN, VOICE_CAP,
    };
    use crate::synthesizer::enums::interpolation_types;
    use crate::synthesizer::types::SynthSystem;

    // --- DEFAULT_MASTER_PARAMETERS: field values ---

    #[test]
    fn test_default_master_gain() {
        assert!((DEFAULT_MASTER_PARAMETERS.master_gain - SYNTHESIZER_GAIN).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_master_gain_is_one() {
        assert!((DEFAULT_MASTER_PARAMETERS.master_gain - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_master_pan_is_zero() {
        assert!((DEFAULT_MASTER_PARAMETERS.master_pan - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_voice_cap() {
        assert_eq!(DEFAULT_MASTER_PARAMETERS.voice_cap, VOICE_CAP);
    }

    #[test]
    fn test_default_voice_cap_is_350() {
        assert_eq!(DEFAULT_MASTER_PARAMETERS.voice_cap, 350);
    }

    #[test]
    fn test_default_interpolation_type_is_hermite() {
        assert_eq!(
            DEFAULT_MASTER_PARAMETERS.interpolation_type,
            interpolation_types::HERMITE
        );
    }

    #[test]
    fn test_default_interpolation_type_value() {
        assert_eq!(DEFAULT_MASTER_PARAMETERS.interpolation_type, 2);
    }

    #[test]
    fn test_default_midi_system_is_gs() {
        assert_eq!(DEFAULT_MASTER_PARAMETERS.midi_system, SynthSystem::Gs);
    }

    #[test]
    fn test_default_monophonic_retrigger_mode_is_false() {
        assert!(!DEFAULT_MASTER_PARAMETERS.monophonic_retrigger_mode);
    }

    #[test]
    fn test_default_reverb_gain_is_one() {
        assert!((DEFAULT_MASTER_PARAMETERS.reverb_gain - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_chorus_gain_is_one() {
        assert!((DEFAULT_MASTER_PARAMETERS.chorus_gain - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_black_midi_mode_is_false() {
        assert!(!DEFAULT_MASTER_PARAMETERS.black_midi_mode);
    }

    #[test]
    fn test_default_transposition_is_zero() {
        assert!((DEFAULT_MASTER_PARAMETERS.transposition - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_device_id_is_all_channels() {
        assert_eq!(
            DEFAULT_MASTER_PARAMETERS.device_id,
            ALL_CHANNELS_OR_DIFFERENT_ACTION
        );
    }

    #[test]
    fn test_default_device_id_is_minus_one() {
        assert_eq!(DEFAULT_MASTER_PARAMETERS.device_id, -1);
    }

    // --- MasterParameterType::default() returns the same values as DEFAULT_MASTER_PARAMETERS ---

    #[test]
    fn test_default_trait_master_gain() {
        let mp = MasterParameterType::default();
        assert!((mp.master_gain - DEFAULT_MASTER_PARAMETERS.master_gain).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_trait_master_pan() {
        let mp = MasterParameterType::default();
        assert!((mp.master_pan - DEFAULT_MASTER_PARAMETERS.master_pan).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_trait_voice_cap() {
        let mp = MasterParameterType::default();
        assert_eq!(mp.voice_cap, DEFAULT_MASTER_PARAMETERS.voice_cap);
    }

    #[test]
    fn test_default_trait_interpolation_type() {
        let mp = MasterParameterType::default();
        assert_eq!(
            mp.interpolation_type,
            DEFAULT_MASTER_PARAMETERS.interpolation_type
        );
    }

    #[test]
    fn test_default_trait_midi_system() {
        let mp = MasterParameterType::default();
        assert_eq!(mp.midi_system, DEFAULT_MASTER_PARAMETERS.midi_system);
    }

    #[test]
    fn test_default_trait_monophonic_retrigger_mode() {
        let mp = MasterParameterType::default();
        assert_eq!(
            mp.monophonic_retrigger_mode,
            DEFAULT_MASTER_PARAMETERS.monophonic_retrigger_mode
        );
    }

    #[test]
    fn test_default_trait_reverb_gain() {
        let mp = MasterParameterType::default();
        assert!((mp.reverb_gain - DEFAULT_MASTER_PARAMETERS.reverb_gain).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_trait_chorus_gain() {
        let mp = MasterParameterType::default();
        assert!((mp.chorus_gain - DEFAULT_MASTER_PARAMETERS.chorus_gain).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_trait_black_midi_mode() {
        let mp = MasterParameterType::default();
        assert_eq!(
            mp.black_midi_mode,
            DEFAULT_MASTER_PARAMETERS.black_midi_mode
        );
    }

    #[test]
    fn test_default_trait_transposition() {
        let mp = MasterParameterType::default();
        assert!((mp.transposition - DEFAULT_MASTER_PARAMETERS.transposition).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_trait_device_id() {
        let mp = MasterParameterType::default();
        assert_eq!(mp.device_id, DEFAULT_MASTER_PARAMETERS.device_id);
    }

    // --- Multiple calls to default() produce independent instances ---

    #[test]
    fn test_default_trait_multiple_instances_are_independent() {
        let mp1 = MasterParameterType::default();
        let mp2 = MasterParameterType::default();
        // Both should have independent values
        assert!((mp1.master_gain - mp2.master_gain).abs() < f64::EPSILON);
        assert_eq!(mp1.voice_cap, mp2.voice_cap);
    }
}
