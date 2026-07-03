"""compare_samples.py - Compare actual waveform samples between two WAV files at a given time range."""
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
    if len(sys.argv) < 3:
        print("Usage: compare_samples.py <ts_wav> <rs_wav> [start_sec] [end_sec]")
        sys.exit(1)

    ts_path = sys.argv[1]
    rs_path = sys.argv[2]
    start_sec = float(sys.argv[3]) if len(sys.argv) > 3 else 88.0
    end_sec = float(sys.argv[4]) if len(sys.argv) > 4 else 93.0

    ts, sr = read_wav(ts_path)
    rs, sr2 = read_wav(rs_path)

    start = int(start_sec * sr)
    end = int(end_sec * sr)

    ts_seg = ts[start:end]
    rs_seg = rs[start:end]
    diff = rs_seg.astype(np.int32) - ts_seg.astype(np.int32)

    print(f'Sample range: {start} to {end} ({sr} Hz)')
    print(f'Time range: {start_sec}s to {end_sec}s')
    print(f'Max abs diff L: {np.max(np.abs(diff[:, 0]))}')
    print(f'Max abs diff R: {np.max(np.abs(diff[:, 1]))}')
    print(f'RMS diff L: {np.sqrt(np.mean(diff[:, 0].astype(np.float64)**2)):.2f}')
    print(f'RMS diff R: {np.sqrt(np.mean(diff[:, 1].astype(np.float64)**2)):.2f}')

    # Find where the biggest differences are
    abs_diff_l = np.abs(diff[:, 0])
    top_indices = np.argsort(abs_diff_l)[-10:][::-1]
    print(f'\nTop 10 biggest difference positions (L):')
    for idx in top_indices:
        t = (start + idx) / sr
        print(f'  time={t:.4f}s  ts={ts_seg[idx, 0]}  rs={rs_seg[idx, 0]}  diff={diff[idx, 0]}')

    # Check peak levels of both versions
    print(f'\nPeak levels {start_sec}-{end_sec}s:')
    print(f'  TS  L: min={ts_seg[:, 0].min()} max={ts_seg[:, 0].max()}')
    print(f'  TS  R: min={ts_seg[:, 1].min()} max={ts_seg[:, 1].max()}')
    print(f'  RS  L: min={rs_seg[:, 0].min()} max={rs_seg[:, 0].max()}')
    print(f'  RS  R: min={rs_seg[:, 1].min()} max={rs_seg[:, 1].max()}')

    # Check for clipping (values at +-32767)
    ts_clip = np.sum(np.abs(ts_seg) >= 32767)
    rs_clip = np.sum(np.abs(rs_seg) >= 32767)
    print(f'\nClipping samples (|val|>=32767):  TS={ts_clip}  RS={rs_clip}')

    # Show difference profile in 0.5s windows
    window = int(0.5 * sr)
    print(f'\nDifference profile (0.5s windows):')
    for i in range(0, len(diff), window):
        seg_diff = diff[i:i+window]
        if len(seg_diff) == 0:
            break
        t = start_sec + i / sr
        rms_l = np.sqrt(np.mean(seg_diff[:, 0].astype(np.float64)**2))
        rms_r = np.sqrt(np.mean(seg_diff[:, 1].astype(np.float64)**2))
        max_l = np.max(np.abs(seg_diff[:, 0]))
        print(f'  {t:.1f}-{t+0.5:.1f}s  RMS_L={rms_l:.1f}  RMS_R={rms_r:.1f}  Max_L={max_l}')

if __name__ == '__main__':
    main()
