"""Compare combined WAV vs sum of individual channel WAVs.

Reports if the combined output equals the sum of individual outputs,
or if there's a non-linear channel interaction.
"""
import sys
import numpy as np
import wave


def read_wav(path):
    with wave.open(path, 'r') as w:
        n = w.getnframes()
        sr = w.getframerate()
        data = np.frombuffer(w.readframes(n), dtype=np.int16).reshape(-1, 2)
        return data, sr


def main():
    combined_path = sys.argv[1]
    individual_paths = sys.argv[2:]

    combined, sr = read_wav(combined_path)
    print(f"Combined: {combined_path} ({len(combined)} frames)")

    # Sum individual channels
    summed = None
    for path in individual_paths:
        data, _ = read_wav(path)
        print(f"Individual: {path} ({len(data)} frames)")
        if summed is None:
            summed = data.astype(np.int32)
        else:
            min_len = min(len(summed), len(data))
            summed[:min_len] += data[:min_len].astype(np.int32)

    # Compare combined vs sum
    min_len = min(len(combined), len(summed))
    combined_32 = combined[:min_len].astype(np.int32)
    summed_32 = summed[:min_len]

    diff = combined_32 - summed_32
    max_abs_diff = np.max(np.abs(diff))
    print(f"\nMax abs difference (combined vs sum): {max_abs_diff}")

    if max_abs_diff == 0:
        print("RESULT: Combined output = sum of individuals (LINEAR, no interaction)")
    else:
        print(f"RESULT: NON-LINEAR interaction detected! Max diff = {max_abs_diff}")
        # Find where the biggest differences are
        abs_diff_per_frame = np.max(np.abs(diff), axis=1)
        top_indices = np.argsort(abs_diff_per_frame)[-10:][::-1]
        print("\nTop 10 differences (by frame):")
        for idx in top_indices:
            t = idx / sr
            print(f"  t={t:.3f}s frame={idx}: combined=({combined_32[idx,0]},{combined_32[idx,1]}) "
                  f"sum=({summed_32[idx,0]},{summed_32[idx,1]}) "
                  f"diff=({diff[idx,0]},{diff[idx,1]})")

        # Show window profile around the max difference
        max_frame = top_indices[0]
        start = max(0, max_frame - int(sr * 0.5))
        end = min(min_len, max_frame + int(sr * 0.5))
        window_diff = abs_diff_per_frame[start:end]
        print(f"\n0.1s window profile around max diff ({max_frame/sr:.3f}s):")
        step = int(sr * 0.1)
        for i in range(0, len(window_diff), step):
            chunk = window_diff[i:i+step]
            t = (start + i) / sr
            print(f"  t={t:.3f}s: max_diff={np.max(chunk)}")


if __name__ == '__main__':
    main()
