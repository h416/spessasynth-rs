"""Find exact time range and profile of differences between two WAVs."""
import sys
import wave
import numpy as np


def read_wav(path):
    with wave.open(path, 'r') as w:
        n = w.getnframes()
        sr = w.getframerate()
        data = np.frombuffer(w.readframes(n), dtype=np.int16).reshape(-1, 2)
        return data, sr


def main():
    ts_path = sys.argv[1]
    rs_path = sys.argv[2]

    ts, sr = read_wav(ts_path)
    rs, _ = read_wav(rs_path)
    min_len = min(len(ts), len(rs))
    delta = rs[:min_len].astype(np.int32) - ts[:min_len].astype(np.int32)
    abs_delta = np.max(np.abs(delta), axis=1)

    # Find all frames where diff > 100
    big_frames = np.where(abs_delta > 100)[0]
    if len(big_frames) > 0:
        print(f"Total frames with diff>100: {len(big_frames)}")
        t_start = big_frames[0] / sr
        t_end = big_frames[-1] / sr
        print(f"Range: {t_start:.4f}s to {t_end:.4f}s")
        # Show max per 0.1s window
        for t in np.arange(t_start - 0.1, t_end + 0.2, 0.1):
            s = int(t * sr)
            e = int((t + 0.1) * sr)
            if s >= 0 and e <= min_len:
                chunk = abs_delta[s:e]
                if np.max(chunk) > 0:
                    print(f"  t={t:.2f}-{t+0.1:.2f}s: max_diff={np.max(chunk)}, mean_diff={np.mean(chunk):.1f}")
    else:
        print("No frames with diff > 100")

    # Also show frames with diff > 10
    small_frames = np.where(abs_delta > 10)[0]
    if len(small_frames) > 0:
        print(f"\nTotal frames with diff>10: {len(small_frames)}")
        t_s = small_frames[0] / sr
        t_e = small_frames[-1] / sr
        print(f"Range: {t_s:.4f}s to {t_e:.4f}s")


if __name__ == "__main__":
    main()
