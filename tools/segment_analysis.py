#!/usr/bin/env python3
"""Segment-by-segment WAV comparison with MIDI event correlation.

Splits WAV files into time segments, computes per-segment difference metrics,
then correlates high-difference segments with MIDI events occurring in/before them.
"""
import struct
import sys
import os
import math
import numpy as np
from collections import defaultdict

# ── WAV reading ───────────────────────────────────────────────────────────────

def read_wav(path):
    """Read a WAV file, return (sample_rate, data) where data is [channels][samples] float."""
    with open(path, 'rb') as f:
        raw = f.read()
    assert raw[:4] == b'RIFF'
    assert raw[8:12] == b'WAVE'
    pos = 12
    fmt_found = False
    while pos < len(raw) - 8:
        chunk_id = raw[pos:pos+4]
        chunk_size = struct.unpack('<I', raw[pos+4:pos+8])[0]
        if chunk_id == b'fmt ':
            audio_fmt = struct.unpack('<H', raw[pos+8:pos+10])[0]
            num_channels = struct.unpack('<H', raw[pos+10:pos+12])[0]
            sample_rate = struct.unpack('<I', raw[pos+12:pos+16])[0]
            bits_per_sample = struct.unpack('<H', raw[pos+22:pos+24])[0]
            fmt_found = True
        elif chunk_id == b'data':
            data_raw = raw[pos+8:pos+8+chunk_size]
            break
        pos += 8 + chunk_size
        if chunk_size % 2:
            pos += 1
    assert fmt_found
    if bits_per_sample == 16:
        samples = np.frombuffer(data_raw, dtype=np.int16).astype(np.float64)
    elif bits_per_sample == 32:
        if audio_fmt == 3:
            samples = np.frombuffer(data_raw, dtype=np.float32).astype(np.float64)
        else:
            samples = np.frombuffer(data_raw, dtype=np.int32).astype(np.float64)
    else:
        raise ValueError(f"Unsupported bits_per_sample: {bits_per_sample}")
    samples = samples.reshape(-1, num_channels).T
    return sample_rate, samples

# ── MIDI parsing (minimal, for event extraction) ──────────────────────────────

def read_varlen(data, pos):
    value = 0
    while pos < len(data):
        b = data[pos]; pos += 1
        value = (value << 7) | (b & 0x7F)
        if not (b & 0x80):
            break
    return value, pos

CC_NAMES = {
    0: "BankMSB", 1: "Mod", 5: "PortaTime", 6: "DataMSB", 7: "Vol",
    10: "Pan", 11: "Expr", 32: "BankLSB", 38: "DataLSB", 64: "Sustain",
    65: "Porta", 66: "Sostenuto", 71: "Reso", 72: "Release", 73: "Attack",
    74: "Bright", 91: "Reverb", 93: "Chorus",
    98: "NRPN_LSB", 99: "NRPN_MSB", 100: "RPN_LSB", 101: "RPN_MSB",
    120: "AllSndOff", 121: "ResetCC", 123: "AllNoteOff",
}

def parse_midi_events(path):
    """Parse MIDI file, return list of (time_seconds, event_description) tuples."""
    with open(path, 'rb') as f:
        data = f.read()
    if data[:4] != b'MThd':
        return []
    header_len = struct.unpack('>I', data[4:8])[0]
    fmt, num_tracks, division = struct.unpack('>HHH', data[8:14])
    pos = 8 + header_len

    # Build tempo map first
    tempo_map = []  # (tick, us_per_beat)
    all_events_by_tick = []  # (tick, track, description)

    for track_num in range(num_tracks):
        if pos >= len(data) or data[pos:pos+4] != b'MTrk':
            break
        track_len = struct.unpack('>I', data[pos+4:pos+8])[0]
        track_start = pos + 8
        track_end = track_start + track_len
        tpos = track_start
        abs_tick = 0
        running_status = 0

        while tpos < track_end:
            delta, tpos = read_varlen(data, tpos)
            abs_tick += delta
            if tpos >= track_end:
                break
            status = data[tpos]

            if status == 0xFF:  # Meta
                tpos += 1
                meta_type = data[tpos]; tpos += 1
                meta_len, tpos = read_varlen(data, tpos)
                meta_data = data[tpos:tpos+meta_len]; tpos += meta_len
                if meta_type == 0x51 and len(meta_data) >= 3:
                    us = (meta_data[0] << 16) | (meta_data[1] << 8) | meta_data[2]
                    tempo_map.append((abs_tick, us))
                    bpm = 60_000_000 / us if us > 0 else 120
                    all_events_by_tick.append((abs_tick, track_num, f"Tempo={bpm:.1f}BPM"))
            elif status == 0xF0 or status == 0xF7:  # SysEx
                tpos += 1
                sysex_len, tpos = read_varlen(data, tpos)
                sysex_data = data[tpos:tpos+sysex_len]; tpos += sysex_len
                mfr = sysex_data[0] if len(sysex_data) > 0 else 0
                mfr_name = {0x7E: "GM", 0x7F: "Univ", 0x41: "Roland", 0x43: "Yamaha"}.get(mfr, f"0x{mfr:02X}")
                all_events_by_tick.append((abs_tick, track_num, f"SysEx({mfr_name} {len(sysex_data)}b)"))
            elif status & 0x80:
                running_status = status
                tpos += 1
                msg_type = status & 0xF0
                ch = (status & 0x0F) + 1
                if msg_type == 0x80:
                    tpos += 2
                elif msg_type == 0x90:
                    note = data[tpos]; vel = data[tpos+1]; tpos += 2
                    if vel > 0:
                        all_events_by_tick.append((abs_tick, track_num, f"NoteOn ch{ch} n{note} v{vel}"))
                    # skip note-off (vel=0) for brevity
                elif msg_type == 0xA0:
                    note = data[tpos]; press = data[tpos+1]; tpos += 2
                    all_events_by_tick.append((abs_tick, track_num, f"PolyPres ch{ch} n{note} p{press}"))
                elif msg_type == 0xB0:
                    cc = data[tpos]; val = data[tpos+1]; tpos += 2
                    name = CC_NAMES.get(cc, f"CC{cc}")
                    all_events_by_tick.append((abs_tick, track_num, f"CC ch{ch} {name}={val}"))
                elif msg_type == 0xC0:
                    prog = data[tpos]; tpos += 1
                    all_events_by_tick.append((abs_tick, track_num, f"ProgChg ch{ch} p{prog}"))
                elif msg_type == 0xD0:
                    press = data[tpos]; tpos += 1
                    all_events_by_tick.append((abs_tick, track_num, f"ChanPres ch{ch} p{press}"))
                elif msg_type == 0xE0:
                    lsb = data[tpos]; msb = data[tpos+1]; tpos += 2
                    bend = (msb << 7) | lsb
                    all_events_by_tick.append((abs_tick, track_num, f"PitchBend ch{ch} v{bend}"))
                else:
                    tpos += 2
            else:
                # Running status
                if running_status:
                    msg_type = running_status & 0xF0
                    ch = (running_status & 0x0F) + 1
                    if msg_type in (0x80, 0x90, 0xA0, 0xB0, 0xE0):
                        d1 = status; d2 = data[tpos] if tpos < track_end else 0; tpos += 1
                        if msg_type == 0x90 and d2 > 0:
                            all_events_by_tick.append((abs_tick, track_num, f"NoteOn ch{ch} n{d1} v{d2}"))
                        elif msg_type == 0xB0:
                            name = CC_NAMES.get(d1, f"CC{d1}")
                            all_events_by_tick.append((abs_tick, track_num, f"CC ch{ch} {name}={d2}"))
                        elif msg_type == 0xE0:
                            bend = (d2 << 7) | d1
                            all_events_by_tick.append((abs_tick, track_num, f"PitchBend ch{ch} v{bend}"))
                        elif msg_type == 0xA0:
                            all_events_by_tick.append((abs_tick, track_num, f"PolyPres ch{ch} n{d1} p{d2}"))
                    elif msg_type in (0xC0, 0xD0):
                        if msg_type == 0xC0:
                            all_events_by_tick.append((abs_tick, track_num, f"ProgChg ch{ch} p{status}"))
                        else:
                            all_events_by_tick.append((abs_tick, track_num, f"ChanPres ch{ch} p{status}"))
                else:
                    tpos += 1
        pos = track_end

    # Convert ticks to seconds using tempo map
    if not tempo_map:
        tempo_map = [(0, 500000)]  # default 120 BPM
    tempo_map.sort(key=lambda x: x[0])

    def ticks_to_seconds(tick):
        elapsed = 0.0
        prev_tick = 0
        us_per_beat = 500000  # default
        for t_tick, t_us in tempo_map:
            if t_tick > tick:
                break
            elapsed += (t_tick - prev_tick) / division * (us_per_beat / 1_000_000)
            prev_tick = t_tick
            us_per_beat = t_us
        elapsed += (tick - prev_tick) / division * (us_per_beat / 1_000_000)
        return elapsed

    result = []
    for tick, track, desc in all_events_by_tick:
        t = ticks_to_seconds(tick)
        result.append((t, track, desc))
    result.sort(key=lambda x: x[0])
    return result

# ── Segment analysis ─────────────────────────────────────────────────────────

def analyze_segments(ts_path, rust_path, midi_path, segment_sec=2.0):
    """Compare WAV files segment by segment, correlate with MIDI events."""
    print(f"\n{'='*70}")
    print(f"  {os.path.basename(midi_path)}")
    print(f"  Segment size: {segment_sec} sec")
    print(f"{'='*70}")

    sr_ts, wav_ts = read_wav(ts_path)
    sr_rs, wav_rs = read_wav(rust_path)
    assert sr_ts == sr_rs, f"Sample rate mismatch: {sr_ts} vs {sr_rs}"

    min_len = min(wav_ts.shape[1], wav_rs.shape[1])
    wav_ts = wav_ts[:, :min_len]
    wav_rs = wav_rs[:, :min_len]

    # Parse MIDI events
    events = parse_midi_events(midi_path)

    segment_samples = int(segment_sec * sr_ts)
    num_segments = (min_len + segment_samples - 1) // segment_samples

    segments = []
    for i in range(num_segments):
        start = i * segment_samples
        end = min(start + segment_samples, min_len)
        t_start = start / sr_ts
        t_end = end / sr_ts

        seg_ts = wav_ts[:, start:end]
        seg_rs = wav_rs[:, start:end]
        diff = seg_ts - seg_rs

        # Metrics
        rms_diff = np.sqrt(np.mean(diff ** 2))
        max_diff = np.max(np.abs(diff))
        mean_abs_diff = np.mean(np.abs(diff))

        # Per-channel correlation
        corrs = []
        for ch in range(min(wav_ts.shape[0], 2)):
            ts_ch = seg_ts[ch]
            rs_ch = seg_rs[ch]
            if np.std(ts_ch) > 0 and np.std(rs_ch) > 0:
                corr = np.corrcoef(ts_ch, rs_ch)[0, 1]
            else:
                corr = 1.0
            corrs.append(corr)

        # Collect events in this segment (and 1 sec before)
        seg_events = [e for e in events if t_start - 1.0 <= e[0] < t_end]
        # Categorize events
        event_types = defaultdict(int)
        for t, tr, desc in seg_events:
            # Extract event category
            cat = desc.split()[0]  # e.g. "NoteOn", "CC", "PitchBend", "SysEx"
            if cat == "CC":
                # Extract CC name
                parts = desc.split()
                cc_part = parts[2] if len(parts) > 2 else "CC?"
                cc_name = cc_part.split("=")[0]
                event_types[f"CC:{cc_name}"] += 1
            else:
                event_types[cat] += 1

        segments.append({
            'index': i,
            't_start': t_start,
            't_end': t_end,
            'rms_diff': rms_diff,
            'max_diff': max_diff,
            'mean_abs_diff': mean_abs_diff,
            'corr_L': corrs[0] if len(corrs) > 0 else 1.0,
            'corr_R': corrs[1] if len(corrs) > 1 else 1.0,
            'event_types': dict(event_types),
            'events': seg_events,
        })

    # Sort by RMS difference (descending)
    sorted_segs = sorted(segments, key=lambda s: s['rms_diff'], reverse=True)

    # Print top segments with highest difference
    print(f"\n  Top 15 segments with highest RMS difference:\n")
    print(f"  {'Seg':>4} {'Time':>12} {'RMS Diff':>10} {'Max Diff':>10} {'Corr(L)':>9} {'Corr(R)':>9}  Key Events")
    print(f"  {'-'*4} {'-'*12} {'-'*10} {'-'*10} {'-'*9} {'-'*9}  {'-'*30}")

    for seg in sorted_segs[:15]:
        time_str = f"{seg['t_start']:.1f}-{seg['t_end']:.1f}s"
        # Top event types by count
        top_events = sorted(seg['event_types'].items(), key=lambda x: -x[1])[:5]
        evt_str = ", ".join(f"{k}({v})" for k, v in top_events)
        print(f"  {seg['index']:4d} {time_str:>12} {seg['rms_diff']:10.2f} {seg['max_diff']:10.2f} "
              f"{seg['corr_L']:9.6f} {seg['corr_R']:9.6f}  {evt_str}")

    # Analyze: which event types appear most often in high-diff segments?
    print(f"\n  Event type frequency in top 10 high-diff segments vs bottom 10:")
    top10 = sorted_segs[:10]
    bot10 = sorted_segs[-10:] if len(sorted_segs) >= 20 else sorted_segs[len(sorted_segs)//2:]

    top_evt_totals = defaultdict(int)
    bot_evt_totals = defaultdict(int)
    for seg in top10:
        for k, v in seg['event_types'].items():
            top_evt_totals[k] += v
    for seg in bot10:
        for k, v in seg['event_types'].items():
            bot_evt_totals[k] += v

    all_event_keys = set(top_evt_totals.keys()) | set(bot_evt_totals.keys())
    print(f"\n  {'Event Type':<25} {'Top10 segs':>10} {'Bottom10':>10} {'Ratio':>8}")
    print(f"  {'-'*25} {'-'*10} {'-'*10} {'-'*8}")
    rows = []
    for k in all_event_keys:
        t = top_evt_totals.get(k, 0)
        b = bot_evt_totals.get(k, 0)
        ratio = t / b if b > 0 else float('inf') if t > 0 else 0
        rows.append((k, t, b, ratio))
    rows.sort(key=lambda x: -x[3])
    for k, t, b, ratio in rows:
        ratio_str = f"{ratio:.2f}x" if ratio != float('inf') else "inf"
        print(f"  {k:<25} {t:10d} {b:10d} {ratio_str:>8}")

    # Print detailed events for the top 3 worst segments
    print(f"\n  Detailed events for top 3 worst segments:")
    for seg in sorted_segs[:3]:
        print(f"\n  --- Segment {seg['index']} ({seg['t_start']:.1f}-{seg['t_end']:.1f}s) "
              f"RMS={seg['rms_diff']:.2f} ---")
        # Show non-NoteOn events (NoteOn is too verbose)
        shown = 0
        for t, tr, desc in seg['events']:
            if not desc.startswith("NoteOn"):
                print(f"    {t:8.3f}s  T{tr:2d}  {desc}")
                shown += 1
                if shown > 30:
                    remaining = len([e for e in seg['events'] if not e[2].startswith("NoteOn")]) - shown
                    if remaining > 0:
                        print(f"    ... ({remaining} more non-NoteOn events)")
                    break

    return sorted_segs


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Segment-by-segment WAV comparison with MIDI event correlation")
    parser.add_argument("ts_wav", help="Path to TypeScript WAV file")
    parser.add_argument("rs_wav", help="Path to Rust WAV file")
    parser.add_argument("midi", help="Path to MIDI file")
    parser.add_argument("--segment-sec", type=float, default=2.0, help="Segment size in seconds (default: 2.0)")
    args = parser.parse_args()

    analyze_segments(args.ts_wav, args.rs_wav, args.midi, segment_sec=args.segment_sec)
