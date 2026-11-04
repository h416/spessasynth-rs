//! The Kaiser window and the modified Bessel function it uses.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/kaiser.ts

/// The modified Bessel function I0, for the Kaiser window.
pub fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    for k in 1..32 {
        // term *= (x / (2 * k)) ** 2
        let r = x / (2.0 * k as f64);
        term *= r * r;
        sum += term;
    }
    sum
}

/// The Kaiser window at `t` (-1 to 1 across the window).
pub fn kaiser(t: f64, beta: f64) -> f64 {
    bessel_i0(beta * (1.0 - t * t).sqrt()) / bessel_i0(beta)
}
