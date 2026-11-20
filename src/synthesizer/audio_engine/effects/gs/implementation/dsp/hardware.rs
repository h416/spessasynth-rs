//! The sample rate of the Sound Canvas DSP.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/hardware.ts

/// The sample rate the Sound Canvas DSP runs at. The hardware's delays, poles and per sample laws are all set at it.
pub const HARDWARE_RATE: f64 = 32_000.0;
