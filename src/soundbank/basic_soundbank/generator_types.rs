/// generator_types.rs
/// purpose: SoundFont2 Generator type constants and limit/default tables.
/// Ported from: src/soundbank/basic_soundbank/generator_types.ts
///
/// Note: TS 4.3.0 renamed `generatorTypes` → `GeneratorTypes` and `generatorLimits` →
/// `GeneratorLimits` (naming-only). Rust keeps the idiomatic `generator_types` module and
/// `GENERATOR_LIMITS` constant names.
///
/// TS 4.3.0 also moved `defaultGeneratorValues` into `basic_preset.ts` (module-private).
/// Rust keeps `DEFAULT_GENERATOR_VALUES` here because it is also used by several
/// engine-side tests; the values are identical.
/// All SoundFont2 Generator enumerations.
/// Equivalent to: GeneratorTypes
#[allow(clippy::module_inception)]
pub mod generator_types {
    // Equivalent to: invalid (TS 4.3.0 renamed INVALID → invalid; Rust keeps INVALID)
    pub const INVALID: i16 = -1;
    pub const START_ADDRS_OFFSET: i16 = 0;
    pub const END_ADDR_OFFSET: i16 = 1;
    pub const STARTLOOP_ADDRS_OFFSET: i16 = 2;
    pub const ENDLOOP_ADDRS_OFFSET: i16 = 3;
    pub const START_ADDRS_COARSE_OFFSET: i16 = 4;
    pub const MOD_LFO_TO_PITCH: i16 = 5;
    pub const VIB_LFO_TO_PITCH: i16 = 6;
    pub const MOD_ENV_TO_PITCH: i16 = 7;
    pub const INITIAL_FILTER_FC: i16 = 8;
    pub const INITIAL_FILTER_Q: i16 = 9;
    pub const MOD_LFO_TO_FILTER_FC: i16 = 10;
    pub const MOD_ENV_TO_FILTER_FC: i16 = 11;
    pub const END_ADDRS_COARSE_OFFSET: i16 = 12;
    pub const MOD_LFO_TO_VOLUME: i16 = 13;
    // Unused1 (14) removed in TS 4.3.0
    pub const CHORUS_EFFECTS_SEND: i16 = 15;
    pub const REVERB_EFFECTS_SEND: i16 = 16;
    pub const PAN: i16 = 17;
    // Unused2 (18), Unused3 (19), Unused4 (20) removed in TS 4.3.0
    pub const DELAY_MOD_LFO: i16 = 21;
    pub const FREQ_MOD_LFO: i16 = 22;
    pub const DELAY_VIB_LFO: i16 = 23;
    pub const FREQ_VIB_LFO: i16 = 24;
    pub const DELAY_MOD_ENV: i16 = 25;
    pub const ATTACK_MOD_ENV: i16 = 26;
    pub const HOLD_MOD_ENV: i16 = 27;
    pub const DECAY_MOD_ENV: i16 = 28;
    pub const SUSTAIN_MOD_ENV: i16 = 29;
    pub const RELEASE_MOD_ENV: i16 = 30;
    pub const KEY_NUM_TO_MOD_ENV_HOLD: i16 = 31;
    pub const KEY_NUM_TO_MOD_ENV_DECAY: i16 = 32;
    pub const DELAY_VOL_ENV: i16 = 33;
    pub const ATTACK_VOL_ENV: i16 = 34;
    pub const HOLD_VOL_ENV: i16 = 35;
    pub const DECAY_VOL_ENV: i16 = 36;
    pub const SUSTAIN_VOL_ENV: i16 = 37;
    pub const RELEASE_VOL_ENV: i16 = 38;
    pub const KEY_NUM_TO_VOL_ENV_HOLD: i16 = 39;
    pub const KEY_NUM_TO_VOL_ENV_DECAY: i16 = 40;
    pub const INSTRUMENT: i16 = 41;
    // Reserved1 (42) removed in TS 4.3.0
    pub const KEY_RANGE: i16 = 43;
    pub const VEL_RANGE: i16 = 44;
    pub const STARTLOOP_ADDRS_COARSE_OFFSET: i16 = 45;
    pub const KEY_NUM: i16 = 46;
    pub const VELOCITY: i16 = 47;
    pub const INITIAL_ATTENUATION: i16 = 48;
    // Reserved2 (49) removed in TS 4.3.0
    pub const ENDLOOP_ADDRS_COARSE_OFFSET: i16 = 50;
    pub const COARSE_TUNE: i16 = 51;
    pub const FINE_TUNE: i16 = 52;
    pub const SAMPLE_ID: i16 = 53;
    pub const SAMPLE_MODES: i16 = 54;
    // Reserved3 (55) removed in TS 4.3.0
    pub const SCALE_TUNING: i16 = 56;
    pub const EXCLUSIVE_CLASS: i16 = 57;
    pub const OVERRIDING_ROOT_KEY: i16 = 58;
    // Unused5 (59) removed in TS 4.3.0
    pub const END_OPER: i16 = 60;

    // Additional generators that are used in system exclusives and will not be saved
    // (controller matrix)

    // [-1000;1000] -> 1/10%
    pub const AMPLITUDE: i16 = 61;
    // [-1000;1000] -> Hz/100
    pub const VIB_LFO_RATE: i16 = 62;
    // [0;1000] -> 1/10%
    pub const VIB_LFO_AMPLITUDE_DEPTH: i16 = 63;
    // Like modLfoToFilterFc
    pub const VIB_LFO_TO_FILTER_FC: i16 = 64;
    // [-1000;1000] -> Hz/100
    pub const MOD_LFO_RATE: i16 = 65;
    // [0;1000] -> 1/10%
    pub const MOD_LFO_AMPLITUDE_DEPTH: i16 = 66;
}

/// Equivalent to: GeneratorType
pub type GeneratorType = i16;

/// Maximum valid generator index.
/// Equivalent to: MAX_GENERATOR = Math.max(...Object.values(GeneratorTypes)) = 66
pub const MAX_GENERATOR: i16 = 66;

/// Total number of generator slots.
/// Equivalent to: GENERATORS_AMOUNT = MAX_GENERATOR + 1 = 67
pub const GENERATORS_AMOUNT: usize = MAX_GENERATOR as usize + 1;

/// Min/max/default/nrpn-scale for a single generator.
/// Equivalent to: interface GeneratorLimit { min, max, def, nrpn }
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeneratorLimit {
    pub min: i32,
    pub max: i32,
    pub def: i32,
    pub nrpn: u8,
}

impl GeneratorLimit {
    const fn new(min: i32, max: i32, def: i32, nrpn: u8) -> Self {
        Self {
            min,
            max,
            def,
            nrpn,
        }
    }
}

/// Shorthand for Some(GeneratorLimit::new(...)) used in the static table below.
const fn lim(min: i32, max: i32, def: i32, nrpn: u8) -> Option<GeneratorLimit> {
    Some(GeneratorLimit::new(min, max, def, nrpn))
}

/// Limit for the `invalid` generator type (index -1, cannot live in the array below).
/// Equivalent to: `[GeneratorTypes.invalid]: { min: 0, max: 0, def: 0, nrpn: 0 }`
pub const INVALID_GENERATOR_LIMIT: GeneratorLimit = GeneratorLimit::new(0, 0, 0, 0);

/// Generator limits indexed by generator index 0-66 (67 entries).
/// None means no limit is defined for that generator
/// (indices removed from the TS 4.3.0 `GeneratorLimits` record: 14, 18-20, 42, 49, 55, 59).
/// The `invalid` (-1) entry of the TS record is [`INVALID_GENERATOR_LIMIT`].
/// Equivalent to: GeneratorLimits
pub const GENERATOR_LIMITS: [Option<GeneratorLimit>; GENERATORS_AMOUNT] = [
    // 0  startAddrsOffset
    lim(0, 32_768, 0, 1),
    // 1  endAddrOffset
    lim(-32_768, 32_768, 0, 1),
    // 2  startloopAddrsOffset
    lim(-32_768, 32_768, 0, 1),
    // 3  endloopAddrsOffset
    lim(-32_768, 32_768, 0, 1),
    // 4  startAddrsCoarseOffset
    lim(0, 32_768, 0, 1),
    // 5  modLfoToPitch
    lim(-12_000, 12_000, 0, 2),
    // 6  vibLfoToPitch
    lim(-12_000, 12_000, 0, 2),
    // 7  modEnvToPitch
    lim(-12_000, 12_000, 0, 2),
    // 8  initialFilterFc
    lim(1_500, 13_500, 13_500, 2),
    // 9  initialFilterQ
    lim(0, 960, 0, 1),
    // 10 modLfoToFilterFc
    lim(-12_000, 12_000, 0, 2),
    // 11 modEnvToFilterFc
    lim(-12_000, 12_000, 0, 2),
    // 12 endAddrsCoarseOffset
    lim(-32_768, 32_768, 0, 1),
    // 13 modLfoToVolume
    lim(-960, 960, 0, 1),
    // 14 (unused1, removed)
    None,
    // 15 chorusEffectsSend
    lim(0, 1_000, 0, 1),
    // 16 reverbEffectsSend
    lim(0, 1_000, 0, 1),
    // 17 pan
    lim(-500, 500, 0, 1),
    // 18 (unused2, removed)
    None,
    // 19 (unused3, removed)
    None,
    // 20 (unused4, removed)
    None,
    // 21 delayModLFO
    lim(-12_000, 5_000, -12_000, 2),
    // 22 freqModLFO
    lim(-16_000, 4_500, 0, 4),
    // 23 delayVibLFO
    lim(-12_000, 5_000, -12_000, 2),
    // 24 freqVibLFO
    lim(-16_000, 4_500, 0, 4),
    // 25 delayModEnv (-32768 = instant, this is done to prevent click for lowpass)
    lim(-32_768, 5_000, -32_768, 2),
    // 26 attackModEnv
    lim(-32_768, 8_000, -32_768, 2),
    // 27 holdModEnv
    lim(-12_000, 5_000, -12_000, 2),
    // 28 decayModEnv
    lim(-12_000, 8_000, -12_000, 2),
    // 29 sustainModEnv
    lim(0, 1_000, 0, 1),
    // 30 releaseModEnv
    lim(-12_000, 8_000, -12_000, 2),
    // 31 keyNumToModEnvHold
    lim(-1_200, 1_200, 0, 1),
    // 32 keyNumToModEnvDecay
    lim(-1_200, 1_200, 0, 1),
    // 33 delayVolEnv
    lim(-12_000, 5_000, -12_000, 2),
    // 34 attackVolEnv
    lim(-12_000, 8_000, -12_000, 2),
    // 35 holdVolEnv
    lim(-12_000, 5_000, -12_000, 2),
    // 36 decayVolEnv
    lim(-12_000, 8_000, -12_000, 2),
    // 37 sustainVolEnv
    lim(0, 1_440, 0, 1),
    // 38 releaseVolEnv
    lim(-12_000, 8_000, -12_000, 2),
    // 39 keyNumToVolEnvHold
    lim(-1_200, 1_200, 0, 1),
    // 40 keyNumToVolEnvDecay
    lim(-1_200, 1_200, 0, 1),
    // 41 instrument (non-value generator)
    lim(0, 0, 0, 0),
    // 42 (reserved1, removed)
    None,
    // 43 keyRange (non-value generator)
    lim(0, 0, 0, 0),
    // 44 velRange (non-value generator)
    lim(0, 0, 0, 0),
    // 45 startloopAddrsCoarseOffset
    lim(-32_768, 32_768, 0, 1),
    // 46 keyNum
    lim(-1, 127, -1, 1),
    // 47 velocity
    lim(-1, 127, -1, 1),
    // 48 initialAttenuation
    lim(0, 1_440, 0, 1),
    // 49 (reserved2, removed)
    None,
    // 50 endloopAddrsCoarseOffset
    lim(-32_768, 32_768, 0, 1),
    // 51 coarseTune
    lim(-120, 120, 0, 1),
    // 52 fineTune
    lim(-12_700, 12_700, 0, 1),
    // 53 sampleID (non-value generator)
    lim(0, 0, 0, 0),
    // 54 sampleModes
    lim(0, 3, 0, 0),
    // 55 (reserved3, removed)
    None,
    // 56 scaleTuning
    lim(0, 1_200, 100, 1),
    // 57 exclusiveClass
    lim(0, 99_999, 0, 0),
    // 58 overridingRootKey
    lim(-1, 127, -1, 0),
    // 59 (unused5, removed)
    None,
    // 60 endOper (non-value generator)
    lim(0, 0, 0, 0),
    // 61 amplitude (NON-STANDARD)
    lim(-1_000, 1_000, 0, 1),
    // 62 vibLfoRate (NON-STANDARD)
    lim(-1_000, 1_000, 0, 1),
    // 63 vibLfoAmplitudeDepth (NON-STANDARD)
    lim(0, 1_000, 0, 1),
    // 64 vibLfoToFilterFc (NON-STANDARD)
    lim(-12_000, 12_000, 0, 2),
    // 65 modLfoRate (NON-STANDARD)
    lim(-1_000, 1_000, 0, 1),
    // 66 modLfoAmplitudeDepth (NON-STANDARD)
    lim(0, 1_000, 0, 1),
];

const fn compute_default_values() -> [i16; GENERATORS_AMOUNT] {
    let mut arr = [0i16; GENERATORS_AMOUNT];
    let mut i: usize = 0;
    while i < GENERATORS_AMOUNT {
        if let Some(lim) = GENERATOR_LIMITS[i] {
            arr[i] = lim.def as i16;
        }
        i += 1;
    }
    arr
}

/// Default generator values as a 67-element i16 array.
/// Equivalent to: defaultGeneratorValues (Int16Array of size GENERATORS_AMOUNT;
/// moved into basic_preset.ts as a module-private constant in TS 4.3.0)
pub const DEFAULT_GENERATOR_VALUES: [i16; GENERATORS_AMOUNT] = compute_default_values();

#[cfg(test)]
mod tests {
    use super::generator_types as gt;
    use super::*;

    // --- generator_types constants ---

    #[test]
    fn test_invalid() {
        assert_eq!(gt::INVALID, -1);
    }

    #[test]
    fn test_start_addrs_offset() {
        assert_eq!(gt::START_ADDRS_OFFSET, 0);
    }

    #[test]
    fn test_initial_filter_fc() {
        assert_eq!(gt::INITIAL_FILTER_FC, 8);
    }

    #[test]
    fn test_delay_mod_lfo() {
        assert_eq!(gt::DELAY_MOD_LFO, 21);
    }

    #[test]
    fn test_scale_tuning() {
        assert_eq!(gt::SCALE_TUNING, 56);
    }

    #[test]
    fn test_amplitude() {
        assert_eq!(gt::AMPLITUDE, 61);
    }

    #[test]
    fn test_vib_lfo_rate() {
        assert_eq!(gt::VIB_LFO_RATE, 62);
    }

    #[test]
    fn test_vib_lfo_amplitude_depth() {
        assert_eq!(gt::VIB_LFO_AMPLITUDE_DEPTH, 63);
    }

    #[test]
    fn test_vib_lfo_to_filter_fc() {
        assert_eq!(gt::VIB_LFO_TO_FILTER_FC, 64);
    }

    #[test]
    fn test_mod_lfo_rate() {
        assert_eq!(gt::MOD_LFO_RATE, 65);
    }

    #[test]
    fn test_mod_lfo_amplitude_depth() {
        assert_eq!(gt::MOD_LFO_AMPLITUDE_DEPTH, 66);
    }

    // --- module-level constants ---

    #[test]
    fn test_generators_amount() {
        assert_eq!(GENERATORS_AMOUNT, 67);
    }

    #[test]
    fn test_max_generator() {
        assert_eq!(MAX_GENERATOR, 66);
    }

    // --- GENERATOR_LIMITS ---

    #[test]
    fn test_limits_start_addrs_offset() {
        let lim = GENERATOR_LIMITS[0].unwrap();
        assert_eq!(lim.min, 0);
        assert_eq!(lim.max, 32_768);
        assert_eq!(lim.def, 0);
        assert_eq!(lim.nrpn, 1);
    }

    #[test]
    fn test_limits_initial_filter_fc() {
        let lim = GENERATOR_LIMITS[8].unwrap();
        assert_eq!(lim.min, 1_500);
        assert_eq!(lim.max, 13_500);
        assert_eq!(lim.def, 13_500);
        assert_eq!(lim.nrpn, 2);
    }

    #[test]
    fn test_limits_unused1_is_none() {
        assert!(GENERATOR_LIMITS[14].is_none());
    }

    #[test]
    fn test_limits_delay_mod_env() {
        let lim = GENERATOR_LIMITS[25].unwrap();
        assert_eq!(lim.def, -32_768);
        assert_eq!(lim.nrpn, 2);
    }

    #[test]
    fn test_limits_key_num() {
        let lim = GENERATOR_LIMITS[46].unwrap();
        assert_eq!(lim.min, -1);
        assert_eq!(lim.max, 127);
        assert_eq!(lim.def, -1);
    }

    #[test]
    fn test_limits_scale_tuning() {
        let lim = GENERATOR_LIMITS[56].unwrap();
        assert_eq!(lim.def, 100);
    }

    #[test]
    fn test_limits_exclusive_class() {
        let lim = GENERATOR_LIMITS[57].unwrap();
        assert_eq!(lim.max, 99_999);
        assert_eq!(lim.nrpn, 0);
    }

    #[test]
    fn test_limits_instrument_is_zero_limit() {
        // TS 4.3.0 defines non-value generators with { min: 0, max: 0, def: 0, nrpn: 0 }
        let lim = GENERATOR_LIMITS[41].unwrap();
        assert_eq!(lim, GeneratorLimit::new(0, 0, 0, 0));
    }

    #[test]
    fn test_limits_key_range_is_zero_limit() {
        let lim = GENERATOR_LIMITS[43].unwrap();
        assert_eq!(lim, GeneratorLimit::new(0, 0, 0, 0));
    }

    #[test]
    fn test_limits_sample_id_is_zero_limit() {
        let lim = GENERATOR_LIMITS[53].unwrap();
        assert_eq!(lim, GeneratorLimit::new(0, 0, 0, 0));
    }

    #[test]
    fn test_limits_end_oper_is_zero_limit() {
        let lim = GENERATOR_LIMITS[60].unwrap();
        assert_eq!(lim, GeneratorLimit::new(0, 0, 0, 0));
    }

    #[test]
    fn test_limits_reserved1_is_none() {
        assert!(GENERATOR_LIMITS[42].is_none());
    }

    #[test]
    fn test_limits_amplitude() {
        let lim = GENERATOR_LIMITS[gt::AMPLITUDE as usize].unwrap();
        assert_eq!(lim.min, -1_000);
        assert_eq!(lim.max, 1_000);
        assert_eq!(lim.def, 0);
        assert_eq!(lim.nrpn, 1);
    }

    #[test]
    fn test_limits_vib_lfo_to_filter_fc() {
        let lim = GENERATOR_LIMITS[gt::VIB_LFO_TO_FILTER_FC as usize].unwrap();
        assert_eq!(lim.min, -12_000);
        assert_eq!(lim.max, 12_000);
        assert_eq!(lim.nrpn, 2);
    }

    #[test]
    fn test_limits_mod_lfo_amplitude_depth() {
        let lim = GENERATOR_LIMITS[gt::MOD_LFO_AMPLITUDE_DEPTH as usize].unwrap();
        assert_eq!(lim.min, 0);
        assert_eq!(lim.max, 1_000);
    }

    #[test]
    fn test_limits_length() {
        assert_eq!(GENERATOR_LIMITS.len(), 67);
    }

    #[test]
    fn test_invalid_generator_limit() {
        assert_eq!(INVALID_GENERATOR_LIMIT, GeneratorLimit::new(0, 0, 0, 0));
    }

    // --- DEFAULT_GENERATOR_VALUES ---

    #[test]
    fn test_default_zero_for_start_addrs_offset() {
        assert_eq!(DEFAULT_GENERATOR_VALUES[0], 0);
    }

    #[test]
    fn test_default_initial_filter_fc() {
        assert_eq!(DEFAULT_GENERATOR_VALUES[8], 13_500);
    }

    #[test]
    fn test_default_delay_mod_lfo() {
        assert_eq!(DEFAULT_GENERATOR_VALUES[21], -12_000);
    }

    #[test]
    fn test_default_delay_mod_env() {
        assert_eq!(DEFAULT_GENERATOR_VALUES[25], -32_768);
    }

    #[test]
    fn test_default_key_num() {
        assert_eq!(DEFAULT_GENERATOR_VALUES[46], -1);
    }

    #[test]
    fn test_default_scale_tuning() {
        assert_eq!(DEFAULT_GENERATOR_VALUES[56], 100);
    }

    #[test]
    fn test_default_overriding_root_key() {
        assert_eq!(DEFAULT_GENERATOR_VALUES[58], -1);
    }

    #[test]
    fn test_default_unused1_is_zero() {
        assert_eq!(DEFAULT_GENERATOR_VALUES[14], 0);
    }

    #[test]
    fn test_default_amplitude_is_zero() {
        assert_eq!(DEFAULT_GENERATOR_VALUES[gt::AMPLITUDE as usize], 0);
    }

    #[test]
    fn test_default_values_length() {
        assert_eq!(DEFAULT_GENERATOR_VALUES.len(), 67);
    }
}
