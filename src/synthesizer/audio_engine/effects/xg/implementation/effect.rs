//! The default implementation of a single XG effect block.
//! Ported from: src/synthesizer/audio_engine/effects/xg/implementation/effect.ts

use super::dsp::effect_processor::DefaultXGEffectProcessor;
use super::dsp::xg_dsp_program::get_memory_size;

/// A snapshot of an XG effect block.
/// Equivalent to: interface XGEffectBlockSnapshot
#[derive(Clone, Debug, PartialEq)]
pub struct XGEffectBlockSnapshot {
    /// The type of the processor.
    pub r#type: u16,
    /// The 16 parameter values for this effect block.
    pub params: [i16; 16],
}

/// The default implementation for XGEffectBlock.
///
/// This struct represents a single XG effect block.
/// Each block contains a specific processor and may be an insertion or system effect.
///
/// The block owns the delay memory that all of its processors share (only one runs at a time).
///
/// Equivalent to: abstract class DefaultXGEffect
pub struct DefaultXGEffect {
    /// The currently used processor: an index into `effect_map`, None for the fallback.
    processor: Option<usize>,
    pub output_left: Vec<f32>,
    pub output_right: Vec<f32>,
    /// The list of effects available in this block (effect type, processor), in map order.
    effect_map: Vec<(u16, Box<dyn DefaultXGEffectProcessor>)>,
    fallback_processor: Box<dyn DefaultXGEffectProcessor>,
    default_type: u16,
    r#type: u16,
    /// The delay memory shared by all processors of this block.
    memory: Vec<f32>,
}

impl DefaultXGEffect {
    /// Initializes the new XG effect block.
    /// - `effect_map`: all the processors of the block, keyed by effect type.
    /// - `fallback_processor`: the fallback processor in case the real processor is missing.
    /// - `default_type`: the default effect type.
    pub fn new(
        effect_map: Vec<(u16, Box<dyn DefaultXGEffectProcessor>)>,
        fallback_processor: Box<dyn DefaultXGEffectProcessor>,
        default_type: u16,
        sample_rate: f64,
        max_buffer_size: usize,
    ) -> Self {
        // Only one processor runs at a time,
        // So they all share one delay memory.
        let mut memory_length = get_memory_size(sample_rate, fallback_processor.memory_size());
        for (_, processor) in &effect_map {
            memory_length = memory_length.max(get_memory_size(sample_rate, processor.memory_size()));
        }
        let processor = effect_map.iter().position(|(t, _)| *t == default_type);
        Self {
            processor,
            output_left: vec![0.0; max_buffer_size],
            output_right: vec![0.0; max_buffer_size],
            effect_map,
            fallback_processor,
            default_type,
            r#type: default_type,
            memory: vec![0.0; memory_length],
        }
    }

    fn find(&self, effect_type: u16) -> Option<usize> {
        self.effect_map.iter().position(|(t, _)| *t == effect_type)
    }

    /// Sets the type of the processor.
    /// Per XG spec, BASIC EFFECT (LSB = 0) is used if the exact match is missing.
    pub fn set_type(&mut self, effect_type: u16) {
        self.r#type = effect_type;
        self.processor = self.find(effect_type).or_else(|| self.find(effect_type & 0xff_00));
        let memory = &mut self.memory;
        match self.processor {
            Some(i) => self.effect_map[i].1.reset(memory),
            None => self.fallback_processor.reset(memory),
        }
    }

    /// Gets the current 16-bit type of the processor.
    pub fn get_type(&self) -> u16 {
        self.r#type
    }

    /// Resets this block to default values, including the processor type.
    pub fn reset(&mut self) {
        self.set_type(self.default_type);
    }

    /// Sets the given effect parameter to the given value.
    pub fn set_parameter(&mut self, param: i32, value: i32) {
        self.processor_mut().set_parameter(param, value);
    }

    fn processor_mut(&mut self) -> &mut dyn DefaultXGEffectProcessor {
        match self.processor {
            Some(i) => self.effect_map[i].1.as_mut(),
            None => self.fallback_processor.as_mut(),
        }
    }

    fn processor_ref(&self) -> &dyn DefaultXGEffectProcessor {
        match self.processor {
            Some(i) => self.effect_map[i].1.as_ref(),
            None => self.fallback_processor.as_ref(),
        }
    }

    /// Runs the current processor into the block's own output buffers.
    /// Equivalent to: this.processor.process(inputLeft, inputRight, this.outputLeft, this.outputRight, sampleCount, isInsertion)
    pub fn process_processor(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        sample_count: usize,
        is_insertion: bool,
    ) {
        let processor: &mut dyn DefaultXGEffectProcessor = match self.processor {
            Some(i) => self.effect_map[i].1.as_mut(),
            None => self.fallback_processor.as_mut(),
        };
        processor.process(
            &mut self.memory,
            input_left,
            input_right,
            &mut self.output_left,
            &mut self.output_right,
            sample_count,
            is_insertion,
        );
    }

    pub fn get_snapshot(&self) -> XGEffectBlockSnapshot {
        XGEffectBlockSnapshot { r#type: self.r#type, params: self.processor_ref().get_snapshot() }
    }

    pub fn apply_snapshot(&mut self, snapshot: &XGEffectBlockSnapshot) {
        self.set_type(snapshot.r#type);
        for i in 0..16 {
            self.set_parameter(i as i32, snapshot.params[i] as i32);
        }
    }
}
