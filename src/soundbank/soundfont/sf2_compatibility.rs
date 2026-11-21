//! Everything that differs between a plain SoundFont 2 file and the bank model (which is SFe).
//! The binary reading is shared with SFe, the rules are all here: what a bank read from SF2 needs changed.
//! Ported from: src/soundbank/soundfont/sf2_compatibility.ts
//!
//! Not ported (writer only, no effect on the MIDI-to-WAV output): what an SF2 file can't store and the loss report
//! (`sf2KeepsGenerator`, `sf2KeepsModulator`, `toSF2SendAmount`, `toSF2Generator`,
//! `toSF2Modulators`, `sf2NeedsDefaultModulators`, `getSF2BankNumber`, `roundTripChanges`,
//! `getSF2LossesInternal`).
use crate::midi::enums::midi_controllers;
use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
use crate::soundbank::basic_soundbank::generator_types::{
    GENERATOR_LIMITS, GeneratorType, INSTRUMENT_ONLY_GENERATORS, MAX_GENERATOR, PRE_VOICE_GENERATORS,
    generator_types,
};
use crate::soundbank::basic_soundbank::modulator::{
    DEFAULT_RESONANT_MOD_AMOUNT_SOURCE, DEFAULT_RESONANT_MOD_SOURCE, DecodedModulator, Modulator,
    copy_default_modulators, is_illegal_source,
};
use crate::soundbank::basic_soundbank::modulator_source::ModulatorSource;
use crate::soundbank::enums::{modulator_curve_types, modulator_sources, modulator_transform_types};

/// `Math.round`: half-up, unlike `f64::round` (half away from zero).
#[inline]
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// The reverb and chorus sends of the SoundFont 2 default modulators: 200, where the bank model uses 1000.
/// Equivalent to: SF2_SEND_SCALE
const SF2_SEND_SCALE: f64 = 5.0;

/// The initial attenuation of an SF2 file is 2.5 times the centibels of the bank (the EMU8000 factor of 0.4).
/// Equivalent to: SF2_ATTENUATION_SCALE
const SF2_ATTENUATION_SCALE: f64 = 0.4;

/// The amount 0 twin of the default CC 71 to filterQOffset modulator, it turns that one off in a zone
/// (SFe 4.0, section 9.2).
/// Equivalent to: resonanceTwin()
fn resonance_twin() -> Modulator {
    DecodedModulator::new(
        DEFAULT_RESONANT_MOD_SOURCE,
        DEFAULT_RESONANT_MOD_AMOUNT_SOURCE,
        generator_types::FILTER_Q_OFFSET,
        0.0,
        0,
    )
}

/// If an instrument zone of an SF2 file without DMOD gets the twin.
/// It needs a CC 71 to initialFilterQ modulator with any SF2 curve. Ignored modulators don't count.
/// Equivalent to: hasResonanceModulator(modulators)
fn has_resonance_modulator(modulators: &[Modulator]) -> bool {
    modulators.iter().any(|m| {
        !is_ignored_modulator(m, false)
            && m.destination == generator_types::INITIAL_FILTER_Q
            && m.primary_source.is_controller(midi_controllers::FILTER_RESONANCE)
            && m.primary_source.curve_type <= modulator_curve_types::SWITCH
            && m.secondary_source.curve_type <= modulator_curve_types::SWITCH
    })
}

/// Every generator a destination can be, the invalid one and endOper aside.
/// Equivalent to: KNOWN_DESTINATIONS.has(destination)
fn is_known_destination(destination: GeneratorType) -> bool {
    // The defined generators are the ones with limits
    (0..=MAX_GENERATOR).contains(&destination)
        && destination != generator_types::END_OPER
        && GENERATOR_LIMITS[destination as usize].is_some()
}

/// A modulator that SFe 4.0 ignores (section 7.4), the SF2 writer drops these (SFe 4.0, section 9.1).
/// The reader gives most of them the invalid destination. The rest are an unknown transform or destination,
/// a destination no modulator can change (or an instrument-only one in a preset zone),
/// an unknown source type, or an illegal or reserved controller as a source.
/// Equivalent to: isIgnoredModulator(m, preset = false)
fn is_ignored_modulator(m: &Modulator, preset: bool) -> bool {
    !is_known_destination(m.destination)
        || PRE_VOICE_GENERATORS.contains(&m.destination)
        || (preset && INSTRUMENT_ONLY_GENERATORS.contains(&m.destination))
        || (m.transform_type != modulator_transform_types::LINEAR as u16
            && m.transform_type != modulator_transform_types::ABSOLUTE as u16)
        || [&m.primary_source, &m.secondary_source]
            .iter()
            .any(|s| s.curve_type > modulator_curve_types::CUSTOM || is_illegal_source(s))
}

/// Equivalent to: isEnvelopeTime(s)
fn is_envelope_time(s: &ModulatorSource) -> bool {
    s.is_controller(midi_controllers::ATTACK_TIME)
        || s.is_controller(midi_controllers::DECAY_TIME)
        || s.is_controller(midi_controllers::RELEASE_TIME)
}

/// A plain CC 91 to reverb send or CC 93 to chorus send modulator (positive unipolar linear, no amount source,
/// linear transform). SF2 files store a fifth of its amount.
/// Equivalent to: isSendModulator(m)
fn is_send_modulator(m: &Modulator) -> bool {
    if m.transform_type != 0 || m.secondary_source.to_source_enum() != 0x0 {
        return false;
    }
    let source = m.primary_source.to_source_enum();
    (source == 0x00_db && m.destination == generator_types::REVERB_EFFECTS_SEND)
        || (source == 0x00_dd && m.destination == generator_types::CHORUS_EFFECTS_SEND)
}

/// The amount of 50 cents in the default modulator is 1600, SF2 has 50.
/// Equivalent to: SF2_DEPTH_SCALE
const SF2_DEPTH_SCALE: f64 = 32.0;

/// The pitch destinations of a mod wheel vibrato.
/// Equivalent to: isVibratoDestination(m)
fn is_vibrato_destination(m: &Modulator) -> bool {
    m.destination == generator_types::MOD_LFO_TO_PITCH
        || m.destination == generator_types::VIB_LFO_TO_PITCH
}

/// Makes an SF2 mod wheel vibrato modulator follow RPN 5: the Modulation Depth Range becomes its amount source,
/// and its amount is multiplied by 32. Other modulators are left alone.
/// Equivalent to: followModulationDepth(m)
pub fn follow_modulation_depth(m: &mut Modulator) {
    // CC 1 as the source, no amount source and an amount that fits after scaling
    if !m.primary_source.is_controller(midi_controllers::MODULATION_WHEEL)
        || m.secondary_source.to_source_enum() != 0x0
        || !is_vibrato_destination(m)
        || m.transform_amount < -1023.0
        || m.transform_amount > 1023.0
    {
        return;
    }
    m.secondary_source.index = modulator_sources::MODULATION_DEPTH_RANGE;
    m.transform_amount *= SF2_DEPTH_SCALE;
}

/// Equivalent to: isDepthRange(s)
fn is_depth_range(s: &ModulatorSource) -> bool {
    !s.is_cc && s.index == modulator_sources::MODULATION_DEPTH_RANGE
}

/// The modulator as a format without the Modulation Depth Range stores it (SF2 and DLS).
/// Returns the same modulator if it does not use the range, a copy without the amount source and with a
/// 32 times smaller amount (limited to -1023 to 1023) if it is a mod wheel vibrato with a pitch destination,
/// and `None` if it can't be stored.
/// Equivalent to: withoutModulationDepth(m)
pub fn without_modulation_depth(m: &Modulator) -> Option<Modulator> {
    if is_depth_range(&m.primary_source) {
        return None;
    }
    if !is_depth_range(&m.secondary_source) {
        return Some(m.clone());
    }
    // The amount source must be the plain range (0x0011), another form would change meaning without it
    if !m.primary_source.is_controller(midi_controllers::MODULATION_WHEEL)
        || !is_vibrato_destination(m)
        || m.secondary_source.to_source_enum() != modulator_sources::MODULATION_DEPTH_RANGE as u16
    {
        return None;
    }
    let mut copy = Modulator::copy_from(m);
    copy.secondary_source.index = modulator_sources::NO_CONTROLLER;
    copy.secondary_source.set_curve_type(modulator_curve_types::LINEAR);
    copy.secondary_source.is_bipolar = false;
    copy.secondary_source.is_negative = false;
    copy.transform_amount = (-1023f64).max(1023f64.min(js_round(m.transform_amount / SF2_DEPTH_SCALE)));
    Some(copy)
}

/// The amount of a send modulator of the default form, read from an SF2 file: five times the file amount,
/// never above a full send and never below the SHORT range.
/// Equivalent to: fromSF2SendAmount(amount)
fn from_sf2_send_amount(amount: f64) -> f64 {
    (-32_768f64).max(1000f64.min(amount * SF2_SEND_SCALE))
}

/// The initial attenuation of an SF2 file in the unit of the bank, true centibels.
/// The EMU8000 attenuation is 0.4 of what the file says, so the factor lives here and not in the synthesizer.
/// Equivalent to: fromSF2Attenuation(value)
pub fn from_sf2_attenuation(value: i32) -> i32 {
    js_round(value as f64 * SF2_ATTENUATION_SCALE) as i32
}

/// The times 5 scaling of the send modulators of the default form, and the mod wheel vibrato following RPN 5.
/// Equivalent to: fromSF2Modulators(modulators)
fn from_sf2_modulators(modulators: &mut [Modulator]) {
    for m in modulators.iter_mut() {
        follow_modulation_depth(m);
        if is_send_modulator(m) {
            m.transform_amount = from_sf2_send_amount(m.transform_amount);
        }
    }
}

/// The default modulators the SF2 reader adds to a list read from DMOD, since SF2 can't store them.
/// Equivalent to: restoredDefaults(read).modulators
fn restored_defaults(read: &[Modulator]) -> Vec<Modulator> {
    // An empty list means no default modulators
    if read.is_empty() {
        return Vec::new();
    }
    let defaults = copy_default_modulators();
    // A modulator that is ignored supersedes nothing (SFe 4.0, section 9.2)
    let valid: Vec<&Modulator> = read.iter().filter(|m| !is_ignored_modulator(m, false)).collect();
    // The envelope times come back unless the list has a CC 72, 73 or 75 modulator of its own (even with amount 0)
    let has_envelope_time = valid
        .iter()
        .any(|m| is_envelope_time(&m.primary_source) || is_envelope_time(&m.secondary_source));
    // The same for the variation send and CC 94
    let has_variation = valid.iter().any(|m| {
        m.primary_source.is_controller(midi_controllers::VARIATION_DEPTH)
            || m.secondary_source.is_controller(midi_controllers::VARIATION_DEPTH)
    });
    defaults
        .into_iter()
        .filter(|m| {
            if m.destination == generator_types::VARIATION_EFFECTS_SEND {
                !has_variation
            } else {
                !has_envelope_time && is_envelope_time(&m.primary_source)
            }
        })
        .collect()
}

/// Turns the default modulators read from the DMOD chunk of an SF2 file into the list of the bank.
/// Equivalent to: fromSF2DefaultModulators(bank, read)
pub fn from_sf2_default_modulators(mut read: Vec<Modulator>) -> Vec<Modulator> {
    from_sf2_modulators(&mut read);
    let restored = restored_defaults(&read);
    read.extend(restored);
    read
}

/// Updates SF2 bank to SFe format as the spec describes.
/// The zone modulators to initialFilterQ stay as they are, they lower the level with the resonance as in SF2
/// (SFe 4.0, section 9.2).
/// `has_dmod`: if the file had a DMOD chunk. Its list has no CC 71 to filterQOffset, so the zones get no twin.
/// Equivalent to: upgradeSF2Bank(bank, hasDMOD)
pub fn upgrade_sf2_bank(bank: &mut BasicSoundBank, has_dmod: bool) {
    for preset in bank.presets.iter_mut() {
        from_sf2_modulators(&mut preset.global_zone.modulators);
        for zone in preset.zones.iter_mut() {
            from_sf2_modulators(&mut zone.zone.modulators);
        }
    }
    for instrument in bank.instruments.iter_mut() {
        from_sf2_modulators(&mut instrument.global_zone.modulators);
        for zone in instrument.zones.iter_mut() {
            from_sf2_modulators(&mut zone.zone.modulators);
        }
    }
    if has_dmod {
        return;
    }
    // Resonance (the last step): a zone with its own CC 71 to Q turns the default CC 71 to filterQOffset off.
    // That way the resonance is not doubled. Instrument zones only, a global zone covers its local zones
    for instrument in bank.instruments.iter_mut() {
        if has_resonance_modulator(&instrument.global_zone.modulators) {
            instrument.global_zone.modulators.push(resonance_twin());
        }
        for zone in instrument.zones.iter_mut() {
            if has_resonance_modulator(&zone.zone.modulators) {
                zone.zone.modulators.push(resonance_twin());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundbank::basic_soundbank::modulator::DecodedModulator;

    #[test]
    fn attenuation_is_rounded_half_up() {
        assert_eq!(from_sf2_attenuation(100), 40);
        assert_eq!(from_sf2_attenuation(1), 0);
        assert_eq!(from_sf2_attenuation(4), 2);
        assert_eq!(from_sf2_attenuation(-4), -2);
    }

    #[test]
    fn mod_wheel_vibrato_follows_rpn5() {
        let mut m = DecodedModulator::new(0x0081, 0, generator_types::VIB_LFO_TO_PITCH, 50.0, 0);
        follow_modulation_depth(&mut m);
        assert_eq!(m.transform_amount, 1600.0);
        assert_eq!(m.secondary_source.index, modulator_sources::MODULATION_DEPTH_RANGE);
    }

    #[test]
    fn restored_defaults_respect_the_list() {
        assert!(restored_defaults(&[]).is_empty());
        let read = vec![DecodedModulator::new(0x0081, 0, generator_types::VIB_LFO_TO_PITCH, 50.0, 0)];
        assert_eq!(restored_defaults(&read).len(), 4);
        let read = vec![DecodedModulator::new(0x00de, 0, generator_types::PAN, 1.0, 0)];
        assert_eq!(restored_defaults(&read).len(), 3);
    }
}
