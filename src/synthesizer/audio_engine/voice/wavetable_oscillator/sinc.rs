//! Kaiser windowed sinc interpolation oscillator (32 points, or 128 points with antialiasing).
//! Ported from: src/synthesizer/audio_engine/voice/wavetable_oscillator/sinc.ts
//!
//! TS has two `SincOscillator` instances (`new SincOscillator(false)` for `sinc32`, `true` for `sinc128`);
//! Rust selects the kernel from `interpolation_type`.
use std::sync::LazyLock;

use super::wavetable_oscillator::WavetableOscillator;
use crate::synthesizer::enums::interpolation_types;

/// Kernel resolution, the amount of table points per sample of distance.
/// The table is linearly interpolated, which keeps the error around -120 dB.
const OVERSAMPLING: usize = 256;

/// The highest playback step the anti-aliased kernel gets stretched to.
/// The tap count grows with the step, so this keeps extreme pitch ups from becoming too slow.
/// Above it, the content between the new Nyquist frequency and 1/16 of the old one folds back.
const MAX_STRETCH: f64 = 16.0;

/// Equivalent to: interface SincKernel
pub(super) struct SincKernel {
    /// Half of the kernel width, in samples.
    half_width: usize,
    /// Polyphase coefficients, `taps` per phase, `OVERSAMPLING` phases.
    /// Each phase is normalized to unity gain at DC.
    phases: Vec<f32>,
    /// The difference between the next phase and this one, for interpolating between the phases.
    phase_deltas: Vec<f32>,
    /// The continuous kernel from -halfWidth to halfWidth (plus a zero guard point),
    /// used when the kernel is stretched for antialiasing.
    table: Vec<f32>,
    /// Differences between the neighboring table points.
    table_deltas: Vec<f32>,
}

/// Zeroth order modified Bessel function of the first kind, for the Kaiser window.
/// Equivalent to: besselI0(x)
fn bessel_i0(x: f64) -> f64 {
    let q = (x * x) / 4.0;
    let mut sum = 1.0;
    let mut term = 1.0;
    for k in 1..100 {
        let k = k as f64;
        term *= q / (k * k);
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

/// Builds a Kaiser windowed sinc kernel.
/// `half_width`: half of the kernel width, in samples. The kernel uses `2 * half_width` taps.
/// `bandwidth`: the cutoff (-6 dB point) of the kernel, relative to the Nyquist frequency.
/// `beta`: the Kaiser window beta, controls the stopband attenuation.
/// Equivalent to: buildKernel(halfWidth, bandwidth, beta)
fn build_kernel(half_width: usize, bandwidth: f64, beta: f64) -> SincKernel {
    let window_norm = bessel_i0(beta);
    let hw = half_width as f64;
    let kernel = |x: f64| -> f64 {
        let w = x / hw;
        if w <= -1.0 || w >= 1.0 {
            return 0.0;
        }
        let window = bessel_i0(beta * (1.0 - w * w).sqrt()) / window_norm;
        let u = std::f64::consts::PI * bandwidth * x;
        let sinc = if u == 0.0 { 1.0 } else { u.sin() / u };
        bandwidth * sinc * window
    };

    // The polyphase table
    // Tap k of phase q is for the point (k + 1 - halfWidth) after the cursor,
    // When the cursor is q / OVERSAMPLING past the point before it
    let taps = half_width * 2;
    let mut rows = vec![0.0f64; taps * (OVERSAMPLING + 1)];
    for q in 0..=OVERSAMPLING {
        let row = q * taps;
        let mut sum = 0.0;
        for k in 0..taps {
            let v = kernel(k as f64 + 1.0 - hw - q as f64 / OVERSAMPLING as f64);
            rows[row + k] = v;
            sum += v;
        }
        for k in 0..taps {
            rows[row + k] /= sum;
        }
    }
    let phase_count = taps * OVERSAMPLING;
    let mut phases = vec![0.0f32; phase_count];
    let mut phase_deltas = vec![0.0f32; phase_count];
    for i in 0..phase_count {
        phases[i] = rows[i] as f32;
        phase_deltas[i] = (rows[i + taps] - rows[i]) as f32;
    }

    // The continuous table
    let table_length = half_width * 2 * OVERSAMPLING + 2;
    let mut table = vec![0.0f32; table_length];
    let mut table_deltas = vec![0.0f32; table_length];
    for (i, t) in table.iter_mut().enumerate() {
        *t = kernel(i as f64 / OVERSAMPLING as f64 - hw) as f32;
    }
    for i in 0..table_length - 1 {
        // TS: tableDeltas[i] = table[i + 1] - table[i] (Float32Array elements, f64 subtraction)
        table_deltas[i] = (table[i + 1] as f64 - table[i] as f64) as f32;
    }

    SincKernel { half_width, phases, phase_deltas, table, table_deltas }
}

/// 32 points, meant for real time playback.
/// Equivalent to: KERNEL_FAST
static KERNEL_FAST: LazyLock<SincKernel> = LazyLock::new(|| build_kernel(16, 0.92, 9.0));
/// 128 points, for offline rendering.
/// Equivalent to: KERNEL_BEST
static KERNEL_BEST: LazyLock<SincKernel> = LazyLock::new(|| build_kernel(64, 0.975, 12.0));

/// The scratch size of an oscillator: the points around the cursor, gathered when they need wrapping
/// around the loop or are outside the sample.
/// The TS `SincOscillator` constructor allocates its `scratch` with this size.
fn scratch_size(best: bool) -> usize {
    let half_width = if best { KERNEL_BEST.half_width } else { KERNEL_FAST.half_width };
    if best {
        (half_width as f64 * MAX_STRETCH).ceil() as usize * 2 + 2
    } else {
        half_width * 2
    }
}

impl WavetableOscillator {
    /// Equivalent to: SincOscillator.process (`best` = `interpolation_type == SINC128`)
    pub(super) fn process_sinc(
        &mut self,
        sample_count: usize,
        tuning_ratio: f64,
        output_buffer: &mut [f64],
    ) -> bool {
        // If true, uses the larger, anti-aliased kernel (antiAlias)
        let best = self.interpolation_type == interpolation_types::SINC128;
        let kernel: &SincKernel = if best { &KERNEL_BEST } else { &KERNEL_FAST };
        let size = scratch_size(best);
        if self.scratch.len() != size {
            self.scratch = vec![0.0; size];
        }

        let step = tuning_ratio * self.playback_step;
        let loop_start = self.loop_start;
        let loop_end = self.loop_end;
        let loop_length = self.loop_length;
        let end = self.end;
        let is_looping = self.is_looping;
        let half_width = kernel.half_width;
        let mut cursor = self.cursor;
        let mut has_looped = self.has_looped;

        // Points at or above this one have to be gathered (wrapped or silent)
        // The lower limit is checked in the loop since it changes after the first loop
        let upper_limit = if is_looping { loop_end } else { end };

        let stretch = if best && step > 1.0 { step.min(MAX_STRETCH) } else { 1.0 };
        if stretch == 1.0 {
            // Polyphase kernel, all taps share the same phase
            let phases = &kernel.phases;
            let phase_deltas = &kernel.phase_deltas;
            let taps = half_width * 2;
            for i in 0..sample_count {
                if is_looping {
                    // Sample can loop more than once per frame! (that's why this 'while' is here)
                    // Testcase: saw_and_square_wave.sf2
                    if cursor >= loop_end {
                        has_looped = true;
                        while cursor >= loop_end {
                            cursor -= loop_length;
                        }
                    }
                } else if cursor + 1.0 >= end {
                    // Same end condition as the other interpolators (the point after the cursor is missing)
                    // Fill the rest with zeros (testcase drum_spam_test.mid)
                    output_buffer[i..sample_count].fill(0.0);
                    self.cursor = cursor;
                    return false;
                }

                // TS: cursor | 0
                let y0 = cursor as i64;
                let mut first = y0 + 1 - half_width as i64;
                let lower_limit = if is_looping && has_looped { loop_start } else { 0.0 };
                let gathered = (first as f64) < lower_limit || (first + taps as i64) as f64 > upper_limit;
                if gathered {
                    self.gather(first, taps, has_looped);
                    first = 0;
                }
                let source: &[f32] = if gathered {
                    &self.scratch
                } else {
                    self.sample_data.as_ref().unwrap()
                };

                let phase = (cursor - y0 as f64) * OVERSAMPLING as f64;
                // TS: phase | 0
                let q = phase as usize;

                let mut row = q * taps;
                let fraction = phase - q as f64;
                let row_end = row + taps;
                let mut first = first as usize;
                let mut out = 0.0f64;
                while row < row_end {
                    out += source[first] as f64
                        * (phases[row] as f64 + phase_deltas[row] as f64 * fraction);
                    first += 1;
                    row += 1;
                }
                output_buffer[i] = out;
                cursor += step;
            }
        } else {
            // Stretched kernel, every tap has its own position in the table
            let table = &kernel.table;
            let table_deltas = &kernel.table_deltas;
            let reach = half_width as f64 * stretch;
            let table_step = OVERSAMPLING as f64 / stretch;
            let center = (half_width * OVERSAMPLING) as f64;
            let gain = 1.0 / stretch;
            for i in 0..sample_count {
                if is_looping {
                    if cursor >= loop_end {
                        has_looped = true;
                        while cursor >= loop_end {
                            cursor -= loop_length;
                        }
                    }
                } else if cursor + 1.0 >= end {
                    output_buffer[i..sample_count].fill(0.0);
                    self.cursor = cursor;
                    return false;
                }

                // The points strictly inside the kernel's reach
                let mut first = (cursor - reach).floor() as i64 + 1;
                let taps = ((cursor + reach).floor() as i64 - first + 1) as usize;
                let mut position = (first as f64 - cursor) * table_step + center;
                let lower_limit = if is_looping && has_looped { loop_start } else { 0.0 };
                let gathered = (first as f64) < lower_limit || (first + taps as i64) as f64 > upper_limit;
                if gathered {
                    self.gather(first, taps, has_looped);
                    first = 0;
                }
                let source: &[f32] = if gathered {
                    &self.scratch
                } else {
                    self.sample_data.as_ref().unwrap()
                };

                let mut out = 0.0f64;
                let mut first = first as usize;
                let last = first + taps;
                while first < last {
                    // TS: position | 0
                    let index = position as usize;
                    out += source[first] as f64
                        * (table[index] as f64 + table_deltas[index] as f64 * (position - index as f64));
                    position += table_step;
                    first += 1;
                }
                output_buffer[i] = out * gain;

                cursor += step;
            }
        }
        self.cursor = cursor;
        self.has_looped = has_looped;
        true
    }

    /// Copies the points into the scratch buffer, wrapping them around the loop.
    /// Points outside the sample are silent.
    /// `first`: the index of the first point. `count`: the amount of points.
    /// `has_looped`: if the cursor has already looped.
    /// Equivalent to: SincOscillator.gather(first, count, hasLooped) (private)
    fn gather(&mut self, first: i64, count: usize, has_looped: bool) {
        let sample_data = self.sample_data.as_ref().unwrap();
        let scratch = &mut self.scratch;
        let loop_start = self.loop_start;
        let loop_end = self.loop_end;
        let loop_length = self.loop_length;
        let end = self.end;
        if self.is_looping {
            // Bring the first point into the loop, the rest just wrap at the loop end
            // Modulo instead of a single wrap: the loop may be shorter than the kernel
            // Testcase: OmegaGMGS2.sf2 drum kit,
            // Crash Cymbal (loop length 1, shortest valid)
            let mut index = first as f64;
            if index >= loop_end {
                index = loop_start + ((index - loop_start) % loop_length);
            } else if index < loop_start && has_looped {
                // After looping, the points before the loop start are the end of the loop
                index = loop_end - 1.0 - ((loop_start - 1.0 - index) % loop_length);
            }
            for s in scratch.iter_mut().take(count) {
                // Before looping, there is no data before the sample start
                *s = if index < 0.0 { 0.0 } else { sample_data[index as usize] };
                index += 1.0;
                if index >= loop_end {
                    index = loop_start;
                }
            }
        } else {
            for (k, s) in scratch.iter_mut().take(count).enumerate() {
                let index = first + k as i64;
                *s = if index >= 0 && (index as f64) < end { sample_data[index as usize] } else { 0.0 };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn osc(kind: u8, data: Vec<f32>) -> WavetableOscillator {
        let mut osc = WavetableOscillator::new(kind);
        osc.end = data.len() as f64;
        osc.sample_data = Some(data);
        osc.playback_step = 1.0;
        osc
    }

    #[test]
    fn sinc_phases_have_unity_gain() {
        for kernel in [&*KERNEL_FAST, &*KERNEL_BEST] {
            let taps = kernel.half_width * 2;
            for q in [0, 1, 100, OVERSAMPLING - 1] {
                let sum: f64 = kernel.phases[q * taps..(q + 1) * taps].iter().map(|&v| v as f64).sum();
                assert!((sum - 1.0).abs() < 1e-5, "phase {q}: {sum}");
            }
        }
    }

    #[test]
    fn sinc_integer_cursor_returns_dc() {
        for kind in [interpolation_types::SINC32, interpolation_types::SINC128] {
            let mut osc = osc(kind, vec![0.5f32; 400]);
            osc.cursor = 200.0;
            let mut buf = vec![0.0f64; 4];
            assert!(osc.process(4, 1.0, &mut buf));
            for v in buf {
                assert!((v - 0.5).abs() < 1e-5, "{v}");
            }
        }
    }

    #[test]
    fn sinc_end_fills_zeros() {
        let mut osc = osc(interpolation_types::SINC32, vec![1.0f32; 8]);
        osc.cursor = 6.5;
        let mut buf = vec![1.0f64; 4];
        assert!(!osc.process(4, 1.0, &mut buf));
        assert_eq!(buf[1..], [0.0, 0.0, 0.0]);
    }

    #[test]
    fn sinc128_stretched_loop_is_finite() {
        let data: Vec<f32> = (0..64).map(|i| ((i as f32) * 0.3).sin()).collect();
        let mut osc = osc(interpolation_types::SINC128, data);
        osc.is_looping = true;
        osc.loop_start = 10.0;
        osc.loop_end = 11.0;
        osc.loop_length = 1.0;
        osc.playback_step = 5.0;
        let mut buf = vec![0.0f64; 128];
        assert!(osc.process(128, 1.0, &mut buf));
        assert!(buf.iter().all(|v| v.is_finite()));
        assert!(osc.has_looped);
    }
}
