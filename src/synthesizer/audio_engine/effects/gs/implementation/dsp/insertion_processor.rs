//! The raw stereo DSP interface for a single Roland GS insertion (EFX) type.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/insertion_processor.ts

/// The raw stereo DSP for a single Roland GS insertion (EFX) type.
///
/// The insertion block (`DefaultGSInsertion`) does the mixing, the send levels and the EFX control sources.
/// Equivalent to: DefaultGSInsertionProcessor
pub trait DefaultGSInsertionProcessor {
    /// The EFX type of this processor, stored as `MSB << 8 | LSB`.
    /// For example `0x30`, `0x10` is `0x3010`.
    /// Equivalent to: type
    fn effect_type(&self) -> u16;

    /// Resets the params to their default values. The EFX control offsets are kept.
    fn reset(&mut self);

    /// Sets an EFX parameter.
    /// `parameter` is the parameter number (0-based, 0-19). EFX parameter 1 is 0.
    fn set_parameter(&mut self, parameter: u8, value: u8);

    /// Sets the EFX control offsets. Each type adds them to its controlled parameters
    /// (the ones marked `+` and `#` in the SC-8850 manual), clamped to the parameter range.
    /// `offset1` and `offset2` are the signed offsets of control source 1 and 2 (-127 to 127).
    fn set_control(&mut self, offset1: i8, offset2: i8);

    /// Process the effect and **OVERWRITES** it to the output.
    /// All buffers start at index 0.
    fn process(
        &mut self,
        input_l: &[f32],
        input_r: &[f32],
        output_l: &mut [f32],
        output_r: &mut [f32],
        sample_count: usize,
    );
}
