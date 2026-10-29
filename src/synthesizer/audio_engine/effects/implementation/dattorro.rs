//! Dattorro Reverb Node
//! by khoin on GitHub, public domain.
//! <https://github.com/khoin/DattorroReverbNode/>
//! Adapted for spessasynth by spessasus.
//! Ported from: src/synthesizer/audio_engine/effects/implementation/dattorro.ts
//!
//! 4.4.0 flattened the delay lines (every delay is read before it is written in the same
//! sample) and computes the excursion LFOs by rotating phasors. The order of reads and
//! writes below follows the TS line by line; changing it changes the output.

/// Delay lengths as fractions of sample rate.
const DELAYS: [f64; 12] = [
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
const TAPS: [f64; 14] = [
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

/// `Math.round` (half up).
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// `2 ** Math.ceil(Math.log2(x))`
fn next_pow2(x: f64) -> usize {
    2f64.powf(x.log2().ceil()) as usize
}

pub struct DattorroReverb {
    // Params
    /// Min: 0, max: sample rate - 1
    pub pre_delay: f64,
    /// Min: 0, max: 1
    pub pre_lpf: f64,
    /// Min: 0, max: 1
    pub input_diffusion1: f64,
    /// Min: 0, max: 1
    pub input_diffusion2: f64,
    /// Min: 0, max: 1
    pub decay: f64,
    /// Min: 0, max: 0.999999
    pub decay_diffusion1: f64,
    /// Min: 0, max: 0.999999
    pub decay_diffusion2: f64,
    /// Min: 0, max: 1
    pub damping: f64,
    /// Min: 0, max: 2
    pub excursion_rate: f64,
    /// Min: 0, max: 2
    pub excursion_depth: f64,
    pub gain: f64,
    sample_rate: f64,
    lp1: f64,
    lp2: f64,
    lp3: f64,

    // Separate lfo phases to allow safe wrapping at 2pi
    exc_phase1: f64,
    exc_phase2: f64,

    p_d_write: usize,
    taps: [usize; 14],
    p_delay: Vec<f32>,
    p_d_mask: usize,

    // Flattened delays compared to original
    delay_buffers: Vec<Vec<f32>>,
    delay_write: [usize; 12],
    delay_read: [usize; 12],
    delay_mask: [usize; 12],
}

impl DattorroReverb {
    pub fn new(sample_rate: f64) -> Self {
        // Pre-delay is always one-second long
        let p_d_mask = next_pow2(sample_rate) - 1;

        let mut s = Self {
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
            exc_phase1: 0.0,
            exc_phase2: 0.0,
            p_d_write: 0,
            taps: [0; 14],
            p_delay: vec![0.0; p_d_mask + 1],
            p_d_mask,
            delay_buffers: Vec::with_capacity(12),
            delay_write: [0; 12],
            delay_read: [0; 12],
            delay_mask: [0; 12],
        };
        for (i, &d) in DELAYS.iter().enumerate() {
            s.make_delay_line(d, i);
        }
        for (i, &t) in TAPS.iter().enumerate() {
            s.taps[i] = js_round(t * sample_rate) as usize;
        }
        s
    }

    /// Note: input is zero-based, while the outputs are startIndex based!
    /// ADDS to the output
    pub fn process(
        &mut self,
        input: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
        start_index: usize,
        sample_count: usize,
    ) {
        // Cache everything we can
        let pd = self.pre_delay as i64 as usize;
        let fi = self.input_diffusion1;
        let si = self.input_diffusion2;
        let dc = self.decay;
        let ft = self.decay_diffusion1;
        let st = self.decay_diffusion2;
        let dp = 1.0 - self.damping;
        let ex = self.excursion_rate / self.sample_rate;
        let ed = (self.excursion_depth * self.sample_rate) / 1000.0;

        let (mut lp1, mut lp2, mut lp3) = (self.lp1, self.lp2, self.lp3);
        let (mut p1, mut p2) = (self.exc_phase1, self.exc_phase2);

        let block_start = self.p_d_write;
        let p_d_mask = self.p_d_mask;
        let taps = self.taps;
        let gain = self.gain;
        let pre_lpf = self.pre_lpf;

        let mut w = self.delay_write;
        let mut r = self.delay_read;
        let m = self.delay_mask;
        let d = &mut self.delay_buffers;
        let p_delay = &mut self.p_delay;
        // Reads a delay line sample as f64 (Float32Array element).
        macro_rules! rd {
            ($k:expr, $idx:expr) => {
                d[$k][$idx] as f64
            };
        }

        const TWO_PI: f64 = 6.283_185_307_179_586;

        // Excursion LFOs
        // Computed by rotating a phasor
        // It starts from the exact values every block,
        // So no error accumulates.
        // Different values for stereo effect
        let step1 = ex * TWO_PI;
        let step2 = ex * 6.2847;
        let step_cos1 = step1.cos();
        let step_sin1 = step1.sin();
        let step_cos2 = step2.cos();
        let step_sin2 = step2.sin();
        let mut cos1 = p1.cos();
        let mut sin1 = p1.sin();
        let mut cos2 = p2.cos();
        let mut sin2 = p2.sin();

        for i in 0..sample_count {
            // Write/read predelay
            p_delay[(block_start + i) & p_d_mask] = input[i];
            let in_sample =
                p_delay[(block_start + i).wrapping_sub(pd) & p_d_mask] as f64;

            // Lowpass filter
            lp1 += pre_lpf * (in_sample - lp1);

            // Pre-tank
            let read0 = rd!(0, r[0]);
            let mut pre = lp1 - fi * read0;
            d[0][w[0]] = pre as f32;

            let read1 = rd!(1, r[1]);
            pre = fi * (pre - read1) + read0;
            d[1][w[1]] = pre as f32;

            let read2 = rd!(2, r[2]);
            pre = fi * pre + read1 - si * read2;
            d[2][w[2]] = pre as f32;

            let read3 = rd!(3, r[3]);
            pre = si * (pre - read3) + read2;
            d[3][w[3]] = pre as f32;

            let split = si * pre + read3;

            // Excursions
            let exc = ed * (1.0 + cos1);
            let exc2 = ed * (1.0 + sin2);

            // Left loop
            let read11 = rd!(11, r[11]);

            // Inlined readDelayCAt(4, exc)
            // `~~exc` truncates; exc >= 0 here
            let f4 = exc - exc.trunc();
            let mut i4 = (exc.trunc() as usize + r[4]).wrapping_sub(1);
            let x4_0 = rd!(4, i4 & m[4]);
            i4 = i4.wrapping_add(1);
            let x4_1 = rd!(4, i4 & m[4]);
            i4 = i4.wrapping_add(1);
            let x4_2 = rd!(4, i4 & m[4]);
            i4 = i4.wrapping_add(1);
            let x4_3 = rd!(4, i4 & m[4]);
            let a4 = (3.0 * (x4_1 - x4_2) - x4_0 + x4_3) * 0.5;
            let b4 = 2.0 * x4_2 + x4_0 - (5.0 * x4_1 + x4_3) * 0.5;
            let c4 = (x4_2 - x4_0) * 0.5;
            let read_c4 = ((a4 * f4 + b4) * f4 + c4) * f4 + x4_1;
            // End of inline

            let mut temp = split + dc * read11 + ft * read_c4;
            d[4][w[4]] = temp as f32; // Tank diffuse 1

            d[5][w[5]] = (read_c4 - ft * temp) as f32; // Long delay 1

            let read5 = rd!(5, r[5]);
            lp2 += dp * (read5 - lp2); // Damp 1

            let read6 = rd!(6, r[6]);
            temp = dc * lp2 - st * read6;
            d[6][w[6]] = temp as f32; // Tank diffuse 2

            d[7][w[7]] = (read6 + st * temp) as f32; // Long delay 2

            // Right loop
            let read7 = rd!(7, r[7]);

            // Inline readDelayCAt(8, exc2)
            let f8 = exc2 - exc2.trunc();
            let mut i8 = (exc2.trunc() as usize + r[8]).wrapping_sub(1);
            let x8_0 = rd!(8, i8 & m[8]);
            i8 = i8.wrapping_add(1);
            let x8_1 = rd!(8, i8 & m[8]);
            i8 = i8.wrapping_add(1);
            let x8_2 = rd!(8, i8 & m[8]);
            i8 = i8.wrapping_add(1);
            let x8_3 = rd!(8, i8 & m[8]);
            let a8 = (3.0 * (x8_1 - x8_2) - x8_0 + x8_3) * 0.5;
            let b8 = 2.0 * x8_2 + x8_0 - (5.0 * x8_1 + x8_3) * 0.5;
            let c8 = (x8_2 - x8_0) * 0.5;
            let read_c8 = ((a8 * f8 + b8) * f8 + c8) * f8 + x8_1;
            // End of inline

            temp = split + dc * read7 + ft * read_c8;
            d[8][w[8]] = temp as f32; // Tank diffuse 3

            d[9][w[9]] = (read_c8 - ft * temp) as f32; // Long delay 3

            let read9 = rd!(9, r[9]);
            lp3 += dp * (read9 - lp3); // Damp 2

            let read10 = rd!(10, r[10]);
            temp = dc * lp3 - st * read10;
            d[10][w[10]] = temp as f32; // Tank diffuse 4

            d[11][w[11]] = (read10 + st * temp) as f32; // Long delay 4

            // Mix down
            let left_sample = rd!(9, (r[9] + taps[0]) & m[9])
                + rd!(9, (r[9] + taps[1]) & m[9])
                - rd!(10, (r[10] + taps[2]) & m[10])
                + rd!(11, (r[11] + taps[3]) & m[11])
                - rd!(5, (r[5] + taps[4]) & m[5])
                - rd!(6, (r[6] + taps[5]) & m[6])
                - rd!(7, (r[7] + taps[6]) & m[7]);

            let right_sample = rd!(5, (r[5] + taps[7]) & m[5])
                + rd!(5, (r[5] + taps[8]) & m[5])
                - rd!(6, (r[6] + taps[9]) & m[6])
                + rd!(7, (r[7] + taps[10]) & m[7])
                - rd!(9, (r[9] + taps[11]) & m[9])
                - rd!(10, (r[10] + taps[12]) & m[10])
                - rd!(11, (r[11] + taps[13]) & m[11]);

            // Write out
            let idx = i + start_index;
            output_left[idx] = (output_left[idx] as f64 + left_sample * gain) as f32;
            output_right[idx] = (output_right[idx] as f64 + right_sample * gain) as f32;

            // Update LFOs (rotate the phasors)
            let next_cos1 = cos1 * step_cos1 - sin1 * step_sin1;
            sin1 = sin1 * step_cos1 + cos1 * step_sin1;
            cos1 = next_cos1;
            let next_cos2 = cos2 * step_cos2 - sin2 * step_sin2;
            sin2 = sin2 * step_cos2 + cos2 * step_sin2;
            cos2 = next_cos2;

            // Advance delays
            for k in 0..12 {
                w[k] = (w[k] + 1) & m[k];
                r[k] = (r[k] + 1) & m[k];
            }
        }
        // Advance the LFO phases and wrap them
        p1 = (p1 + step1 * sample_count as f64) % TWO_PI;
        p2 = (p2 + step2 * sample_count as f64) % TWO_PI;

        // Update preDelay index
        self.p_d_write = (block_start + sample_count) & p_d_mask;

        // Save state
        self.lp1 = lp1;
        self.lp2 = lp2;
        self.lp3 = lp3;
        self.exc_phase1 = p1;
        self.exc_phase2 = p2;
        self.delay_write = w;
        self.delay_read = r;
    }

    fn make_delay_line(&mut self, length: f64, index: usize) {
        // Len, array, write, read, mask
        let len = js_round(length * self.sample_rate);
        let next_pow2 = next_pow2(len);
        self.delay_buffers.push(vec![0.0; next_pow2]);
        debug_assert_eq!(self.delay_buffers.len(), index + 1);
        self.delay_write[index] = len as usize - 1;
        self.delay_read[index] = 0;
        self.delay_mask[index] = next_pow2 - 1;
    }

    // Read delay methods inlined for performance
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(r: &mut DattorroReverb, input: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let mut l = vec![0.0f32; input.len()];
        let mut rr = vec![0.0f32; input.len()];
        for (k, chunk) in input.chunks(128).enumerate() {
            r.process(chunk, &mut l, &mut rr, k * 128, chunk.len());
        }
        (l, rr)
    }

    #[test]
    fn delay_buffers_are_power_of_two() {
        let r = DattorroReverb::new(44_100.0);
        for (k, b) in r.delay_buffers.iter().enumerate() {
            assert!(b.len().is_power_of_two());
            assert_eq!(r.delay_mask[k], b.len() - 1);
        }
        assert!(r.p_delay.len().is_power_of_two());
        assert!(r.p_delay.len() >= 44_100);
    }

    #[test]
    fn silence_in_silence_out() {
        let mut r = DattorroReverb::new(44_100.0);
        let (l, rr) = render(&mut r, &vec![0.0; 1024]);
        assert!(l.iter().chain(rr.iter()).all(|&x| x == 0.0));
    }

    #[test]
    fn impulse_produces_finite_stereo_output() {
        let mut r = DattorroReverb::new(44_100.0);
        r.decay = 0.7;
        let mut input = vec![0.0f32; 44_100];
        input[0] = 1.0;
        let (l, rr) = render(&mut r, &input);
        assert!(l.iter().chain(rr.iter()).all(|x| x.is_finite()));
        assert!(l.iter().any(|&x| x != 0.0));
        assert_ne!(l, rr);
    }

    #[test]
    fn gain_scales_output() {
        let mut input = vec![0.0f32; 8192];
        input[0] = 1.0;
        let mut a = DattorroReverb::new(44_100.0);
        let mut b = DattorroReverb::new(44_100.0);
        b.gain = 0.5;
        let (la, _) = render(&mut a, &input);
        let (lb, _) = render(&mut b, &input);
        let ea: f64 = la.iter().map(|&x| (x as f64).abs()).sum();
        let eb: f64 = lb.iter().map(|&x| (x as f64).abs()).sum();
        assert!((eb / ea - 0.5).abs() < 1e-3);
    }

    #[test]
    fn start_index_offsets_output() {
        let mut input = vec![0.0f32; 4096];
        input[0] = 1.0;
        let mut a = DattorroReverb::new(44_100.0);
        let mut la = vec![0.0f32; 4096];
        let mut ra = vec![0.0f32; 4096];
        a.process(&input, &mut la, &mut ra, 0, 4096);
        let mut b = DattorroReverb::new(44_100.0);
        let mut lb = vec![0.0f32; 4096 + 10];
        let mut rb = vec![0.0f32; 4096 + 10];
        b.process(&input, &mut lb, &mut rb, 10, 4096);
        assert_eq!(&lb[10..], &la[..]);
    }
}
