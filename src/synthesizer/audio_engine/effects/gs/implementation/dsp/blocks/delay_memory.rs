//! The delay memory of the insertion block, shared by every EFX type.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/delay_memory.ts

use std::cell::RefCell;
use std::rc::Rc;

use super::delay_line::DelayLine;

/// The delay memory of the insertion block, shared by every EFX type.
///
/// Only one type runs at a time, so each type takes its lines from the start of the same buffer.
/// The buffer grows while the types are built (never while they run) to fit the hungriest one.
/// TS moves the lines handed out so far to the larger buffer; the Rust lines refer to the memory itself,
/// so growing it is enough.
pub struct DelayMemory {
    buffer: Rc<RefCell<Vec<f32>>>,
    cursor: usize,
}

impl Default for DelayMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl DelayMemory {
    pub fn new() -> Self {
        Self {
            buffer: Rc::new(RefCell::new(Vec::new())),
            cursor: 0,
        }
    }

    /// Starts the lines of the next type at the beginning of the buffer.
    pub fn rewind(&mut self) {
        self.cursor = 0;
    }

    /// A delay line that lives in this memory. `max_delay` is the longest delay that will be read, in samples.
    pub fn line(&mut self, max_delay: f64) -> DelayLine {
        let size = DelayLine::size_for(max_delay);
        let offset = self.cursor;
        self.cursor += size;
        let mut buffer = self.buffer.borrow_mut();
        if self.cursor > buffer.len() {
            // TS replaces the buffer with a new zeroed one of the new size
            *buffer = vec![0.0; self.cursor];
        }
        drop(buffer);
        DelayLine::in_memory(Rc::clone(&self.buffer), offset, size)
    }
}
