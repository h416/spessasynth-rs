//! The stage and wave identifiers of the EFX types.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/enums.ts

/// The LFO waves, in the order of the parameter.
/// Equivalent to: TremoloWaves
pub mod tremolo_waves {
    pub const TRIANGLE: u8 = 0;
    pub const SQUARE: u8 = 1;
    pub const SINE: u8 = 2;
    pub const SAW_UP: u8 = 3;
    pub const SAW_DOWN: u8 = 4;
}

/// The second stages of the combinations.
/// Equivalent to: CombinationStages
pub mod combination_stages {
    pub const CHORUS: u8 = 0;
    pub const FLANGER: u8 = 1;
    pub const DELAY: u8 = 2;
}

/// The effects of the parallel combinations.
/// Equivalent to: ParallelStages
pub mod parallel_stages {
    pub const DRIVE: u8 = 0;
    pub const PHASER: u8 = 1;
    pub const WAH: u8 = 2;
    pub const ROTARY: u8 = 3;
}
