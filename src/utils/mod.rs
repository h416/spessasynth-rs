/// utils/mod.rs
/// purpose: Public API of the utils module.
/// Ported from: src/utils/exports.ts
///
/// Per CLAUDE.md, TypeScript's `exports.ts` maps to `mod.rs` in Rust.
/// Types/constants are defined here; re-exports expose sub-module items.
///
/// Note: `SpessaSynthCoreUtils` (a JS-only utility aggregate object) is not ported.
pub mod big_endian;
pub mod bit_mask;
pub mod indexed_array;
pub mod little_endian;
pub mod load_date;
pub mod loggin;
pub mod midi_hacks;
pub mod other;
pub mod riff_chunk;
pub mod string;
pub mod sysex_detector;
pub mod variable_length_quantity;
pub mod write_wav;

// --- Re-exports (equivalent to the `export { ... } from "..."` lines in exports.ts) ---

pub use indexed_array::IndexedByteArray;
pub use riff_chunk::FourCC;
pub use write_wav::audio_to_wav;

// --- Types and constants from exports.ts ---

/// WAV metadata fields embedded into the INFO LIST chunk.
/// Equivalent to: `interface WaveMetadata` in exports.ts
#[derive(Debug, Clone, Default)]
pub struct WaveMetadata {
    /// Song title (INAM chunk).
    pub title: Option<String>,
    /// Artist name (IART chunk).
    pub artist: Option<String>,
    /// Album name (IPRD chunk).
    pub album: Option<String>,
    /// Genre (IGNR chunk).
    pub genre: Option<String>,
}

/// Loop start/end points in seconds.
/// Equivalent to the inline `loop?: { start: number; end: number }` type in WaveWriteOptions.
#[derive(Debug, Clone)]
pub struct WaveLoopPoints {
    /// Loop start in seconds.
    pub start: f64,
    /// Loop end in seconds.
    pub end: f64,
}

/// Options for WAV file writing.
/// Equivalent to: `interface WaveWriteOptions` in exports.ts
#[derive(Debug, Clone)]
pub struct WaveWriteOptions {
    /// Normalize audio to prevent clipping. Recommended.
    /// Equivalent to: `normalizeAudio: boolean`
    pub normalize_audio: bool,
    /// Loop start/end points in seconds. `None` means no CUE chunk is written.
    /// Equivalent to: `loop?: { start: number; end: number }`
    pub loop_points: Option<WaveLoopPoints>,
    /// Metadata written into the INFO LIST chunk.
    /// Equivalent to: `metadata: Partial<WaveMetadata>`
    pub metadata: WaveMetadata,
}

impl Default for WaveWriteOptions {
    fn default() -> Self {
        Self {
            normalize_audio: true,
            loop_points: None,
            metadata: WaveMetadata::default(),
        }
    }
}

/// Default WAV write options.
/// Equivalent to: `DEFAULT_WAV_WRITE_OPTIONS` in exports.ts
pub const DEFAULT_WAV_WRITE_OPTIONS: WaveWriteOptions = WaveWriteOptions {
    normalize_audio: true,
    loop_points: None,
    metadata: WaveMetadata {
        title: None,
        artist: None,
        album: None,
        genre: None,
    },
};
