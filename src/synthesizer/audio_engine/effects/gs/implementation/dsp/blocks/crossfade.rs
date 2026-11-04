//! The crossfade of a block whose delays jump when a parameter changes.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/crossfade.ts

/// How long a delay change crossfades from the old reads to the new ones.
const FADE_SECONDS: f64 = 0.01;

/// The crossfade of a block whose delays jump when a parameter changes (a delay time, a pre delay, a depth): the
/// old reads fade out while the new ones fade in, so a change while playing does not click.
///
/// The block keeps its settings twice, `current` and `previous`. A change only marks the target, the fade picks it
/// up when the running one is done, so a sweep becomes a chain of fades. After `clear()` the first change applies at
/// once (a type select or a reset).
#[derive(Clone)]
pub struct Crossfade {
    /// The weight of the current reads, 1 when no fade runs (the previous reads get `1 - gain`).
    pub gain: f64,
    /// Whether the previous reads are needed this sample.
    pub active: bool,
    step: f64,
    pending: bool,
    immediate: bool,
}

impl Crossfade {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            gain: 1.0,
            active: false,
            step: 1.0 / (FADE_SECONDS * sample_rate),
            pending: false,
            immediate: true,
        }
    }

    /// Marks the target settings as changed.
    pub fn request(&mut self) {
        self.pending = true;
    }

    /// Steps the fade, once per sample before the reads.
    /// Returns whether the block now takes its target: the current settings become the previous ones.
    #[inline]
    pub fn tick(&mut self) -> bool {
        let mut take = false;
        if self.active {
            self.gain += self.step;
            if self.gain >= 1.0 {
                self.gain = 1.0;
                self.active = false;
            }
        } else if self.pending {
            self.pending = false;
            take = true;
            if !self.immediate {
                self.active = true;
                self.gain = 0.0;
            }
        }
        self.immediate = false;
        take
    }

    pub fn clear(&mut self) {
        self.gain = 1.0;
        self.active = false;
        self.immediate = true;
    }
}
