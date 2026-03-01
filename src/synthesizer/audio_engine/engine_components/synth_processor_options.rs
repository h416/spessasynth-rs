/// synth_processor_options.rs
/// purpose: Default synthesizer processor options constant.
/// Ported from: src/synthesizer/audio_engine/engine_components/synth_processor_options.ts
use crate::synthesizer::types::SynthProcessorOptions;

/// Default synthesizer options.
/// Equivalent to: DEFAULT_SYNTH_OPTIONS
pub const DEFAULT_SYNTH_OPTIONS: SynthProcessorOptions = SynthProcessorOptions {
    enable_event_system: true,
    initial_time: 0.0,
    enable_effects: true,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_synth_options_enable_event_system() {
        assert!(DEFAULT_SYNTH_OPTIONS.enable_event_system);
    }

    #[test]
    fn test_default_synth_options_initial_time() {
        assert_eq!(DEFAULT_SYNTH_OPTIONS.initial_time, 0.0);
    }

    #[test]
    fn test_default_synth_options_enable_effects() {
        assert!(DEFAULT_SYNTH_OPTIONS.enable_effects);
    }

    #[test]
    fn test_default_synth_options_matches_default_trait() {
        let from_trait = SynthProcessorOptions::default();
        assert_eq!(
            DEFAULT_SYNTH_OPTIONS.enable_event_system,
            from_trait.enable_event_system
        );
        assert_eq!(DEFAULT_SYNTH_OPTIONS.initial_time, from_trait.initial_time);
        assert_eq!(
            DEFAULT_SYNTH_OPTIONS.enable_effects,
            from_trait.enable_effects
        );
    }
}
