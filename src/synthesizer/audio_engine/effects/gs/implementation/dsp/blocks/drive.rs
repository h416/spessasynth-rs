//! The drive of the Sound Canvas: filters, a gain, a clipper and waveshaper, filters again and a level compensation.
//! Ported from: src/synthesizer/audio_engine/effects/gs/implementation/dsp/blocks/drive.ts

use super::biquad::Biquad;
use super::first_order_filter::{DC_BLOCKER_HZ, FirstOrderFilter};
use super::oversampler::Oversampler;

/// A drive variant: the fixed filters around the clipper, the gain in front of it and the level compensation.
/// Equivalent to: interface DrivePreset
pub struct DrivePreset {
    /// The gain in front of the clipper per drive step, with the fixed gains of the pre filters folded in.
    pub gain: f64,
    /// The level compensation by drive (over 128), the last entry holds for every higher drive.
    pub post_gain: &'static [u8],
    /// The fixed gain of the post filters.
    pub post_filter_gain: f64,
    /// Designs the three pre filters, the post filter and the post peaking filter.
    pub design: fn(
        &mut FirstOrderFilter,
        &mut FirstOrderFilter,
        &mut FirstOrderFilter,
        &mut FirstOrderFilter,
        &mut Biquad,
    ),
    /// Designs the second order sections of the bass drives, one in front of the clipper (after the pre filters)
    /// and one at the end. Without it both are off.
    pub design_bass: Option<fn(&mut Biquad, &mut Biquad)>,
}

/// The level compensation of the guitar drives (`DRIVE_B`).
const GUITAR_POST_GAIN: [u8; 7] = [58, 58, 29, 21, 17, 15, 14];

/// The guitar Overdrive: band limited to 330-2400 Hz, 18 dB more gain than the drive value says, a soft -10 dB
/// high cut after.
pub static GUITAR_OVERDRIVE: DrivePreset = DrivePreset {
    gain: 8.0 * 0.996_02 * 0.979_59,
    post_gain: &GUITAR_POST_GAIN,
    post_filter_gain: 127.0 / 128.0,
    design: |pre1, pre2, pre3, post, peak| {
        pre1.set_high_pass(328.5, 1.0);
        pre2.set_high_pass(202.9, 1.0);
        pre3.set_low_pass(2368.0, 1.0);
        post.set_high_shelf(1002.0, -10.08);
        peak.set_off();
    },
    design_bass: None,
};

fn guitar_distortion_design(
    pre1: &mut FirstOrderFilter,
    pre2: &mut FirstOrderFilter,
    pre3: &mut FirstOrderFilter,
    post: &mut FirstOrderFilter,
    peak: &mut Biquad,
) {
    pre1.set_pole_zero(-84.12, 678.1, 1.0083);
    pre2.set_off();
    pre3.set_low_pass(1548.0, 1.0);
    post.set_high_shelf(202.9, -6.06);
    peak.set_peaking(1063.9, 0.53, -6.02);
}

/// The guitar Distortion: a low cut with its zero in the right half plane (the lows come back inverted and 18 dB
/// down), a 1.5 kHz low pass, 30 dB more gain than the drive value says, a -6 dB high cut and a dip at 1 kHz after.
pub static GUITAR_DISTORTION: DrivePreset = DrivePreset {
    gain: (32.0 * 127.0) / 128.0,
    post_gain: &GUITAR_POST_GAIN,
    post_filter_gain: 1.0,
    design: guitar_distortion_design,
    design_bass: None,
};

/// The Distortion of the combinations, the guitar multis and the parallel types: the guitar Distortion with a plain
/// 678 Hz high pass in front instead of the low cut, and its own level compensation (`DISTORTION_DRIVE_B`).
pub static GUITAR_DISTORTION_HIGH_PASS: DrivePreset = DrivePreset {
    gain: (32.0 * 127.0) / 128.0,
    post_gain: &[35, 35, 22, 19, 18, 17],
    post_filter_gain: 1.0,
    design: |pre1, pre2, pre3, post, peak| {
        guitar_distortion_design(pre1, pre2, pre3, post, peak);
        pre1.set_high_pass(678.1, 1.0);
    },
    design_bass: None,
};

fn humanizer_design(
    pre1: &mut FirstOrderFilter,
    pre2: &mut FirstOrderFilter,
    pre3: &mut FirstOrderFilter,
    post: &mut FirstOrderFilter,
    peak: &mut Biquad,
) {
    pre1.set_off();
    pre2.set_off();
    pre3.set_off();
    post.set_off();
    peak.set_off();
}

const HUMANIZER_POST_GAIN: [u8; 11] = [77, 77, 38, 25, 19, 16, 14, 13, 12, 11, 10];

/// The Humanizer's drive: no filters around the clipper, a gain of 2 per drive step in front of it (6 dB more than
/// the drive value says) and its own level compensation.
pub static HUMANIZER_DRIVE: DrivePreset = DrivePreset {
    gain: 2.0,
    post_gain: &HUMANIZER_POST_GAIN,
    post_filter_gain: 127.0 / 128.0,
    design: humanizer_design,
    design_bass: None,
};

/// Rotary Multi's drive: the Humanizer's, but half the level after the clipper.
pub static ROTARY_MULTI_DRIVE: DrivePreset = DrivePreset {
    gain: 2.0,
    post_gain: &HUMANIZER_POST_GAIN,
    post_filter_gain: 0.5,
    design: humanizer_design,
    design_bass: None,
};

/// The level compensation of the bass drives is over 32, four times the guitar drives' scale.
const BASS_POST_SCALE: f64 = 4.0;

/// Bass Multi's Overdrive: a 371 Hz high pass in front, 18 dB more gain than the drive value says, a -20 dB high cut
/// after (from 371 Hz, its zero at 3.5 kHz) and its own level compensation (`BASS_DRIVE_B`).
pub static BASS_OVERDRIVE: DrivePreset = DrivePreset {
    gain: (8.0 * 127.0) / 128.0,
    post_gain: &[79, 79, 41, 30, 25, 22, 21, 20],
    post_filter_gain: ((BASS_POST_SCALE * 127.0) / 128.0) * 0.125,
    design: |pre1, pre2, pre3, post, peak| {
        pre1.set_high_pass(370.98, 1.004_05);
        pre2.set_off();
        pre3.set_off();
        post.set_pole_zero(3519.8, 370.98, 0.101_215);
        peak.set_off();
    },
    design_bass: None,
};

/// Bass Multi's Distortion: a 589 Hz high pass and a 2.1 kHz low pass in front, 30 dB more gain than the drive value
/// says, then a high shelf (+6 dB from 2 kHz), a dip at 1.4 kHz, a boost of the lows below 50 Hz and its own level
/// compensation (`BASS_DISTORTION_DRIVE_B`).
pub static BASS_DISTORTION: DrivePreset = DrivePreset {
    gain: 32.0,
    post_gain: &[16, 16, 10, 9, 8],
    post_filter_gain: ((BASS_POST_SCALE * 127.0) / 128.0) * 0.125,
    design: |pre1, pre2, pre3, post, peak| {
        pre1.set_high_pass(588.61, 0.991_736);
        pre2.set_off();
        pre3.set_off();
        post.set_pole_zero(1968.04, 3866.21, 2.010_93);
        peak.set_peaking(1417.1, 1.190_24, -6.0206);
    },
    design_bass: Some(|pre, post| {
        // A second order low pass, 0.941 at DC
        let w = pre.prewarp(2109.7);
        pre.set_analog(0.0, 0.0, 0.941_176 * w * w, 1.0, w / 0.735_465, w * w);
        // Unity at Nyquist, 3 (+9.5 dB) at DC
        let wp = post.prewarp(28.1846);
        let wz = post.prewarp(48.8166);
        post.set_analog(1.0, wz / 0.135_549, wz * wz, 1.0, wp / 0.782_248, wp * wp);
    }),
};

/// The drive of the Sound Canvas: filters, a gain, a hard clipper at full scale and a soft saturating waveshaper,
/// filters again and a level compensation.
///
/// At 32 kHz and up the clipper runs at the host rate. Below that it runs at twice the rate.
pub struct Drive {
    /// This sample's input after the DC blocker, for the path around a switched off drive.
    pub input: f64,
    dc_blocker: FirstOrderFilter,
    pre1: FirstOrderFilter,
    pre2: FirstOrderFilter,
    pre3: FirstOrderFilter,
    post: FirstOrderFilter,
    peak: Biquad,
    bass_pre: Biquad,
    bass_post: Biquad,
    oversampler: Option<Oversampler>,
    bass: bool,
    preset: &'static DrivePreset,
    gain: f64,
    post_gain: f64,
}

impl Drive {
    pub fn new(sample_rate: f64) -> Self {
        let mut dc_blocker = FirstOrderFilter::new(sample_rate);
        dc_blocker.set_high_pass(DC_BLOCKER_HZ, 1.0);
        Self {
            input: 0.0,
            dc_blocker,
            pre1: FirstOrderFilter::new(sample_rate),
            pre2: FirstOrderFilter::new(sample_rate),
            pre3: FirstOrderFilter::new(sample_rate),
            post: FirstOrderFilter::new(sample_rate),
            peak: Biquad::new(sample_rate),
            bass_pre: Biquad::new(sample_rate),
            bass_post: Biquad::new(sample_rate),
            oversampler: if sample_rate < 32_000.0 {
                Some(Oversampler::new())
            } else {
                None
            },
            bass: false,
            preset: &GUITAR_OVERDRIVE,
            gain: 0.0,
            post_gain: 0.0,
        }
    }

    /// The hard clip and the waveshaper: linear (x 0.875) below a quarter of full scale, then a quadratic
    /// saturation that flattens out at full scale.
    #[inline]
    fn shape(x: f64) -> f64 {
        let c = (-1.0f64).max(1.0f64.min(x));
        c * (0.875 - 0.5 * 0.0f64.max(0.992_187_5 * c.abs() - 0.25))
    }

    pub fn set_preset(&mut self, preset: &'static DrivePreset) {
        self.preset = preset;
        (preset.design)(&mut self.pre1, &mut self.pre2, &mut self.pre3, &mut self.post, &mut self.peak);
        self.bass = preset.design_bass.is_some();
        if let Some(design_bass) = preset.design_bass {
            design_bass(&mut self.bass_pre, &mut self.bass_post);
        } else {
            self.bass_pre.set_off();
            self.bass_post.set_off();
        }
    }

    /// Call after `set_preset`. `drive` is the drive parameter (0-127, 0 is the same as 1).
    pub fn set_drive(&mut self, drive: u8) {
        let p = self.preset;
        let d = drive.max(1) as usize;
        self.gain = p.gain * d as f64;
        self.post_gain = (p.post_gain[d.min(p.post_gain.len() - 1)] as f64 / 128.0) * p.post_filter_gain;
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        self.input = self.dc_blocker.process(x);
        let a = self.pre1.process(self.input);
        let b = self.pre2.process(a);
        let mut y = self.pre3.process(b);
        if self.bass {
            y = self.bass_pre.process(y);
        }
        y *= self.gain;
        if let Some(oversampler) = &mut self.oversampler {
            oversampler.up(y);
            let (first, second) = (oversampler.first, oversampler.second);
            y = oversampler.down(Self::shape(first), Self::shape(second));
        } else {
            y = Self::shape(y);
        }
        let posted = self.post.process(y);
        y = self.peak.process(posted);
        if self.bass {
            y = self.bass_post.process(y);
        }
        self.post_gain * y
    }

    pub fn clear(&mut self) {
        self.dc_blocker.clear();
        self.input = 0.0;
        self.pre1.clear();
        self.pre2.clear();
        self.pre3.clear();
        self.post.clear();
        self.peak.clear();
        self.bass_pre.clear();
        self.bass_post.clear();
        if let Some(oversampler) = &mut self.oversampler {
            oversampler.clear();
        }
    }
}
