#![allow(unused)]

pub mod midi;
pub mod render;
pub mod sequencer;
pub mod soundbank;
pub mod synthesizer;
pub mod utils;

// Convenience re-exports
pub use midi::basic_midi::BasicMidi;
pub use render::{render_midi_file_to_wav, render_midi_to_wav, RenderOptions};
pub use sequencer::sequencer::SpessaSynthSequencer;
pub use soundbank::sound_bank_loader::load_sound_bank;
pub use synthesizer::audio_engine::engine_components::synth_constants::DEFAULT_SYNTH_MODE;
pub use synthesizer::processor::SpessaSynthProcessor;
pub use synthesizer::types::SynthProcessorOptions;
pub use utils::{audio_to_wav, WaveLoopPoints, WaveMetadata, WaveWriteOptions};
