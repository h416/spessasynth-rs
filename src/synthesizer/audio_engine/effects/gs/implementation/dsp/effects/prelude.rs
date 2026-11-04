//! What every EFX type file uses: the effect base, the processor trait, the shared gain laws and the delay memory.

pub(crate) use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::blocks::delay_memory::DelayMemory;
pub(crate) use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::glue::*;
pub(crate) use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::gs_effect::{
    reset_base, set_parameter_base,
    GSEffect, GSEffectBase, gs_effect_common,
};
pub(crate) use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::insertion_processor::DefaultGSInsertionProcessor;
