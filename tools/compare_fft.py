#!/usr/bin/env python3
"""Compare two WAV files in the frequency domain using FFT."""
import sys
import wave
import numpy as np

def read_wav_samples(path):
    with wave.open(path, 'rb') as w:
        nch = w.getnchannels()
        sw = w.getsampwidth()
        sr = w.getframerate()
        nf = w.getnframes()
        raw = w.readframes(nf)
    samples = np.frombuffer(raw, dtype=np.int16).reshape(-1, nch).astype(np.float64)
    return samples, sr, nf

def main():
    if len(sys.argv) != 3:
        print("Usage: compare_fft.py <wav1> <wav2>")
        sys.exit(1)

    s1, sr1, nf1 = read_wav_samples(sys.argv[1])
    s2, sr2, nf2 = read_wav_samples(sys.argv[2])

    print(f"File 1: {sys.argv[1]}")
    print(f"  Sample rate: {sr1}, Frames: {nf1}, Channels: {s1.shape[1]}")
    print(f"File 2: {sys.argv[2]}")
    print(f"  Sample rate: {sr2}, Frames: {nf2}, Channels: {s2.shape[1]}")

    if sr1 != sr2:
        print("ERROR: Sample rates differ")
        sys.exit(1)

    # Use minimum length
    n = min(nf1, nf2)
    s1 = s1[:n]
    s2 = s2[:n]

    # Time-domain comparison
    diff = np.abs(s1 - s2)
    print(f"\n=== Time Domain (first {n} samples) ===")
    print(f"  Mean absolute diff: {diff.mean():.2f}")
    print(f"  Max absolute diff:  {diff.max():.0f}")
    exact = np.sum(diff == 0)
    total = diff.size
    print(f"  Exact matches: {exact}/{total} ({100*exact/total:.2f}%)")

    # FFT comparison per channel
    for ch in range(s1.shape[1]):
        ch_name = "Left" if ch == 0 else "Right"
        f1 = np.fft.rfft(s1[:, ch])
        f2 = np.fft.rfft(s2[:, ch])

        mag1 = np.abs(f1)
        mag2 = np.abs(f2)

        # Spectral correlation (magnitude)
        if np.linalg.norm(mag1) > 0 and np.linalg.norm(mag2) > 0:
            corr = np.dot(mag1, mag2) / (np.linalg.norm(mag1) * np.linalg.norm(mag2))
        else:
            corr = 0.0

        # Magnitude difference
        mag_diff = np.abs(mag1 - mag2)

        # Phase difference (weighted by magnitude to focus on significant frequencies)
        phase1 = np.angle(f1)
        phase2 = np.angle(f2)
        phase_diff = np.abs(np.angle(np.exp(1j * (phase1 - phase2))))
        weights = (mag1 + mag2) / 2.0
        if weights.sum() > 0:
            weighted_phase_diff = np.average(phase_diff, weights=weights)
        else:
            weighted_phase_diff = 0.0

        print(f"\n=== FFT {ch_name} Channel ===")
        print(f"  Spectral correlation: {corr:.6f}")
        print(f"  Mean magnitude diff:  {mag_diff.mean():.2f}")
        print(f"  Max magnitude diff:   {mag_diff.max():.2f}")
        print(f"  Weighted phase diff:  {weighted_phase_diff:.4f} rad ({np.degrees(weighted_phase_diff):.2f} deg)")

        # Top 10 frequency bins with largest magnitude difference
        top_idx = np.argsort(mag_diff)[-10:][::-1]
        freqs = np.fft.rfftfreq(n, d=1.0/sr1)
        print(f"  Top 10 freq bins with largest diff:")
        for idx in top_idx:
            print(f"    {freqs[idx]:8.1f} Hz: mag1={mag1[idx]:.0f}, mag2={mag2[idx]:.0f}, diff={mag_diff[idx]:.0f}")

if __name__ == "__main__":
    main()
