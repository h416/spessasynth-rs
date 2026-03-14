/// Dattorro Reverb
/// Based on DattorroReverbNode by khoin on GitHub (public domain).
/// https://github.com/khoin/DattorroReverbNode/
/// Adapted for spessasynth by spessasus, ported to Rust.

/// Internal delay line with power-of-2 buffer for fast masking.
struct DattorroDelayLine {
    buffer: Vec<f32>,
    write_index: usize,
    read_index: usize,
    mask: usize,
}

impl DattorroDelayLine {
    fn new(length: usize) -> Self {
        let next_pow2 = length.next_power_of_two();
        Self {
            buffer: vec![0.0; next_pow2],
            write_index: length - 1,
            read_index: 0,
            mask: next_pow2 - 1,
        }
    }

    #[inline]
    fn write(&mut self, sample: f64) -> f64 {
        self.buffer[self.write_index] = sample as f32;
        sample
    }

    #[inline]
    fn read(&self) -> f64 {
        self.buffer[self.read_index] as f64
    }

    #[inline]
    fn read_at(&self, offset: i16) -> f64 {
        self.buffer[(self.read_index + offset as usize) & self.mask] as f64
    }

    /// Cubic interpolation read (Niemitalo).
    #[inline]
    fn read_cubic_at(&self, i: f64) -> f64 {
        let frac = i - (i as i32 as f64);
        // Use wrapping arithmetic since index can underflow before masking
        let mut int = (i as i32).wrapping_add(self.read_index as i32).wrapping_sub(1) as usize;
        let mask = self.mask;

        let x0 = self.buffer[int & mask] as f64;
        int = int.wrapping_add(1);
        let x1 = self.buffer[int & mask] as f64;
        int = int.wrapping_add(1);
        let x2 = self.buffer[int & mask] as f64;
        int = int.wrapping_add(1);
        let x3 = self.buffer[int & mask] as f64;

        let a = (3.0 * (x1 - x2) - x0 + x3) / 2.0;
        let b = 2.0 * x2 + x0 - (5.0 * x1 + x3) / 2.0;
        let c = (x2 - x0) / 2.0;

        ((a * frac + b) * frac + c) * frac + x1
    }

    #[inline]
    fn advance(&mut self) {
        self.write_index = (self.write_index + 1) & self.mask;
        self.read_index = (self.read_index + 1) & self.mask;
    }
}

/// Delay lengths as fractions of sample rate.
const DELAY_LENGTHS: [f64; 12] = [
    0.004_771_345,
    0.003_595_309,
    0.012_734_787,
    0.009_307_483,
    0.022_579_886,
    0.149_625_349,
    0.060_481_839,
    0.124_995_8,
    0.030_509_727,
    0.141_695_508,
    0.089_244_313,
    0.106_280_031,
];

/// Tap offsets as fractions of sample rate.
const TAP_FRACTIONS: [f64; 14] = [
    0.008_937_872,
    0.099_929_438,
    0.064_278_754,
    0.067_067_639,
    0.066_866_033,
    0.006_283_391,
    0.035_818_689,
    0.011_861_161,
    0.121_870_905,
    0.041_262_054,
    0.089_815_53,
    0.070_931_756,
    0.011_256_342,
    0.004_065_724,
];

pub struct DattorroReverb {
    // Parameters
    pub pre_delay: f64,
    pub pre_lpf: f64,
    pub input_diffusion1: f64,
    pub input_diffusion2: f64,
    pub decay: f64,
    pub decay_diffusion1: f64,
    pub decay_diffusion2: f64,
    pub damping: f64,
    pub excursion_rate: f64,
    pub excursion_depth: f64,
    pub gain: f64,

    // Internal state
    sample_rate: f64,
    lp1: f64,
    lp2: f64,
    lp3: f64,
    exc_phase: f64,

    // Pre-delay buffer
    p_delay: Vec<f32>,
    pd_write: usize,
    pd_length: usize,

    // Output taps
    taps: [i16; 14],

    // 12 delay lines
    delays: Vec<DattorroDelayLine>,
}

impl DattorroReverb {
    pub fn new(sample_rate: f64) -> Self {
        let pd_length = sample_rate as usize;
        let p_delay = vec![0.0; pd_length];

        let delays: Vec<DattorroDelayLine> = DELAY_LENGTHS
            .iter()
            .map(|&frac| {
                let len = (frac * sample_rate).round() as usize;
                DattorroDelayLine::new(len)
            })
            .collect();

        let mut taps = [0i16; 14];
        for (i, &frac) in TAP_FRACTIONS.iter().enumerate() {
            taps[i] = (frac * sample_rate).round() as i16;
        }

        Self {
            pre_delay: 0.0,
            pre_lpf: 0.5,
            input_diffusion1: 0.75,
            input_diffusion2: 0.625,
            decay: 0.5,
            decay_diffusion1: 0.7,
            decay_diffusion2: 0.5,
            damping: 0.005,
            excursion_rate: 0.1,
            excursion_depth: 0.2,
            gain: 1.0,
            sample_rate,
            lp1: 0.0,
            lp2: 0.0,
            lp3: 0.0,
            exc_phase: 0.0,
            p_delay,
            pd_write: 0,
            pd_length,
            taps,
            delays,
        }
    }

    /// Process reverb. Input is zero-based, outputs are startIndex-based (ADDS to output).
    pub fn process(
        &mut self,
        input: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        let pd = self.pre_delay as usize;
        let fi = self.input_diffusion1;
        let si = self.input_diffusion2;
        let dc = self.decay;
        let ft = self.decay_diffusion1;
        let st = self.decay_diffusion2;
        let dp = 1.0 - self.damping;
        let ex = self.excursion_rate / self.sample_rate;
        let ed = (self.excursion_depth * self.sample_rate) / 1000.0;
        let block_start = self.pd_write;

        // Write to pre-delay
        for j in 0..sample_count {
            self.p_delay[(block_start + j) % self.pd_length] = input[j];
        }

        let d = &mut self.delays;
        for i in 0..sample_count {
            // Pre-delay read with LPF
            let pd_read_idx =
                (self.pd_length + self.pd_write - pd + i) % self.pd_length;
            self.lp1 += self.pre_lpf * (self.p_delay[pd_read_idx] as f64 - self.lp1);

            // Pre-tank: 4-stage input diffusion
            // Read before write to satisfy borrow checker
            let r0 = d[0].read();
            let mut pre = d[0].write(self.lp1 - fi * r0);

            let r1 = d[1].read();
            pre = d[1].write(fi * (pre - r1) + r0);

            let r1 = d[1].read();
            let r2 = d[2].read();
            pre = d[2].write(fi * pre + r1 - si * r2);

            let r2 = d[2].read();
            let r3 = d[3].read();
            pre = d[3].write(si * (pre - r3) + r2);

            let r3 = d[3].read();
            let split = si * pre + r3;

            // Excursion modulation
            let exc = ed * (1.0 + (self.exc_phase * 6.28).cos());
            let exc2 = ed * (1.0 + (self.exc_phase * 6.2847).sin());

            // Left decay tank
            let r11 = d[11].read();
            let r4c = d[4].read_cubic_at(exc);
            let mut temp = d[4].write(split + dc * r11 + ft * r4c);

            let r4c = d[4].read_cubic_at(exc);
            d[5].write(r4c - ft * temp);

            let r5 = d[5].read();
            self.lp2 += dp * (r5 - self.lp2);

            let r6 = d[6].read();
            temp = d[6].write(dc * self.lp2 - st * r6);

            let r6 = d[6].read();
            d[7].write(r6 + st * temp);

            // Right decay tank
            let r7 = d[7].read();
            let r8c = d[8].read_cubic_at(exc2);
            temp = d[8].write(split + dc * r7 + ft * r8c);

            let r8c = d[8].read_cubic_at(exc2);
            d[9].write(r8c - ft * temp);

            let r9 = d[9].read();
            self.lp3 += dp * (r9 - self.lp3);

            let r10 = d[10].read();
            temp = d[10].write(dc * self.lp3 - st * r10);

            let r10 = d[10].read();
            d[11].write(r10 + st * temp);

            // Stereo mix-down from taps
            let left_sample = d[9].read_at(self.taps[0])
                + d[9].read_at(self.taps[1])
                - d[10].read_at(self.taps[2])
                + d[11].read_at(self.taps[3])
                - d[5].read_at(self.taps[4])
                - d[6].read_at(self.taps[5])
                - d[7].read_at(self.taps[6]);

            let idx = i + start_index;
            output_left[idx] += (left_sample * self.gain) as f32;

            let right_sample = d[5].read_at(self.taps[7])
                + d[5].read_at(self.taps[8])
                - d[6].read_at(self.taps[9])
                + d[7].read_at(self.taps[10])
                - d[9].read_at(self.taps[11])
                - d[10].read_at(self.taps[12])
                - d[11].read_at(self.taps[13]);

            output_right[idx] += (right_sample * self.gain) as f32;

            // Advance phase
            self.exc_phase += ex;

            // Advance all delay lines
            for dl in d.iter_mut() {
                dl.advance();
            }
        }

        // Update pre-delay write index
        self.pd_write = (block_start + sample_count) % self.pd_length;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // DattorroDelayLine tests
    // -----------------------------------------------------------------------

    #[test]
    fn delay_line_buffer_is_power_of_two() {
        let dl = DattorroDelayLine::new(100);
        assert_eq!(dl.buffer.len(), 128); // next_power_of_two(100)
        assert_eq!(dl.mask, 127);
    }

    #[test]
    fn delay_line_write_returns_sample() {
        let mut dl = DattorroDelayLine::new(8);
        let ret = dl.write(0.5);
        assert_eq!(ret, 0.5);
    }

    #[test]
    fn delay_line_read_returns_written_after_delay() {
        let mut dl = DattorroDelayLine::new(4);
        // write_index starts at length-1 = 3, read_index starts at 0
        // So delay = write_index - read_index = 3 samples
        dl.write(1.0);
        // read_index=0 reads the initial zero
        assert_eq!(dl.read(), 0.0);
        // Advance 3 times to bring read_index to where we wrote
        dl.advance();
        dl.advance();
        dl.advance();
        // Now read_index should point to where we wrote 1.0
        // write was at index 3, after 3 advances read_index = 3
        assert_eq!(dl.read(), 1.0);
    }

    #[test]
    fn delay_line_advance_wraps() {
        let mut dl = DattorroDelayLine::new(4);
        // buffer size = 4 (already power of 2), mask = 3
        for _ in 0..10 {
            dl.advance();
        }
        // Should not panic, indices wrap via mask
        assert!(dl.write_index < dl.buffer.len());
        assert!(dl.read_index < dl.buffer.len());
    }

    // -----------------------------------------------------------------------
    // Cubic interpolation tests
    // -----------------------------------------------------------------------

    #[test]
    fn cubic_at_integer_position_returns_exact_sample() {
        // Fill a delay line with known values
        let mut dl = DattorroDelayLine::new(16);
        // Write samples 0,1,2,...,15 into the buffer
        for i in 0..16 {
            dl.buffer[i] = i as f32;
        }
        dl.read_index = 2; // so read_cubic_at(0) reads around index 2

        // At integer position i=1, the cubic should return x1 exactly
        // (when a=0, b=0, c=anything, frac=0 -> result = x1)
        // int = 1 + 2 - 1 = 2, so x0=buf[2], x1=buf[3], x2=buf[4], x3=buf[5]
        // For linear data: x0=2, x1=3, x2=4, x3=5
        // a = (3*(3-4) - 2 + 5)/2 = (-3-2+5)/2 = 0
        // b = 2*4 + 2 - (5*3+5)/2 = 10 - 10 = 0
        // c = (4-2)/2 = 1
        // result = (0*0+0)*0+1)*0 + 3 = 3
        let val = dl.read_cubic_at(1.0);
        assert!(
            (val - 3.0).abs() < 1e-6,
            "Cubic at integer: expected 3.0, got {}",
            val
        );
    }

    #[test]
    fn cubic_interpolates_linear_data_exactly() {
        // For linear data, cubic interpolation should be exact
        let mut dl = DattorroDelayLine::new(16);
        for i in 0..16 {
            dl.buffer[i] = (i as f32) * 2.0; // linear: 0, 2, 4, 6, ...
        }
        dl.read_index = 2;

        // At fractional position i=1.5, should interpolate linearly
        // int starts at 1+2-1=2, x0=4, x1=6, x2=8, x3=10
        // For linear data, a=0, b=0, c=2, frac=0.5
        // result = c*frac + x1 = 2*0.5 + 6 = 7
        let val = dl.read_cubic_at(1.5);
        assert!(
            (val - 7.0).abs() < 1e-4,
            "Cubic on linear data: expected 7.0, got {}",
            val
        );
    }

    #[test]
    fn cubic_handles_zero_fractional_part() {
        let mut dl = DattorroDelayLine::new(16);
        for i in 0..16 {
            dl.buffer[i] = (i as f32).sin();
        }
        dl.read_index = 0;

        // frac=0 -> result should be x1 (the sample at int+1 after -1 offset)
        let val = dl.read_cubic_at(3.0);
        // int = 3+0-1=2, x1 = buffer[3]
        let expected = (3.0_f32).sin() as f64;
        assert!(
            (val - expected).abs() < 1e-5,
            "Cubic at zero frac: expected {}, got {}",
            expected,
            val
        );
    }

    #[test]
    fn cubic_result_is_bounded_by_neighboring_samples() {
        // For smooth data, cubic interpolation should stay within a reasonable range
        let mut dl = DattorroDelayLine::new(32);
        for i in 0..32 {
            dl.buffer[i] = (i as f32 * 0.3).sin();
        }
        dl.read_index = 4;

        for frac_10 in 0..10 {
            let pos = 2.0 + frac_10 as f64 * 0.1;
            let val = dl.read_cubic_at(pos);
            // Should not produce wild values for smooth input
            assert!(
                val.abs() < 2.0,
                "Cubic produced wild value {} at pos {}",
                val,
                pos
            );
        }
    }

    // -----------------------------------------------------------------------
    // DattorroReverb impulse response tests
    // -----------------------------------------------------------------------

    #[test]
    fn reverb_new_creates_valid_instance() {
        let rev = DattorroReverb::new(44100.0);
        assert_eq!(rev.sample_rate, 44100.0);
        assert_eq!(rev.delays.len(), 12);
        assert_eq!(rev.taps.len(), 14);
    }

    #[test]
    fn reverb_silence_in_silence_out() {
        let mut rev = DattorroReverb::new(44100.0);
        rev.gain = 1.0;
        let input = vec![0.0f32; 512];
        let mut out_l = vec![0.0f32; 512];
        let mut out_r = vec![0.0f32; 512];
        rev.process(&input, &mut out_l, &mut out_r, 0, 512);

        for i in 0..512 {
            assert!(
                out_l[i].abs() < 1e-10 && out_r[i].abs() < 1e-10,
                "Silence in should produce silence out at sample {}",
                i
            );
        }
    }

    #[test]
    fn reverb_impulse_produces_output() {
        let mut rev = DattorroReverb::new(44100.0);
        rev.gain = 1.0;
        rev.pre_lpf = 1.0; // Let the impulse through

        // Send an impulse
        let mut input = vec![0.0f32; 4096];
        input[0] = 1.0;
        let mut out_l = vec![0.0f32; 4096];
        let mut out_r = vec![0.0f32; 4096];
        rev.process(&input, &mut out_l, &mut out_r, 0, 4096);

        // After some delay, there should be non-zero output
        let energy_l: f64 = out_l.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let energy_r: f64 = out_r.iter().map(|&s| (s as f64) * (s as f64)).sum();
        assert!(
            energy_l > 1e-6,
            "Left channel should have energy after impulse, got {}",
            energy_l
        );
        assert!(
            energy_r > 1e-6,
            "Right channel should have energy after impulse, got {}",
            energy_r
        );
    }

    #[test]
    fn reverb_left_right_are_different() {
        let mut rev = DattorroReverb::new(44100.0);
        rev.gain = 1.0;
        rev.pre_lpf = 1.0;

        let mut input = vec![0.0f32; 4096];
        input[0] = 1.0;
        let mut out_l = vec![0.0f32; 4096];
        let mut out_r = vec![0.0f32; 4096];
        rev.process(&input, &mut out_l, &mut out_r, 0, 4096);

        // L and R should differ (different tap sets)
        let diff: f64 = out_l
            .iter()
            .zip(out_r.iter())
            .map(|(&l, &r)| ((l - r) as f64).abs())
            .sum();
        assert!(
            diff > 1e-6,
            "Left and right should differ for stereo reverb"
        );
    }

    #[test]
    fn reverb_impulse_decays_over_time() {
        let mut rev = DattorroReverb::new(44100.0);
        rev.gain = 1.0;
        rev.pre_lpf = 1.0;
        rev.decay = 0.2; // Fast decay

        // Use a longer buffer so the reverb has time to build up and then decay
        let n = 44100; // 1 second
        let mut input = vec![0.0f32; n];
        input[0] = 1.0;
        let mut out_l = vec![0.0f32; n];
        let mut out_r = vec![0.0f32; n];
        rev.process(&input, &mut out_l, &mut out_r, 0, n);

        // Compare energy in the second quarter vs the last quarter
        let q = n / 4;
        let energy_early: f64 = out_l[q..2 * q]
            .iter()
            .map(|&s| (s as f64) * (s as f64))
            .sum();
        let energy_late: f64 = out_l[3 * q..]
            .iter()
            .map(|&s| (s as f64) * (s as f64))
            .sum();
        assert!(
            energy_early > energy_late,
            "Reverb should decay: early energy {} vs late energy {}",
            energy_early,
            energy_late
        );
    }

    #[test]
    fn reverb_gain_scales_output() {
        let mut rev1 = DattorroReverb::new(44100.0);
        rev1.gain = 1.0;
        rev1.pre_lpf = 1.0;
        let mut rev2 = DattorroReverb::new(44100.0);
        rev2.gain = 0.5;
        rev2.pre_lpf = 1.0;

        let mut input = vec![0.0f32; 2048];
        input[0] = 1.0;

        let mut out1_l = vec![0.0f32; 2048];
        let mut out1_r = vec![0.0f32; 2048];
        rev1.process(&input, &mut out1_l, &mut out1_r, 0, 2048);

        let mut out2_l = vec![0.0f32; 2048];
        let mut out2_r = vec![0.0f32; 2048];
        rev2.process(&input, &mut out2_l, &mut out2_r, 0, 2048);

        // Output with gain=0.5 should be half of gain=1.0
        for i in 0..2048 {
            if out1_l[i].abs() > 1e-8 {
                let ratio = out2_l[i] / out1_l[i];
                assert!(
                    (ratio - 0.5).abs() < 1e-4,
                    "Gain ratio at {}: expected 0.5, got {}",
                    i,
                    ratio
                );
            }
        }
    }

    #[test]
    fn reverb_output_has_no_nan_or_inf() {
        let mut rev = DattorroReverb::new(44100.0);
        rev.gain = 1.0;
        rev.pre_lpf = 1.0;
        rev.decay = 0.9; // High decay

        let mut input = vec![0.0f32; 8192];
        input[0] = 1.0;
        input[100] = -0.5;
        input[500] = 0.8;
        let mut out_l = vec![0.0f32; 8192];
        let mut out_r = vec![0.0f32; 8192];
        rev.process(&input, &mut out_l, &mut out_r, 0, 8192);

        for i in 0..8192 {
            assert!(out_l[i].is_finite(), "NaN/Inf in left at sample {}", i);
            assert!(out_r[i].is_finite(), "NaN/Inf in right at sample {}", i);
        }
    }

    #[test]
    fn reverb_start_index_offset_works() {
        let mut rev = DattorroReverb::new(44100.0);
        rev.gain = 1.0;
        rev.pre_lpf = 1.0;

        let mut input = vec![0.0f32; 128];
        input[0] = 1.0;
        let mut out_l = vec![0.0f32; 256];
        let mut out_r = vec![0.0f32; 256];

        // Process with start_index=64
        rev.process(&input, &mut out_l, &mut out_r, 64, 128);

        // First 64 samples should be untouched (0.0)
        for i in 0..64 {
            assert_eq!(out_l[i], 0.0, "Output before start_index should be 0");
        }
    }
}
