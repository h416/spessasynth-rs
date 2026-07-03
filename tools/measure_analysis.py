#!/usr/bin/env python3
"""Measure-by-measure WAV comparison with detailed MIDI event correlation.

Splits WAV files by measure boundaries (using MIDI tempo + time signature),
computes per-measure difference metrics, and correlates high-difference
measures with MIDI events to identify which event types cause the most divergence.
"""
import struct
import sys
import os
import math
import numpy as np
from collections import defaultdict, Counter

# ── WAV reading ───────────────────────────────────────────────────────────────

def read_wav(path):
    """Read a WAV file, return (sample_rate, data) where data is [channels][samples] float64."""
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

# ── MIDI parsing ──────────────────────────────────────────────────────────────

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

def parse_midi(path):
    """Parse MIDI file, return (division, tempo_map, time_sig_map, events).

    tempo_map: [(tick, us_per_beat), ...]
    time_sig_map: [(tick, numerator, denominator_power), ...]
    events: [(tick, track, description), ...]
    """
    with open(path, 'rb') as f:
        data = f.read()
    if data[:4] != b'MThd':
        return 0, [], [], []
    header_len = struct.unpack('>I', data[4:8])[0]
    fmt, num_tracks, division = struct.unpack('>HHH', data[8:14])
    pos = 8 + header_len

    tempo_map = []
    time_sig_map = []
    all_events = []

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
                    all_events.append((abs_tick, track_num, f"Tempo={bpm:.1f}BPM"))
                elif meta_type == 0x58 and len(meta_data) >= 2:
                    num = meta_data[0]
                    denom_pow = meta_data[1]
                    time_sig_map.append((abs_tick, num, denom_pow))
                    all_events.append((abs_tick, track_num, f"TimeSig={num}/{2**denom_pow}"))
            elif status == 0xF0 or status == 0xF7:  # SysEx
                tpos += 1
                sysex_len, tpos = read_varlen(data, tpos)
                sysex_data = data[tpos:tpos+sysex_len]; tpos += sysex_len
                mfr = sysex_data[0] if len(sysex_data) > 0 else 0
                mfr_name = {0x7E: "GM", 0x7F: "Univ", 0x41: "Roland", 0x43: "Yamaha"}.get(mfr, f"0x{mfr:02X}")
                # Decode GS SysEx details
                detail = ""
                if mfr == 0x41 and len(sysex_data) > 6 and sysex_data[2] == 0x42 and sysex_data[3] == 0x12:
                    addr = (sysex_data[4], sysex_data[5], sysex_data[6])
                    val = sysex_data[7] if len(sysex_data) > 7 else 0
                    detail = f" addr={addr[0]:02X}.{addr[1]:02X}.{addr[2]:02X} val={val}"
                all_events.append((abs_tick, track_num, f"SysEx({mfr_name} {len(sysex_data)}b{detail})"))
            elif status & 0x80:
                running_status = status
                tpos += 1
                msg_type = status & 0xF0
                ch = (status & 0x0F) + 1
                if msg_type == 0x80:
                    note = data[tpos]; vel = data[tpos+1]; tpos += 2
                    all_events.append((abs_tick, track_num, f"NoteOff ch{ch} n{note}"))
                elif msg_type == 0x90:
                    note = data[tpos]; vel = data[tpos+1]; tpos += 2
                    if vel > 0:
                        all_events.append((abs_tick, track_num, f"NoteOn ch{ch} n{note} v{vel}"))
                    else:
                        all_events.append((abs_tick, track_num, f"NoteOff ch{ch} n{note}"))
                elif msg_type == 0xA0:
                    note = data[tpos]; press = data[tpos+1]; tpos += 2
                    all_events.append((abs_tick, track_num, f"PolyPres ch{ch} n{note} p{press}"))
                elif msg_type == 0xB0:
                    cc = data[tpos]; val = data[tpos+1]; tpos += 2
                    name = CC_NAMES.get(cc, f"CC{cc}")
                    all_events.append((abs_tick, track_num, f"CC ch{ch} {name}={val}"))
                elif msg_type == 0xC0:
                    prog = data[tpos]; tpos += 1
                    all_events.append((abs_tick, track_num, f"ProgChg ch{ch} p{prog}"))
                elif msg_type == 0xD0:
                    press = data[tpos]; tpos += 1
                    all_events.append((abs_tick, track_num, f"ChanPres ch{ch} p{press}"))
                elif msg_type == 0xE0:
                    lsb = data[tpos]; msb = data[tpos+1]; tpos += 2
                    bend = (msb << 7) | lsb
                    all_events.append((abs_tick, track_num, f"PitchBend ch{ch} v{bend}"))
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
                            all_events.append((abs_tick, track_num, f"NoteOn ch{ch} n{d1} v{d2}"))
                        elif msg_type == 0x90 and d2 == 0:
                            all_events.append((abs_tick, track_num, f"NoteOff ch{ch} n{d1}"))
                        elif msg_type == 0x80:
                            all_events.append((abs_tick, track_num, f"NoteOff ch{ch} n{d1}"))
                        elif msg_type == 0xB0:
                            name = CC_NAMES.get(d1, f"CC{d1}")
                            all_events.append((abs_tick, track_num, f"CC ch{ch} {name}={d2}"))
                        elif msg_type == 0xE0:
                            bend = (d2 << 7) | d1
                            all_events.append((abs_tick, track_num, f"PitchBend ch{ch} v{bend}"))
                        elif msg_type == 0xA0:
                            all_events.append((abs_tick, track_num, f"PolyPres ch{ch} n{d1} p{d2}"))
                    elif msg_type in (0xC0, 0xD0):
                        if msg_type == 0xC0:
                            all_events.append((abs_tick, track_num, f"ProgChg ch{ch} p{status}"))
                        else:
                            all_events.append((abs_tick, track_num, f"ChanPres ch{ch} p{status}"))
                else:
                    tpos += 1
        pos = track_end

    if not tempo_map:
        tempo_map = [(0, 500000)]
    tempo_map.sort(key=lambda x: x[0])
    time_sig_map.sort(key=lambda x: x[0])

    return division, tempo_map, time_sig_map, all_events


def build_measure_boundaries(division, tempo_map, time_sig_map, total_seconds):
    """Build measure boundaries in seconds from tempo map and time signature map.

    Returns list of (measure_num, start_sec, end_sec, beats_per_measure).
    """
    if not time_sig_map:
        time_sig_map = [(0, 4, 2)]  # default 4/4

    # Convert ticks to seconds
    def ticks_to_seconds(tick):
        elapsed = 0.0
        prev_tick = 0
        us_per_beat = 500000
        for t_tick, t_us in tempo_map:
            if t_tick > tick:
                break
            elapsed += (t_tick - prev_tick) / division * (us_per_beat / 1_000_000)
            prev_tick = t_tick
            us_per_beat = t_us
        elapsed += (tick - prev_tick) / division * (us_per_beat / 1_000_000)
        return elapsed

    # Build measures iterating through ticks
    measures = []
    measure_num = 0
    current_tick = 0
    ts_idx = 0

    while True:
        # Find current time signature
        while ts_idx + 1 < len(time_sig_map) and time_sig_map[ts_idx + 1][0] <= current_tick:
            ts_idx += 1

        _, num, denom_pow = time_sig_map[ts_idx]
        denom = 2 ** denom_pow
        # Ticks per measure = division * 4 * num / denom
        ticks_per_measure = int(division * 4 * num / denom)

        start_sec = ticks_to_seconds(current_tick)
        end_tick = current_tick + ticks_per_measure
        end_sec = ticks_to_seconds(end_tick)

        if start_sec > total_seconds + 1.0:
            break

        measures.append((measure_num, start_sec, min(end_sec, total_seconds + 2.0), num))
        measure_num += 1
        current_tick = end_tick

        if measure_num > 5000:  # safety limit
            break

    return measures


def ticks_to_seconds(tick, division, tempo_map):
    """Convert MIDI tick to seconds."""
    elapsed = 0.0
    prev_tick = 0
    us_per_beat = 500000
    for t_tick, t_us in tempo_map:
        if t_tick > tick:
            break
        elapsed += (t_tick - prev_tick) / division * (us_per_beat / 1_000_000)
        prev_tick = t_tick
        us_per_beat = t_us
    elapsed += (tick - prev_tick) / division * (us_per_beat / 1_000_000)
    return elapsed


# ── Analysis ──────────────────────────────────────────────────────────────────

def categorize_event(desc):
    """Categorize an event description into a category for correlation."""
    if desc.startswith("NoteOn"):
        return "NoteOn"
    elif desc.startswith("NoteOff"):
        return "NoteOff"
    elif desc.startswith("CC"):
        parts = desc.split()
        if len(parts) >= 3:
            cc_part = parts[2].split("=")[0]
            return f"CC:{cc_part}"
        return "CC:?"
    elif desc.startswith("PitchBend"):
        return "PitchBend"
    elif desc.startswith("ProgChg"):
        return "ProgChg"
    elif desc.startswith("ChanPres"):
        return "ChanPres"
    elif desc.startswith("PolyPres"):
        return "PolyPres"
    elif desc.startswith("SysEx"):
        return "SysEx"
    elif desc.startswith("Tempo"):
        return "Tempo"
    elif desc.startswith("TimeSig"):
        return "TimeSig"
    return desc.split()[0]


def analyze_by_measure(ts_path, rust_path, midi_path):
    """Compare WAV files measure by measure, correlating with MIDI events."""
    name = os.path.basename(midi_path).replace('.mid', '')
    print(f"\n{'='*80}")
    print(f"  {name}")
    print(f"{'='*80}")

    sr_ts, wav_ts = read_wav(ts_path)
    sr_rs, wav_rs = read_wav(rust_path)
    assert sr_ts == sr_rs, f"Sample rate mismatch: {sr_ts} vs {sr_rs}"

    min_len = min(wav_ts.shape[1], wav_rs.shape[1])
    wav_ts = wav_ts[:, :min_len]
    wav_rs = wav_rs[:, :min_len]
    total_seconds = min_len / sr_ts

    # Parse MIDI
    division, tempo_map, time_sig_map, events = parse_midi(midi_path)

    # Convert event ticks to seconds
    events_with_sec = []
    for tick, track, desc in events:
        sec = ticks_to_seconds(tick, division, tempo_map)
        events_with_sec.append((sec, tick, track, desc))
    events_with_sec.sort(key=lambda x: x[0])

    # Build measure boundaries
    measures = build_measure_boundaries(division, tempo_map, time_sig_map, total_seconds)

    print(f"  Sample rate: {sr_ts}, Duration: {total_seconds:.2f}s, Measures: {len(measures)}")
    print(f"  Division: {division}, Tempo changes: {sum(1 for _,_,d in events if d.startswith('Tempo'))}")
    print(f"  Time sig changes: {sum(1 for _,_,d in events if d.startswith('TimeSig'))}")

    # Per-measure analysis
    measure_data = []
    for m_num, m_start, m_end, beats in measures:
        s_start = int(m_start * sr_ts)
        s_end = int(min(m_end * sr_ts, min_len))
        if s_start >= s_end:
            continue

        seg_ts = wav_ts[:, s_start:s_end]
        seg_rs = wav_rs[:, s_start:s_end]
        diff = seg_ts - seg_rs

        rms_diff = np.sqrt(np.mean(diff ** 2))
        max_diff = np.max(np.abs(diff))
        mean_abs_diff = np.mean(np.abs(diff))

        # Percentage of samples that differ
        nonzero_pct = np.count_nonzero(diff) / diff.size * 100

        # Amplitude info (to detect gain differences)
        ts_rms = np.sqrt(np.mean(seg_ts ** 2))
        rs_rms = np.sqrt(np.mean(seg_rs ** 2))
        gain_ratio = rs_rms / ts_rms if ts_rms > 1.0 else 0.0

        # Events in this measure (and 0.5s before)
        measure_events = [e for e in events_with_sec if m_start - 0.5 <= e[0] < m_end]

        # Categorize events
        event_cats = defaultdict(int)
        event_channels = defaultdict(set)
        for sec, tick, tr, desc in measure_events:
            cat = categorize_event(desc)
            event_cats[cat] += 1
            # Extract channel
            parts = desc.split()
            for p in parts:
                if p.startswith("ch"):
                    try:
                        ch = int(p[2:])
                        event_channels[cat].add(ch)
                    except ValueError:
                        pass

        measure_data.append({
            'num': m_num,
            't_start': m_start,
            't_end': m_end,
            'beats': beats,
            'rms_diff': rms_diff,
            'max_diff': max_diff,
            'mean_abs_diff': mean_abs_diff,
            'nonzero_pct': nonzero_pct,
            'ts_rms': ts_rms,
            'rs_rms': rs_rms,
            'gain_ratio': gain_ratio,
            'event_cats': dict(event_cats),
            'event_channels': dict(event_channels),
            'events': measure_events,
        })

    if not measure_data:
        print("  No measures to analyze!")
        return []

    # ── Global stats ──
    all_rms = [m['rms_diff'] for m in measure_data]
    print(f"\n  Overall: mean_rms={np.mean(all_rms):.2f}, median={np.median(all_rms):.2f}, "
          f"max={np.max(all_rms):.2f}, std={np.std(all_rms):.2f}")

    # ── Gain ratio analysis ──
    gain_ratios = [m['gain_ratio'] for m in measure_data if m['gain_ratio'] > 0]
    if gain_ratios:
        mean_gr = np.mean(gain_ratios)
        std_gr = np.std(gain_ratios)
        print(f"  Gain ratio (Rust/TS): mean={mean_gr:.6f}, std={std_gr:.6f}")
        if abs(mean_gr - 1.0) > 0.001:
            print(f"  ** Systematic gain difference: {(mean_gr - 1.0) * 100:.4f}% **")

    # ── Top 20 worst measures ──
    sorted_measures = sorted(measure_data, key=lambda m: m['rms_diff'], reverse=True)

    print(f"\n  Top 20 measures with highest RMS difference:")
    print(f"  {'M#':>4} {'Time':>14} {'RMS':>8} {'Max':>8} {'Diff%':>6} {'GainR':>7}  Key Events")
    print(f"  {'-'*4} {'-'*14} {'-'*8} {'-'*8} {'-'*6} {'-'*7}  {'-'*35}")

    for m in sorted_measures[:20]:
        time_str = f"{m['t_start']:.1f}-{m['t_end']:.1f}s"
        top_events = sorted(m['event_cats'].items(), key=lambda x: -x[1])[:4]
        evt_str = ", ".join(f"{k}({v})" for k, v in top_events if k not in ('NoteOff',))
        gain_str = f"{m['gain_ratio']:.4f}" if m['gain_ratio'] > 0 else "  -  "
        print(f"  {m['num']:4d} {time_str:>14} {m['rms_diff']:8.2f} {m['max_diff']:8.0f} "
              f"{m['nonzero_pct']:5.1f}% {gain_str:>7}  {evt_str}")

    # ── Difference pattern over time ──
    print(f"\n  RMS difference over time (every 10 measures):")
    for i in range(0, len(measure_data), 10):
        chunk = measure_data[i:i+10]
        avg_rms = np.mean([m['rms_diff'] for m in chunk])
        max_rms = max(m['rms_diff'] for m in chunk)
        t = chunk[0]['t_start']
        bar = '#' * min(int(avg_rms / 2), 60)
        print(f"  M{i:4d} ({t:6.1f}s) avg={avg_rms:7.2f} max={max_rms:7.2f} |{bar}")

    # ── Event type correlation ──
    # Compare event frequencies in top quartile vs bottom quartile
    q_size = max(len(sorted_measures) // 4, 5)
    top_q = sorted_measures[:q_size]
    bot_q = sorted_measures[-q_size:]

    top_cat_totals = defaultdict(int)
    bot_cat_totals = defaultdict(int)
    top_cat_measures = defaultdict(int)  # how many measures have this event type
    bot_cat_measures = defaultdict(int)

    for m in top_q:
        for k, v in m['event_cats'].items():
            top_cat_totals[k] += v
            top_cat_measures[k] += 1
    for m in bot_q:
        for k, v in m['event_cats'].items():
            bot_cat_totals[k] += v
            bot_cat_measures[k] += 1

    all_cats = set(top_cat_totals.keys()) | set(bot_cat_totals.keys())

    print(f"\n  Event correlation: top {q_size} worst measures vs bottom {q_size}")
    print(f"  {'Event Type':<20} {'Top(count)':>10} {'Top(meas)':>10} {'Bot(count)':>10} {'Bot(meas)':>10} {'Ratio':>7}")
    print(f"  {'-'*20} {'-'*10} {'-'*10} {'-'*10} {'-'*10} {'-'*7}")

    rows = []
    for k in all_cats:
        t = top_cat_totals.get(k, 0)
        b = bot_cat_totals.get(k, 0)
        tm = top_cat_measures.get(k, 0)
        bm = bot_cat_measures.get(k, 0)
        # Normalize by number of measures
        t_avg = t / q_size
        b_avg = b / q_size
        ratio = t_avg / b_avg if b_avg > 0 else (float('inf') if t_avg > 0 else 0)
        rows.append((k, t, tm, b, bm, ratio))
    rows.sort(key=lambda x: -x[5])

    for k, t, tm, b, bm, ratio in rows:
        if k in ('NoteOff',):
            continue
        ratio_str = f"{ratio:.2f}x" if ratio != float('inf') else "  inf"
        print(f"  {k:<20} {t:10d} {tm:10d} {b:10d} {bm:10d} {ratio_str:>7}")

    # ── Detailed analysis for top 5 worst measures ──
    print(f"\n  Detailed events for top 5 worst measures:")
    for m in sorted_measures[:5]:
        print(f"\n  === Measure {m['num']} ({m['t_start']:.2f}-{m['t_end']:.2f}s) "
              f"RMS={m['rms_diff']:.2f}, Max={m['max_diff']:.0f}, "
              f"GainRatio={m['gain_ratio']:.5f} ===")

        # Group events by channel
        by_channel = defaultdict(list)
        for sec, tick, tr, desc in m['events']:
            parts = desc.split()
            ch = "?"
            for p in parts:
                if p.startswith("ch"):
                    ch = p
                    break
            by_channel[ch].append((sec, desc))

        # Show non-NoteOn/NoteOff events
        shown = 0
        for sec, tick, tr, desc in m['events']:
            if desc.startswith("NoteOn") or desc.startswith("NoteOff"):
                continue
            print(f"    {sec:8.3f}s  T{tr:2d}  {desc}")
            shown += 1
            if shown > 25:
                remaining = sum(1 for _,_,_,d in m['events']
                              if not d.startswith("NoteOn") and not d.startswith("NoteOff")) - shown
                if remaining > 0:
                    print(f"    ... ({remaining} more)")
                break

        # Count NoteOn per channel
        note_counts = defaultdict(int)
        for sec, tick, tr, desc in m['events']:
            if desc.startswith("NoteOn"):
                parts = desc.split()
                for p in parts:
                    if p.startswith("ch"):
                        note_counts[p] += 1
                        break
        if note_counts:
            note_str = ", ".join(f"{ch}:{cnt}" for ch, cnt in sorted(note_counts.items()))
            print(f"    NoteOns: {note_str}")

    # ── Identify "onset" of divergence ──
    print(f"\n  Divergence onset detection (first measure where RMS > thresholds):")
    for threshold in [1, 5, 10, 50, 100]:
        for m in measure_data:
            if m['rms_diff'] > threshold:
                print(f"    RMS > {threshold:4d}: Measure {m['num']} ({m['t_start']:.2f}s)")
                # Show events in that measure
                non_note = [(s, d) for s, _, _, d in m['events']
                           if not d.startswith("NoteOn") and not d.startswith("NoteOff")]
                for sec, desc in non_note[:5]:
                    print(f"      {sec:.3f}s  {desc}")
                break
        else:
            print(f"    RMS > {threshold:4d}: never")

    # ── Channel-specific analysis ──
    print(f"\n  Per-channel event presence in top 10 worst measures:")
    top10 = sorted_measures[:10]
    channel_in_worst = defaultdict(lambda: defaultdict(int))
    for m in top10:
        for cat, channels in m['event_channels'].items():
            for ch in channels:
                channel_in_worst[ch][cat] += 1

    for ch in sorted(channel_in_worst.keys()):
        cats = channel_in_worst[ch]
        cat_str = ", ".join(f"{k}({v})" for k, v in sorted(cats.items(), key=lambda x: -x[1])[:5])
        print(f"    ch{ch:2d}: {cat_str}")

    return sorted_measures


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Measure-by-measure WAV comparison with MIDI event correlation")
    parser.add_argument("ts_wav", help="Path to TypeScript WAV file")
    parser.add_argument("rs_wav", help="Path to Rust WAV file")
    parser.add_argument("midi", help="Path to MIDI file")
    args = parser.parse_args()

    analyze_by_measure(args.ts_wav, args.rs_wav, args.midi)
