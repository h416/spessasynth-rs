"""Quick WAV diff - reports max abs difference between two WAVs in a time range."""
import sys
import numpy as np
import wave

def read_wav(path):
    with wave.open(path, 'r') as w:
        n = w.getnframes()
        sr = w.getframerate()
        data = np.frombuffer(w.readframes(n), dtype=np.int16).reshape(-1, 2)
        return data, sr

ts, sr = read_wav(sys.argv[1])
rs, _ = read_wav(sys.argv[2])
start_sec = float(sys.argv[3]) if len(sys.argv) > 3 else 88.0
end_sec = float(sys.argv[4]) if len(sys.argv) > 4 else 95.0
s = int(start_sec * sr)
e = int(end_sec * sr)
d = rs[s:e].astype(np.int32) - ts[s:e].astype(np.int32)
print(np.max(np.abs(d)))
