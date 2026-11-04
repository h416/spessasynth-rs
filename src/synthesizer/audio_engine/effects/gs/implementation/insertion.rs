//! The default GS insertion (EFX) block: owns one processor per EFX type,
//! mixes the selected one into the output and the effect sends.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/insertion.ts
//!              src/synthesizer/audio_engine/effects/gs/interface/gs_insertion_block.ts

use std::collections::HashMap;

use super::dsp::blocks::delay_memory::DelayMemory;
use super::dsp::effect_list::gs_effect_list;
use super::dsp::effects::thru::GSThru;
use super::dsp::insertion_processor::DefaultGSInsertionProcessor;
use crate::synthesizer::audio_engine::synth_constants::EFX_SENDS_GAIN_CORRECTION;
use crate::synthesizer::enums::gs_insertion_control_sources;
use crate::utils::loggin::SpessaLog;

/// The amount of EFX parameters.
const PARAMETER_COUNT: usize = 20;

/// Default send level to reverb, for every EFX type.
const DEFAULT_SEND_LEVEL_TO_REVERB: u8 = 40;

/// The default EFX control depth (0%).
const DEFAULT_CONTROL_DEPTH: u8 = 64;

/// The offset an EFX control source adds to its parameter, like Sound Canvas VA computes it:
/// `(depth - 64) * value / 63`, truncated and stored in a signed byte. Depth 0 counts as 1 (both are -100%).
/// Equivalent to: controlOffset(depth, value)
fn control_offset(depth: u8, value: i8) -> i8 {
    let amount = (if depth == 0 { 1 } else { depth as i32 }) - 64;
    // (Math.trunc((amount * value) / 63) << 24) >> 24
    ((amount * value as i32) / 63) as i8
}

/// A snapshot of a GS insertion block.
/// Equivalent to: interface GSInsertionBlockSnapshot
#[derive(Clone, Debug, PartialEq)]
pub struct GSInsertionBlockSnapshot {
    /// The EFX type of this block, stored as `MSB << 8 | LSB`.
    /// For example `0x30`, `0x10` is `0x3010`.
    /// Equivalent to: type
    pub r#type: u16,
    /// The 20 EFX parameters set since the last type change. 255 means the type default.
    /// Equivalent to: params
    pub params: [u8; PARAMETER_COUNT],
    /// Equivalent to: sendLevelToReverb
    pub send_level_to_reverb: u8,
    /// Equivalent to: sendLevelToChorus
    pub send_level_to_chorus: u8,
    /// Equivalent to: sendLevelToDelay
    pub send_level_to_delay: u8,
    /// Equivalent to: controlSource1
    pub control_source1: u8,
    /// Equivalent to: controlDepth1
    pub control_depth1: u8,
    /// Equivalent to: controlSource2
    pub control_source2: u8,
    /// Equivalent to: controlDepth2
    pub control_depth2: u8,
    /// The last controller values of both control sources (signed, pitch bend is centered).
    /// Equivalent to: controlValues
    pub control_values: [i8; 2],
}

/// The default implementation of the GS insertion block.
/// Equivalent to: DefaultGSInsertion
pub struct DefaultGSInsertion {
    /// `0-127`. Equivalent to: sendLevelToReverb
    pub send_level_to_reverb: u8,
    /// `0-127`. Equivalent to: sendLevelToChorus
    pub send_level_to_chorus: u8,
    /// `0-127`. Equivalent to: sendLevelToDelay
    pub send_level_to_delay: u8,
    /// Equivalent to: controlSource1
    pub control_source1: u8,
    /// Equivalent to: controlDepth1
    pub control_depth1: u8,
    /// Equivalent to: controlSource2
    pub control_source2: u8,
    /// Equivalent to: controlDepth2
    pub control_depth2: u8,
    /// The last controller values of both control sources (signed, pitch bend is centered).
    /// Equivalent to: controlValues
    control_values: [i8; 2],
    /// Every processor of this block. The first one is the fallback.
    processors: Vec<Box<dyn DefaultGSInsertionProcessor>>,
    /// The list of effects available in this block: effect type to an index into `processors`.
    /// Equivalent to: effectMap
    effect_map: HashMap<u16, usize>,
    /// The currently used processor, an index into `processors`.
    /// Equivalent to: processor
    processor: usize,
    /// The requested EFX type.
    /// Equivalent to: type
    r#type: u16,
    /// The parameters set since the last type change.
    /// 255 means the parameter uses the type default.
    /// Equivalent to: params
    params: [u8; PARAMETER_COUNT],
    /// The left output of the processor.
    /// Equivalent to: outputLeft
    output_left: Vec<f32>,
    /// The right output of the processor.
    /// Equivalent to: outputRight
    output_right: Vec<f32>,
}

/// The index of the processor used when the requested type is not available.
/// Equivalent to: fallbackProcessor
const FALLBACK_PROCESSOR: usize = 0;

impl DefaultGSInsertion {
    /// Constructs a new default GS insertion block.
    /// Equivalent to: constructor(sampleRate, maxBufferSize)
    pub fn new(sample_rate: f64, max_buffer_size: usize) -> Self {
        // The types run at the host rate and share one delay memory (only one runs at a time)
        let mut memory = DelayMemory::new();
        let mut processors: Vec<Box<dyn DefaultGSInsertionProcessor>> =
            vec![Box::new(GSThru::new(sample_rate))];
        let mut effect_map = HashMap::new();
        // The fallback is already a Thru, the list's first entry
        for constructor in gs_effect_list().into_iter().skip(1) {
            let processor = constructor(sample_rate, &mut memory);
            effect_map.insert(processor.effect_type(), processors.len());
            processors.push(processor);
        }
        // The fallback handles type 0 and any unsupported type
        effect_map.insert(processors[FALLBACK_PROCESSOR].effect_type(), FALLBACK_PROCESSOR);
        let mut block = Self {
            send_level_to_reverb: DEFAULT_SEND_LEVEL_TO_REVERB,
            send_level_to_chorus: 0,
            send_level_to_delay: 0,
            control_source1: gs_insertion_control_sources::OFF,
            control_depth1: DEFAULT_CONTROL_DEPTH,
            control_source2: gs_insertion_control_sources::OFF,
            control_depth2: DEFAULT_CONTROL_DEPTH,
            control_values: [0; 2],
            processors,
            effect_map,
            processor: FALLBACK_PROCESSOR,
            r#type: 0,
            params: [255; PARAMETER_COUNT],
            output_left: vec![0.0; max_buffer_size],
            output_right: vec![0.0; max_buffer_size],
        };
        block.reset();
        block
    }

    /// Equivalent to: setType(type)
    pub fn set_type(&mut self, efx_type: u16) {
        self.r#type = efx_type;
        let processor = self.effect_map.get(&efx_type).copied();
        if processor.is_none() {
            SpessaLog::gs_fail("EFX Processor", &efx_type.to_be_bytes(), "Using Thru.");
        }
        self.processor = processor.unwrap_or(FALLBACK_PROCESSOR);
        // The control offsets carry over to the new type
        // They go in before the reset, which settles the rotary rotors at the speed they select
        self.update_control();
        self.processors[self.processor].reset();
        self.params.fill(255);
        self.send_level_to_reverb = DEFAULT_SEND_LEVEL_TO_REVERB;
        self.send_level_to_chorus = 0;
        self.send_level_to_delay = 0;
    }

    /// Equivalent to: getType()
    pub fn get_type(&self) -> u16 {
        self.r#type
    }

    /// Equivalent to: reset()
    pub fn reset(&mut self) {
        self.control_source1 = gs_insertion_control_sources::OFF;
        self.control_depth1 = DEFAULT_CONTROL_DEPTH;
        self.control_source2 = gs_insertion_control_sources::OFF;
        self.control_depth2 = DEFAULT_CONTROL_DEPTH;
        self.control_values.fill(0);
        self.set_type(0);
    }

    /// Feeds a controller to the EFX control sources: `source` is the control change number,
    /// `CHANNEL_PRESSURE` or `PITCH_BEND`, `value` the controller value (pitch bend is 0-16383).
    /// Equivalent to: setControllerValue(source, value)
    pub fn set_controller_value(&mut self, source: u8, value: i32) {
        // Only control changes 1-95, channel pressure and pitch bend can be sources
        if !(1..=95).contains(&source)
            && source != gs_insertion_control_sources::CHANNEL_PRESSURE
            && source != gs_insertion_control_sources::PITCH_BEND
        {
            return;
        }
        let signed = if source == gs_insertion_control_sources::PITCH_BEND {
            (value - 8192) >> 6
        } else {
            value
        };
        let mut matched = false;
        if source == self.control_source1 {
            self.control_values[0] = signed as i8;
            matched = true;
        }
        if source == self.control_source2 {
            self.control_values[1] = signed as i8;
            matched = true;
        }
        // The current depths apply from the next matching controller on, like in Sound Canvas VA
        if matched {
            self.update_control();
        }
    }

    /// Sets an EFX parameter. `param` is 0-based (EFX parameter 1 is 0).
    /// Equivalent to: setParameter(param, value)
    pub fn set_parameter(&mut self, param: u8, value: u8) {
        self.params[param as usize] = value;
        self.processors[self.processor].set_parameter(param, value);
    }

    /// Processes the insertion: reads the zero-indexed stereo input,
    /// ADDS to the `start_index`-based stereo output
    /// and ADDS to the zero-indexed mono effect sends.
    /// Equivalent to: process(...)
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        output_reverb: &mut [f32],
        output_chorus: &mut [f32],
        output_delay: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        let processed_left = &mut self.output_left;
        let processed_right = &mut self.output_right;
        self.processors[self.processor].process(
            input_left,
            input_right,
            processed_left,
            processed_right,
            sample_count,
        );

        let to_reverb = (self.send_level_to_reverb as f64 / 127.0) * EFX_SENDS_GAIN_CORRECTION;
        let to_chorus = (self.send_level_to_chorus as f64 / 127.0) * EFX_SENDS_GAIN_CORRECTION;
        let to_delay = (self.send_level_to_delay as f64 / 127.0) * EFX_SENDS_GAIN_CORRECTION;
        for i in 0..sample_count {
            let l = processed_left[i] as f64;
            let r = processed_right[i] as f64;
            let idx = start_index + i;
            output_left[idx] = (output_left[idx] as f64 + l) as f32;
            output_right[idx] = (output_right[idx] as f64 + r) as f32;
            // The sends are mono
            let mono = (l + r) * 0.5;
            output_reverb[i] = (output_reverb[i] as f64 + mono * to_reverb) as f32;
            output_chorus[i] = (output_chorus[i] as f64 + mono * to_chorus) as f32;
            output_delay[i] = (output_delay[i] as f64 + mono * to_delay) as f32;
        }
    }

    /// Equivalent to: getSnapshot()
    pub fn get_snapshot(&self) -> GSInsertionBlockSnapshot {
        GSInsertionBlockSnapshot {
            r#type: self.r#type,
            params: self.params,
            send_level_to_reverb: self.send_level_to_reverb,
            send_level_to_chorus: self.send_level_to_chorus,
            send_level_to_delay: self.send_level_to_delay,
            control_source1: self.control_source1,
            control_depth1: self.control_depth1,
            control_source2: self.control_source2,
            control_depth2: self.control_depth2,
            control_values: self.control_values,
        }
    }

    /// Equivalent to: applySnapshot(snapshot)
    pub fn apply_snapshot(&mut self, snapshot: &GSInsertionBlockSnapshot) {
        self.control_source1 = snapshot.control_source1;
        self.control_depth1 = snapshot.control_depth1;
        self.control_source2 = snapshot.control_source2;
        self.control_depth2 = snapshot.control_depth2;
        self.control_values = snapshot.control_values;
        self.set_type(snapshot.r#type);
        for (i, &value) in snapshot.params.iter().enumerate() {
            if value != 255 {
                self.set_parameter(i as u8, value);
            }
        }
        self.send_level_to_reverb = snapshot.send_level_to_reverb;
        self.send_level_to_chorus = snapshot.send_level_to_chorus;
        self.send_level_to_delay = snapshot.send_level_to_delay;
    }

    /// Sends the offsets of both control sources to the processor.
    /// Equivalent to: updateControl()
    fn update_control(&mut self) {
        let offset1 = control_offset(self.control_depth1, self.control_values[0]);
        let offset2 = control_offset(self.control_depth2, self.control_values[1]);
        self.processors[self.processor].set_control(offset1, offset2);
    }
}
