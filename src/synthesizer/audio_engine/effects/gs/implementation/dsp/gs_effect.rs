//! The base of every clean GS insertion (EFX) type: the 20 parameters and the EFX control offsets.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/gs_effect.ts

use super::blocks::delay_memory::DelayMemory;
use super::insertion_processor::DefaultGSInsertionProcessor;

/// The number of EFX parameters (1-20 in the manuals, 0-19 here).
pub const PARAMETER_COUNT: usize = 20;

/// The state every clean GS insertion (EFX) type shares.
///
/// It keeps the 20 parameters and the EFX control offsets. A type owns its blocks, turns the parameters into
/// block settings in `update` (called after every change) and runs the blocks in `process` at the host's rate.
/// TS has this as the abstract class `GSEffect`; Rust keeps the fields here and the abstract methods in the
/// `GSEffect` trait.
pub struct GSEffectBase {
    pub sample_rate: f64,
    /// The current parameters (0-19, EFX parameter 1 is 0).
    pub parameters: [u8; PARAMETER_COUNT],
    /// The default parameters of this type.
    defaults: [u8; PARAMETER_COUNT],
    /// The largest valid value of the parameters that do not span 0-127 (switches, Q indices), as
    /// `(parameter, limit)`. Like the hardware, a value above it is ignored.
    limits: &'static [(u8, u8)],
    /// The smallest valid value of the parameters that do not start at 0 (a value below it is ignored too).
    minimums: &'static [(u8, u8)],
    /// Set while `reset` applies the defaults, so blocks can jump to them instead of moving there.
    pub resetting: bool,
    control: [i8; 2],
}

impl GSEffectBase {
    /// `sample_rate` is the host rate, in Hertz, `memory` the shared delay memory, for the types with long delay
    /// lines.
    pub fn new(
        sample_rate: f64,
        memory: Option<&mut DelayMemory>,
        defaults: [u8; PARAMETER_COUNT],
        limits: &'static [(u8, u8)],
        minimums: &'static [(u8, u8)],
    ) -> Self {
        // This type's lines start where every other type's do
        if let Some(memory) = memory {
            memory.rewind();
        }
        Self {
            sample_rate,
            parameters: [0; PARAMETER_COUNT],
            defaults,
            limits,
            minimums,
            resetting: false,
            control: [0; 2],
        }
    }

    /// A parameter (0-19) as a number.
    #[inline]
    pub fn p(&self, parameter: usize) -> u8 {
        self.parameters[parameter]
    }

    /// A parameter with the offset of an EFX control source added, clamped to 0-127.
    /// `source` is the control source (0 or 1), `parameter` the parameter (0-19).
    pub fn controlled(&self, source: usize, parameter: usize) -> u8 {
        0i32.max(127i32.min(self.parameters[parameter] as i32 + self.control[source] as i32)) as u8
    }

    /// The offset of an EFX control source, a signed byte (for the parameters it moves by a scaled amount).
    /// `source` is the control source (0 or 1).
    pub fn control_offset(&self, source: usize) -> i8 {
        self.control[source]
    }

    fn limit(&self, parameter: u8) -> u8 {
        self.limits
            .iter()
            .find(|(p, _)| *p == parameter)
            .map_or(127, |(_, v)| *v)
    }

    fn minimum(&self, parameter: u8) -> u8 {
        self.minimums
            .iter()
            .find(|(p, _)| *p == parameter)
            .map_or(0, |(_, v)| *v)
    }
}

/// The abstract part of every clean GS insertion (EFX) type.
/// Equivalent to: abstract class GSEffect
pub trait GSEffect {
    /// Equivalent to: type
    fn effect_type(&self) -> u16;

    fn base(&self) -> &GSEffectBase;

    fn base_mut(&mut self) -> &mut GSEffectBase;

    /// Clears the audio state (filter states, delay lines).
    fn clear(&mut self);

    /// Applies the parameters and the control offsets to the blocks.
    fn update(&mut self);

    fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        sample_count: usize,
    );

    /// Resets the parameters to the defaults. A type that overrides it calls `reset_base` (TS `super.reset()`).
    fn reset(&mut self) {
        reset_base(self);
    }

    /// Sets a parameter. A type that overrides it calls `set_parameter_base` (TS `super.setParameter()`).
    fn set_parameter(&mut self, parameter: u8, value: u8) {
        set_parameter_base(self, parameter, value);
    }
}

/// `GSEffect.reset` of TS.
pub fn reset_base<T: GSEffect + ?Sized>(effect: &mut T) {
    let base = effect.base_mut();
    base.parameters = base.defaults;
    effect.clear();
    effect.base_mut().resetting = true;
    effect.update();
    effect.base_mut().resetting = false;
}

/// `GSEffect.setParameter` of TS.
pub fn set_parameter_base<T: GSEffect + ?Sized>(effect: &mut T, parameter: u8, value: u8) {
    let base = effect.base();
    if parameter as usize >= PARAMETER_COUNT || value > base.limit(parameter) || value < base.minimum(parameter) {
        return;
    }
    effect.base_mut().parameters[parameter as usize] = value;
    effect.update();
}

impl<T: GSEffect> DefaultGSInsertionProcessor for T {
    fn effect_type(&self) -> u16 {
        GSEffect::effect_type(self)
    }

    fn reset(&mut self) {
        GSEffect::reset(self);
    }

    fn set_parameter(&mut self, parameter: u8, value: u8) {
        GSEffect::set_parameter(self, parameter, value);
    }

    fn set_control(&mut self, offset1: i8, offset2: i8) {
        let base = self.base_mut();
        base.control[0] = offset1;
        base.control[1] = offset2;
        self.update();
    }

    fn process(
        &mut self,
        input_l: &[f32],
        input_r: &[f32],
        output_l: &mut [f32],
        output_r: &mut [f32],
        sample_count: usize,
    ) {
        GSEffect::process(self, input_l, input_r, output_l, output_r, sample_count);
    }
}

/// The `effect_type`, `base` and `base_mut` of a type.
/// `$type` is the type (`MSB << 8 | LSB`), or `dynamic` for a shared struct that keeps it in `self.effect_type`.
/// The base is `self.base`, or the field path after a `;` (for a type built on a shared struct).
macro_rules! gs_effect_common {
    (@base $($field:ident).+) => {
        fn base(&self) -> &$crate::synthesizer::audio_engine::effects::gs::implementation::dsp::gs_effect::GSEffectBase {
            &self.$($field).+
        }

        fn base_mut(
            &mut self,
        ) -> &mut $crate::synthesizer::audio_engine::effects::gs::implementation::dsp::gs_effect::GSEffectBase {
            &mut self.$($field).+
        }
    };
    (dynamic) => {
        fn effect_type(&self) -> u16 {
            self.effect_type
        }
        gs_effect_common!(@base base);
    };
    ($type:expr; $($field:ident).+) => {
        fn effect_type(&self) -> u16 {
            $type
        }
        gs_effect_common!(@base $($field).+);
    };
    ($type:expr) => {
        fn effect_type(&self) -> u16 {
            $type
        }
        gs_effect_common!(@base base);
    };
}
pub(crate) use gs_effect_common;
