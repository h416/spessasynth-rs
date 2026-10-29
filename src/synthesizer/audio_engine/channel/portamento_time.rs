//! portamento_time.rs
//! purpose: Converts MIDI portamento time (CC#5) to seconds using the Sound Canvas rate table.
//! Ported from: src/synthesizer/audio_engine/channel/portamento_time.ts
//!
//! Reference measurements by John Novak:
//! <https://github.com/dosbox-staging/dosbox-staging/pull/2705>
//!
//! The table used to be linearly interpolated, then replaced by a PCHIP function created by
//! Benjamin Rosseaux (4.4.1 and earlier). Since 4.4.2 it uses rates measured on the original
//! Sound Canvas. The pitch moves linearly towards the target at a constant rate, so the time
//! scales linearly with the distance.

/// The Sound Canvas portamento rate table, indexed by CC 5.
/// The rate is in 1/16 semitones per second.
/// Measured on the SC-88Pro from CC 5 = 16 up. Below that, the values are the SC-VA steps
/// scaled to the hardware (0.8 times).
/// Equivalent to: PORTAMENTO_RATES
const PORTAMENTO_RATES: [u16; 128] = [
    0, 44_640, 19_840, 12_753, 9920, 7761, 6376, 5578, 4823, 4250, 3880, 3498, 3187, 2976, 2746,
    2548, 2383, 2263, 2127, 2008, 1922, 1824, 1734, 1670, 1596, 1528, 1476, 1418, 1364, 1323,
    1276, 1232, 1191, 1123, 1063, 1003, 955, 911, 867, 831, 793, 763, 735, 705, 681, 659, 635,
    615, 595, 561, 529, 501, 477, 455, 433, 414, 396, 380, 366, 352, 340, 328, 316, 306, 296,
    280, 264, 250, 238, 226, 216, 206, 198, 190, 182, 176, 170, 164, 158, 152, 148, 140, 132,
    124, 118, 112, 108, 102, 98, 94, 90, 88, 84, 82, 78, 76, 74, 70, 66, 62, 58, 56, 54, 50, 48,
    46, 44, 42, 40, 38, 36, 34, 32, 30, 28, 26, 24, 22, 20, 18, 16, 14, 12, 10, 8, 6, 4, 2,
];

/// Converts portamento time to seconds.
/// `time`: MIDI CC#5 value (0–127, integer).
/// `distance`: pitch distance in semitones to slide over.
/// Equivalent to: portamentoTimeToSeconds
pub fn portamento_time_to_seconds(time: u8, distance: f64) -> f64 {
    // PortaTimeToRate is the constant rate of the portamento, as that's how the synths work.
    if time < 1 {
        return 0.0;
    }
    (distance * 16.0) / PORTAMENTO_RATES[time as usize] as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cc_zero_any_distance_is_zero() {
        assert_eq!(portamento_time_to_seconds(0, 1.0), 0.0);
        assert_eq!(portamento_time_to_seconds(0, 100.0), 0.0);
    }

    #[test]
    fn test_table_endpoints() {
        assert_eq!(PORTAMENTO_RATES[0], 0);
        assert_eq!(PORTAMENTO_RATES[1], 44_640);
        assert_eq!(PORTAMENTO_RATES[127], 2);
    }

    #[test]
    fn test_rate_is_sixteenth_semitones_per_second() {
        // CC 5 = 127: 2/16 semitone per second -> 8 s per semitone
        assert_eq!(portamento_time_to_seconds(127, 1.0), 8.0);
        // CC 5 = 64: rate 296 -> 12 semitones take 192 / 296 s
        assert_eq!(portamento_time_to_seconds(64, 12.0), 192.0 / 296.0);
    }

    #[test]
    fn test_proportional_to_distance() {
        for cc in [1u8, 16, 32, 64, 96, 127] {
            let t1 = portamento_time_to_seconds(cc, 1.0);
            let t2 = portamento_time_to_seconds(cc, 3.0);
            assert!((t2 - 3.0 * t1).abs() < 1e-12, "cc={cc}: t1={t1}, t2={t2}");
        }
    }

    #[test]
    fn test_monotonically_non_decreasing() {
        let mut prev = 0.0f64;
        for cc in 0u8..=127 {
            let t = portamento_time_to_seconds(cc, 1.0);
            assert!(t >= prev, "time decreased at cc={cc}: {prev} -> {t}");
            prev = t;
        }
    }
}
