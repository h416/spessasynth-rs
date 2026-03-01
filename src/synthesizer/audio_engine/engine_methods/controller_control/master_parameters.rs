/// master_parameters.rs
/// purpose: Master parameter get/set handlers for SynthesizerCore.
/// Ported from: src/synthesizer/audio_engine/engine_methods/controller_control/master_parameters.ts
use crate::synthesizer::audio_engine::engine_components::voice::Voice;
use crate::synthesizer::audio_engine::synthesizer_core::SynthesizerCore;
use crate::synthesizer::types::{
    MasterParameterChangeCallback, MasterParameterType, SynthProcessorEvent,
};
use crate::utils::loggin::spessa_synth_warn;

impl SynthesizerCore {
    /// Sets a master parameter and fires a MasterParameterChange event.
    ///
    /// Equivalent to: setMasterParameterInternal(parameter, value)
    pub fn set_master_parameter(&mut self, change: MasterParameterChangeCallback) {
        match change {
            MasterParameterChangeCallback::MasterGain(v) => {
                self.master_parameters.master_gain = v;
            }

            MasterParameterChangeCallback::MasterPan(pan) => {
                self.master_parameters.master_pan = pan;
                // Convert from [-1, 1] to [0, 1] where 0 = left
                let p = pan / 2.0 + 0.5;
                self.pan_left = 1.0 - p;
                self.pan_right = p;
            }

            MasterParameterChangeCallback::VoiceCap(cap) => {
                // Infinity not allowed; clamp to 1_000_000
                let cap = cap.min(1_000_000);
                self.master_parameters.voice_cap = cap;
                let cap = cap as usize;
                if cap > self.voices.len() {
                    spessa_synth_warn(&format!(
                        "Allocating {} new voices!",
                        cap - self.voices.len()
                    ));
                    let sample_rate = self.sample_rate;
                    for _ in self.voices.len()..cap {
                        self.voices.push(Voice::new(sample_rate));
                    }
                }
            }

            MasterParameterChangeCallback::InterpolationType(t) => {
                self.master_parameters.interpolation_type = t;
            }

            MasterParameterChangeCallback::MidiSystem(sys) => {
                self.master_parameters.midi_system = sys;
            }

            MasterParameterChangeCallback::MonophonicRetriggerMode(v) => {
                self.master_parameters.monophonic_retrigger_mode = v;
            }

            MasterParameterChangeCallback::ReverbGain(v) => {
                self.master_parameters.reverb_gain = v;
            }

            MasterParameterChangeCallback::ChorusGain(v) => {
                self.master_parameters.chorus_gain = v;
            }

            MasterParameterChangeCallback::BlackMidiMode(v) => {
                self.master_parameters.black_midi_mode = v;
            }

            MasterParameterChangeCallback::Transposition(semitones) => {
                // Temporarily reset so that transposeChannel computes relative to 0
                self.master_parameters.transposition = 0.0;
                let current_time = self.current_time;
                let enable_event_system = self.enable_event_system;
                let voices = &mut self.voices;
                let mut events = Vec::new();
                for ch in self.midi_channels.iter_mut() {
                    if let Some(ev) = ch.transpose_channel(
                        semitones,
                        false,
                        0.0,
                        voices,
                        current_time,
                        enable_event_system,
                    ) {
                        events.push(ev);
                    }
                }
                self.master_parameters.transposition = semitones;
                for ev in events {
                    self.call_event(ev);
                }
            }

            MasterParameterChangeCallback::DeviceId(id) => {
                self.master_parameters.device_id = id;
            }
        }

        self.call_event(SynthProcessorEvent::MasterParameterChange(change));
    }

    /// Returns the current value of all master parameters.
    ///
    /// Equivalent to: getAllMasterParametersInternal()
    pub fn get_all_master_parameters(&self) -> MasterParameterType {
        self.master_parameters.clone()
    }
}
