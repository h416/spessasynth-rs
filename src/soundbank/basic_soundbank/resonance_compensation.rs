//! The CC 71 to gain twins of the resonance modulators, for banks made before the `gain` generator.
//! Ported from: src/soundbank/basic_soundbank/resonance_compensation.ts
use crate::midi::enums::midi_controllers;
use crate::soundbank::basic_soundbank::basic_soundbank::BasicSoundBank;
use crate::soundbank::basic_soundbank::generator_types::generator_types;
use crate::soundbank::basic_soundbank::modulator::{
    DEFAULT_RESONANT_MOD_SOURCE, Modulator, RESONANT_COMPENSATOR_AMOUNT_SOURCE,
};
use crate::soundbank::basic_soundbank::modulator_source::ModulatorSource;

/// `Math.round`: half-up, unlike `f64::round` (half away from zero).
#[inline]
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// Banks made before the `gain` generator relied on the engine keeping CC 71 from changing the level.
/// Gives every CC 71 to resonance modulator (in the default modulators and in every zone) a CC 71 to gain twin at half
/// its amount, unless the list already has a CC 71 to gain modulator. SF2 and DLS drop gain modulators when written,
/// so a round trip gives the same file.
///
/// The old SpessaSynth "resonant modulator" behavior, now with standard SFe!
/// Equivalent to: addResonanceCompensation(bank)
pub fn add_resonance_compensation(bank: &mut BasicSoundBank) {
    let added = get_resonant_compensators(&bank.default_modulators, false);
    bank.default_modulators.extend(added);
    for instrument in bank.instruments.iter_mut() {
        let added = get_resonant_compensators(&instrument.global_zone.modulators, false);
        instrument.global_zone.add_modulators(&added);
        for zone in instrument.zones.iter_mut() {
            let added = get_resonant_compensators(&zone.zone.modulators, false);
            zone.zone.add_modulators(&added);
        }
    }
    // Preset modulators add to the instrument's ones, and so do their twins
    for preset in bank.presets.iter_mut() {
        let added = get_resonant_compensators(&preset.global_zone.modulators, true);
        preset.global_zone.add_modulators(&added);
        for zone in preset.zones.iter_mut() {
            let added = get_resonant_compensators(&zone.zone.modulators, true);
            zone.zone.add_modulators(&added);
        }
    }
}

/// The compensators a list needs, to be added at its end.
/// `additive`: if the list is of a preset zone, where the amounts add to the instrument's ones.
/// Equivalent to: getResonantCompensators(modulators, additive = false)
pub fn get_resonant_compensators(modulators: &[Modulator], additive: bool) -> Vec<Modulator> {
    let mut added = Vec::new();
    if modulators.iter().any(|m| {
        m.destination == generator_types::GAIN
            && m.primary_source.is_controller(midi_controllers::FILTER_RESONANCE)
    }) {
        return added;
    }
    for m in modulators {
        if m.destination != generator_types::INITIAL_FILTER_Q
            || m.primary_source.to_source_enum() != DEFAULT_RESONANT_MOD_SOURCE
            || m.secondary_source.to_source_enum() != 0
        {
            continue;
        }
        let amount = m.transform_amount;
        if additive {
            // Moves the twin of the instrument along with its modulator
            // Exact as long as the summed resonance amount stays positive
            if amount != 0.0 {
                added.push(get_resonant_compensator(js_round(amount / 2.0), false));
            }
            continue;
        }
        // Above 64 (overrides the default compensator, even with 0)
        // Rounded, since the amount is written as a whole number
        added.push(get_resonant_compensator(js_round(amount.max(0.0) / 2.0), false));
        // A negative amount raises the resonance below 64
        if amount < 0.0 {
            added.push(get_resonant_compensator(js_round(amount / 2.0), true));
        }
    }
    added
}

/// Equivalent to: getResonantCompensator(amount, below64)
fn get_resonant_compensator(amount: f64, below64: bool) -> Modulator {
    let mut switch_source = ModulatorSource::from_source_enum(RESONANT_COMPENSATOR_AMOUNT_SOURCE);
    switch_source.is_negative = below64;
    Modulator::new(
        ModulatorSource::from_source_enum(DEFAULT_RESONANT_MOD_SOURCE),
        switch_source,
        generator_types::GAIN,
        amount,
        0,
    )
}
