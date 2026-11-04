//! Every clean EFX type: Thru and the 64 SC-88Pro types.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/effect_list.ts

use super::blocks::delay_memory::DelayMemory;
use super::effects::auto_pan::GSAutoPan;
use super::effects::auto_wah::GSAutoWah;
use super::effects::bass_multi::GSBassMulti;
use super::effects::chorus_parallel_flanger::GSChorusParallelFlanger;
use super::effects::chorus_to_flanger::GSChorusFlanger;
use super::effects::clean_guitar_multi::{GSCleanGuitarMulti1, GSCleanGuitarMulti2};
use super::effects::compressor::GSCompressor;
use super::effects::distortion::GSDistortion;
use super::effects::drive_combinations::{GSDistortionChorus, GSDistortionDelay, GSDistortionFlanger, GSOverdriveChorus, GSOverdriveDelay, GSOverdriveFlanger};
use super::effects::enhancer::GSEnhancer;
use super::effects::enhancer_combinations::{GSEnhancerChorus, GSEnhancerDelay, GSEnhancerFlanger};
use super::effects::equalizer::GSEqualizer;
use super::effects::feedback_pitch_shifter::GSFeedbackPitchShifter;
use super::effects::gate_reverb::GSGateReverb;
use super::effects::guitar_multis::{GSGuitarMulti1, GSGuitarMulti2, GSGuitarMulti3};
use super::effects::hexa_chorus::GSHexaChorus;
use super::effects::humanizer::GSHumanizer;
use super::effects::keyboard_multi::GSKeyboardMulti;
use super::effects::limiter::GSLimiter;
use super::effects::lo_fi_1::GSLoFi1;
use super::effects::lo_fi_2::GSLoFi2;
use super::effects::modulation_delay::GSModulationDelay;
use super::effects::modulation_parallel_delay::{GSChorusParallelDelay, GSFlangerParallelDelay};
use super::effects::modulation_to_delay::{GSChorusDelay, GSFlangerDelay};
use super::effects::overdrive::GSOverdrive;
use super::effects::parallel_combinations::{GSOverdriveParallelAutoWah, GSOverdriveParallelOverdrive, GSOverdriveParallelPhaser, GSOverdriveParallelRotary, GSPhaserParallelAutoWah, GSPhaserParallelRotary};
use super::effects::phaser::GSPhaser;
use super::effects::reverb::GSReverb;
use super::effects::rhodes_multi::GSRhodesMulti;
use super::effects::rotary::GSRotary;
use super::effects::rotary_multi::GSRotaryMulti;
use super::effects::space_d::GSSpaceD;
use super::effects::spectrum::GSSpectrum;
use super::effects::step_flanger::GSStepFlanger;
use super::effects::stereo_chorus::GSStereoChorus;
use super::effects::stereo_delay::GSStereoDelay;
use super::effects::stereo_flanger::GSStereoFlanger;
use super::effects::tap_delay::{GSQuadTapDelay, GSTripleTapDelay};
use super::effects::three_d_auto::GSThreeDAuto;
use super::effects::three_d_chorus::GSThreeDChorus;
use super::effects::three_d_delay::GSThreeDDelay;
use super::effects::three_d_manual::GSThreeDManual;
use super::effects::time_control_delay::GSTimeControlDelay;
use super::effects::tremolo::GSTremolo;
use super::effects::tremolo_chorus::GSTremoloChorus;
use super::effects::two_voice_pitch_shifter::GSTwoVoicePitchShifter;
use super::effects::thru::GSThru;
use super::insertion_processor::DefaultGSInsertionProcessor;

/// Creates a clean EFX type. All types of one insertion block share the memory.
/// Equivalent to: GSEffectConstructor
pub type GSEffectConstructor = fn(f64, &mut DelayMemory) -> Box<dyn DefaultGSInsertionProcessor>;

macro_rules! constructor {
    ($t:ty) => {
        (|sample_rate, memory| Box::new(<$t>::new(sample_rate, memory))) as GSEffectConstructor
    };
}

/// Thru takes no memory.
fn thru(sample_rate: f64, _memory: &mut DelayMemory) -> Box<dyn DefaultGSInsertionProcessor> {
    Box::new(GSThru::new(sample_rate))
}

/// Every clean EFX type: Thru and the 64 SC-88Pro types.
/// Equivalent to: GS_EFFECT_LIST
pub fn gs_effect_list() -> Vec<GSEffectConstructor> {
    vec![
        thru as GSEffectConstructor,
        constructor!(GSEqualizer),
        constructor!(GSSpectrum),
        constructor!(GSEnhancer),
        constructor!(GSOverdrive),
        constructor!(GSDistortion),
        constructor!(GSTremolo),
        constructor!(GSAutoPan),
        constructor!(GSCompressor),
        constructor!(GSLimiter),
        constructor!(GSStereoChorus),
        constructor!(GSStereoFlanger),
        constructor!(GSStepFlanger),
        constructor!(GSSpaceD),
        constructor!(GSHexaChorus),
        constructor!(GSTremoloChorus),
        constructor!(GSStereoDelay),
        constructor!(GSModulationDelay),
        constructor!(GSTripleTapDelay),
        constructor!(GSQuadTapDelay),
        constructor!(GSTimeControlDelay),
        constructor!(GSTwoVoicePitchShifter),
        constructor!(GSFeedbackPitchShifter),
        constructor!(GSPhaser),
        constructor!(GSAutoWah),
        constructor!(GSHumanizer),
        constructor!(GSRotary),
        constructor!(GSReverb),
        constructor!(GSGateReverb),
        constructor!(GSLoFi1),
        constructor!(GSLoFi2),
        constructor!(GSThreeDAuto),
        constructor!(GSThreeDManual),
        constructor!(GSThreeDChorus),
        constructor!(GSThreeDDelay),
        constructor!(GSChorusDelay),
        constructor!(GSFlangerDelay),
        constructor!(GSChorusFlanger),
        constructor!(GSChorusParallelDelay),
        constructor!(GSFlangerParallelDelay),
        constructor!(GSChorusParallelFlanger),
        constructor!(GSOverdriveParallelOverdrive),
        constructor!(GSOverdriveParallelRotary),
        constructor!(GSOverdriveParallelPhaser),
        constructor!(GSOverdriveParallelAutoWah),
        constructor!(GSPhaserParallelRotary),
        constructor!(GSPhaserParallelAutoWah),
        constructor!(GSOverdriveChorus),
        constructor!(GSOverdriveFlanger),
        constructor!(GSOverdriveDelay),
        constructor!(GSDistortionChorus),
        constructor!(GSDistortionFlanger),
        constructor!(GSDistortionDelay),
        constructor!(GSEnhancerChorus),
        constructor!(GSEnhancerFlanger),
        constructor!(GSEnhancerDelay),
        constructor!(GSRhodesMulti),
        constructor!(GSKeyboardMulti),
        constructor!(GSGuitarMulti1),
        constructor!(GSGuitarMulti2),
        constructor!(GSGuitarMulti3),
        constructor!(GSCleanGuitarMulti1),
        constructor!(GSCleanGuitarMulti2),
        constructor!(GSBassMulti),
        constructor!(GSRotaryMulti),
    ]
}
