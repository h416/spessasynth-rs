//! A delay line with whole, linear, band-limited (sinc), Lagrange and 32 kHz grid reads.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/delay_line.ts

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::LazyLock;

use super::kaiser::kaiser;
use crate::synthesizer::audio_engine::effects::gs::implementation::dsp::js_round;

/// The band-limited read: a Kaiser windowed sinc of 16 taps (beta 8), 7 of them newer than the delay.
/// Within 1.1e-4 of an exact fractional delay up to 0.28 fs (12.5 kHz at 44.1 kHz).
const SINC_TAPS: usize = 16;
const SINC_NEWER: usize = 7;
const SINC_PHASES: usize = 1024;

/// One row of taps per fraction (0 to 1 in `SINC_PHASES` steps), each row normalized to unity at DC.
static SINC_TABLE: LazyLock<Vec<f32>> = LazyLock::new(|| {
    let mut table = vec![0f32; (SINC_PHASES + 1) * SINC_TAPS];
    for phase in 0..=SINC_PHASES {
        let row = phase * SINC_TAPS;
        let mut sum = 0.0;
        for k in 0..SINC_TAPS {
            let t = k as f64 - SINC_NEWER as f64 - phase as f64 / SINC_PHASES as f64;
            let sinc = if t == 0.0 {
                1.0
            } else {
                (std::f64::consts::PI * t).sin() / (std::f64::consts::PI * t)
            };
            let v = sinc * kaiser(t / (SINC_TAPS as f64 / 2.0), 8.0);
            table[row + k] = v as f32;
            sum += v;
        }
        for k in 0..SINC_TAPS {
            table[row + k] = (table[row + k] as f64 / sum) as f32;
        }
    }
    table
});

/// A delay line in samples. Write one sample, then read any delay up to the length it was made with.
/// A delay of 0 reads the sample just written.
///
/// TS keeps a `Float32Array` that may be a view of the shared `DelayMemory`. Rust keeps the memory
/// and the line's offset in it, so a line of the shared memory sees the memory after it grows.
pub struct DelayLine {
    /// The samples (a power of two), at `offset` in `memory`.
    memory: Rc<RefCell<Vec<f32>>>,
    offset: usize,
    mask: usize,
    position: usize,
}

impl DelayLine {
    /// A line with its own memory.
    /// Equivalent to: new DelayLine(maxDelay)
    pub fn new(max_delay: f64) -> Self {
        let size = Self::size_for(max_delay);
        Self::in_memory(Rc::new(RefCell::new(vec![0.0; size])), 0, size)
    }

    /// A line at `offset` in a shared memory.
    /// Equivalent to: new DelayLine(maxDelay, buffer)
    pub(crate) fn in_memory(memory: Rc<RefCell<Vec<f32>>>, offset: usize, size: usize) -> Self {
        Self {
            memory,
            offset,
            mask: size - 1,
            position: 0,
        }
    }

    /// The buffer size a line needs for a delay, with room for the band-limited read's older taps.
    /// Equivalent to: DelayLine.sizeFor(maxDelay)
    pub fn size_for(max_delay: f64) -> usize {
        let mut size = 2usize;
        while (size as f64) < max_delay + SINC_TAPS as f64 - SINC_NEWER as f64 + 2.0 {
            size *= 2;
        }
        size
    }

    #[inline]
    fn at(&self, buffer: &[f32], index: isize) -> f64 {
        buffer[self.offset + (index as usize & self.mask)] as f64
    }

    #[inline]
    pub fn write(&mut self, x: f64) {
        self.position = (self.position + 1) & self.mask;
        self.memory.borrow_mut()[self.offset + self.position] = x as f32;
    }

    /// Reads a delay in whole samples.
    #[inline]
    pub fn read(&self, delay: isize) -> f64 {
        let buffer = self.memory.borrow();
        self.at(&buffer, self.position as isize - delay)
    }

    /// Reads between two samples, linearly interpolated.
    pub fn read_linear(&self, delay: f64) -> f64 {
        let buffer = self.memory.borrow();
        let whole = delay.floor();
        let w = whole as isize;
        let a = self.at(&buffer, self.position as isize - w);
        let b = self.at(&buffer, self.position as isize - w - 1);
        a + (b - a) * (delay - whole)
    }

    /// Reads between samples through the band-limited interpolator (whole delays read directly).
    /// Delays shorter than its newer taps fall back to the Lagrange read.
    pub fn read_sinc(&self, delay: f64) -> f64 {
        let whole = delay.floor();
        let fraction = delay - whole;
        let w = whole as isize;
        if fraction == 0.0 {
            let buffer = self.memory.borrow();
            return self.at(&buffer, self.position as isize - w);
        }
        if w < SINC_NEWER as isize {
            return self.read_lagrange(delay);
        }
        let row = js_round(fraction * SINC_PHASES as f64) as usize * SINC_TAPS;
        let newest = self.position as isize - w + SINC_NEWER as isize;
        let buffer = self.memory.borrow();
        let table = &*SINC_TABLE;
        let mut y = 0.0;
        for k in 0..SINC_TAPS {
            y += table[row + k] as f64 * self.at(&buffer, newest - k as isize);
        }
        y
    }

    /// Reads between samples through a 4-point Lagrange interpolator, which needs no newer samples (the short delays
    /// inside a loop). Whole delays read directly. A negative delay reads the newest sample.
    pub fn read_lagrange(&self, delay: f64) -> f64 {
        let buffer = self.memory.borrow();
        if delay <= 0.0 {
            return self.at(&buffer, self.position as isize);
        }
        let whole = delay.floor();
        let m = delay - whole;
        if m == 0.0 {
            return self.at(&buffer, self.position as isize - whole as isize);
        }
        // The points sit at whole - 1 to whole + 2, moved back until the first one is the newest sample
        let first = 0.0f64.max(whole - 1.0);
        let t = delay - first;
        let p = self.position as isize - first as isize;
        let x0 = self.at(&buffer, p);
        let x1 = self.at(&buffer, p - 1);
        let x2 = self.at(&buffer, p - 2);
        let x3 = self.at(&buffer, p - 3);
        (-x0 * (t - 1.0) * (t - 2.0) * (t - 3.0)) / 6.0 + (x1 * t * (t - 2.0) * (t - 3.0)) / 2.0
            - (x2 * t * (t - 1.0) * (t - 3.0)) / 2.0
            + (x3 * t * (t - 1.0) * (t - 2.0)) / 6.0
    }

    /// A read between whole 32 kHz samples, linear like the hardware's, each of the two points band-limited at the
    /// host rate. `delay` is in 32 kHz samples, `step` is one 32 kHz sample in host samples and `offset` the host
    /// samples taken off both points (1 for a read before the write).
    pub fn read_grid(&self, delay: f64, step: f64, offset: f64) -> f64 {
        let whole = delay.floor();
        let fraction = delay - whole;
        let a = self.read_sinc(0.0f64.max(whole * step - offset));
        if fraction == 0.0 {
            return a;
        }
        let b = self.read_sinc(0.0f64.max((whole + 1.0) * step - offset));
        a + (b - a) * fraction
    }

    pub fn clear(&mut self) {
        let size = self.mask + 1;
        self.memory.borrow_mut()[self.offset..self.offset + size].fill(0.0);
    }
}
