//! The GS and XG controller matrix of one channel.
//! Ported from: src/synthesizer/audio_engine/channel/controller_matrix.ts
use crate::soundbank::basic_soundbank::modulator_curve::SOURCE_VALUE_14_BIT;
use crate::midi::enums::midi_controllers;
use crate::synthesizer::audio_engine::channel::enums::{
    MatrixDestination, MatrixSource, matrix_destinations as d, matrix_sources,
};

const SOURCE_COUNT: usize = 6;
/// Equivalent to: MATRIX_DESTINATION_COUNT
pub const MATRIX_DESTINATION_COUNT: usize = 11;
/// The output row of the whole channel. Rows 0 to 127 are the per-key rows for poly pressure.
/// Equivalent to: MATRIX_CHANNEL_ROW
pub const MATRIX_CHANNEL_ROW: usize = 128 * MATRIX_DESTINATION_COUNT;

/// Centered destinations rest at 64, the LFO depths at 0.
/// Equivalent to: NEUTRAL
const NEUTRAL: [u8; MATRIX_DESTINATION_COUNT] = {
    let mut n = [0u8; MATRIX_DESTINATION_COUNT];
    n[d::PITCH] = 64;
    n[d::CUTOFF] = 64;
    n[d::AMPLITUDE] = 64;
    n[d::LFO1_RATE] = 64;
    n[d::LFO2_RATE] = 64;
    n
};

/// The full range of each destination, from the SC-88Pro and XG manuals.
/// Centered cells scale (value - 64) / 64 onto it (pitch is in semitone steps instead),
/// the LFO depths scale value / 127 onto it.
/// Pitch, cutoff and the LFO pitch and filter depths are cents, the rates are Hz,
/// the amplitude and the LFO amplitude depths are fractions (100 % = 1).
/// Equivalent to: RANGE
const RANGE: [f64; MATRIX_DESTINATION_COUNT] = {
    let mut r = [0.0f64; MATRIX_DESTINATION_COUNT];
    r[d::CUTOFF] = 9600.0;
    r[d::AMPLITUDE] = 1.0;
    r[d::LFO1_RATE] = 10.0;
    r[d::LFO2_RATE] = 10.0;
    r[d::LFO1_PITCH] = 600.0;
    r[d::LFO2_PITCH] = 600.0;
    r[d::LFO1_FILTER] = 2400.0;
    r[d::LFO2_FILTER] = 2400.0;
    r[d::LFO1_AMPLITUDE] = 1.0;
    r[d::LFO2_AMPLITUDE] = 1.0;
    r
};

/// The rules of a matrix: GS applies amplitude linearly, XG in decibels.
/// Equivalent to: "gs" | "xg"
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MatrixSystem {
    #[default]
    Gs,
    Xg,
}

/// The state of a channel's GS and XG controller matrix, as stored in a channel snapshot.
/// Equivalent to: ControllerMatrixSnapshot
#[derive(Clone, Debug, PartialEq)]
pub struct ControllerMatrixSnapshot {
    /// The system of the last written cell. GS applies amplitude linearly, XG in decibels.
    pub system: MatrixSystem,
    /// The raw depth bytes, source * destination.
    pub depths: Vec<u8>,
}

/// The GS and XG controller matrix of one channel.
/// Sources (mod wheel, pitch wheel, pressures and two assignable controllers)
/// drive pitch, cutoff, amplitude and the LFOs with depths set by System Exclusive.
/// Equivalent to: class ControllerMatrix
#[derive(Clone, Debug)]
pub struct ControllerMatrix {
    /// The rules of the last written cell. GS applies amplitude linearly, XG in decibels.
    pub system: MatrixSystem,
    /// True when any cell differs from its neutral value.
    pub active: bool,
    /// True when the poly pressure row is not neutral, the per-key rows are then used.
    pub poly_pressure_active: bool,
    /// True when every per-key row is computed: the channel is in per-note pitch mode and a pitch wheel cell is set.
    pub per_note_rows: bool,
    /// Any LFO1 depth cell is set, so the vibrato LFO has to run even without depths of its own.
    pub lfo1_active: bool,
    /// Any LFO2 depth cell is set, so the modulation LFO has to run even without depths of its own.
    pub lfo2_active: bool,
    /// The raw depth bytes, source * destination.
    pub depths: [u8; SOURCE_COUNT * MATRIX_DESTINATION_COUNT],
    /// The outputs: 128 per-key rows and the channel row, each with one value per destination,
    /// in the units of RANGE.
    pub outputs: Vec<f64>,
    // Source values from 0 to 1 (the pitch wheel from -1 to 1), reused between the rows
    values: [f64; SOURCE_COUNT],
    // The pitch wheel row is not neutral
    pitch_wheel_active: bool,
    // CC 76 (vibrato rate) on the LFO1 rate, in Hz. It only counts while the matrix is on
    vibrato_rate: f64,
}

impl Default for ControllerMatrix {
    fn default() -> Self {
        Self::new()
    }
}

impl ControllerMatrix {
    /// Equivalent to: constructor()
    pub fn new() -> Self {
        let mut m = Self {
            system: MatrixSystem::Gs,
            active: false,
            poly_pressure_active: false,
            per_note_rows: false,
            lfo1_active: false,
            lfo2_active: false,
            depths: [0; SOURCE_COUNT * MATRIX_DESTINATION_COUNT],
            outputs: vec![0.0; MATRIX_CHANNEL_ROW + MATRIX_DESTINATION_COUNT],
            values: [0.0; SOURCE_COUNT],
            pitch_wheel_active: false,
            vibrato_rate: 0.0,
        };
        m.reset();
        m
    }

    /// Gets a snapshot of this controller matrix.
    /// Equivalent to: getSnapshot()
    pub fn get_snapshot(&self) -> ControllerMatrixSnapshot {
        ControllerMatrixSnapshot {
            system: self.system,
            depths: self.depths.to_vec(),
        }
    }

    /// Applies a snapshot to the controller matrix.
    /// Equivalent to: applySnapshot(snapshot)
    pub fn apply_snapshot(&mut self, snapshot: &ControllerMatrixSnapshot) {
        self.load(snapshot.system, &snapshot.depths);
    }

    /// Clears the table to neutral and turns the matrix off.
    /// Equivalent to: reset()
    pub fn reset(&mut self) {
        for s in 0..SOURCE_COUNT {
            self.depths[s * MATRIX_DESTINATION_COUNT..(s + 1) * MATRIX_DESTINATION_COUNT]
                .copy_from_slice(&NEUTRAL);
        }
        self.outputs.fill(0.0);
        self.system = MatrixSystem::Gs;
        self.active = false;
        self.poly_pressure_active = false;
        self.pitch_wheel_active = false;
        self.per_note_rows = false;
        self.lfo1_active = false;
        self.lfo2_active = false;
    }

    /// Writes one cell. Switching between GS and XG rules clears the table first.
    /// The caller recomputes.
    /// Equivalent to: setCell(rules, source, destination, value)
    pub fn set_cell(
        &mut self,
        rules: MatrixSystem,
        source: MatrixSource,
        destination: MatrixDestination,
        mut value: u8,
    ) {
        if rules != self.system {
            self.reset();
            self.system = rules;
        }
        // Pitch control goes from -24 to +24 semitones on both
        if destination == d::PITCH {
            value = value.clamp(0x28, 0x58);
        }
        self.depths[source * MATRIX_DESTINATION_COUNT + destination] = value;
        self.update_flags();
    }

    /// Restores the whole table (from a snapshot). The caller recomputes.
    /// Equivalent to: load(rules, depths)
    pub fn load(&mut self, rules: MatrixSystem, depths: &[u8]) {
        self.reset();
        self.system = rules;
        let n = depths.len().min(self.depths.len());
        self.depths[..n].copy_from_slice(&depths[..n]);
        self.update_flags();
    }

    /// Recomputes the outputs from the current source values.
    /// `controllers`: the channel's controller table (14-bit values),
    /// `pitch_wheel`, `pressure`, `cc1`, `cc2`: the channel's MIDI parameters,
    /// `poly_pressures`: the per-key pressures,
    /// `pitch_wheels`: the per-key pitch wheels when the channel is in per-note pitch mode.
    /// Equivalent to: recompute(controllers, params, polyPressures, pitchWheels)
    #[allow(clippy::too_many_arguments)]
    pub fn recompute(
        &mut self,
        controllers: &[i16],
        pitch_wheel: i32,
        pressure: i32,
        cc1: usize,
        cc2: usize,
        poly_pressures: &[u8],
        pitch_wheels: Option<&[i16]>,
    ) {
        if !self.active {
            return;
        }
        let v = &mut self.values;
        v[matrix_sources::MODULATION_WHEEL] = (controllers[1] >> 7) as f64 / 127.0;
        // The pitch wheel maps like an SFe source (center-max), so 16383 is exactly +1
        v[matrix_sources::PITCH_WHEEL] =
            (SOURCE_VALUE_14_BIT[pitch_wheel as usize] as f64 - 8192.0) / 8192.0;
        v[matrix_sources::CHANNEL_PRESSURE] = pressure as f64 / 127.0;
        v[matrix_sources::POLY_PRESSURE] = 0.0;
        v[matrix_sources::CC1] = (controllers[cc1] >> 7) as f64 / 127.0;
        v[matrix_sources::CC2] = (controllers[cc2] >> 7) as f64 / 127.0;
        // Up to 10 Hz either way, for special cases such as J-Cycle.mid
        self.vibrato_rate = ((controllers[midi_controllers::VIBRATO_RATE as usize] as f64 - 8192.0)
            / 8192.0)
            * RANGE[d::LFO1_RATE];
        self.compute_row(MATRIX_CHANNEL_ROW);
        // Every key has its own pitch wheel in the per-note mode
        let per_note = pitch_wheels.is_some() && self.pitch_wheel_active;
        self.per_note_rows = per_note;
        if !per_note && !self.poly_pressure_active {
            return;
        }
        for key in 0..128 {
            let p = poly_pressures[key];
            if let (true, Some(pw)) = (per_note, pitch_wheels) {
                self.values[matrix_sources::PITCH_WHEEL] =
                    (SOURCE_VALUE_14_BIT[pw[key] as usize] as f64 - 8192.0) / 8192.0;
            } else if p == 0 {
                continue;
            }
            self.values[matrix_sources::POLY_PRESSURE] = p as f64 / 127.0;
            self.compute_row(key * MATRIX_DESTINATION_COUNT);
        }
    }

    /// Equivalent to: updateFlags() (private)
    fn update_flags(&mut self) {
        let dp = &self.depths;
        let mut active = false;
        let mut paf = false;
        let mut pitch_wheel = false;
        let mut lfo1 = false;
        let mut lfo2 = false;
        for s in 0..SOURCE_COUNT {
            let row = s * MATRIX_DESTINATION_COUNT;
            for dest in 0..MATRIX_DESTINATION_COUNT {
                if dp[row + dest] == NEUTRAL[dest] {
                    continue;
                }
                active = true;
                if s == matrix_sources::POLY_PRESSURE {
                    paf = true;
                }
                if s == matrix_sources::PITCH_WHEEL {
                    pitch_wheel = true;
                }
                if (d::LFO1_PITCH..=d::LFO1_AMPLITUDE).contains(&dest) {
                    lfo1 = true;
                } else if dest >= d::LFO2_PITCH {
                    lfo2 = true;
                }
            }
        }
        self.active = active;
        self.poly_pressure_active = paf;
        self.pitch_wheel_active = pitch_wheel;
        self.lfo1_active = lfo1;
        self.lfo2_active = lfo2;
        if !active {
            self.outputs.fill(0.0);
        }
    }

    /// Equivalent to: computeRow(offset) (private)
    fn compute_row(&mut self, offset: usize) {
        for dest in 0..MATRIX_DESTINATION_COUNT {
            let neutral = NEUTRAL[dest];
            let mut sum = 0.0;
            for s in 0..SOURCE_COUNT {
                let dv = self.depths[s * MATRIX_DESTINATION_COUNT + dest];
                if dv == neutral {
                    continue;
                }
                let scale = if dest == d::PITCH {
                    // One semitone per step
                    (dv as f64 - 64.0) * 100.0
                } else if neutral == 64 {
                    ((dv as f64 - 64.0) / 64.0) * RANGE[dest]
                } else {
                    (dv as f64 / 127.0) * RANGE[dest]
                };
                sum += scale * self.values[s];
            }
            self.outputs[offset + dest] = sum;
        }
        self.outputs[offset + d::LFO1_RATE] += self.vibrato_rate;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_matrix_is_inactive() {
        let m = ControllerMatrix::new();
        assert!(!m.active);
        assert_eq!(m.depths[d::PITCH], 64);
        assert_eq!(m.depths[d::LFO1_PITCH], 0);
    }

    #[test]
    fn mod_wheel_to_lfo1_pitch() {
        let mut m = ControllerMatrix::new();
        m.set_cell(MatrixSystem::Gs, matrix_sources::MODULATION_WHEEL, d::LFO1_PITCH, 127);
        assert!(m.active && m.lfo1_active && !m.lfo2_active);
        let mut cc = vec![0i16; 128];
        cc[1] = 127 << 7;
        cc[midi_controllers::VIBRATO_RATE as usize] = 8192;
        m.recompute(&cc, 8192, 0, 16, 17, &[0; 128], None);
        assert!((m.outputs[MATRIX_CHANNEL_ROW + d::LFO1_PITCH] - 600.0).abs() < 1e-9);
    }

    #[test]
    fn pitch_cell_is_clamped_and_system_switch_resets() {
        let mut m = ControllerMatrix::new();
        m.set_cell(MatrixSystem::Gs, matrix_sources::CC1, d::PITCH, 0);
        assert_eq!(m.depths[matrix_sources::CC1 * MATRIX_DESTINATION_COUNT], 0x28);
        m.set_cell(MatrixSystem::Xg, matrix_sources::CC2, d::CUTOFF, 64);
        assert_eq!(m.system, MatrixSystem::Xg);
        assert!(!m.active);
    }
}
