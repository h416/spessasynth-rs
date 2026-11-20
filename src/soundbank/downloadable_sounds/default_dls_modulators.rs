//! default_dls_modulators.rs
//! purpose: Default DLS modulator constants used when loading DLS soundbanks.
//! Ported from: src/soundbank/downloadable_sounds/default_dls_modulators.ts
use std::sync::LazyLock;

use crate::soundbank::basic_soundbank::generator_types::generator_types;
use crate::soundbank::basic_soundbank::modulator::{DecodedModulator, Modulator};

/// DLS reverb effect modulator.
/// sourceEnum = 0x00db, destination = reverbEffectsSend, amount = 1000.
/// Equivalent to: DEFAULT_DLS_REVERB
pub static DEFAULT_DLS_REVERB: LazyLock<Modulator> = LazyLock::new(|| {
    DecodedModulator::new(0x00db, 0x0, generator_types::REVERB_EFFECTS_SEND, 1000.0, 0)
});

/// DLS chorus effect modulator.
/// sourceEnum = 0x00dd, destination = chorusEffectsSend, amount = 1000.
/// Equivalent to: DEFAULT_DLS_CHORUS
pub static DEFAULT_DLS_CHORUS: LazyLock<Modulator> = LazyLock::new(|| {
    DecodedModulator::new(0x00dd, 0x0, generator_types::CHORUS_EFFECTS_SEND, 1000.0, 0)
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;

    #[test]
    fn test_default_dls_modulators() {
        assert_eq!(DEFAULT_DLS_REVERB.primary_source.to_source_enum(), 0x00db);
        assert_eq!(DEFAULT_DLS_REVERB.secondary_source.to_source_enum(), 0);
        assert_eq!(DEFAULT_DLS_REVERB.destination, gt::REVERB_EFFECTS_SEND);
        assert_eq!(DEFAULT_DLS_REVERB.transform_amount, 1000.0);
        assert_eq!(DEFAULT_DLS_CHORUS.primary_source.to_source_enum(), 0x00dd);
        assert_eq!(DEFAULT_DLS_CHORUS.destination, gt::CHORUS_EFFECTS_SEND);
        assert_eq!(DEFAULT_DLS_CHORUS.transform_amount, 1000.0);
    }
}
