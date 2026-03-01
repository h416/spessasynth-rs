use crate::soundbank::basic_soundbank::generator_types::GeneratorType;
/// awe32.rs
/// purpose: SoundBlaster AWE32 NRPN generator offset handler for MidiChannel.
/// Ported from: src/synthesizer/audio_engine/engine_methods/controller_control/data_entry/awe32.ts
///
/// References:
///   http://archive.gamedev.net/archive/reference/articles/article445.html
///   https://github.com/user-attachments/files/15757220/adip301.pdf
use crate::soundbank::basic_soundbank::generator_types::generator_types as gt;
use crate::synthesizer::audio_engine::engine_components::voice::Voice;
use crate::synthesizer::audio_engine::synthesizer_core::MidiChannel;
use crate::utils::loggin::spessa_synth_warn;

/// AWE32 NRPN generator index → SF2 GeneratorType mapping.
/// Equivalent to: AWE_NRPN_GENERATOR_MAPPINGS
const AWE_NRPN_GENERATOR_MAPPINGS: [GeneratorType; 27] = [
    gt::DELAY_MOD_LFO,
    gt::FREQ_MOD_LFO,
    gt::DELAY_VIB_LFO,
    gt::FREQ_VIB_LFO,
    gt::DELAY_MOD_ENV,
    gt::ATTACK_MOD_ENV,
    gt::HOLD_MOD_ENV,
    gt::DECAY_MOD_ENV,
    gt::SUSTAIN_MOD_ENV,
    gt::RELEASE_MOD_ENV,
    gt::DELAY_VOL_ENV,
    gt::ATTACK_VOL_ENV,
    gt::HOLD_VOL_ENV,
    gt::DECAY_VOL_ENV,
    gt::SUSTAIN_VOL_ENV,
    gt::RELEASE_VOL_ENV,
    gt::FINE_TUNE,
    gt::MOD_LFO_TO_PITCH,
    gt::VIB_LFO_TO_PITCH,
    gt::MOD_ENV_TO_PITCH,
    gt::MOD_LFO_TO_VOLUME,
    gt::INITIAL_FILTER_FC,
    gt::INITIAL_FILTER_Q,
    gt::MOD_LFO_TO_FILTER_FC,
    gt::MOD_ENV_TO_FILTER_FC,
    gt::CHORUS_EFFECTS_SEND,
    gt::REVERB_EFFECTS_SEND,
];

#[inline]
fn clip(v: f64, min: f64, max: f64) -> f64 {
    v.max(min).min(max)
}

#[inline]
fn ms_to_timecents(ms: f64) -> i16 {
    (1200.0 * (ms / 1000.0).log2()).max(-32_768.0) as i16
}

#[inline]
fn hz_to_cents(hz: f64) -> i16 {
    (6900.0 + 1200.0 * (hz / 440.0).log2()) as i16
}

impl MidiChannel {
    /// Emulates AWE32 NRPN generator changes, similarly to FluidSynth.
    ///
    /// `awe_gen`: NRPN fine value (index into AWE_NRPN_GENERATOR_MAPPINGS)
    /// `data_lsb`: data entry LSB (0-127)
    /// `data_msb`: data entry MSB (0-127)
    ///
    /// Equivalent to: handleAWE32NRPN(aweGen, dataLSB, dataMSB)
    pub fn handle_awe32_nrpn(
        &mut self,
        awe_gen: usize,
        data_lsb: u8,
        data_msb: u8,
        voices: &mut [Voice],
    ) {
        let mut data_value = ((data_msb as i32) << 7) | (data_lsb as i32);
        // Center the value (reported range 0..127 uses only LSB; full 14-bit range centered at 8192)
        data_value -= 8192;

        let Some(&generator) = AWE_NRPN_GENERATOR_MAPPINGS.get(awe_gen) else {
            spessa_synth_warn(&format!("Invalid AWE32 LSB: {}", awe_gen));
            return;
        };

        match generator {
            // Delays
            gt::DELAY_MOD_LFO | gt::DELAY_VIB_LFO | gt::DELAY_VOL_ENV | gt::DELAY_MOD_ENV => {
                let ms = 4.0 * clip(data_value as f64, 0.0, 5900.0);
                self.set_generator_override(generator, ms_to_timecents(ms), false, voices);
            }

            // Attacks
            gt::ATTACK_VOL_ENV | gt::ATTACK_MOD_ENV => {
                let ms = clip(data_value as f64, 0.0, 5940.0);
                self.set_generator_override(generator, ms_to_timecents(ms), false, voices);
            }

            // Holds
            gt::HOLD_VOL_ENV | gt::HOLD_MOD_ENV => {
                let ms = clip(data_value as f64, 0.0, 8191.0);
                self.set_generator_override(generator, ms_to_timecents(ms), false, voices);
            }

            // Decays and releases
            gt::DECAY_MOD_ENV | gt::DECAY_VOL_ENV | gt::RELEASE_VOL_ENV | gt::RELEASE_MOD_ENV => {
                let ms = 4.0 * clip(data_value as f64, 0.0, 5940.0);
                self.set_generator_override(generator, ms_to_timecents(ms), false, voices);
            }

            // LFO frequencies (realtime)
            gt::FREQ_VIB_LFO | gt::FREQ_MOD_LFO => {
                let hz = 0.084 * data_lsb as f64;
                self.set_generator_override(generator, hz_to_cents(hz), true, voices);
            }

            // Sustains
            gt::SUSTAIN_VOL_ENV | gt::SUSTAIN_MOD_ENV => {
                let centibels = (data_lsb as f64 * 7.5) as i16;
                self.set_generator_override(generator, centibels, false, voices);
            }

            // Pitch fine tune (realtime)
            gt::FINE_TUNE => {
                self.set_generator_override(generator, data_value as i16, true, voices);
            }

            // LFO to pitch (realtime)
            gt::MOD_LFO_TO_PITCH | gt::VIB_LFO_TO_PITCH => {
                let cents = (clip(data_value as f64, -127.0, 127.0) * 9.375) as i16;
                self.set_generator_override(generator, cents, true, voices);
            }

            // Envelope to pitch
            gt::MOD_ENV_TO_PITCH => {
                let cents = (clip(data_value as f64, -127.0, 127.0) * 9.375) as i16;
                self.set_generator_override(generator, cents, false, voices);
            }

            // Mod LFO to volume (realtime)
            gt::MOD_LFO_TO_VOLUME => {
                let centibels = (1.875 * data_lsb as f64) as i16;
                self.set_generator_override(generator, centibels, true, voices);
            }

            // Filter cutoff (realtime)
            gt::INITIAL_FILTER_FC => {
                let fc_cents = (4335 + 59 * data_lsb as i32) as i16;
                self.set_generator_override(generator, fc_cents, true, voices);
            }

            // Filter Q (realtime)
            gt::INITIAL_FILTER_Q => {
                let centibels = (215.0 * (data_lsb as f64 / 127.0)) as i16;
                self.set_generator_override(generator, centibels, true, voices);
            }

            // Mod LFO to filter Fc (realtime)
            gt::MOD_LFO_TO_FILTER_FC => {
                let cents = (clip(data_value as f64, -64.0, 63.0) * 56.25) as i16;
                self.set_generator_override(generator, cents, true, voices);
            }

            // Mod env to filter Fc
            gt::MOD_ENV_TO_FILTER_FC => {
                let cents = (clip(data_value as f64, -64.0, 63.0) * 56.25) as i16;
                self.set_generator_override(generator, cents, false, voices);
            }

            // Effects
            gt::CHORUS_EFFECTS_SEND | gt::REVERB_EFFECTS_SEND => {
                let val = (clip(data_value as f64, 0.0, 255.0) * (1000.0 / 255.0)) as i16;
                self.set_generator_override(generator, val, false, voices);
            }

            _ => {
                // Should not happen
            }
        }
    }
}
