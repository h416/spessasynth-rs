//! The raw DSP processor interface of the Yamaha XG effects.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/dsp/effect_processor.ts

/// The raw DSP processor for Yamaha XG effects.
/// Reverb, Chorus, Variation and Insertion all use it.
///
/// The TS processors keep a `subarray` view of the block's shared delay memory.
/// Here the block owns the memory and passes it to every call that touches it.
///
/// Equivalent to: interface DefaultXGEffectProcessor (+ DefaultXGEffectProcessorConstructor.memorySize)
pub trait DefaultXGEffectProcessor {
    /// The type of the effect. 16-bit ID, `(MSB << 8) | LSB` (e.g. 0x4100)
    fn effect_type(&self) -> u16;

    /// The delay memory length the processor needs at 44.1 kHz.
    /// Equivalent to: static memorySize
    fn memory_size(&self) -> usize;

    /// Resets all parameters to default.
    fn reset(&mut self, memory: &mut [f32]);

    /// Sets the given effect parameter to the given value.
    /// `param` is 0-based, `value` is 14-bit for two-byte params, 7-bit for single-byte params.
    fn set_parameter(&mut self, param: i32, value: i32);

    /// Process the effect and **OVERWRITES** it to the output.
    /// If `is_insertion` is false, only wet output is sent.
    /// If true, if the effect has the "Dry/Wet" parameter, it honors it.
    #[allow(clippy::too_many_arguments)]
    fn process(
        &mut self,
        memory: &mut [f32],
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
        is_insertion: bool,
    );

    /// Gets a snapshot of this effect processor:
    /// a copy of the 16 parameter values (14-bit wide params, 7-bit single-byte params).
    fn get_snapshot(&self) -> [i16; 16];
}
