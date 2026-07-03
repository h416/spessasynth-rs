#!/usr/bin/env python3
"""
sample_diff.py - Sample-level WAV diff analysis tool.

Identifies the exact nature of differences between two WAV files:
- Phase shift: waveforms match after N-sample shift
- Amplitude difference: same shape but different scale
- Complete mismatch: entirely different waveforms

Usage:
    python tools/sample_diff.py <wav1> <wav2> --start 69 --end 76
"""

import argparse
import sys
import numpy as np
import wave
import struct


def read_wav_as_int16(path):
    """Read a WAV file and return (sample_rate, data) where data is int16 numpy array (interleaved stereo)."""
    with wave.open(path, 'rb') as wf:
        n_channels = wf.getnchannels()
        sampwidth = wf.getsampwidth()
        sample_rate = wf.getframerate()
        n_frames = wf.getnframes()
        raw = wf.readframes(n_frames)

    if sampwidth == 2:
        data = np.frombuffer(raw, dtype=np.int16)
    elif sampwidth == 3:
        # 24-bit: convert to int32 then scale to int16
        n_samples = len(raw) // 3
        data = np.zeros(n_samples, dtype=np.int32)
        for i in range(n_samples):
            b = raw[i*3:(i+1)*3]
            val = int.from_bytes(b, byteorder='little', signed=True)
            data[i] = val
        data = (data >> 8).astype(np.int16)
    else:
        print(f"Error: Unsupported sample width {sampwidth}", file=sys.stderr)
        sys.exit(1)

    # Reshape to (n_frames, n_channels)
    data = data.reshape(-1, n_channels)
    return sample_rate, data


def find_top_diffs(diff, n=10):
    """Find top-N positions with largest absolute differences."""
    abs_diff = np.abs(diff)
    # Get max across channels
    max_per_sample = np.max(abs_diff, axis=1)
    indices = np.argsort(max_per_sample)[::-1][:n]
    return indices


def analyze_phase_shift(data1, data2, channel, window=200, max_shift=50):
    """Check if data2 is a phase-shifted version of data1.

    Returns (best_shift, correlation) or None if no good match.
    """
    # Use the region around the max diff
    n = len(data1)
    if n < window:
        return None

    ch1 = data1[:, channel].astype(np.float64)
    ch2 = data2[:, channel].astype(np.float64)

    # Try different shifts
    best_shift = 0
    best_corr = -1.0
    base_energy = np.sqrt(np.sum(ch1**2) * np.sum(ch2**2))
    if base_energy < 1e-10:
        return None

    for shift in range(-max_shift, max_shift + 1):
        if shift >= 0:
            s1 = ch1[shift:]
            s2 = ch2[:n - shift]
        else:
            s1 = ch1[:n + shift]
            s2 = ch2[-shift:]

        if len(s1) == 0:
            continue
        corr = np.sum(s1 * s2) / base_energy
        if corr > best_corr:
            best_corr = corr
            best_shift = shift

    return best_shift, best_corr


def analyze_amplitude_ratio(data1, data2, channel):
    """Check if data2 = scale * data1 (amplitude difference).

    Returns (ratio, r_squared) or None.
    """
    ch1 = data1[:, channel].astype(np.float64)
    ch2 = data2[:, channel].astype(np.float64)

    # Only use samples where both have significant values
    mask = (np.abs(ch1) > 10) & (np.abs(ch2) > 10)
    if np.sum(mask) < 100:
        return None

    ch1_m = ch1[mask]
    ch2_m = ch2[mask]

    # Least squares: ratio = sum(ch1*ch2) / sum(ch1*ch1)
    denom = np.sum(ch1_m * ch1_m)
    if denom < 1e-10:
        return None
    ratio = np.sum(ch1_m * ch2_m) / denom

    # R-squared
    predicted = ch1_m * ratio
    ss_res = np.sum((ch2_m - predicted) ** 2)
    ss_tot = np.sum((ch2_m - np.mean(ch2_m)) ** 2)
    if ss_tot < 1e-10:
        return None
    r_squared = 1.0 - ss_res / ss_tot

    return ratio, r_squared


def print_samples_around(data1, data2, sample_idx, sample_rate, context=10, offset=0):
    """Print sample values around a given index."""
    n = len(data1)
    start = max(0, sample_idx - context)
    end = min(n, sample_idx + context + 1)

    time = (sample_idx + offset) / sample_rate
    print(f"\n  Sample #{sample_idx + offset} (t={time:.6f}s):")
    print(f"  {'idx':>8} | {'TS L':>8} {'TS R':>8} | {'RS L':>8} {'RS R':>8} | {'dL':>8} {'dR':>8}")
    print(f"  {'-'*8}-+-{'-'*8}-{'-'*8}-+-{'-'*8}-{'-'*8}-+-{'-'*8}-{'-'*8}")

    for i in range(start, end):
        marker = " <<" if i == sample_idx else ""
        global_idx = i + offset
        d_l = int(data1[i, 0]) - int(data2[i, 0])
        d_r = int(data1[i, 1]) - int(data2[i, 1])
        print(f"  {global_idx:>8} | {data1[i,0]:>8} {data1[i,1]:>8} | {data2[i,0]:>8} {data2[i,1]:>8} | {d_l:>8} {d_r:>8}{marker}")


def main():
    parser = argparse.ArgumentParser(description="Sample-level WAV diff analysis")
    parser.add_argument("wav1", help="First WAV file (TS)")
    parser.add_argument("wav2", help="Second WAV file (Rust)")
    parser.add_argument("--start", type=float, default=0, help="Start time in seconds")
    parser.add_argument("--end", type=float, default=None, help="End time in seconds")
    parser.add_argument("--top", type=int, default=10, help="Number of top diff positions to show")
    parser.add_argument("--context", type=int, default=10, help="Context samples around each diff")
    args = parser.parse_args()

    print(f"Loading {args.wav1}...")
    sr1, data1 = read_wav_as_int16(args.wav1)
    print(f"Loading {args.wav2}...")
    sr2, data2 = read_wav_as_int16(args.wav2)

    if sr1 != sr2:
        print(f"Error: Sample rates differ ({sr1} vs {sr2})", file=sys.stderr)
        sys.exit(1)

    if data1.shape[1] != data2.shape[1]:
        print(f"Error: Channel counts differ ({data1.shape[1]} vs {data2.shape[1]})", file=sys.stderr)
        sys.exit(1)

    sr = sr1
    n_channels = data1.shape[1]
    min_len = min(len(data1), len(data2))

    # Determine sample range
    start_sample = int(args.start * sr)
    end_sample = int(args.end * sr) if args.end else min_len
    start_sample = max(0, min(start_sample, min_len))
    end_sample = max(start_sample, min(end_sample, min_len))

    print(f"\nSample rate: {sr} Hz, Channels: {n_channels}")
    print(f"Analysis range: {args.start:.2f}s - {end_sample/sr:.2f}s "
          f"(samples {start_sample} - {end_sample}, {end_sample - start_sample} samples)")

    # Extract range
    seg1 = data1[start_sample:end_sample]
    seg2 = data2[start_sample:end_sample]
    diff = seg1.astype(np.int32) - seg2.astype(np.int32)

    # Overall statistics
    abs_diff = np.abs(diff)
    max_diff = np.max(abs_diff)
    mean_diff = np.mean(abs_diff)
    nonzero_count = np.count_nonzero(np.max(abs_diff, axis=1))
    total_samples = len(seg1)

    print(f"\n{'='*70}")
    print(f"OVERALL STATISTICS")
    print(f"{'='*70}")
    print(f"  max_diff:    {max_diff}")
    print(f"  mean_diff:   {mean_diff:.2f}")
    print(f"  nonzero samples: {nonzero_count}/{total_samples} ({100*nonzero_count/total_samples:.1f}%)")

    # Per-channel stats
    for ch in range(n_channels):
        ch_name = "L" if ch == 0 else "R"
        ch_diff = np.abs(diff[:, ch])
        print(f"  ch{ch_name} max_diff: {np.max(ch_diff)}, mean: {np.mean(ch_diff):.2f}, "
              f"rms: {np.sqrt(np.mean(ch_diff.astype(np.float64)**2)):.2f}")

    # Time distribution of differences
    print(f"\n{'='*70}")
    print(f"TIME DISTRIBUTION (0.5s bins)")
    print(f"{'='*70}")
    bin_size = int(0.5 * sr)
    n_bins = (end_sample - start_sample + bin_size - 1) // bin_size
    for b in range(n_bins):
        b_start = b * bin_size
        b_end = min((b + 1) * bin_size, len(seg1))
        bin_diff = abs_diff[b_start:b_end]
        bin_max = np.max(bin_diff)
        bin_mean = np.mean(bin_diff)
        if bin_max > 100:  # Only show bins with significant differences
            time_start = (start_sample + b_start) / sr
            time_end = (start_sample + b_end) / sr
            print(f"  {time_start:.2f}s-{time_end:.2f}s: max={bin_max:>6}, mean={bin_mean:>8.2f}")

    # Top diff positions
    print(f"\n{'='*70}")
    print(f"TOP {args.top} MAX DIFF POSITIONS")
    print(f"{'='*70}")
    top_indices = find_top_diffs(diff, args.top)
    for rank, idx in enumerate(top_indices):
        max_at = np.max(np.abs(diff[idx]))
        time = (start_sample + idx) / sr
        print(f"\n--- #{rank+1}: diff={max_at} at sample {start_sample + idx} (t={time:.6f}s) ---")
        print_samples_around(seg1, seg2, idx, sr, context=args.context, offset=start_sample)

    # Pattern analysis
    print(f"\n{'='*70}")
    print(f"PATTERN ANALYSIS")
    print(f"{'='*70}")

    # Focus on the region with largest differences for pattern analysis
    # Find 1-second window with maximum total difference
    window_size = sr  # 1 second
    if len(seg1) >= window_size:
        max_per_sample = np.max(abs_diff, axis=1).astype(np.float64)
        # Sliding window sum
        cumsum = np.cumsum(max_per_sample)
        window_sums = cumsum[window_size:] - cumsum[:-window_size]
        if len(window_sums) > 0:
            best_window_start = np.argmax(window_sums)
            hot_start = best_window_start
            hot_end = hot_start + window_size
        else:
            hot_start = 0
            hot_end = len(seg1)
    else:
        hot_start = 0
        hot_end = len(seg1)

    hot_seg1 = seg1[hot_start:hot_end]
    hot_seg2 = seg2[hot_start:hot_end]
    hot_time = (start_sample + hot_start) / sr

    print(f"\nHot zone: {hot_time:.2f}s - {(start_sample + hot_end)/sr:.2f}s")

    for ch in range(n_channels):
        ch_name = "L" if ch == 0 else "R"
        print(f"\n  Channel {ch_name}:")

        # Phase shift analysis
        phase_result = analyze_phase_shift(hot_seg1, hot_seg2, ch,
                                            window=min(len(hot_seg1), 4000), max_shift=50)
        if phase_result:
            shift, corr = phase_result
            print(f"    Phase shift: best_shift={shift} samples, correlation={corr:.6f}")
            if corr > 0.95 and shift != 0:
                print(f"    >>> LIKELY PHASE SHIFT of {shift} samples ({shift/sr*1000:.3f} ms)")
            elif corr > 0.95 and shift == 0:
                print(f"    >>> Waveforms are correlated (no phase shift)")

        # Amplitude ratio analysis
        amp_result = analyze_amplitude_ratio(hot_seg1, hot_seg2, ch)
        if amp_result:
            ratio, r2 = amp_result
            print(f"    Amplitude ratio (TS/Rust): {ratio:.6f}, R²={r2:.6f}")
            if r2 > 0.95 and abs(ratio - 1.0) > 0.01:
                print(f"    >>> LIKELY AMPLITUDE DIFFERENCE: Rust = {1/ratio:.4f}x TS")
            elif r2 < 0.5:
                print(f"    >>> LIKELY COMPLETE MISMATCH (low correlation)")

    # Zero-crossing analysis to detect voice presence differences
    print(f"\n{'='*70}")
    print(f"VOICE ACTIVITY ANALYSIS (0.1s bins in hot zone)")
    print(f"{'='*70}")
    activity_bin = int(0.1 * sr)
    for b_start in range(hot_start, hot_end, activity_bin):
        b_end = min(b_start + activity_bin, hot_end)
        s1 = seg1[b_start:b_end, 0].astype(np.float64)
        s2 = seg2[b_start:b_end, 0].astype(np.float64)
        rms1 = np.sqrt(np.mean(s1**2))
        rms2 = np.sqrt(np.mean(s2**2))
        d = abs_diff[b_start:b_end]
        d_max = np.max(d)

        if d_max > 500 or abs(rms1 - rms2) > 100:
            time = (start_sample + b_start) / sr
            rms_ratio = rms2 / rms1 if rms1 > 1 else float('inf')
            print(f"  {time:.2f}s: TS_rms={rms1:.1f} RS_rms={rms2:.1f} ratio={rms_ratio:.4f} max_diff={d_max}")

    print(f"\n{'='*70}")
    print("DONE")


if __name__ == "__main__":
    main()
