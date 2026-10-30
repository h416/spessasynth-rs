//! insertion/mod.rs
//! purpose: The default GS insertion (EFX) block: owns one processor per EFX type,
//! mixes the selected one into the output and the effect sends.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/insertion.ts
//!              src/synthesizer/audio_engine/effects/gs/interface/gs_insertion_block.ts

pub mod auto_pan;
pub mod auto_wah;
pub mod convert;
pub mod insertion_processor;
pub mod ph_auto_wah;
pub mod phaser;
pub mod stereo_eq;
pub mod thru;
pub mod tremolo;
pub mod utils;

use std::collections::HashMap;

use self::auto_pan::AutoPanFx;
use self::auto_wah::AutoWahFx;
use self::insertion_processor::DefaultGSInsertionProcessor;
use self::ph_auto_wah::PhAutoWahFx;
use self::phaser::PhaserFx;
use self::stereo_eq::StereoEqFx;
use self::thru::ThruFx;
use self::tremolo::TremoloFx;
use crate::synthesizer::audio_engine::synth_constants::EFX_SENDS_GAIN_CORRECTION;
use crate::utils::loggin::SpessaLog;

/// The amount of EFX parameters.
const PARAMETER_COUNT: usize = 20;

/// Default send level to reverb, for every EFX type.
const DEFAULT_SEND_LEVEL_TO_REVERB: u8 = 40;

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
    /// The list of effects available in this block, by effect type.
    /// The selected one is moved out into `processor` (see `select`).
    /// Equivalent to: effectMap
    effect_map: HashMap<u16, Box<dyn DefaultGSInsertionProcessor>>,
    /// The processor used when the requested type is not available. `None` while selected.
    /// Equivalent to: fallbackProcessor
    fallback_processor: Option<Box<dyn DefaultGSInsertionProcessor>>,
    /// The currently used processor.
    /// Equivalent to: processor
    processor: Box<dyn DefaultGSInsertionProcessor>,
    /// Where `processor` came from: `Some(type)` for an `effect_map` entry, `None` for the fallback.
    processor_slot: Option<u16>,
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

impl DefaultGSInsertion {
    /// Constructs a new default GS insertion block.
    /// Equivalent to: constructor(sampleRate, maxBufferSize)
    pub fn new(sample_rate: f64, max_buffer_size: usize) -> Self {
        // Equivalent to: INSERTION_LIST
        let insertion_list: Vec<Box<dyn DefaultGSInsertionProcessor>> = vec![
            Box::new(ThruFx::new(sample_rate)),
            Box::new(StereoEqFx::new(sample_rate)),
            Box::new(PhaserFx::new(sample_rate)),
            Box::new(AutoPanFx::new(sample_rate)),
            Box::new(AutoWahFx::new(sample_rate)),
            Box::new(PhAutoWahFx::new(sample_rate, max_buffer_size)),
            Box::new(TremoloFx::new(sample_rate)),
        ];
        let effect_map = insertion_list.into_iter().map(|p| (p.effect_type(), p)).collect();
        let mut block = Self {
            send_level_to_reverb: DEFAULT_SEND_LEVEL_TO_REVERB,
            send_level_to_chorus: 0,
            send_level_to_delay: 0,
            effect_map,
            fallback_processor: None,
            processor: Box::new(ThruFx::new(sample_rate)),
            processor_slot: None,
            r#type: 0,
            params: [255; PARAMETER_COUNT],
            output_left: vec![0.0; max_buffer_size],
            output_right: vec![0.0; max_buffer_size],
        };
        block.reset();
        block
    }

    /// Moves the processor for `slot` (`Some(type)` from `effect_map`, `None` for the fallback)
    /// into `processor`, returning the previous one to where it belongs.
    /// TS keeps references to the map entries; Rust moves the selected one out.
    fn select(&mut self, slot: Option<u16>) {
        if slot == self.processor_slot {
            return;
        }
        let selected = match slot {
            Some(efx_type) => self.effect_map.remove(&efx_type),
            None => self.fallback_processor.take(),
        }
        .expect("insertion processor is registered");
        let previous = std::mem::replace(&mut self.processor, selected);
        match self.processor_slot {
            Some(efx_type) => {
                self.effect_map.insert(efx_type, previous);
            }
            None => self.fallback_processor = Some(previous),
        }
        self.processor_slot = slot;
    }

    /// Equivalent to: setType(type)
    pub fn set_type(&mut self, efx_type: u16) {
        self.r#type = efx_type;
        let available =
            self.processor_slot == Some(efx_type) || self.effect_map.contains_key(&efx_type);
        if !available {
            SpessaLog::gs_fail("EFX Processor", &efx_type.to_be_bytes(), "Using Thru.");
        }
        // this.processor = processor ?? this.fallbackProcessor;
        self.select(if available { Some(efx_type) } else { None });
        self.processor.reset();
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
        self.set_type(0);
    }

    /// Sets an EFX parameter. `param` is 0-based (EFX parameter 1 is 0).
    /// Equivalent to: setParameter(param, value)
    pub fn set_parameter(&mut self, param: u8, value: u8) {
        self.params[param as usize] = value;
        self.processor.set_parameter(param, value);
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
        self.processor.process(
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
        }
    }

    /// Equivalent to: applySnapshot(snapshot)
    pub fn apply_snapshot(&mut self, snapshot: &GSInsertionBlockSnapshot) {
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
}
