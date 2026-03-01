/// connection_block.rs
/// purpose: DLS connection block (single DLS articulator) with SF2 ↔ DLS conversion.
/// Ported from: src/soundbank/downloadable_sounds/connection_block.ts
use std::fmt;

use crate::soundbank::basic_soundbank::basic_zone::BasicZone;
use crate::soundbank::basic_soundbank::generator::Generator;
use crate::soundbank::basic_soundbank::generator_types::{GeneratorType, generator_types as gt};
use crate::soundbank::basic_soundbank::modulator::Modulator;
use crate::soundbank::basic_soundbank::modulator_source::ModulatorSource;
use crate::soundbank::downloadable_sounds::articulation::DownloadableSoundsArticulation;
use crate::soundbank::downloadable_sounds::connection_source::ConnectionSource;
use crate::soundbank::downloadable_sounds::default_dls_modulators::{
    DEFAULT_DLS_CHORUS, DEFAULT_DLS_REVERB,
};
use crate::soundbank::enums::{
    DLSDestination, DLSSource, DLSTransform, dls_destinations, dls_sources, modulator_curve_types,
};
use crate::utils::bit_mask::bit_mask_to_bool;
use crate::utils::indexed_array::IndexedByteArray;
use crate::utils::little_endian::{read_little_endian_indexed, write_dword, write_word};
use crate::utils::loggin::{spessa_synth_info, spessa_synth_warn};

// ---------------------------------------------------------------------------
// Private enums for union return types
// ---------------------------------------------------------------------------

/// Return type for `from_sf_destination`.
/// Equivalent to:
///   DLSDestination
///   | { source: DLSSource; destination: DLSDestination; isBipolar: boolean; amount: number; }
///   | undefined
enum SFDestResult {
    Simple(DLSDestination),
    Complex {
        source: DLSSource,
        destination: DLSDestination,
        is_bipolar: bool,
        amount: i32,
    },
}

/// Return type for `to_sf_destination`.
/// Equivalent to: GeneratorType | undefined | { gen: GeneratorType; newAmount: number }
enum ToSFDestResult {
    Simple(GeneratorType),
    WithAmount {
        generator_type: GeneratorType,
        new_amount: i32,
    },
}

// ---------------------------------------------------------------------------
// Private helper: invalid SF2 generator types (not convertible to DLS)
// Equivalent to: const invalidGeneratorTypes = new Set<GeneratorType>([...])
// ---------------------------------------------------------------------------

fn is_invalid_generator_type(gen_type: GeneratorType) -> bool {
    matches!(
        gen_type,
        gt::SAMPLE_MODES
            | gt::INITIAL_ATTENUATION
            | gt::KEY_RANGE
            | gt::VEL_RANGE
            | gt::SAMPLE_ID
            | gt::FINE_TUNE
            | gt::COARSE_TUNE
            | gt::START_ADDRS_OFFSET
            | gt::START_ADDRS_COARSE_OFFSET
            | gt::END_ADDR_OFFSET
            | gt::END_ADDRS_COARSE_OFFSET
            | gt::STARTLOOP_ADDRS_OFFSET
            | gt::STARTLOOP_ADDRS_COARSE_OFFSET
            | gt::ENDLOOP_ADDRS_OFFSET
            | gt::ENDLOOP_ADDRS_COARSE_OFFSET
            | gt::OVERRIDING_ROOT_KEY
            | gt::EXCLUSIVE_CLASS
    )
}

// ---------------------------------------------------------------------------
// Private helper: compare a parsed Modulator with a DecodedModulator constant
// Equivalent to: Modulator.isIdentical(m, decodedConst, checkAmount)
// ---------------------------------------------------------------------------

fn modulator_identical_to_decoded(
    m: &Modulator,
    d: &crate::soundbank::basic_soundbank::modulator::DecodedModulator,
    check_amount: bool,
) -> bool {
    m.primary_source.to_source_enum() == d.source_enum
        && m.secondary_source.to_source_enum() == d.secondary_source_enum
        && m.destination == d.destination
        && m.transform_type == d.transform_type
        && (!check_amount || m.transform_amount == d.transform_amount as f64)
}

// ---------------------------------------------------------------------------
// Private helper: convert SF2 generator/modulator destination to DLS
// Equivalent to: private static fromSFDestination(dest, amount)
// ---------------------------------------------------------------------------

fn from_sf_destination(dest: GeneratorType, amount: i32) -> Option<SFDestResult> {
    match dest {
        gt::INITIAL_ATTENUATION => Some(SFDestResult::Complex {
            // Amount does not get EMU corrected for modulator attenuation;
            // generator attenuation is handled in wsmp.
            destination: dls_destinations::GAIN,
            amount: -amount,
            is_bipolar: false,
            source: dls_sources::NONE,
        }),
        gt::FINE_TUNE => Some(SFDestResult::Simple(dls_destinations::PITCH)),
        gt::PAN => Some(SFDestResult::Simple(dls_destinations::PAN)),
        gt::KEY_NUM => Some(SFDestResult::Simple(dls_destinations::KEY_NUM)),
        gt::REVERB_EFFECTS_SEND => Some(SFDestResult::Simple(dls_destinations::REVERB_SEND)),
        gt::CHORUS_EFFECTS_SEND => Some(SFDestResult::Simple(dls_destinations::CHORUS_SEND)),
        gt::FREQ_MOD_LFO => Some(SFDestResult::Simple(dls_destinations::MOD_LFO_FREQ)),
        gt::DELAY_MOD_LFO => Some(SFDestResult::Simple(dls_destinations::MOD_LFO_DELAY)),
        gt::DELAY_VIB_LFO => Some(SFDestResult::Simple(dls_destinations::VIB_LFO_DELAY)),
        gt::FREQ_VIB_LFO => Some(SFDestResult::Simple(dls_destinations::VIB_LFO_FREQ)),
        gt::DELAY_VOL_ENV => Some(SFDestResult::Simple(dls_destinations::VOL_ENV_DELAY)),
        gt::ATTACK_VOL_ENV => Some(SFDestResult::Simple(dls_destinations::VOL_ENV_ATTACK)),
        gt::HOLD_VOL_ENV => Some(SFDestResult::Simple(dls_destinations::VOL_ENV_HOLD)),
        gt::DECAY_VOL_ENV => Some(SFDestResult::Simple(dls_destinations::VOL_ENV_DECAY)),
        gt::SUSTAIN_VOL_ENV => Some(SFDestResult::Complex {
            destination: dls_destinations::VOL_ENV_SUSTAIN,
            amount: 1000 - amount,
            is_bipolar: false,
            source: dls_sources::NONE,
        }),
        gt::RELEASE_VOL_ENV => Some(SFDestResult::Simple(dls_destinations::VOL_ENV_RELEASE)),
        gt::DELAY_MOD_ENV => Some(SFDestResult::Simple(dls_destinations::MOD_ENV_DELAY)),
        gt::ATTACK_MOD_ENV => Some(SFDestResult::Simple(dls_destinations::MOD_ENV_ATTACK)),
        gt::HOLD_MOD_ENV => Some(SFDestResult::Simple(dls_destinations::MOD_ENV_HOLD)),
        gt::DECAY_MOD_ENV => Some(SFDestResult::Simple(dls_destinations::MOD_ENV_DECAY)),
        gt::SUSTAIN_MOD_ENV => Some(SFDestResult::Complex {
            destination: dls_destinations::MOD_ENV_SUSTAIN,
            amount: 1000 - amount,
            is_bipolar: false,
            source: dls_sources::NONE,
        }),
        gt::RELEASE_MOD_ENV => Some(SFDestResult::Simple(dls_destinations::MOD_ENV_RELEASE)),
        gt::INITIAL_FILTER_FC => Some(SFDestResult::Simple(dls_destinations::FILTER_CUTOFF)),
        gt::INITIAL_FILTER_Q => Some(SFDestResult::Simple(dls_destinations::FILTER_Q)),
        gt::MOD_ENV_TO_FILTER_FC => Some(SFDestResult::Complex {
            source: dls_sources::MOD_ENV,
            destination: dls_destinations::FILTER_CUTOFF,
            amount,
            is_bipolar: false,
        }),
        gt::MOD_ENV_TO_PITCH => Some(SFDestResult::Complex {
            source: dls_sources::MOD_ENV,
            destination: dls_destinations::PITCH,
            amount,
            is_bipolar: false,
        }),
        gt::MOD_LFO_TO_FILTER_FC => Some(SFDestResult::Complex {
            source: dls_sources::MOD_LFO,
            destination: dls_destinations::FILTER_CUTOFF,
            amount,
            is_bipolar: true,
        }),
        gt::MOD_LFO_TO_VOLUME => Some(SFDestResult::Complex {
            source: dls_sources::MOD_LFO,
            destination: dls_destinations::GAIN,
            amount,
            is_bipolar: true,
        }),
        gt::MOD_LFO_TO_PITCH => Some(SFDestResult::Complex {
            source: dls_sources::MOD_LFO,
            destination: dls_destinations::PITCH,
            amount,
            is_bipolar: true,
        }),
        gt::VIB_LFO_TO_PITCH => Some(SFDestResult::Complex {
            source: dls_sources::VIBRATO_LFO,
            destination: dls_destinations::PITCH,
            amount,
            is_bipolar: true,
        }),
        gt::KEY_NUM_TO_VOL_ENV_HOLD => Some(SFDestResult::Complex {
            source: dls_sources::KEY_NUM,
            destination: dls_destinations::VOL_ENV_HOLD,
            amount,
            is_bipolar: true,
        }),
        gt::KEY_NUM_TO_VOL_ENV_DECAY => Some(SFDestResult::Complex {
            source: dls_sources::KEY_NUM,
            destination: dls_destinations::VOL_ENV_DECAY,
            amount,
            is_bipolar: true,
        }),
        gt::KEY_NUM_TO_MOD_ENV_HOLD => Some(SFDestResult::Complex {
            source: dls_sources::KEY_NUM,
            destination: dls_destinations::MOD_ENV_HOLD,
            amount,
            is_bipolar: true,
        }),
        gt::KEY_NUM_TO_MOD_ENV_DECAY => Some(SFDestResult::Complex {
            source: dls_sources::KEY_NUM,
            destination: dls_destinations::MOD_ENV_DECAY,
            amount,
            is_bipolar: true,
        }),
        gt::SCALE_TUNING => Some(SFDestResult::Complex {
            // Scale tuning implemented via KeyNum → pitch at 12,800 cents.
            // scaleTuning * 128 = DLS amount (regular = 12,800; half = 6400 etc.)
            source: dls_sources::KEY_NUM,
            destination: dls_destinations::PITCH,
            amount: amount * 128,
            // According to DLS table 4, isBipolar should be false.
            is_bipolar: false,
        }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// ConnectionBlock
// ---------------------------------------------------------------------------

/// A single DLS articulator (connection block).
/// Equivalent to: class ConnectionBlock
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionBlock {
    /// Like SF2 modulator primary source.
    /// Equivalent to: source: ConnectionSource
    pub source: ConnectionSource,
    /// Like SF2 modulator secondary source.
    /// Equivalent to: control: ConnectionSource
    pub control: ConnectionSource,
    /// Like SF2 destination generator.
    /// Equivalent to: destination: DLSDestination
    pub destination: DLSDestination,
    /// Output transform type (bits 0-3 of usTransform).
    /// Equivalent to: transform: DLSTransform
    pub transform: DLSTransform,
    /// Like SF2 amount, but 32-bit (scale >> 16 gives the 16-bit short value).
    /// Equivalent to: scale: number (int32 in JS via `| 0`)
    pub scale: i32,
}

impl ConnectionBlock {
    /// Creates a new ConnectionBlock with the given parameters.
    /// Equivalent to: constructor(source, control, destination, transform, scale)
    pub fn new(
        source: ConnectionSource,
        control: ConnectionSource,
        destination: DLSDestination,
        transform: DLSTransform,
        scale: i32,
    ) -> Self {
        Self {
            source,
            control,
            destination,
            transform,
            scale,
        }
    }

    /// True when both source and control are `none` (i.e. this is a static generator).
    /// Equivalent to: get isStaticParameter(): boolean
    #[inline]
    pub fn is_static_parameter(&self) -> bool {
        self.source.source == dls_sources::NONE && self.control.source == dls_sources::NONE
    }

    /// The high 16 bits of `scale`, used as a signed 16-bit SF2-style amount.
    /// Equivalent to: get shortScale(): number { return this.scale >> 16; }
    #[inline]
    pub fn short_scale(&self) -> i32 {
        self.scale >> 16
    }

    /// Reads one connection block from a binary stream.
    /// Equivalent to: static read(artData: IndexedByteArray)
    pub fn read(art_data: &mut IndexedByteArray) -> Self {
        let us_source = read_little_endian_indexed(art_data, 2) as DLSSource;
        let us_control = read_little_endian_indexed(art_data, 2) as DLSSource;
        let us_destination = read_little_endian_indexed(art_data, 2) as DLSDestination;
        let us_transform = read_little_endian_indexed(art_data, 2);
        // `| 0` in JS coerces to int32; in Rust we cast to i32 directly.
        let l_scale = read_little_endian_indexed(art_data, 4) as i32;

        /*
         DLS Specification 2.10 <art2-ck>:
         Bits 0-3:   output transform
         Bits 4-7:   control input transform
         Bits 8-9:   control bipolar / invert flags
         Bits 10-13: source input transform
         Bits 14-15: source bipolar / invert flags
        */
        let transform = (us_transform & 0x0f) as DLSTransform;

        let control_transform = ((us_transform >> 4) & 0x0f) as DLSTransform;
        let control_bipolar = bit_mask_to_bool(us_transform, 8);
        let control_invert = bit_mask_to_bool(us_transform, 9);
        let control = ConnectionSource::new(
            us_control,
            control_transform,
            control_bipolar,
            control_invert,
        );

        let source_transform = ((us_transform >> 10) & 0x0f) as DLSTransform;
        let source_bipolar = bit_mask_to_bool(us_transform, 14);
        let source_invert = bit_mask_to_bool(us_transform, 15);
        let source =
            ConnectionSource::new(us_source, source_transform, source_bipolar, source_invert);

        Self::new(source, control, us_destination, transform, l_scale)
    }

    /// Creates a copy of the given ConnectionBlock.
    /// Equivalent to: static copyFrom(inputBlock)
    pub fn copy_from(input: &ConnectionBlock) -> Self {
        Self {
            source: ConnectionSource::copy_from(&input.source),
            control: ConnectionSource::copy_from(&input.control),
            destination: input.destination,
            transform: input.transform,
            scale: input.scale,
        }
    }

    /// Converts an SF2 modulator to DLS and pushes it into `articulation`.
    /// Equivalent to: static fromSFModulator(m, articulation)
    pub fn from_sf_modulator(m: &Modulator, articulation: &mut DownloadableSoundsArticulation) {
        let failed = |msg: &str| {
            spessa_synth_warn(&format!(
                "Failed converting SF modulator into DLS:\n {} \n({msg})",
                m
            ));
        };

        if m.transform_type != 0 {
            failed("Absolute transform type is not supported");
            return;
        }
        // Skip the default DLS effect modulators
        if modulator_identical_to_decoded(m, &DEFAULT_DLS_CHORUS, true)
            || modulator_identical_to_decoded(m, &DEFAULT_DLS_REVERB, true)
        {
            return;
        }

        let mut source = match ConnectionSource::from_sf_source(&m.primary_source) {
            Some(s) => s,
            None => {
                failed("Invalid primary source");
                return;
            }
        };
        let mut control = match ConnectionSource::from_sf_source(&m.secondary_source) {
            Some(c) => c,
            None => {
                failed("Invalid secondary source");
                return;
            }
        };

        let dls_destination = match from_sf_destination(m.destination, m.transform_amount as i32) {
            Some(d) => d,
            None => {
                failed("Invalid destination");
                return;
            }
        };

        let mut amount = m.transform_amount as i32;
        let destination: DLSDestination;

        match dls_destination {
            SFDestResult::Simple(dest) => {
                destination = dest;
            }
            SFDestResult::Complex {
                source: src,
                destination: dest,
                is_bipolar,
                amount: amt,
            } => {
                destination = dest;
                amount = amt;
                /*
                 Check for a special case, e.g. mod wheel → vibLfoToPitch:
                 In DLS: modLFO source, modwheel control, pitch destination.
                */
                if src != dls_sources::NONE {
                    if control.source != dls_sources::NONE && source.source != dls_sources::NONE {
                        failed("Articulation generators with secondary source are not supported");
                        return;
                    }
                    // Move source to control if needed
                    if source.source != dls_sources::NONE {
                        control = source.clone();
                    }
                    source = ConnectionSource::new(
                        src,
                        modulator_curve_types::LINEAR,
                        is_bipolar,
                        false,
                    );
                }
            }
        }

        let bloc = ConnectionBlock::new(source, control, destination, 0, amount << 16);
        articulation.connection_blocks.push(bloc);
    }

    /// Converts an SF2 generator to DLS and pushes it into `articulation`.
    /// Equivalent to: static fromSFGenerator(generator, articulation)
    pub fn from_sf_generator(
        generator: &Generator,
        articulation: &mut DownloadableSoundsArticulation,
    ) {
        if is_invalid_generator_type(generator.generator_type) {
            return;
        }

        let amount_i32 = generator.generator_value as i32;
        let dls_destination = match from_sf_destination(generator.generator_type, amount_i32) {
            Some(d) => d,
            None => {
                spessa_synth_warn(&format!(
                    "Failed converting SF2 generator into DLS:\n {generator} \n(Invalid type)"
                ));
                return;
            }
        };

        let mut source = ConnectionSource::default();
        let destination: DLSDestination;
        let amount: i32;

        match dls_destination {
            SFDestResult::Simple(dest) => {
                destination = dest;
                amount = amount_i32;
            }
            SFDestResult::Complex {
                source: src,
                destination: dest,
                is_bipolar,
                amount: amt,
            } => {
                destination = dest;
                amount = amt;
                source.source = src;
                source.bipolar = is_bipolar;
            }
        }

        articulation.connection_blocks.push(ConnectionBlock::new(
            source,
            ConnectionSource::default(),
            destination,
            0,
            amount << 16,
        ));
    }

    /// Serialises this connection block to 12 bytes.
    /// Equivalent to: write()
    pub fn write(&self) -> IndexedByteArray {
        let mut out = IndexedByteArray::new(12);
        write_word(&mut out, self.source.source as u32);
        write_word(&mut out, self.control.source as u32);
        write_word(&mut out, self.destination as u32);
        let transform_enum: u32 = (self.transform as u32)
            | ((self.control.to_transform_flag() as u32) << 4)
            | ((self.source.to_transform_flag() as u32) << 10);
        write_word(&mut out, transform_enum);
        write_dword(&mut out, self.scale as u32);
        out
    }

    /// Converts this static-parameter connection block into an SF2 generator.
    /// Caller should first verify `is_static_parameter()`.
    /// Equivalent to: toSFGenerator(zone: BasicZone)
    pub fn to_sf_generator(&self, zone: &mut BasicZone) {
        let value = self.short_scale();
        match self.destination {
            dls_destinations::PAN => {
                zone.set_generator(gt::PAN, Some(value as f64), true);
            }
            dls_destinations::GAIN => {
                // Turn to centibels and apply EMU correction
                let centibels = (-(value as f64) / 0.4).round() as i32;
                zone.add_to_generator(gt::INITIAL_ATTENUATION, centibels, true);
            }
            dls_destinations::FILTER_CUTOFF => {
                zone.set_generator(gt::INITIAL_FILTER_FC, Some(value as f64), true);
            }
            dls_destinations::FILTER_Q => {
                zone.set_generator(gt::INITIAL_FILTER_Q, Some(value as f64), true);
            }
            dls_destinations::MOD_LFO_FREQ => {
                zone.set_generator(gt::FREQ_MOD_LFO, Some(value as f64), true);
            }
            dls_destinations::MOD_LFO_DELAY => {
                zone.set_generator(gt::DELAY_MOD_LFO, Some(value as f64), true);
            }
            dls_destinations::VIB_LFO_FREQ => {
                zone.set_generator(gt::FREQ_VIB_LFO, Some(value as f64), true);
            }
            dls_destinations::VIB_LFO_DELAY => {
                zone.set_generator(gt::DELAY_VIB_LFO, Some(value as f64), true);
            }
            dls_destinations::VOL_ENV_DELAY => {
                zone.set_generator(gt::DELAY_VOL_ENV, Some(value as f64), true);
            }
            dls_destinations::VOL_ENV_ATTACK => {
                zone.set_generator(gt::ATTACK_VOL_ENV, Some(value as f64), true);
            }
            dls_destinations::VOL_ENV_HOLD => {
                zone.set_generator(gt::HOLD_VOL_ENV, Some(value as f64), true);
            }
            dls_destinations::VOL_ENV_DECAY => {
                zone.set_generator(gt::DECAY_VOL_ENV, Some(value as f64), true);
            }
            dls_destinations::VOL_ENV_RELEASE => {
                zone.set_generator(gt::RELEASE_VOL_ENV, Some(value as f64), true);
            }
            dls_destinations::VOL_ENV_SUSTAIN => {
                // Gain seems to be (1000 - value) = sustain cB
                zone.set_generator(gt::SUSTAIN_VOL_ENV, Some((1000 - value) as f64), true);
            }
            dls_destinations::MOD_ENV_DELAY => {
                zone.set_generator(gt::DELAY_MOD_ENV, Some(value as f64), true);
            }
            dls_destinations::MOD_ENV_ATTACK => {
                zone.set_generator(gt::ATTACK_MOD_ENV, Some(value as f64), true);
            }
            dls_destinations::MOD_ENV_HOLD => {
                zone.set_generator(gt::HOLD_MOD_ENV, Some(value as f64), true);
            }
            dls_destinations::MOD_ENV_DECAY => {
                zone.set_generator(gt::DECAY_MOD_ENV, Some(value as f64), true);
            }
            dls_destinations::MOD_ENV_RELEASE => {
                zone.set_generator(gt::RELEASE_MOD_ENV, Some(value as f64), true);
            }
            dls_destinations::MOD_ENV_SUSTAIN => {
                // DLS uses 0.1%, SF uses 0.1%
                zone.set_generator(gt::SUSTAIN_MOD_ENV, Some((1000 - value) as f64), true);
            }
            dls_destinations::REVERB_SEND => {
                zone.set_generator(gt::REVERB_EFFECTS_SEND, Some(value as f64), true);
            }
            dls_destinations::CHORUS_SEND => {
                zone.set_generator(gt::CHORUS_EFFECTS_SEND, Some(value as f64), true);
            }
            dls_destinations::PITCH => {
                let current = zone.fine_tuning();
                zone.set_fine_tuning(current + value);
            }
            _ => {
                spessa_synth_info(&format!(
                    "Failed converting DLS articulator into SF generator: {self}\n(invalid destination)"
                ));
            }
        }
    }

    /// Converts this connection block into an SF2 modulator and adds it to `zone`.
    /// Equivalent to: toSFModulator(zone: BasicZone)
    pub fn to_sf_modulator(&self, zone: &mut BasicZone) {
        let mut amount = self.short_scale();
        let modulator_destination: GeneratorType;
        let mut primary_source: ModulatorSource;
        let mut secondary_source = ModulatorSource::default();

        let special_destination = self.to_combined_sf_destination();
        if let Some(special_dest) = special_destination {
            /*
             Special compound destination (e.g. modLfoToPitch encoded as
             modLFO source + pitch destination).  The SF2 equivalent is a
             single modulator: CC#1 → modLfoToPitch.
            */
            modulator_destination = special_dest;
            primary_source = match self.control.to_sf_source() {
                Some(s) => s,
                None => {
                    self.failed_conversion("Invalid control");
                    return;
                }
            };
        } else {
            let converted_dest = match self.to_sf_destination() {
                Some(d) => d,
                None => {
                    self.failed_conversion("Invalid destination");
                    return;
                }
            };
            match converted_dest {
                ToSFDestResult::WithAmount {
                    generator_type,
                    new_amount,
                } => {
                    amount = new_amount;
                    modulator_destination = generator_type;
                }
                ToSFDestResult::Simple(generator_type) => {
                    modulator_destination = generator_type;
                }
            }

            primary_source = match self.source.to_sf_source() {
                Some(s) => s,
                None => {
                    self.failed_conversion("Invalid source");
                    return;
                }
            };
            secondary_source = match self.control.to_sf_source() {
                Some(s) => s,
                None => {
                    self.failed_conversion("Invalid control");
                    return;
                }
            };
        }

        // Output transform: if the source curve is linear, copy the output
        // transform into the source curve (Fury.dls concave-output test case).
        if self.transform != modulator_curve_types::LINEAR
            && primary_source.curve_type == modulator_curve_types::LINEAR
        {
            primary_source.curve_type = self.transform;
        }

        if modulator_destination == gt::INITIAL_ATTENUATION {
            if self.source.source == dls_sources::VELOCITY
                || self.source.source == dls_sources::VOLUME
                || self.source.source == dls_sources::EXPRESSION
            {
                /*
                 Some DLS banks (Fury.dls, House.rmi) omit the invert flag for
                 attenuation articulators; without inversion the voice would be
                 inaudible.  Most players invert implicitly.
                */
                primary_source.is_negative = true;
            }
            // Corrupted gm.dls guard: clamp to valid centibel range.
            amount = amount.clamp(0, 960);
        }

        let mod_ = Modulator::new(
            primary_source,
            secondary_source,
            modulator_destination,
            amount as f64,
            0,
            false,
            false,
        );
        zone.add_modulators(&[mod_]);
    }

    /// Returns the SF2 compound generator that combines this block's DLS source + destination,
    /// or `None` if no such compound generator exists.
    /// Equivalent to: toCombinedSFDestination(): GeneratorType | undefined
    pub fn to_combined_sf_destination(&self) -> Option<GeneratorType> {
        let src = self.source.source;
        let dest = self.destination;
        if src == dls_sources::VIBRATO_LFO && dest == dls_destinations::PITCH {
            Some(gt::VIB_LFO_TO_PITCH)
        } else if src == dls_sources::MOD_LFO && dest == dls_destinations::PITCH {
            Some(gt::MOD_LFO_TO_PITCH)
        } else if src == dls_sources::MOD_LFO && dest == dls_destinations::FILTER_CUTOFF {
            Some(gt::MOD_LFO_TO_FILTER_FC)
        } else if src == dls_sources::MOD_LFO && dest == dls_destinations::GAIN {
            Some(gt::MOD_LFO_TO_VOLUME)
        } else if src == dls_sources::MOD_ENV && dest == dls_destinations::FILTER_CUTOFF {
            Some(gt::MOD_ENV_TO_FILTER_FC)
        } else if src == dls_sources::MOD_ENV && dest == dls_destinations::PITCH {
            Some(gt::MOD_ENV_TO_PITCH)
        } else {
            None
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn failed_conversion(&self, msg: &str) {
        spessa_synth_info(&format!(
            "Failed converting DLS articulator into SF2:\n {self}\n({msg})"
        ));
    }

    /// Converts this block's DLS destination to an SF2 generator type (possibly with a new amount).
    /// Equivalent to: private toSFDestination()
    fn to_sf_destination(&self) -> Option<ToSFDestResult> {
        let amount = self.short_scale();
        match self.destination {
            dls_destinations::NONE => None,
            dls_destinations::PAN => Some(ToSFDestResult::Simple(gt::PAN)),
            dls_destinations::GAIN => Some(ToSFDestResult::WithAmount {
                // DLS uses gain, SF uses attenuation
                generator_type: gt::INITIAL_ATTENUATION,
                new_amount: -amount,
            }),
            dls_destinations::PITCH => Some(ToSFDestResult::Simple(gt::FINE_TUNE)),
            dls_destinations::KEY_NUM => Some(ToSFDestResult::Simple(gt::OVERRIDING_ROOT_KEY)),
            dls_destinations::VOL_ENV_DELAY => Some(ToSFDestResult::Simple(gt::DELAY_VOL_ENV)),
            dls_destinations::VOL_ENV_ATTACK => Some(ToSFDestResult::Simple(gt::ATTACK_VOL_ENV)),
            dls_destinations::VOL_ENV_HOLD => Some(ToSFDestResult::Simple(gt::HOLD_VOL_ENV)),
            dls_destinations::VOL_ENV_DECAY => Some(ToSFDestResult::Simple(gt::DECAY_VOL_ENV)),
            dls_destinations::VOL_ENV_SUSTAIN => Some(ToSFDestResult::WithAmount {
                generator_type: gt::SUSTAIN_VOL_ENV,
                new_amount: 1000 - amount,
            }),
            dls_destinations::VOL_ENV_RELEASE => Some(ToSFDestResult::Simple(gt::RELEASE_VOL_ENV)),
            dls_destinations::MOD_ENV_DELAY => Some(ToSFDestResult::Simple(gt::DELAY_MOD_ENV)),
            dls_destinations::MOD_ENV_ATTACK => Some(ToSFDestResult::Simple(gt::ATTACK_MOD_ENV)),
            dls_destinations::MOD_ENV_HOLD => Some(ToSFDestResult::Simple(gt::HOLD_MOD_ENV)),
            dls_destinations::MOD_ENV_DECAY => Some(ToSFDestResult::Simple(gt::DECAY_MOD_ENV)),
            dls_destinations::MOD_ENV_SUSTAIN => Some(ToSFDestResult::WithAmount {
                generator_type: gt::SUSTAIN_MOD_ENV,
                new_amount: 1000 - amount,
            }),
            dls_destinations::MOD_ENV_RELEASE => Some(ToSFDestResult::Simple(gt::RELEASE_MOD_ENV)),
            dls_destinations::FILTER_CUTOFF => Some(ToSFDestResult::Simple(gt::INITIAL_FILTER_FC)),
            dls_destinations::FILTER_Q => Some(ToSFDestResult::Simple(gt::INITIAL_FILTER_Q)),
            dls_destinations::CHORUS_SEND => Some(ToSFDestResult::Simple(gt::CHORUS_EFFECTS_SEND)),
            dls_destinations::REVERB_SEND => Some(ToSFDestResult::Simple(gt::REVERB_EFFECTS_SEND)),
            dls_destinations::MOD_LFO_FREQ => Some(ToSFDestResult::Simple(gt::FREQ_MOD_LFO)),
            dls_destinations::MOD_LFO_DELAY => Some(ToSFDestResult::Simple(gt::DELAY_MOD_LFO)),
            dls_destinations::VIB_LFO_FREQ => Some(ToSFDestResult::Simple(gt::FREQ_VIB_LFO)),
            dls_destinations::VIB_LFO_DELAY => Some(ToSFDestResult::Simple(gt::DELAY_VIB_LFO)),
            _ => None,
        }
    }

    // Private computed properties for Display / toString
    // Equivalent to: private get transformName() / private get destinationName()

    fn transform_name(&self) -> String {
        match self.transform {
            modulator_curve_types::LINEAR => "linear".to_string(),
            modulator_curve_types::CONCAVE => "concave".to_string(),
            modulator_curve_types::CONVEX => "convex".to_string(),
            modulator_curve_types::SWITCH => "switch".to_string(),
            v => v.to_string(),
        }
    }

    fn destination_name(&self) -> String {
        match self.destination {
            dls_destinations::NONE => "none".to_string(),
            dls_destinations::GAIN => "gain".to_string(),
            dls_destinations::PITCH => "pitch".to_string(),
            dls_destinations::PAN => "pan".to_string(),
            dls_destinations::KEY_NUM => "keyNum".to_string(),
            dls_destinations::CHORUS_SEND => "chorusSend".to_string(),
            dls_destinations::REVERB_SEND => "reverbSend".to_string(),
            dls_destinations::MOD_LFO_FREQ => "modLfoFreq".to_string(),
            dls_destinations::MOD_LFO_DELAY => "modLfoDelay".to_string(),
            dls_destinations::VIB_LFO_FREQ => "vibLfoFreq".to_string(),
            dls_destinations::VIB_LFO_DELAY => "vibLfoDelay".to_string(),
            dls_destinations::VOL_ENV_ATTACK => "volEnvAttack".to_string(),
            dls_destinations::VOL_ENV_DECAY => "volEnvDecay".to_string(),
            dls_destinations::VOL_ENV_RELEASE => "volEnvRelease".to_string(),
            dls_destinations::VOL_ENV_SUSTAIN => "volEnvSustain".to_string(),
            dls_destinations::VOL_ENV_DELAY => "volEnvDelay".to_string(),
            dls_destinations::VOL_ENV_HOLD => "volEnvHold".to_string(),
            dls_destinations::MOD_ENV_ATTACK => "modEnvAttack".to_string(),
            dls_destinations::MOD_ENV_DECAY => "modEnvDecay".to_string(),
            dls_destinations::MOD_ENV_RELEASE => "modEnvRelease".to_string(),
            dls_destinations::MOD_ENV_SUSTAIN => "modEnvSustain".to_string(),
            dls_destinations::MOD_ENV_DELAY => "modEnvDelay".to_string(),
            dls_destinations::MOD_ENV_HOLD => "modEnvHold".to_string(),
            dls_destinations::FILTER_CUTOFF => "filterCutoff".to_string(),
            dls_destinations::FILTER_Q => "filterQ".to_string(),
            v => v.to_string(),
        }
    }
}

impl fmt::Display for ConnectionBlock {
    /// Equivalent to: toString()
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Source: {},\nControl: {},\nScale: {} >> 16 = {},\nOutput transform: {}\nDestination: {}",
            self.source,
            self.control,
            self.scale,
            self.short_scale(),
            self.transform_name(),
            self.destination_name(),
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
    use crate::soundbank::downloadable_sounds::articulation::{
        DlsMode, DownloadableSoundsArticulation,
    };
    use crate::soundbank::enums::{dls_destinations as dd, dls_sources as ds};
    use crate::utils::indexed_array::IndexedByteArray;

    // Helper: build the 12-byte binary for a connection block
    fn make_block_bytes(
        us_source: u16,
        us_control: u16,
        us_destination: u16,
        us_transform: u16,
        l_scale: i32,
    ) -> IndexedByteArray {
        let mut buf = IndexedByteArray::new(12);
        write_word(&mut buf, us_source as u32);
        write_word(&mut buf, us_control as u32);
        write_word(&mut buf, us_destination as u32);
        write_word(&mut buf, us_transform as u32);
        write_dword(&mut buf, l_scale as u32);
        buf.current_index = 0;
        buf
    }

    // Helper: create a simple static block (source = none, control = none)
    fn static_block(destination: DLSDestination, scale: i32) -> ConnectionBlock {
        ConnectionBlock::new(
            ConnectionSource::default(),
            ConnectionSource::default(),
            destination,
            0,
            scale,
        )
    }

    // ── new / fields ─────────────────────────────────────────────────────────

    #[test]
    fn test_new_stores_all_fields() {
        let src = ConnectionSource::new(ds::VELOCITY, 0, false, false);
        let ctrl = ConnectionSource::default();
        let block = ConnectionBlock::new(src.clone(), ctrl.clone(), dd::PITCH, 1, 1000);
        assert_eq!(block.source, src);
        assert_eq!(block.control, ctrl);
        assert_eq!(block.destination, dd::PITCH);
        assert_eq!(block.transform, 1);
        assert_eq!(block.scale, 1000);
    }

    // ── is_static_parameter ──────────────────────────────────────────────────

    #[test]
    fn test_is_static_parameter_true_when_both_none() {
        let block = static_block(dd::PITCH, 0);
        assert!(block.is_static_parameter());
    }

    #[test]
    fn test_is_static_parameter_false_when_source_not_none() {
        let block = ConnectionBlock::new(
            ConnectionSource::new(ds::VELOCITY, 0, false, false),
            ConnectionSource::default(),
            dd::PITCH,
            0,
            0,
        );
        assert!(!block.is_static_parameter());
    }

    #[test]
    fn test_is_static_parameter_false_when_control_not_none() {
        let block = ConnectionBlock::new(
            ConnectionSource::default(),
            ConnectionSource::new(ds::VELOCITY, 0, false, false),
            dd::PITCH,
            0,
            0,
        );
        assert!(!block.is_static_parameter());
    }

    // ── short_scale ───────────────────────────────────────────────────────────

    #[test]
    fn test_short_scale_positive() {
        let block = static_block(dd::PITCH, 100 << 16);
        assert_eq!(block.short_scale(), 100);
    }

    #[test]
    fn test_short_scale_zero() {
        let block = static_block(dd::PITCH, 0);
        assert_eq!(block.short_scale(), 0);
    }

    #[test]
    fn test_short_scale_negative() {
        // -5 << 16 = -327680
        let block = static_block(dd::PITCH, -5 << 16);
        assert_eq!(block.short_scale(), -5);
    }

    #[test]
    fn test_short_scale_fractional_lower_bits_ignored() {
        // scale = (42 << 16) | 0xFFFF → shortScale still 42
        let block = static_block(dd::PITCH, (42 << 16) | 0xFFFF);
        assert_eq!(block.short_scale(), 42);
    }

    // ── read ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_read_static_pan_block() {
        // source=none(0), control=none(0), dest=pan(4), transform=0, scale=500<<16
        let mut buf = make_block_bytes(0, 0, dd::PAN, 0, 500 << 16);
        let block = ConnectionBlock::read(&mut buf);
        assert_eq!(block.source.source, ds::NONE);
        assert_eq!(block.control.source, ds::NONE);
        assert_eq!(block.destination, dd::PAN);
        assert_eq!(block.transform, 0);
        assert_eq!(block.short_scale(), 500);
    }

    #[test]
    fn test_read_decodes_transform_bits() {
        // us_transform = 0b1100_0010_0010_0011 = 0xC223
        // bits 0-3: transform = 3 (switch)
        // bits 4-7: control transform = 2 (convex)
        // bit 8: control_bipolar = 0
        // bit 9: control_invert = 1
        // bits 10-13: source transform = 0 (linear)
        // bit 14: source_bipolar = 1
        // bit 15: source_invert = 1
        let us_transform: u16 = 0b1100_0010_0010_0011;
        let mut buf = make_block_bytes(
            ds::VELOCITY as u16,
            ds::MODULATION_WHEEL as u16,
            dd::PITCH,
            us_transform,
            0,
        );
        let block = ConnectionBlock::read(&mut buf);
        assert_eq!(block.transform, 3); // switch
        assert_eq!(block.control.transform, 2); // convex
        assert!(!block.control.bipolar);
        assert!(block.control.invert);
        assert_eq!(block.source.transform, 0); // linear
        assert!(block.source.bipolar);
        assert!(block.source.invert);
    }

    #[test]
    fn test_read_advances_cursor_by_12() {
        let mut buf = IndexedByteArray::new(24); // 2 blocks
        // Fill with static_block data (all zeros gives a valid block)
        let block = ConnectionBlock::read(&mut buf);
        assert_eq!(buf.current_index, 12);
        let _ = block;
    }

    #[test]
    fn test_read_negative_scale() {
        let scale: i32 = -1 << 16; // -65536
        let mut buf = make_block_bytes(0, 0, dd::GAIN, 0, scale);
        let block = ConnectionBlock::read(&mut buf);
        assert_eq!(block.scale, scale);
        assert_eq!(block.short_scale(), -1);
    }

    // ── write ────────────────────────────────────────────────────────────────

    #[test]
    fn test_write_produces_12_bytes() {
        let block = static_block(dd::PAN, 500 << 16);
        let out = block.write();
        assert_eq!(out.len(), 12);
    }

    #[test]
    fn test_write_read_roundtrip() {
        let src = ConnectionSource::new(ds::VELOCITY, 1, true, false);
        let ctrl = ConnectionSource::new(ds::MODULATION_WHEEL, 0, false, true);
        let block = ConnectionBlock::new(src, ctrl, dd::PITCH, 0, 200 << 16);
        let mut written = block.write();
        written.current_index = 0;
        let recovered = ConnectionBlock::read(&mut written);
        assert_eq!(recovered.source.source, ds::VELOCITY);
        assert_eq!(recovered.control.source, ds::MODULATION_WHEEL);
        assert_eq!(recovered.destination, dd::PITCH);
        assert_eq!(recovered.short_scale(), 200);
    }

    // ── copy_from ─────────────────────────────────────────────────────────────

    #[test]
    fn test_copy_from_equal() {
        let block = static_block(dd::GAIN, 100 << 16);
        let copy = ConnectionBlock::copy_from(&block);
        assert_eq!(copy, block);
    }

    #[test]
    fn test_copy_from_independent() {
        let block = ConnectionBlock::new(
            ConnectionSource::new(ds::VELOCITY, 0, false, false),
            ConnectionSource::default(),
            dd::PITCH,
            0,
            50 << 16,
        );
        let mut copy = ConnectionBlock::copy_from(&block);
        copy.source.source = ds::NONE;
        assert_eq!(block.source.source, ds::VELOCITY); // original unchanged
    }

    // ── is_invalid_generator_type ─────────────────────────────────────────────

    #[test]
    fn test_invalid_generator_sample_modes() {
        assert!(is_invalid_generator_type(gt::SAMPLE_MODES));
    }

    #[test]
    fn test_invalid_generator_initial_attenuation() {
        assert!(is_invalid_generator_type(gt::INITIAL_ATTENUATION));
    }

    #[test]
    fn test_invalid_generator_key_range() {
        assert!(is_invalid_generator_type(gt::KEY_RANGE));
    }

    #[test]
    fn test_valid_generator_pan() {
        assert!(!is_invalid_generator_type(gt::PAN));
    }

    #[test]
    fn test_valid_generator_filter_fc() {
        assert!(!is_invalid_generator_type(gt::INITIAL_FILTER_FC));
    }

    // ── from_sf_generator ────────────────────────────────────────────────────

    fn make_articulation() -> DownloadableSoundsArticulation {
        DownloadableSoundsArticulation::new()
    }

    fn make_generator(gen_type: GeneratorType, value: i16) -> Generator {
        Generator::new_unvalidated(gen_type, value as f64)
    }

    #[test]
    fn test_from_sf_generator_pan_creates_static_block() {
        let g = make_generator(gt::PAN, 200);
        let mut art = make_articulation();
        ConnectionBlock::from_sf_generator(&g, &mut art);
        assert_eq!(art.connection_blocks.len(), 1);
        let block = &art.connection_blocks[0];
        assert!(block.is_static_parameter());
        assert_eq!(block.destination, dd::PAN);
        assert_eq!(block.short_scale(), 200);
    }

    #[test]
    fn test_from_sf_generator_skips_invalid_type() {
        let g = make_generator(gt::SAMPLE_MODES, 0);
        let mut art = make_articulation();
        ConnectionBlock::from_sf_generator(&g, &mut art);
        assert!(art.connection_blocks.is_empty());
    }

    #[test]
    fn test_from_sf_generator_skips_initial_attenuation() {
        let g = make_generator(gt::INITIAL_ATTENUATION, 100);
        let mut art = make_articulation();
        ConnectionBlock::from_sf_generator(&g, &mut art);
        assert!(art.connection_blocks.is_empty());
    }

    #[test]
    fn test_from_sf_generator_sustain_vol_env_inverts() {
        // sustainVolEnv: DLS amount = 1000 - sfValue
        let g = make_generator(gt::SUSTAIN_VOL_ENV, 200);
        let mut art = make_articulation();
        ConnectionBlock::from_sf_generator(&g, &mut art);
        assert_eq!(art.connection_blocks.len(), 1);
        let block = &art.connection_blocks[0];
        assert_eq!(block.destination, dd::VOL_ENV_SUSTAIN);
        assert_eq!(block.short_scale(), 1000 - 200);
    }

    #[test]
    fn test_from_sf_generator_scale_tuning_uses_key_num_source() {
        let g = make_generator(gt::SCALE_TUNING, 100);
        let mut art = make_articulation();
        ConnectionBlock::from_sf_generator(&g, &mut art);
        assert_eq!(art.connection_blocks.len(), 1);
        let block = &art.connection_blocks[0];
        assert_eq!(block.source.source, ds::KEY_NUM);
        assert_eq!(block.destination, dd::PITCH);
        assert_eq!(block.short_scale(), 100 * 128);
    }

    #[test]
    fn test_from_sf_generator_vib_lfo_to_pitch_uses_vib_source() {
        let g = make_generator(gt::VIB_LFO_TO_PITCH, 50);
        let mut art = make_articulation();
        ConnectionBlock::from_sf_generator(&g, &mut art);
        assert_eq!(art.connection_blocks.len(), 1);
        let block = &art.connection_blocks[0];
        assert_eq!(block.source.source, ds::VIBRATO_LFO);
        assert_eq!(block.destination, dd::PITCH);
        assert_eq!(block.short_scale(), 50);
    }

    // ── to_combined_sf_destination ────────────────────────────────────────────

    #[test]
    fn test_combined_dest_vib_lfo_pitch() {
        let block = ConnectionBlock::new(
            ConnectionSource::new(ds::VIBRATO_LFO, 0, false, false),
            ConnectionSource::default(),
            dd::PITCH,
            0,
            0,
        );
        assert_eq!(
            block.to_combined_sf_destination(),
            Some(gt::VIB_LFO_TO_PITCH)
        );
    }

    #[test]
    fn test_combined_dest_mod_lfo_pitch() {
        let block = ConnectionBlock::new(
            ConnectionSource::new(ds::MOD_LFO, 0, false, false),
            ConnectionSource::default(),
            dd::PITCH,
            0,
            0,
        );
        assert_eq!(
            block.to_combined_sf_destination(),
            Some(gt::MOD_LFO_TO_PITCH)
        );
    }

    #[test]
    fn test_combined_dest_mod_lfo_filter() {
        let block = ConnectionBlock::new(
            ConnectionSource::new(ds::MOD_LFO, 0, false, false),
            ConnectionSource::default(),
            dd::FILTER_CUTOFF,
            0,
            0,
        );
        assert_eq!(
            block.to_combined_sf_destination(),
            Some(gt::MOD_LFO_TO_FILTER_FC)
        );
    }

    #[test]
    fn test_combined_dest_mod_lfo_gain() {
        let block = ConnectionBlock::new(
            ConnectionSource::new(ds::MOD_LFO, 0, false, false),
            ConnectionSource::default(),
            dd::GAIN,
            0,
            0,
        );
        assert_eq!(
            block.to_combined_sf_destination(),
            Some(gt::MOD_LFO_TO_VOLUME)
        );
    }

    #[test]
    fn test_combined_dest_mod_env_filter() {
        let block = ConnectionBlock::new(
            ConnectionSource::new(ds::MOD_ENV, 0, false, false),
            ConnectionSource::default(),
            dd::FILTER_CUTOFF,
            0,
            0,
        );
        assert_eq!(
            block.to_combined_sf_destination(),
            Some(gt::MOD_ENV_TO_FILTER_FC)
        );
    }

    #[test]
    fn test_combined_dest_mod_env_pitch() {
        let block = ConnectionBlock::new(
            ConnectionSource::new(ds::MOD_ENV, 0, false, false),
            ConnectionSource::default(),
            dd::PITCH,
            0,
            0,
        );
        assert_eq!(
            block.to_combined_sf_destination(),
            Some(gt::MOD_ENV_TO_PITCH)
        );
    }

    #[test]
    fn test_combined_dest_none_for_static() {
        let block = static_block(dd::PAN, 0);
        assert_eq!(block.to_combined_sf_destination(), None);
    }

    // ── to_sf_generator ───────────────────────────────────────────────────────

    #[test]
    fn test_to_sf_gen_pan() {
        let mut zone = BasicZone::new();
        let block = static_block(dd::PAN, 100 << 16);
        block.to_sf_generator(&mut zone);
        assert_eq!(zone.get_generator(gt::PAN, -999), 100);
    }

    #[test]
    fn test_to_sf_gen_sustain_vol_env_inverted() {
        let mut zone = BasicZone::new();
        // DLS sustain 200 → SF sustain 1000 - 200 = 800
        let block = static_block(dd::VOL_ENV_SUSTAIN, 200 << 16);
        block.to_sf_generator(&mut zone);
        assert_eq!(zone.get_generator(gt::SUSTAIN_VOL_ENV, -999), 800);
    }

    #[test]
    fn test_to_sf_gen_sustain_mod_env_inverted() {
        let mut zone = BasicZone::new();
        let block = static_block(dd::MOD_ENV_SUSTAIN, 300 << 16);
        block.to_sf_generator(&mut zone);
        assert_eq!(zone.get_generator(gt::SUSTAIN_MOD_ENV, -999), 700);
    }

    #[test]
    fn test_to_sf_gen_pitch_adds_fine_tuning() {
        let mut zone = BasicZone::new();
        // Fine tuning starts at 0; add 50
        let block = static_block(dd::PITCH, 50 << 16);
        block.to_sf_generator(&mut zone);
        assert_eq!(zone.fine_tuning(), 50);
    }

    #[test]
    fn test_to_sf_gen_gain_adds_to_attenuation() {
        let mut zone = BasicZone::new();
        // DLS gain -200 → attenuation +round(200/0.4) = +500
        let block = static_block(dd::GAIN, (-200_i32) << 16);
        block.to_sf_generator(&mut zone);
        let expected = (200_f64 / 0.4).round() as i32;
        assert_eq!(zone.get_generator(gt::INITIAL_ATTENUATION, -999), expected);
    }

    #[test]
    fn test_to_sf_gen_vol_env_attack() {
        let mut zone = BasicZone::new();
        let block = static_block(dd::VOL_ENV_ATTACK, 50 << 16);
        block.to_sf_generator(&mut zone);
        assert_eq!(zone.get_generator(gt::ATTACK_VOL_ENV, -999), 50);
    }

    #[test]
    fn test_to_sf_gen_reverb_send() {
        let mut zone = BasicZone::new();
        let block = static_block(dd::REVERB_SEND, 200 << 16);
        block.to_sf_generator(&mut zone);
        assert_eq!(zone.get_generator(gt::REVERB_EFFECTS_SEND, -999), 200);
    }

    #[test]
    fn test_to_sf_gen_chorus_send() {
        let mut zone = BasicZone::new();
        let block = static_block(dd::CHORUS_SEND, 150 << 16);
        block.to_sf_generator(&mut zone);
        assert_eq!(zone.get_generator(gt::CHORUS_EFFECTS_SEND, -999), 150);
    }

    // ── Display ──────────────────────────────────────────────────────────────

    #[test]
    fn test_display_contains_key_parts() {
        let block = static_block(dd::PAN, 100 << 16);
        let s = block.to_string();
        assert!(s.contains("Source:"), "got: {s}");
        assert!(s.contains("Control:"), "got: {s}");
        assert!(s.contains("Scale:"), "got: {s}");
        assert!(s.contains("Destination:"), "got: {s}");
    }

    #[test]
    fn test_display_destination_name() {
        let block = static_block(dd::PAN, 0);
        assert!(
            block.to_string().contains("pan"),
            "expected 'pan' in display"
        );
    }

    #[test]
    fn test_display_short_scale() {
        let block = static_block(dd::PAN, 42 << 16);
        let s = block.to_string();
        assert!(s.contains("42"), "expected short_scale 42 in display: {s}");
    }
}
