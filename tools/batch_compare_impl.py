#!/usr/bin/env python3
"""Batch WAV comparison: TS vs Rust output."""

import sys
import wave
import struct
import numpy as np
from datetime import datetime
from pathlib import Path


def read_wav_stereo(path: str) -> tuple[np.ndarray, np.ndarray, int]:
    """Read a WAV file and return (left, right, sample_rate) as int16 arrays."""
    with wave.open(path, "rb") as w:
        n_channels = w.getnchannels()
        sample_width = w.getsampwidth()
        sample_rate = w.getframerate()
        n_frames = w.getnframes()
        raw = w.readframes(n_frames)

    if sample_width == 2:
        fmt = f"<{n_frames * n_channels}h"
        samples = np.array(struct.unpack(fmt, raw), dtype=np.float64)
    else:
        raise ValueError(f"Unsupported sample width: {sample_width}")

    if n_channels == 2:
        left = samples[0::2]
        right = samples[1::2]
    elif n_channels == 1:
        left = samples
        right = samples.copy()
    else:
        raise ValueError(f"Unsupported channel count: {n_channels}")

    return left, right, sample_rate


def rms(arr: np.ndarray) -> float:
    """Compute RMS of an array."""
    if len(arr) == 0:
        return 0.0
    return float(np.sqrt(np.mean(arr ** 2)))


def correlation(a: np.ndarray, b: np.ndarray) -> float:
    """Compute Pearson correlation coefficient."""
    if len(a) == 0 or len(b) == 0:
        return 0.0
    a_mean = a - np.mean(a)
    b_mean = b - np.mean(b)
    denom = np.sqrt(np.sum(a_mean ** 2) * np.sum(b_mean ** 2))
    if denom == 0:
        return 1.0
    return float(np.sum(a_mean * b_mean) / denom)


def compare_pair(ts_path: str, rs_path: str) -> dict:
    """Compare a pair of WAV files."""
    ts_size = Path(ts_path).stat().st_size
    rs_size = Path(rs_path).stat().st_size

    ts_l, ts_r, ts_sr = read_wav_stereo(ts_path)
    rs_l, rs_r, rs_sr = read_wav_stereo(rs_path)

    ts_duration = len(ts_l) / ts_sr
    rs_duration = len(rs_l) / rs_sr

    # Align lengths (use shorter)
    min_len = min(len(ts_l), len(rs_l))
    ts_l, ts_r = ts_l[:min_len], ts_r[:min_len]
    rs_l, rs_r = rs_l[:min_len], rs_r[:min_len]

    diff_l = ts_l - rs_l
    diff_r = ts_r - rs_r

    rms_l = rms(diff_l)
    rms_r = rms(diff_r)
    max_l = float(np.max(np.abs(diff_l))) if min_len > 0 else 0.0
    max_r = float(np.max(np.abs(diff_r))) if min_len > 0 else 0.0
    corr_l = correlation(ts_l, rs_l)
    corr_r = correlation(ts_r, rs_r)

    # 1-second window RMS diff profile
    sample_rate = ts_sr
    window = sample_rate  # 1 second
    worst_start = 0.0
    worst_rms = 0.0
    worst_ch = "L"

    for ch_label, diff in [("L", diff_l), ("R", diff_r)]:
        for i in range(0, len(diff), window):
            chunk = diff[i : i + window]
            r = rms(chunk)
            if r > worst_rms:
                worst_rms = r
                worst_start = i / sample_rate
                worst_ch = ch_label

    worst_end = worst_start + 1.0

    # Judgment
    if rms_l < 10 and rms_r < 10 and max_l < 500 and max_r < 500:
        result = "PASS"
    elif rms_l < 50 and rms_r < 50 and max_l < 2000 and max_r < 2000:
        result = "WARN"
    else:
        result = "FAIL"

    return {
        "ts_size": ts_size,
        "rs_size": rs_size,
        "ts_duration": ts_duration,
        "rs_duration": rs_duration,
        "rms_l": rms_l,
        "rms_r": rms_r,
        "max_l": max_l,
        "max_r": max_r,
        "corr_l": corr_l,
        "corr_r": corr_r,
        "worst_start": worst_start,
        "worst_end": worst_end,
        "worst_rms": worst_rms,
        "worst_ch": worst_ch,
        "result": result,
    }


def format_report(results: dict[str, dict]) -> str:
    """Format comparison results as a report."""
    lines = []
    lines.append("=== WAV Comparison Report ===")
    lines.append(f"Date: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    lines.append("")

    counts = {"PASS": 0, "WARN": 0, "FAIL": 0}

    for name, r in results.items():
        counts[r["result"]] = counts.get(r["result"], 0) + 1
        lines.append(f"--- {name} ---")
        lines.append(f"Duration: TS={r['ts_duration']:.2f}s  RS={r['rs_duration']:.2f}s")
        lines.append(f"Size: TS={r['ts_size']}  RS={r['rs_size']}")
        lines.append(f"RMS diff:  L={r['rms_l']:.1f}  R={r['rms_r']:.1f}")
        lines.append(f"Max diff:  L={r['max_l']:.0f}  R={r['max_r']:.0f}")
        lines.append(f"Correlation: L={r['corr_l']:.5f}  R={r['corr_r']:.5f}")
        lines.append(
            f"Worst 1s window: {r['worst_start']:.1f}-{r['worst_end']:.1f}s  "
            f"RMS_{r['worst_ch']}={r['worst_rms']:.1f}"
        )
        lines.append(f"Result: {r['result']}")
        lines.append("")

    total = sum(counts.values())
    lines.append("--- Summary ---")
    lines.append(f"PASS: {counts['PASS']}/{total}  WARN: {counts['WARN']}/{total}  FAIL: {counts['FAIL']}/{total}")

    return "\n".join(lines)


def main():
    if len(sys.argv) < 4:
        print("Usage: batch_compare_impl.py <ts_dir> <rs_dir> <name1> [name2 ...]")
        sys.exit(1)

    ts_dir = sys.argv[1]
    rs_dir = sys.argv[2]
    names = sys.argv[3:]

    results = {}
    for name in names:
        ts_path = f"{ts_dir}/{name}.wav"
        rs_path = f"{rs_dir}/{name}.wav"

        if not Path(ts_path).exists():
            print(f"  SKIP: {ts_path} not found")
            continue
        if not Path(rs_path).exists():
            print(f"  SKIP: {rs_path} not found")
            continue

        print(f"  Comparing: {name}")
        results[name] = compare_pair(ts_path, rs_path)

    report = format_report(results)
    print("")
    print(report)

    # Save report
    report_path = Path(ts_dir).parent / "compare_report.txt"
    report_path.write_text(report + "\n")
    print(f"\nReport saved to: {report_path}")


if __name__ == "__main__":
    main()
