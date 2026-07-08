/// synth_processor_options.rs
/// purpose: Default synthesizer processor options constant.
/// Ported from: src/synthesizer/audio_engine/synth_processor_options.ts (spessasynth_core
/// 4.3.0; moved out of engine_components/ in the upstream 4.3.0 restructuring)
///
/// Changes from 4.2.0 (reviewed against the 4.3.0 diff): `getDefaultSynthOptions(sampleRate)`
/// (which constructed default reverb/chorus/delay processors) was removed — the processors
/// became optional in `SynthProcessorOptions` and the core constructs the defaults itself.
/// `DEFAULT_SYNTH_OPTIONS` is now exported directly with the reshaped fields
/// (`effectsEnabled`/`eventsEnabled`/`maxBufferSize`/`initialTime`).
use crate::synthesizer::audio_engine::synth_constants::SPESSA_BUFSIZE;
use crate::synthesizer::types::SynthProcessorOptions;

/// Default synthesizer options.
/// Equivalent to: DEFAULT_SYNTH_OPTIONS
pub const DEFAULT_SYNTH_OPTIONS: SynthProcessorOptions = SynthProcessorOptions {
    effects_enabled: true,
    max_buffer_size: SPESSA_BUFSIZE,
    initial_time: 0.0,
    events_enabled: true,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_synth_options_events_enabled() {
        assert!(DEFAULT_SYNTH_OPTIONS.events_enabled);
    }

    #[test]
    fn test_default_synth_options_initial_time() {
        assert_eq!(DEFAULT_SYNTH_OPTIONS.initial_time, 0.0);
    }

    #[test]
    fn test_default_synth_options_effects_enabled() {
        assert!(DEFAULT_SYNTH_OPTIONS.effects_enabled);
    }

    #[test]
    fn test_default_synth_options_max_buffer_size() {
        assert_eq!(DEFAULT_SYNTH_OPTIONS.max_buffer_size, SPESSA_BUFSIZE);
        assert_eq!(DEFAULT_SYNTH_OPTIONS.max_buffer_size, 128);
    }

    #[test]
    fn test_default_synth_options_matches_default_trait() {
        let from_trait = SynthProcessorOptions::default();
        assert_eq!(
            DEFAULT_SYNTH_OPTIONS.events_enabled,
            from_trait.events_enabled
        );
        assert_eq!(DEFAULT_SYNTH_OPTIONS.initial_time, from_trait.initial_time);
        assert_eq!(
            DEFAULT_SYNTH_OPTIONS.effects_enabled,
            from_trait.effects_enabled
        );
        assert_eq!(
            DEFAULT_SYNTH_OPTIONS.max_buffer_size,
            from_trait.max_buffer_size
        );
    }
}
