#!/usr/bin/env python3
"""Compute the mean gain ratio (Rust/TS) for each test WAV pair."""
import argparse
import sys
import numpy as np
import os, glob


def analyze_pair(ts_path, rs_path):
    name = os.path.basename(ts_path)
    ts_raw = np.fromfile(ts_path, dtype=np.uint8)
    rs_raw = np.fromfile(rs_path, dtype=np.uint8)

    # Skip WAV header (44 bytes)
    ts_samples = np.frombuffer(ts_raw[44:], dtype=np.int16).astype(np.float64)
    rs_samples = np.frombuffer(rs_raw[44:], dtype=np.int16).astype(np.float64)

    min_len = min(len(ts_samples), len(rs_samples))
    ts_s = ts_samples[:min_len]
    rs_s = rs_samples[:min_len]

    # Only look at samples where both are above a threshold (to avoid noise)
    mask = (np.abs(ts_s) > 100) & (np.abs(rs_s) > 100)
    if mask.sum() < 1000:
        print(f"{name}: too few loud samples ({mask.sum()})")
        return

    ts_loud = ts_s[mask]
    rs_loud = rs_s[mask]

    # Compute per-sample ratio
    ratios = rs_loud / ts_loud

    # Filter out extreme outliers (> 2x or < 0.5x)
    valid = (ratios > 0.5) & (ratios < 2.0)
    ratios_clean = ratios[valid]

    if len(ratios_clean) < 100:
        print(f"{name}: too few valid ratios")
        return

    mean_ratio = np.mean(ratios_clean)
    median_ratio = np.median(ratios_clean)
    std_ratio = np.std(ratios_clean)

    print(f"{name}: mean_ratio={mean_ratio:.8f} median={median_ratio:.8f} std={std_ratio:.8f} n={len(ratios_clean)}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Compute gain ratio (Rust/TS) for WAV file(s)")
    parser.add_argument("ts_path", help="TypeScript WAV file or directory")
    parser.add_argument("rs_path", help="Rust WAV file or directory")
    args = parser.parse_args()

    if os.path.isfile(args.ts_path) and os.path.isfile(args.rs_path):
        analyze_pair(args.ts_path, args.rs_path)
    elif os.path.isdir(args.ts_path) and os.path.isdir(args.rs_path):
        for ts_wav in sorted(glob.glob(os.path.join(args.ts_path, "*.wav"))):
            name = os.path.basename(ts_wav)
            rs_wav = os.path.join(args.rs_path, name)
            if os.path.exists(rs_wav):
                analyze_pair(ts_wav, rs_wav)
    else:
        print("Error: both arguments must be files or both must be directories")
        sys.exit(1)
