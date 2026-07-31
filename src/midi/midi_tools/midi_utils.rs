/// midi_utils.rs
/// purpose: General-purpose MIDI message analysis (SysEx GM/GS/XG detection) and GS SysEx
/// message construction.
/// Ported from: src/midi/midi_tools/midi_utils.ts (spessasynth_core 4.3.0)
///
/// TS 4.3.0 introduced this file, which absorbed and replaced two 4.2.0 files:
/// - `src/midi/midi_tools/get_gs_on.ts` (the `getGsOn` free function, now `MidiUtils::gs_reset`;
///   the temporary Rust `get_gs_on` compatibility wrapper was removed in Task 18)
/// - `src/utils/sysex_detector.ts` (the `isXGOn`/`isGSOn`/`isGMOn`/`isGM2On`/`isGSDrumsOn`/
///   `syxToChannel` free functions, now unified into `MidiUtils::analyze_sysex` which returns a
///   single `AnalyzedMIDIMessage` classification instead of several independent booleans, plus
///   many previously-unrecognized GS/XG/GM SysEx messages: reverb/chorus/delay/variation/
///   insertion effect parameters, master/per-channel key-shift and fine-tune, and — notably —
///   GS/XG SysEx-encoded bank-select/program-change/mono-poly messages, which are now converted
///   to regular Controller Change / Program Change classifications (see `write/rmidi.rs`, which
///   uses this to replace such SysEx events with regular CC/PC events during RMIDI bank
///   correction; this is the "wider MIDI/SysEx support" mentioned in the 4.3.0 release notes).
///
/// Task 17 ported only the subset of `MIDIUtils` consumed by `write/rmidi.rs`: `analyze_sysex`
/// (plus its private `analyze_gm`/`analyze_gs`/`analyze_xg` helpers), `syx_to_channel`,
/// `channel_to_syx`, `gs_data`, `gs_message`, `gs_drum_change`, and `gs_reset`.
///
/// Task 18 adds `analyze_rpn`/`analyze_nrpn` (channel-parameter-tracker helpers used by
/// `parameter_tracker.rs` / `modify_midi.rs` / `used_programs_and_keys.rs`), completing the
/// `MIDIUtils` port and letting those three files drop their dependency on the pre-4.3.0
/// `utils/sysex_detector.rs` free functions (now deleted).
use crate::midi::enums::{
    midi_controllers, midi_message_types, non_registered_lsb, non_registered_msb,
    registered_parameter_types,
};
use crate::midi::midi_message::MidiMessage;

// ─────────────────────────────────────────────────────────────────────────────
// AnalyzedMidiMessage
// ─────────────────────────────────────────────────────────────────────────────

/// The result of analyzing a MIDI System Exclusive message (or, for the RPN/NRPN analyzers not
/// ported here, a registered/non-registered parameter number).
/// Equivalent to: AnalyzedMIDIMessage (TS 4.3.0)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnalyzedMidiMessage {
    Other,
    XgReset,
    GmOn,
    GmOff,
    Gm2On,
    GsReset,
    ReverbParam,
    ChorusParam,
    DelayParam,
    VariationParam,
    InsertionParam,
    DrumsOn { channel: u8, is_drum: bool },
    DrumSetup,
    ProgramChange { channel: u8, value: u8 },
    ControllerChange { channel: u8, controller: u8, value: u8 },
    MasterKeyShift { value: i32 },
    KeyShift { channel: u8, value: i32 },
    /// Value in cents.
    MasterFineTune { value: f64 },
    /// Value in cents.
    FineTune { channel: u8, value: f64 },
    /// Random pan (new in 4.3.16): a pan of 0 in the XG/GS part parameters selects random
    /// panning rather than a pan position, so it is a channel MIDI parameter, not a CC.
    /// Equivalent to: { type: "Channel MIDI Param", parameter: "randomPan", value: true }
    RandomPan { channel: u8 },
}

/// Safe byte access: TypeScript's `arr[outOfRangeIndex]` yields `undefined`, which behaves as 0
/// in the bitwise arithmetic these analyzers perform. `.get(i)` mirrors that without panicking.
fn byte_at(syx: &[u8], i: usize) -> u8 {
    syx.get(i).copied().unwrap_or(0)
}

// ─────────────────────────────────────────────────────────────────────────────
// MidiUtils
// ─────────────────────────────────────────────────────────────────────────────

/// A general purpose class for handling MIDI messages.
/// Equivalent to: class MIDIUtils
pub struct MidiUtils;

impl MidiUtils {
    /// Analyzes a MIDI System Exclusive message and returns an identification and data for it.
    /// * `syx` - the System Exclusive message, WITHOUT the first 0xF0 System Exclusive byte!
    ///
    /// Equivalent to: MIDIUtils.analyzeSysEx(syx)
    pub fn analyze_sysex(syx: &[u8]) -> AnalyzedMidiMessage {
        // At least Manufacturer ID, Device ID and XG/GS model ID
        if syx.len() < 3 {
            return AnalyzedMidiMessage::Other;
        }
        match syx[0] {
            // Non realtime GM / Realtime GM
            0x7e | 0x7f => Self::analyze_gm(syx),
            // Roland
            0x41 => Self::analyze_gs(syx),
            // Yamaha
            0x43 => Self::analyze_xg(syx),
            _ => AnalyzedMidiMessage::Other,
        }
    }

    /// Analyzes a MIDI Registered Parameter Number and returns an identification and data for it.
    /// * `channel` - the MIDI channel number.
    /// * `rpn` - the 14-bit RPN number.
    /// * `value` - the 14-bit value for that number.
    ///
    /// Equivalent to: MIDIUtils.analyzeRPN(channel, rpn, value)
    pub fn analyze_rpn(channel: u8, rpn: u16, value: u16) -> AnalyzedMidiMessage {
        match rpn {
            registered_parameter_types::FINE_TUNING => AnalyzedMidiMessage::FineTune {
                channel,
                value: (value as f64 - 8192.0) / 81.92,
            },
            registered_parameter_types::COARSE_TUNING => AnalyzedMidiMessage::KeyShift {
                channel,
                value: (value >> 7) as i32 - 64,
            },
            _ => AnalyzedMidiMessage::Other,
        }
    }

    /// Analyzes a MIDI Non-Registered Parameter Number and returns an identification and data
    /// for it.
    /// * `channel` - the MIDI channel number.
    /// * `nrpn` - the 14-bit NRPN number.
    /// * `value` - the 14-bit value for that number.
    ///
    /// Equivalent to: MIDIUtils.analyzeNRPN(channel, nrpn, value)
    pub fn analyze_nrpn(channel: u8, nrpn: u16, value: u16) -> AnalyzedMidiMessage {
        let msb = (nrpn >> 7) as u8;
        let lsb = (nrpn & 0x7f) as u8;
        match msb {
            non_registered_msb::PART_PARAMETER => match lsb {
                non_registered_lsb::TVF_CUTOFF_FREQUENCY => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::BRIGHTNESS,
                    value: (value >> 7) as u8,
                },
                non_registered_lsb::TVF_RESONANCE => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::FILTER_RESONANCE,
                    value: (value >> 7) as u8,
                },
                non_registered_lsb::ENVELOPE_ATTACK_TIME => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::ATTACK_TIME,
                    value: (value >> 7) as u8,
                },
                non_registered_lsb::ENVELOPE_DECAY_TIME => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::DECAY_TIME,
                    value: (value >> 7) as u8,
                },
                non_registered_lsb::ENVELOPE_RELEASE_TIME => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::RELEASE_TIME,
                    value: (value >> 7) as u8,
                },
                _ => AnalyzedMidiMessage::Other,
            },
            non_registered_msb::DRUM_PITCH
            | non_registered_msb::DRUM_PITCH_FINE
            | non_registered_msb::DRUM_LEVEL
            | non_registered_msb::DRUM_PAN
            | non_registered_msb::DRUM_REVERB
            | non_registered_msb::DRUM_CHORUS
            | non_registered_msb::DRUM_DELAY => AnalyzedMidiMessage::DrumSetup,
            _ => AnalyzedMidiMessage::Other,
        }
    }

    /// Converts GS/XG "part number" to MIDI channel number.
    /// Equivalent to: MIDIUtils.syxToChannel(part)
    pub fn syx_to_channel(part: u8) -> u8 {
        const MAP: [u8; 16] = [9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15];
        MAP[(part % 16) as usize]
    }

    /// Converts MIDI channel number to GS/XG "part number".
    /// Equivalent to: MIDIUtils.channelToSyx(channel)
    pub fn channel_to_syx(channel: u8) -> u8 {
        const MAP: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 10, 11, 12, 13, 14, 15];
        MAP[(channel % 16) as usize]
    }

    /// Gets raw GS System Exclusive message bytes, without the 0xF0 status byte.
    /// * `a1`/`a2`/`a3` - Address bytes.
    /// * `data` - Data, can be multiple bytes.
    ///
    /// Equivalent to: MIDIUtils.gsData(a1, a2, a3, data)
    pub fn gs_data(a1: u8, a2: u8, a3: u8, data: &[u8]) -> Vec<u8> {
        // Calculate checksum
        // SC 8850 manual, page 245
        let sum: u32 = a1 as u32
            + a2 as u32
            + a3 as u32
            + data.iter().map(|&b| b as u32).sum::<u32>();
        let checksum = ((128 - (sum % 128)) & 0x7f) as u8;
        let mut out = Vec::with_capacity(7 + data.len());
        out.push(0x41); // Roland
        out.push(0x10); // Device ID (defaults to 16 on Roland)
        out.push(0x42); // GS
        out.push(0x12); // Command ID (DT1)
        out.push(a1);
        out.push(a2);
        out.push(a3);
        out.extend_from_slice(data);
        out.push(checksum);
        out.push(0xf7); // End of exclusive
        out
    }

    /// Gets a GS System Exclusive MIDI message.
    /// Equivalent to: MIDIUtils.gsMessage(ticks, a1, a2, a3, data)
    pub fn gs_message(ticks: u32, a1: u8, a2: u8, a3: u8, data: &[u8]) -> MidiMessage {
        MidiMessage::new(
            ticks,
            midi_message_types::SYSTEM_EXCLUSIVE,
            Self::gs_data(a1, a2, a3, data),
        )
    }

    /// Gets a GS drum-map-change System Exclusive MIDI message.
    /// * `drum_map` - 0 turns the channel into a melodic channel, other values turn it into a
    ///   drum channel.
    ///
    /// Equivalent to: MIDIUtils.gsDrumChange(ticks, channel, drumMap)
    pub fn gs_drum_change(ticks: u32, channel: u8, drum_map: u8) -> MidiMessage {
        let chan_address = 0x10 | Self::channel_to_syx(channel);
        Self::gs_message(ticks, 40, chan_address, 0x15, &[drum_map])
    }

    /// Gets a GS reset message System Exclusive MIDI message.
    /// Equivalent to: MIDIUtils.gsReset(ticks)
    pub fn gs_reset(ticks: u32) -> MidiMessage {
        Self::gs_message(
            ticks,
            0x40, // System parameter - Address
            0x00, // Global mode parameter - Address
            0x7f, // MODE SET - Address
            &[0x00], // 00 = GS Reset - Data
        )
    }

    // ─────────────────────────────────────────────────────────────────────
    // Private analyzers
    // ─────────────────────────────────────────────────────────────────────

    fn analyze_gm(syx: &[u8]) -> AnalyzedMidiMessage {
        if syx.len() < 4 {
            return AnalyzedMidiMessage::Other;
        }

        if syx[2] == 0x04 {
            // Device control
            return match syx[3] {
                0x03 => {
                    // Master Fine-Tuning
                    let tuning_value =
                        ((byte_at(syx, 5) as i32) << 7 | byte_at(syx, 6) as i32) - 8192;
                    let cents = tuning_value as f64 / 81.92; // [-100;+99] cents range
                    AnalyzedMidiMessage::MasterFineTune { value: cents }
                }
                0x04 => {
                    // Master Coarse Tuning
                    AnalyzedMidiMessage::MasterKeyShift {
                        value: byte_at(syx, 5) as i32 - 64,
                    }
                }
                0x05 => {
                    // Global Parameter control
                    if byte_at(syx, 4) != 0x01 // Slot Path Length
                        || byte_at(syx, 5) != 0x01 // Parameter ID Width
                        || byte_at(syx, 6) != 0x01 // Value Width
                        || byte_at(syx, 7) != 0x01
                    // Slot Path MSB
                    {
                        return AnalyzedMidiMessage::Other;
                    }
                    // Slot Path LSB
                    match byte_at(syx, 8) {
                        0x01 => {
                            // Reverb - Parameter
                            match byte_at(syx, 9) {
                                0x01 | 0x02 => AnalyzedMidiMessage::ReverbParam,
                                _ => AnalyzedMidiMessage::Other,
                            }
                        }
                        0x02 => {
                            // Chorus - Parameter
                            match byte_at(syx, 9) {
                                0x01..=0x04 => AnalyzedMidiMessage::ChorusParam,
                                _ => AnalyzedMidiMessage::Other,
                            }
                        }
                        _ => AnalyzedMidiMessage::Other,
                    }
                }
                _ => AnalyzedMidiMessage::Other,
            };
        }

        if syx[2] != 0x09 {
            return AnalyzedMidiMessage::Other;
        }
        match syx[3] {
            0x01 => AnalyzedMidiMessage::GmOn,
            0x02 => AnalyzedMidiMessage::GmOff,
            0x03 => AnalyzedMidiMessage::Gm2On,
            _ => AnalyzedMidiMessage::Other,
        }
    }

    fn analyze_xg(syx: &[u8]) -> AnalyzedMidiMessage {
        // Ensure XG
        if syx[2] != 0x4c || syx.len() < 7 {
            return AnalyzedMidiMessage::Other;
        }
        let a1 = syx[3]; // Address 1
        let a2 = syx[4]; // Address 2
        let a3 = syx[5]; // Address 3
        let data = syx[6];

        if a1 == 0x00 && a2 == 0x00 {
            // XG SYSTEM
            return match a3 {
                0x00 => {
                    // MASTER TUNE
                    let tune = ((byte_at(syx, 6) & 15) as i32) << 12
                        | ((byte_at(syx, 7) & 15) as i32) << 8
                        | ((byte_at(syx, 8) & 15) as i32) << 4
                        | (byte_at(syx, 9) & 15) as i32;
                    let cents = (tune - 1024) as f64 / 10.0;
                    AnalyzedMidiMessage::MasterFineTune { value: cents }
                }
                0x06 => {
                    // TRANSPOSE
                    AnalyzedMidiMessage::MasterKeyShift {
                        value: data as i32 - 64,
                    }
                }
                // XG SYSTEM ON / ALL PARAMETER RESET
                0x7e | 0x7f => AnalyzedMidiMessage::XgReset,
                _ => AnalyzedMidiMessage::Other,
            };
        }

        // XG EFFECT 1
        if a1 == 0x02 && a2 == 0x01 {
            if a3 <= 0x15 {
                return AnalyzedMidiMessage::ReverbParam;
            }
            if a3 <= 0x35 {
                return AnalyzedMidiMessage::ChorusParam;
            }
            return AnalyzedMidiMessage::VariationParam;
        }

        // XG EFFECT 2
        if a1 == 0x03 && a2 == 0x00 {
            return AnalyzedMidiMessage::VariationParam;
        }

        // XG MULTI PART (a2 is the channel number)
        if a1 == 0x08 {
            let channel = a2;
            // Avoid invalid channels
            if channel >= 16 {
                return AnalyzedMidiMessage::Other;
            }
            return match a3 {
                0x01 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::BANK_SELECT,
                    value: data,
                },
                0x02 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::BANK_SELECT_LSB,
                    value: data,
                },
                0x03 => AnalyzedMidiMessage::ProgramChange { channel, value: data },
                0x05 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: if data == 1 {
                        midi_controllers::POLY_MODE_ON
                    } else {
                        midi_controllers::MONO_MODE_ON
                    },
                    value: 0,
                },
                0x07 => AnalyzedMidiMessage::DrumsOn {
                    channel,
                    is_drum: data > 0,
                },
                0x08 => AnalyzedMidiMessage::KeyShift {
                    channel,
                    value: data as i32 - 64,
                },
                0x0b => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::MAIN_VOLUME,
                    value: data,
                },
                0x0e => {
                    // Pan, except for random,
                    // Which is a different parameter
                    if data == 0 {
                        AnalyzedMidiMessage::RandomPan { channel }
                    } else {
                        AnalyzedMidiMessage::ControllerChange {
                            channel,
                            controller: midi_controllers::PAN,
                            value: data,
                        }
                    }
                }
                0x12 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::CHORUS_DEPTH,
                    value: data,
                },
                0x13 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::REVERB_DEPTH,
                    value: data,
                },
                0x15 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::VIBRATO_RATE,
                    value: data,
                },
                0x16 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::VIBRATO_DEPTH,
                    value: data,
                },
                0x17 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::VIBRATO_DELAY,
                    value: data,
                },
                0x18 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::BRIGHTNESS,
                    value: data,
                },
                0x19 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::FILTER_RESONANCE,
                    value: data,
                },
                0x1a => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::ATTACK_TIME,
                    value: data,
                },
                0x1b => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::DECAY_TIME,
                    value: data,
                },
                0x0c => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::RELEASE_TIME,
                    value: data,
                },
                _ => AnalyzedMidiMessage::Other,
            };
        }

        // Drum part setup
        if a1 >> 4 == 3 {
            return AnalyzedMidiMessage::DrumSetup;
        }

        AnalyzedMidiMessage::Other
    }

    fn analyze_gs(syx: &[u8]) -> AnalyzedMidiMessage {
        if syx.len() < 10
            // Model ID (GS)
            || syx[2] != 0x42
            // 0x12: DT1 (Device Transmit)
            || syx[3] != 0x12
        {
            return AnalyzedMidiMessage::Other; // Something else
        }

        // Address
        let a1 = syx[4];
        let a2 = syx[5];
        let a3 = syx[6];
        let data = syx[7];

        // GS reset check
        if
        // Address 1 is 0x00 for SC-88 SYSTEM MODE SET and 0x40 for SC-55 MODE SET
        (a1 == 0x00 || a1 == 0x40) && a2 == 0x00 {
            // System Parameter
            match a3 {
                // MODE SET
                0x7f => {
                    return match data {
                        // GS Reset/Mode-1
                        0x00 => AnalyzedMidiMessage::GsReset,
                        // GS Off, default to gm
                        0x7f => AnalyzedMidiMessage::GmOn,
                        _ => AnalyzedMidiMessage::Other,
                    };
                }
                // Master Tune
                0x00 => {
                    let tune = (data as i32) << 12
                        | (byte_at(syx, 8) as i32) << 8
                        | (byte_at(syx, 9) as i32) << 4
                        | byte_at(syx, 10) as i32;
                    let cents = (tune - 1024) as f64 / 10.0;
                    return AnalyzedMidiMessage::MasterFineTune { value: cents };
                }
                _ => {}
            }
        }

        if a1 == 0x41 {
            return AnalyzedMidiMessage::DrumSetup;
        }
        if a1 != 0x40 {
            return AnalyzedMidiMessage::Other;
        }

        if a2 == 0x00 && a3 == 0x05 {
            return AnalyzedMidiMessage::MasterKeyShift {
                value: data as i32 - 64,
            };
        }

        // Effects
        if a2 == 0x01 {
            if (0x30..=0x37).contains(&a3) {
                return AnalyzedMidiMessage::ReverbParam;
            }
            if (0x38..=0x40).contains(&a3) {
                return AnalyzedMidiMessage::ChorusParam;
            }
            if (0x50..=0x5a).contains(&a3) {
                return AnalyzedMidiMessage::DelayParam;
            }
        }

        // EFX Parameter
        if a2 == 0x03 && a3 <= 0x7f {
            return AnalyzedMidiMessage::InsertionParam;
        }

        // Patch parameter
        if a2 >> 4 == 1 {
            let channel = Self::syx_to_channel(a2 & 0x0f);
            return match a3 {
                0x00 => {
                    // Tone number
                    AnalyzedMidiMessage::ProgramChange { channel, value: data }
                }
                0x13 => {
                    // Mono/poly
                    AnalyzedMidiMessage::ControllerChange {
                        channel,
                        controller: if data == 1 {
                            midi_controllers::POLY_MODE_ON
                        } else {
                            midi_controllers::MONO_MODE_ON
                        },
                        value: 0,
                    }
                }
                0x15 => AnalyzedMidiMessage::DrumsOn {
                    channel,
                    is_drum: data > 0,
                },
                0x16 => AnalyzedMidiMessage::KeyShift {
                    channel,
                    value: data as i32 - 64,
                },
                0x19 => {
                    // Part level (cc#7)
                    AnalyzedMidiMessage::ControllerChange {
                        channel,
                        controller: midi_controllers::MAIN_VOLUME,
                        value: data,
                    }
                }
                0x1c => {
                    // Pan position, except for random,
                    // Which is a different parameter
                    if data == 0 {
                        AnalyzedMidiMessage::RandomPan { channel }
                    } else {
                        AnalyzedMidiMessage::ControllerChange {
                            channel,
                            controller: midi_controllers::PAN,
                            value: data,
                        }
                    }
                }
                0x21 => {
                    // Chorus send
                    AnalyzedMidiMessage::ControllerChange {
                        channel,
                        controller: midi_controllers::CHORUS_DEPTH,
                        value: data,
                    }
                }
                0x22 => {
                    // Reverb send
                    AnalyzedMidiMessage::ControllerChange {
                        channel,
                        controller: midi_controllers::REVERB_DEPTH,
                        value: data,
                    }
                }
                0x2a => {
                    // Fine tune (0-16384)
                    let tune = (data as i32) << 7 | byte_at(syx, 8) as i32;
                    let tune_cents = (tune - 8192) as f64 / 81.92;
                    AnalyzedMidiMessage::FineTune {
                        channel,
                        value: tune_cents,
                    }
                }
                0x2c => {
                    // Delay send
                    AnalyzedMidiMessage::ControllerChange {
                        channel,
                        controller: midi_controllers::VARIATION_DEPTH,
                        value: data,
                    }
                }
                0x30 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::VIBRATO_RATE,
                    value: data,
                },
                0x31 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::VIBRATO_DEPTH,
                    value: data,
                },
                0x32 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::BRIGHTNESS,
                    value: data,
                },
                0x33 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::FILTER_RESONANCE,
                    value: data,
                },
                0x34 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::ATTACK_TIME,
                    value: data,
                },
                0x35 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::DECAY_TIME,
                    value: data,
                },
                0x36 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::RELEASE_TIME,
                    value: data,
                },
                0x37 => AnalyzedMidiMessage::ControllerChange {
                    channel,
                    controller: midi_controllers::VIBRATO_DELAY,
                    value: data,
                },
                _ => AnalyzedMidiMessage::Other,
            };
        }

        // Patch Parameter Tone Map
        if a2 >> 4 == 4 {
            let channel = Self::syx_to_channel(a2 & 0x0f);
            return match a3 {
                0x00 | 0x01 => {
                    // Tone map number (cc#32)
                    AnalyzedMidiMessage::ControllerChange {
                        channel,
                        controller: midi_controllers::BANK_SELECT_LSB,
                        value: data,
                    }
                }
                0x22 => AnalyzedMidiMessage::InsertionParam,
                _ => AnalyzedMidiMessage::Other,
            };
        }

        AnalyzedMidiMessage::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── gs_reset ────────────────────────────────────────────────────────────

    #[test]
    fn test_gs_reset_full_sysex_payload() {
        let msg = MidiUtils::gs_reset(0);
        assert_eq!(
            msg.data,
            vec![0x41, 0x10, 0x42, 0x12, 0x40, 0x00, 0x7f, 0x00, 0x41, 0xf7]
        );
    }

    #[test]
    fn test_gs_reset_ticks() {
        let msg = MidiUtils::gs_reset(480);
        assert_eq!(msg.ticks, 480);
    }

    #[test]
    fn test_gs_drum_change_channel_0_melodic() {
        let msg = MidiUtils::gs_drum_change(0, 0, 0);
        // data layout: [Roland, DeviceID, GS, DT1, a1=40, a2=chanAddress, a3=0x15, drumMap, checksum, F7]
        // Channel 0 → syx part 1; addr = 0x10 | 1 = 0x11
        assert_eq!(msg.data[4], 40);
        assert_eq!(msg.data[5], 0x11);
        assert_eq!(msg.data[6], 0x15);
        assert_eq!(msg.data[7], 0);
    }

    // ── syx_to_channel / channel_to_syx ─────────────────────────────────────

    #[test]
    fn test_syx_to_channel_part_0_is_channel_9() {
        assert_eq!(MidiUtils::syx_to_channel(0), 9);
    }

    #[test]
    fn test_syx_to_channel_part_1_is_channel_0() {
        assert_eq!(MidiUtils::syx_to_channel(1), 0);
    }

    #[test]
    fn test_channel_to_syx_channel_9_is_part_0() {
        assert_eq!(MidiUtils::channel_to_syx(9), 0);
    }

    #[test]
    fn test_channel_to_syx_channel_0_is_part_1() {
        assert_eq!(MidiUtils::channel_to_syx(0), 1);
    }

    #[test]
    fn test_syx_channel_roundtrip() {
        for ch in 0..16u8 {
            let part = MidiUtils::channel_to_syx(ch);
            assert_eq!(MidiUtils::syx_to_channel(part), ch);
        }
    }

    // ── analyze_sysex: too short / unknown manufacturer ─────────────────────

    #[test]
    fn test_analyze_sysex_too_short() {
        assert_eq!(
            MidiUtils::analyze_sysex(&[0x41, 0x10]),
            AnalyzedMidiMessage::Other
        );
    }

    #[test]
    fn test_analyze_sysex_unknown_manufacturer() {
        assert_eq!(
            MidiUtils::analyze_sysex(&[0x99, 0x00, 0x00]),
            AnalyzedMidiMessage::Other
        );
    }

    // ── analyze_sysex: GM ─────────────────────────────────────────────────

    #[test]
    fn test_analyze_gm_on() {
        assert_eq!(
            MidiUtils::analyze_sysex(&[0x7e, 0x7f, 0x09, 0x01]),
            AnalyzedMidiMessage::GmOn
        );
    }

    #[test]
    fn test_analyze_gm_off() {
        assert_eq!(
            MidiUtils::analyze_sysex(&[0x7e, 0x7f, 0x09, 0x02]),
            AnalyzedMidiMessage::GmOff
        );
    }

    #[test]
    fn test_analyze_gm2_on() {
        assert_eq!(
            MidiUtils::analyze_sysex(&[0x7e, 0x7f, 0x09, 0x03]),
            AnalyzedMidiMessage::Gm2On
        );
    }

    #[test]
    fn test_analyze_gm_master_volume_not_ported_returns_other() {
        // Device control (syx[2]==0x04) subtype 0x01 (Master Volume) is not one of the ported
        // branches (0x03/0x04/0x05) — should fall to Other.
        assert_eq!(
            MidiUtils::analyze_sysex(&[0x7f, 0x7f, 0x04, 0x01, 0x00, 0x7f]),
            AnalyzedMidiMessage::Other
        );
    }

    #[test]
    fn test_analyze_gm_master_coarse_tuning() {
        let result = MidiUtils::analyze_sysex(&[0x7f, 0x7f, 0x04, 0x04, 0x00, 70]);
        assert_eq!(result, AnalyzedMidiMessage::MasterKeyShift { value: 6 });
    }

    // ── analyze_sysex: GS ─────────────────────────────────────────────────

    #[test]
    fn test_analyze_gs_reset() {
        let syx = [0x41, 0x10, 0x42, 0x12, 0x40, 0x00, 0x7f, 0x00, 0x41, 0xf7];
        assert_eq!(MidiUtils::analyze_sysex(&syx), AnalyzedMidiMessage::GsReset);
    }

    #[test]
    fn test_analyze_gs_off_returns_gm_on() {
        let syx = [0x41, 0x10, 0x42, 0x12, 0x40, 0x00, 0x7f, 0x7f, 0x00, 0xf7];
        assert_eq!(MidiUtils::analyze_sysex(&syx), AnalyzedMidiMessage::GmOn);
    }

    #[test]
    fn test_analyze_gs_drums_on_channel() {
        // a1=0x40, a2=0x10 (part 0 → channel 9, drum bank flag), a3=0x15, data=1
        let syx = [0x41, 0x10, 0x42, 0x12, 0x40, 0x10, 0x15, 0x01, 0x00, 0xf7];
        assert_eq!(
            MidiUtils::analyze_sysex(&syx),
            AnalyzedMidiMessage::DrumsOn {
                channel: 9,
                is_drum: true
            }
        );
    }

    #[test]
    fn test_analyze_gs_drums_off_channel() {
        let syx = [0x41, 0x10, 0x42, 0x12, 0x40, 0x11, 0x15, 0x00, 0x00, 0xf7];
        assert_eq!(
            MidiUtils::analyze_sysex(&syx),
            AnalyzedMidiMessage::DrumsOn {
                channel: 0,
                is_drum: false
            }
        );
    }

    #[test]
    fn test_analyze_gs_program_change() {
        // a2=0x11 (part 1 → channel 0), a3=0x00 (tone number), data=42
        let syx = [0x41, 0x10, 0x42, 0x12, 0x40, 0x11, 0x00, 42, 0x00, 0xf7];
        assert_eq!(
            MidiUtils::analyze_sysex(&syx),
            AnalyzedMidiMessage::ProgramChange {
                channel: 0,
                value: 42
            }
        );
    }

    #[test]
    fn test_analyze_gs_reverb_param() {
        let syx = [0x41, 0x10, 0x42, 0x12, 0x40, 0x01, 0x30, 0x05, 0x00, 0xf7];
        assert_eq!(
            MidiUtils::analyze_sysex(&syx),
            AnalyzedMidiMessage::ReverbParam
        );
    }

    #[test]
    fn test_analyze_gs_chorus_param() {
        let syx = [0x41, 0x10, 0x42, 0x12, 0x40, 0x01, 0x38, 0x05, 0x00, 0xf7];
        assert_eq!(
            MidiUtils::analyze_sysex(&syx),
            AnalyzedMidiMessage::ChorusParam
        );
    }

    #[test]
    fn test_analyze_gs_delay_param() {
        let syx = [0x41, 0x10, 0x42, 0x12, 0x40, 0x01, 0x50, 0x05, 0x00, 0xf7];
        assert_eq!(
            MidiUtils::analyze_sysex(&syx),
            AnalyzedMidiMessage::DelayParam
        );
    }

    #[test]
    fn test_analyze_gs_too_short() {
        let syx = [0x41, 0x10, 0x42, 0x12, 0x40, 0x00, 0x7f];
        assert_eq!(MidiUtils::analyze_sysex(&syx), AnalyzedMidiMessage::Other);
    }

    #[test]
    fn test_analyze_gs_wrong_command_id() {
        let syx = [0x41, 0x10, 0x42, 0x00, 0x40, 0x00, 0x7f, 0x00, 0x00, 0xf7];
        assert_eq!(MidiUtils::analyze_sysex(&syx), AnalyzedMidiMessage::Other);
    }

    // ── analyze_sysex: XG ─────────────────────────────────────────────────

    #[test]
    fn test_analyze_xg_reset() {
        let syx = [0x43, 0x10, 0x4c, 0x00, 0x00, 0x7e, 0x00];
        assert_eq!(MidiUtils::analyze_sysex(&syx), AnalyzedMidiMessage::XgReset);
    }

    #[test]
    fn test_analyze_xg_all_param_reset() {
        let syx = [0x43, 0x10, 0x4c, 0x00, 0x00, 0x7f, 0x00];
        assert_eq!(MidiUtils::analyze_sysex(&syx), AnalyzedMidiMessage::XgReset);
    }

    #[test]
    fn test_analyze_xg_bank_select_msb() {
        // a1=0x08, a2=channel 3, a3=0x01 (bank select MSB), data=5
        let syx = [0x43, 0x10, 0x4c, 0x08, 0x03, 0x01, 5];
        assert_eq!(
            MidiUtils::analyze_sysex(&syx),
            AnalyzedMidiMessage::ControllerChange {
                channel: 3,
                controller: midi_controllers::BANK_SELECT,
                value: 5
            }
        );
    }

    #[test]
    fn test_analyze_xg_program_change() {
        let syx = [0x43, 0x10, 0x4c, 0x08, 2, 0x03, 17];
        assert_eq!(
            MidiUtils::analyze_sysex(&syx),
            AnalyzedMidiMessage::ProgramChange {
                channel: 2,
                value: 17
            }
        );
    }

    #[test]
    fn test_analyze_xg_drums_on() {
        let syx = [0x43, 0x10, 0x4c, 0x08, 9, 0x07, 1];
        assert_eq!(
            MidiUtils::analyze_sysex(&syx),
            AnalyzedMidiMessage::DrumsOn {
                channel: 9,
                is_drum: true
            }
        );
    }

    #[test]
    fn test_analyze_xg_invalid_channel_returns_other() {
        let syx = [0x43, 0x10, 0x4c, 0x08, 16, 0x07, 1];
        assert_eq!(MidiUtils::analyze_sysex(&syx), AnalyzedMidiMessage::Other);
    }

    #[test]
    fn test_analyze_xg_reverb_param() {
        let syx = [0x43, 0x10, 0x4c, 0x02, 0x01, 0x00, 0x05];
        assert_eq!(
            MidiUtils::analyze_sysex(&syx),
            AnalyzedMidiMessage::ReverbParam
        );
    }

    #[test]
    fn test_analyze_xg_drum_setup() {
        let syx = [0x43, 0x10, 0x4c, 0x30, 0x00, 0x00, 0x05];
        assert_eq!(
            MidiUtils::analyze_sysex(&syx),
            AnalyzedMidiMessage::DrumSetup
        );
    }

    #[test]
    fn test_analyze_xg_master_key_shift() {
        let syx = [0x43, 0x10, 0x4c, 0x00, 0x00, 0x06, 70];
        assert_eq!(
            MidiUtils::analyze_sysex(&syx),
            AnalyzedMidiMessage::MasterKeyShift { value: 6 }
        );
    }

    #[test]
    fn test_analyze_xg_wrong_id() {
        let syx = [0x43, 0x10, 0x00, 0x00, 0x00, 0x7e, 0x00];
        assert_eq!(MidiUtils::analyze_sysex(&syx), AnalyzedMidiMessage::Other);
    }

    // ── analyze_rpn ─────────────────────────────────────────────────────────

    #[test]
    fn test_analyze_rpn_fine_tuning() {
        // value=8192 (center) -> 0 cents
        assert_eq!(
            MidiUtils::analyze_rpn(2, registered_parameter_types::FINE_TUNING, 8192),
            AnalyzedMidiMessage::FineTune { channel: 2, value: 0.0 }
        );
    }

    #[test]
    fn test_analyze_rpn_coarse_tuning() {
        // value = 65 << 7 -> (65)-64 = 1 semitone
        assert_eq!(
            MidiUtils::analyze_rpn(3, registered_parameter_types::COARSE_TUNING, 65 << 7),
            AnalyzedMidiMessage::KeyShift { channel: 3, value: 1 }
        );
    }

    #[test]
    fn test_analyze_rpn_unknown_returns_other() {
        assert_eq!(
            MidiUtils::analyze_rpn(0, registered_parameter_types::MODULATION_DEPTH, 0),
            AnalyzedMidiMessage::Other
        );
    }

    // ── analyze_nrpn ────────────────────────────────────────────────────────

    #[test]
    fn test_analyze_nrpn_vibrato_rate_is_drum_setup() {
        // msb=PART_PARAMETER(1), lsb=vibratoRate(0x08) -> not a "part parameter" case ported below
        // (only tvfCutoff/tvfResonance/attack/decay/release are ported; vibrato falls to Other)
        let nrpn = ((non_registered_msb::PART_PARAMETER as u16) << 7)
            | non_registered_lsb::VIBRATO_RATE as u16;
        assert_eq!(MidiUtils::analyze_nrpn(0, nrpn, 0), AnalyzedMidiMessage::Other);
    }

    #[test]
    fn test_analyze_nrpn_tvf_cutoff_frequency() {
        let nrpn = ((non_registered_msb::PART_PARAMETER as u16) << 7)
            | non_registered_lsb::TVF_CUTOFF_FREQUENCY as u16;
        assert_eq!(
            MidiUtils::analyze_nrpn(5, nrpn, 100 << 7),
            AnalyzedMidiMessage::ControllerChange {
                channel: 5,
                controller: midi_controllers::BRIGHTNESS,
                value: 100
            }
        );
    }

    #[test]
    fn test_analyze_nrpn_drum_pitch_is_drum_setup() {
        let nrpn = (non_registered_msb::DRUM_PITCH as u16) << 7;
        assert_eq!(
            MidiUtils::analyze_nrpn(9, nrpn, 0),
            AnalyzedMidiMessage::DrumSetup
        );
    }

    #[test]
    fn test_analyze_nrpn_unknown_msb_returns_other() {
        let nrpn = (0x7fu16) << 7; // awe32, not a tracked NRPN msb here
        assert_eq!(MidiUtils::analyze_nrpn(0, nrpn, 0), AnalyzedMidiMessage::Other);
    }
}
