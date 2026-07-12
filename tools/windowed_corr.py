#!/usr/bin/env python3
"""windowed_corr.py - Per-window correlation / max-diff between two WAVs.

Splits both WAVs into fixed-length time windows and prints, per window, the
Pearson correlation and max sample difference for L and R. This is the fastest
triage for TS-vs-Rust divergence:

  * If specific windows drop (e.g. 0.90) then recover to 1.000, the divergence
    is a SPECIFIC instrument / event playing in those windows (not float drift).
    Follow up with tools/isolate_channel.py + tools/gen_single_note.py.
  * If correlation decays monotonically, it is an accumulating effect
    (e.g. a reverb/feedback tail).

Usage:
    python tools/windowed_corr.py <ts.wav> <rs.wav> [--win 10] [--thresh 0.999]

Options:
    --win     window length in seconds (default: 10)
    --thresh  flag windows whose correlation is below this (default: 0.9999)
"""
import argparse
import sys
import wave
import numpy as np


def read_wav(path):
    with wave.open(path, "r") as w:
        sr = w.getframerate()
        n = w.getnframes()
        ch = w.getnchannels()
        d = np.frombuffer(w.readframes(n), dtype=np.int16).astype(np.float64)
    if ch == 2:
        d = d.reshape(-1, 2)
    else:
        d = np.column_stack([d, d])
    return d, sr


def main():
    ap = argparse.ArgumentParser(description="Per-window correlation between two WAVs.")
    ap.add_argument("ts_wav")
    ap.add_argument("rs_wav")
    ap.add_argument("--win", type=float, default=10.0, help="window seconds (default 10)")
    ap.add_argument("--thresh", type=float, default=0.9999, help="flag corr below this")
    args = ap.parse_args()

    ts, sr = read_wav(args.ts_wav)
    rs, sr2 = read_wav(args.rs_wav)
    if sr != sr2:
        print(f"WARNING: sample rates differ: {sr} vs {sr2}", file=sys.stderr)
    n = min(len(ts), len(rs))
    ts, rs = ts[:n], rs[:n]

    w = int(args.win * sr)
    if w <= 0:
        print("window too small", file=sys.stderr)
        sys.exit(1)

    print(f"win={args.win}s  samples={n}  dur={n / sr:.1f}s  (flag corr < {args.thresh})")
    print(f"{'t(s)':>7s}  {'Lcorr':>8s} {'Lmax':>6s}   {'Rcorr':>8s} {'Rmax':>6s}")
    overall_max = 0
    worst = (1.0, None)
    for i in range(0, n - 1, w):
        seg_ts = ts[i:i + w]
        seg_rs = rs[i:i + w]
        t = i / sr
        cols = []
        silent = True
        for c in (0, 1):
            a = seg_ts[:, c]
            b = seg_rs[:, c]
            rms = np.sqrt((a * a).mean())
            md = int(np.abs(a - b).max())
            overall_max = max(overall_max, md)
            if rms < 20:
                cols.append(("  --  ", md))
                continue
            silent = False
            corr = np.corrcoef(a, b)[0, 1]
            cols.append((f"{corr:8.5f}", md))
            if corr < worst[0]:
                worst = (corr, t)
        flag = ""
        if not silent:
            worst_c = min(
                (float(c[0]) for c in cols if c[0].strip() != "--"),
                default=1.0,
            )
            if worst_c < args.thresh:
                flag = "  <-- diverges"
        print(f"{t:7.1f}  {cols[0][0]:>8s} {cols[0][1]:6d}   {cols[1][0]:>8s} {cols[1][1]:6d}{flag}")

    print("-" * 50)
    print(f"overall max sample diff: {overall_max}")
    if worst[1] is not None:
        print(f"worst window: t={worst[1]:.1f}s  corr={worst[0]:.5f}")


if __name__ == "__main__":
    main()
