//! The controller matrix sources and destinations.
//! Ported from: src/synthesizer/audio_engine/channel/enums.ts

/// The matrix sources, in the order of the GS and XG parameter blocks.
/// CC1 and CC2 are the assignable controllers (GS CC1 / CC2, XG AC1 / AC2).
/// Equivalent to: MatrixSources
pub mod matrix_sources {
    pub const MODULATION_WHEEL: usize = 0;
    pub const PITCH_WHEEL: usize = 1;
    pub const CHANNEL_PRESSURE: usize = 2;
    pub const POLY_PRESSURE: usize = 3;
    pub const CC1: usize = 4;
    pub const CC2: usize = 5;
}
/// Equivalent to: MatrixSource
pub type MatrixSource = usize;

/// The matrix destinations, in the GS order. XG uses the first three and the LFO1 depths.
/// Equivalent to: MatrixDestinations
pub mod matrix_destinations {
    pub const PITCH: usize = 0;
    pub const CUTOFF: usize = 1;
    pub const AMPLITUDE: usize = 2;
    pub const LFO1_RATE: usize = 3;
    pub const LFO1_PITCH: usize = 4;
    pub const LFO1_FILTER: usize = 5;
    pub const LFO1_AMPLITUDE: usize = 6;
    pub const LFO2_RATE: usize = 7;
    pub const LFO2_PITCH: usize = 8;
    pub const LFO2_FILTER: usize = 9;
    pub const LFO2_AMPLITUDE: usize = 10;
}
/// Equivalent to: MatrixDestination
pub type MatrixDestination = usize;
