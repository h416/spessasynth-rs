/// parameter_tracker.rs
/// purpose: Tracks RPN/NRPN selection (MSB/LSB) and data-entry (MSB/LSB) MIDI Controller Change
///          messages for a single MIDI channel, exposing the decoded parameter via
///          `MidiUtils::analyze_rpn`/`analyze_nrpn` whenever a data-entry value is received.
/// Ported from: src/midi/midi_tools/parameter_tracker.ts (spessasynth_core 4.3.0)
///
/// New in TS 4.3.0 (no previous version to diff against). Used by `modify_midi.rs` and
/// `used_programs_and_keys.rs` to recognize RPN Coarse/Fine Tuning and the handful of GS/XG
/// NRPN "part parameters" (filter cutoff/resonance, envelope attack/decay/release) and NRPN
/// drum-setup messages, replacing ad-hoc byte inspection.
use crate::midi::enums::midi_controllers;
use crate::midi::midi_tools::midi_utils::{AnalyzedMidiMessage, MidiUtils};
use crate::synthesizer::audio_engine::synth_constants::{DEFAULT_NRPN, DEFAULT_RPN};

// ─────────────────────────────────────────────────────────────────────────────
// ParameterController
// ─────────────────────────────────────────────────────────────────────────────

/// A single tracked RPN/NRPN component (MSB, LSB, or a data-entry byte).
///
/// Besides the 7-bit MIDI value, this remembers *where* (track/event index) the underlying
/// Controller Change event lives, so callers (`modify_midi.rs`'s `deleteParameter`) can locate
/// and delete it later even though it may be on a different track than the event currently being
/// processed.
///
/// Equivalent to: interface ParameterController
#[derive(Clone, Copy, Debug, Default)]
pub struct ParameterController {
    /// The 7-bit MIDI value.
    pub v: u8,
    /// Track index of the MIDI Controller Change event that set this value.
    pub track: usize,
    /// Event index (within that track) of the MIDI Controller Change event that set this value.
    pub event: usize,
}

// ─────────────────────────────────────────────────────────────────────────────
// ParameterTracker
// ─────────────────────────────────────────────────────────────────────────────

/// A class for tracking RPN/NRPN messages.
/// Equivalent to: class ParameterTracker
pub struct ParameterTracker {
    /// Equivalent to: public rpnMSB
    pub rpn_msb: ParameterController,
    /// Equivalent to: public rpnLSB
    pub rpn_lsb: ParameterController,
    /// Equivalent to: public nrpnMSB
    pub nrpn_msb: ParameterController,
    /// Equivalent to: public nrpnLSB
    pub nrpn_lsb: ParameterController,
    /// Equivalent to: public dataMSB
    pub data_msb: ParameterController,
    /// Equivalent to: public dataLSB
    pub data_lsb: ParameterController,

    /// Equivalent to: private readonly channel
    channel: u8,
    /// Equivalent to: private isRegistered = true
    is_registered: bool,
}

impl ParameterTracker {
    /// Equivalent to: public constructor(channel: number)
    pub fn new(channel: u8) -> Self {
        Self {
            rpn_msb: ParameterController {
                v: DEFAULT_RPN,
                track: 0,
                event: 0,
            },
            rpn_lsb: ParameterController {
                v: DEFAULT_RPN,
                track: 0,
                event: 0,
            },
            nrpn_msb: ParameterController {
                v: DEFAULT_NRPN,
                track: 0,
                event: 0,
            },
            nrpn_lsb: ParameterController {
                v: DEFAULT_NRPN,
                track: 0,
                event: 0,
            },
            data_msb: ParameterController::default(),
            data_lsb: ParameterController::default(),
            channel,
            is_registered: true,
        }
    }

    /// The currently selected parameter number's MSB (RPN or NRPN, whichever was last selected).
    /// Equivalent to: public get paramMSB()
    pub fn param_msb(&self) -> ParameterController {
        if self.is_registered {
            self.rpn_msb
        } else {
            self.nrpn_msb
        }
    }

    /// The currently selected parameter number's LSB (RPN or NRPN, whichever was last selected).
    /// Equivalent to: public get paramLSB()
    pub fn param_lsb(&self) -> ParameterController {
        if self.is_registered {
            self.rpn_lsb
        } else {
            self.nrpn_lsb
        }
    }

    /// Mutable access to the currently-selected parameter's MSB and LSB together.
    ///
    /// TypeScript's `paramMSB`/`paramLSB` getters return references to the live tracked object,
    /// so callers (`modify_midi.rs`'s `deleteParameter`) can mutate `.event` on them in place
    /// after deleting an event located before the cached position. `ParameterController` is a
    /// plain `Copy` struct in Rust, so the value-returning `param_msb`/`param_lsb` above can't
    /// provide that; this method returns both as mutable references instead (as a pair, since
    /// `deleteParameter` needs to compare and adjust both together).
    pub fn param_msb_lsb_mut(&mut self) -> (&mut ParameterController, &mut ParameterController) {
        if self.is_registered {
            (&mut self.rpn_msb, &mut self.rpn_lsb)
        } else {
            (&mut self.nrpn_msb, &mut self.nrpn_lsb)
        }
    }

    /// Mutable access to the currently-selected parameter's LSB alone.
    /// See [`Self::param_msb_lsb_mut`] for why this exists alongside the value-returning
    /// `param_lsb`.
    pub fn param_lsb_mut(&mut self) -> &mut ParameterController {
        if self.is_registered {
            &mut self.rpn_lsb
        } else {
            &mut self.nrpn_lsb
        }
    }

    /// Resets RPN/NRPN selection and data entry to their default ("no parameter selected") state.
    /// Equivalent to: public reset()
    pub fn reset(&mut self) {
        self.is_registered = true;
        self.rpn_lsb.v = DEFAULT_RPN;
        self.rpn_msb.v = DEFAULT_RPN;
        self.nrpn_msb.v = DEFAULT_NRPN;
        self.nrpn_lsb.v = DEFAULT_NRPN;
        self.reset_data();
    }

    /// Feeds a single MIDI Controller Change (`cc`, `v`) into the tracker.
    ///
    /// `track`/`event` identify where this Controller Change event lives, cached for later
    /// lookup/deletion (see `ParameterController`).
    ///
    /// Returns `Some(analyzed)` only when `cc` is Data Entry MSB or LSB (i.e. a value was just
    /// written to the currently selected RPN/NRPN parameter); returns `None` otherwise (including
    /// for RPN/NRPN MSB/LSB selection messages, which only update internal state).
    ///
    /// Equivalent to: public controllerChange(cc, v, track, event)
    pub fn controller_change(
        &mut self,
        cc: u8,
        v: u8,
        track: usize,
        event: usize,
    ) -> Option<AnalyzedMidiMessage> {
        match cc {
            _ if cc == midi_controllers::REGISTERED_PARAMETER_MSB => {
                self.reset_data();
                self.is_registered = true;
                self.rpn_msb = ParameterController { v, track, event };
                None
            }

            _ if cc == midi_controllers::REGISTERED_PARAMETER_LSB => {
                self.reset_data();
                self.is_registered = true;
                self.rpn_lsb = ParameterController { v, track, event };
                None
            }

            _ if cc == midi_controllers::NON_REGISTERED_PARAMETER_MSB => {
                self.reset_data();
                self.is_registered = false;
                self.nrpn_msb = ParameterController { v, track, event };
                None
            }

            _ if cc == midi_controllers::NON_REGISTERED_PARAMETER_LSB => {
                self.reset_data();
                self.is_registered = false;
                self.nrpn_lsb = ParameterController { v, track, event };
                None
            }

            _ if cc == midi_controllers::DATA_ENTRY_MSB => {
                self.data_msb = ParameterController { v, track, event };
                Some(self.analyze())
            }

            _ if cc == midi_controllers::DATA_ENTRY_LSB => {
                self.data_lsb = ParameterController { v, track, event };
                Some(self.analyze())
            }

            _ => None,
        }
    }

    /// Equivalent to: private resetData()
    ///
    /// Called whenever the parameter number (MSB or LSB) changes because this is technically not
    /// MIDI 1.0 behavior, but some MIDI files only send the data MSB:
    /// <https://github.com/spessasus/spessasynth_core/pull/78#discussion_r3233413622>
    fn reset_data(&mut self) {
        self.data_lsb.v = 0;
        self.data_msb.v = 0;
    }

    /// Equivalent to: private analyze()
    fn analyze(&self) -> AnalyzedMidiMessage {
        let v = ((self.data_msb.v as u16) << 7) | self.data_lsb.v as u16;
        if self.is_registered {
            let rpn = ((self.rpn_msb.v as u16) << 7) | self.rpn_lsb.v as u16;
            MidiUtils::analyze_rpn(self.channel, rpn, v)
        } else {
            let nrpn = ((self.nrpn_msb.v as u16) << 7) | self.nrpn_lsb.v as u16;
            MidiUtils::analyze_nrpn(self.channel, nrpn, v)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::enums::registered_parameter_types;

    // ── constructor defaults ─────────────────────────────────────────────────

    #[test]
    fn test_new_defaults_to_registered() {
        let t = ParameterTracker::new(0);
        assert_eq!(t.param_msb().v, DEFAULT_RPN);
        assert_eq!(t.param_lsb().v, DEFAULT_RPN);
    }

    #[test]
    fn test_new_rpn_default_value() {
        let t = ParameterTracker::new(3);
        assert_eq!(t.rpn_msb.v, DEFAULT_RPN);
        assert_eq!(t.rpn_lsb.v, DEFAULT_RPN);
    }

    #[test]
    fn test_new_nrpn_default_value() {
        let t = ParameterTracker::new(3);
        assert_eq!(t.nrpn_msb.v, DEFAULT_NRPN);
        assert_eq!(t.nrpn_lsb.v, DEFAULT_NRPN);
    }

    // ── controller_change: RPN/NRPN selection ────────────────────────────────

    #[test]
    fn test_rpn_msb_selects_registered_and_returns_none() {
        let mut t = ParameterTracker::new(0);
        let r = t.controller_change(midi_controllers::REGISTERED_PARAMETER_MSB, 0, 1, 2);
        assert!(r.is_none());
        assert_eq!(t.param_msb().v, 0);
        assert_eq!(t.param_msb().track, 1);
        assert_eq!(t.param_msb().event, 2);
    }

    #[test]
    fn test_nrpn_msb_selects_unregistered() {
        let mut t = ParameterTracker::new(0);
        t.controller_change(midi_controllers::NON_REGISTERED_PARAMETER_MSB, 1, 0, 0);
        // param_msb should now report the NRPN MSB (1), not the RPN default.
        assert_eq!(t.param_msb().v, 1);
    }

    #[test]
    fn test_switching_rpn_to_nrpn_resets_data() {
        let mut t = ParameterTracker::new(0);
        t.controller_change(midi_controllers::REGISTERED_PARAMETER_MSB, 0, 0, 0);
        t.controller_change(midi_controllers::DATA_ENTRY_MSB, 65, 0, 1);
        // Now switch to NRPN: data should reset.
        t.controller_change(midi_controllers::NON_REGISTERED_PARAMETER_MSB, 1, 0, 2);
        assert_eq!(t.data_msb.v, 0);
        assert_eq!(t.data_lsb.v, 0);
    }

    // ── controller_change: data entry / analyze ──────────────────────────────

    #[test]
    fn test_data_entry_msb_triggers_analyze() {
        let mut t = ParameterTracker::new(2);
        // Select RPN Coarse Tuning (registeredParameterTypes.coarseTuning = 0x00_02)
        t.controller_change(midi_controllers::REGISTERED_PARAMETER_MSB, 0, 0, 0);
        t.controller_change(
            midi_controllers::REGISTERED_PARAMETER_LSB,
            registered_parameter_types::COARSE_TUNING as u8,
            0,
            1,
        );
        let result = t.controller_change(midi_controllers::DATA_ENTRY_MSB, 65, 0, 2);
        assert_eq!(
            result,
            Some(AnalyzedMidiMessage::KeyShift { channel: 2, value: 1 })
        );
    }

    #[test]
    fn test_data_entry_lsb_combines_with_msb() {
        let mut t = ParameterTracker::new(0);
        t.controller_change(midi_controllers::REGISTERED_PARAMETER_MSB, 0, 0, 0);
        t.controller_change(
            midi_controllers::REGISTERED_PARAMETER_LSB,
            registered_parameter_types::FINE_TUNING as u8,
            0,
            1,
        );
        t.controller_change(midi_controllers::DATA_ENTRY_MSB, 64, 0, 2); // MSB=64 -> v=8192 so far
        let result = t.controller_change(midi_controllers::DATA_ENTRY_LSB, 0, 0, 3);
        assert_eq!(
            result,
            Some(AnalyzedMidiMessage::FineTune { channel: 0, value: 0.0 })
        );
    }

    #[test]
    fn test_unrelated_controller_returns_none() {
        let mut t = ParameterTracker::new(0);
        assert!(t.controller_change(7, 100, 0, 0).is_none());
    }

    // ── reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_reset_restores_registered_and_defaults() {
        let mut t = ParameterTracker::new(0);
        t.controller_change(midi_controllers::NON_REGISTERED_PARAMETER_MSB, 5, 0, 0);
        t.controller_change(midi_controllers::DATA_ENTRY_MSB, 10, 0, 1);
        t.reset();
        assert_eq!(t.rpn_msb.v, DEFAULT_RPN);
        assert_eq!(t.rpn_lsb.v, DEFAULT_RPN);
        assert_eq!(t.nrpn_msb.v, DEFAULT_NRPN);
        assert_eq!(t.nrpn_lsb.v, DEFAULT_NRPN);
        assert_eq!(t.data_msb.v, 0);
        assert_eq!(t.data_lsb.v, 0);
        // Back to registered.
        assert_eq!(t.param_msb().v, DEFAULT_RPN);
    }

    // ── param_msb_lsb_mut / param_lsb_mut ────────────────────────────────────

    #[test]
    fn test_param_msb_lsb_mut_mutates_tracked_state() {
        let mut t = ParameterTracker::new(0);
        t.controller_change(midi_controllers::REGISTERED_PARAMETER_MSB, 0, 2, 5);
        t.controller_change(midi_controllers::REGISTERED_PARAMETER_LSB, 1, 2, 6);
        {
            let (msb, lsb) = t.param_msb_lsb_mut();
            msb.event -= 1;
            lsb.event -= 1;
        }
        assert_eq!(t.rpn_msb.event, 4);
        assert_eq!(t.rpn_lsb.event, 5);
    }

    #[test]
    fn test_param_lsb_mut_targets_nrpn_when_unregistered() {
        let mut t = ParameterTracker::new(0);
        t.controller_change(midi_controllers::NON_REGISTERED_PARAMETER_LSB, 7, 1, 1);
        t.param_lsb_mut().event = 99;
        assert_eq!(t.nrpn_lsb.event, 99);
        // RPN LSB (unrelated) must stay untouched.
        assert_eq!(t.rpn_lsb.event, 0);
    }
}
