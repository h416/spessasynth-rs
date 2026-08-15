#![allow(unused)]
// This crate is a file-by-file, function-by-function port of the TypeScript
// spessasynth_core. Version bumps are ported by diffing two upstream trees and
// checking each change against the Rust side, so the value of keeping the two
// readable side by side outweighs idiomatic Rust here. The lints below all fire
// on code that is shaped the way it is *because* the TypeScript is shaped that
// way; taking clippy's advice would break that correspondence. They are allowed
// crate-wide so that the warnings clippy does emit are worth reading.
//
// Statement shape mirrors the TypeScript:
#![allow(clippy::needless_range_loop)] // `for (let i = 0; i < n; i++)`
#![allow(clippy::manual_range_contains)] // `x >= a && x < b`
#![allow(clippy::manual_is_multiple_of)] // `x % n === 0`
#![allow(clippy::explicit_counter_loop)] // counter incremented in the loop body
// Numeric literals are captured from the TypeScript verbatim (see the
// SplitMix32 reference sequence in utils/other.rs): trimming digits to what f64
// can represent would destroy the provenance, which is the point of pinning them.
#![allow(clippy::excessive_precision)]
// f32 is used only inside sample buffers; everything else is f64, and the casts
// at the boundary are written out even when redundant so that the buffer/scalar
// split stays visible. Losing one is how a rounding divergence gets introduced.
#![allow(clippy::unnecessary_cast)]
// API surface mirrors the TypeScript rather than Rust conventions:
#![allow(clippy::new_without_default)] // `new X()` with no Default in the TS
#![allow(clippy::should_implement_trait)] // e.g. a fallible `default()`
#![allow(clippy::too_many_arguments)] // upstream method signatures
#![allow(clippy::module_inception)] // `voice/voice.rs` <- `voice/voice.ts`
// Test arithmetic spelled out as `rate * channels * bytes` documents what each
// factor is; folding the 1s away makes the assertion harder to check.
#![allow(clippy::identity_op)]

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
pub use synthesizer::audio_engine::synth_constants::DEFAULT_SYNTH_MODE;
pub use synthesizer::processor::SpessaSynthProcessor;
pub use synthesizer::types::SynthProcessorOptions;
pub use utils::{audio_to_wav, WaveLoopPoints, WaveMetadata, WaveWriteOptions};
