#!/usr/bin/env python3
"""Analyze MIDI files to list event types and special features."""
import struct
import sys
import os
from collections import Counter, defaultdict

def read_varlen(data, pos):
    """Read a MIDI variable-length quantity."""
    value = 0
    while pos < len(data):
        b = data[pos]
        pos += 1
        value = (value << 7) | (b & 0x7F)
        if not (b & 0x80):
            break
    return value, pos

def analyze_midi(path):
    with open(path, 'rb') as f:
        data = f.read()

    # Parse header
    if data[:4] != b'MThd':
        print(f"  Not a valid MIDI file")
        return

    header_len = struct.unpack('>I', data[4:8])[0]
    fmt, num_tracks, division = struct.unpack('>HHH', data[8:14])

    print(f"  Format: {fmt}, Tracks: {num_tracks}, Division: {division}")

    pos = 8 + header_len

    event_counts = Counter()
    cc_counts = Counter()
    sysex_messages = []
    meta_events = Counter()
    channels_used = set()
    program_changes = defaultdict(list)  # channel -> list of programs
    pitch_bend_channels = set()
    aftertouch_channels = set()
    poly_pressure_channels = set()
    nrpn_channels = set()
    rpn_channels = set()
    has_tempo_change = False
    sysex_types = []

    for track_num in range(num_tracks):
        if pos >= len(data) or data[pos:pos+4] != b'MTrk':
            break
        track_len = struct.unpack('>I', data[pos+4:pos+8])[0]
        track_start = pos + 8
        track_end = track_start + track_len
        pos = track_start

        running_status = 0

        while pos < track_end:
            # Delta time
            delta, pos = read_varlen(data, pos)

            if pos >= track_end:
                break

            status = data[pos]

            if status == 0xFF:
                # Meta event
                pos += 1
                meta_type = data[pos]
                pos += 1
                meta_len, pos = read_varlen(data, pos)
                meta_data = data[pos:pos+meta_len]
                pos += meta_len

                meta_events[meta_type] += 1
                if meta_type == 0x51:
                    has_tempo_change = True

            elif status == 0xF0 or status == 0xF7:
                # SysEx
                pos += 1
                sysex_len, pos = read_varlen(data, pos)
                sysex_data = data[pos:pos+sysex_len]
                pos += sysex_len

                event_counts['SysEx'] += 1

                # Identify SysEx type
                if len(sysex_data) >= 1:
                    manufacturer = sysex_data[0]
                    if manufacturer == 0x7E or manufacturer == 0x7F:
                        sub = sysex_data[2] if len(sysex_data) > 2 else 0
                        if sub == 0x09:
                            if len(sysex_data) > 3:
                                if sysex_data[3] == 0x01:
                                    sysex_types.append("GM1 System On")
                                elif sysex_data[3] == 0x03:
                                    sysex_types.append("GM2 System On")
                                else:
                                    sysex_types.append(f"GM System ({sysex_data[3]:02X})")
                        elif sub == 0x04:
                            sysex_types.append("Device Control (master vol/pan/tune)")
                        elif sub == 0x08:
                            sysex_types.append("MIDI Tuning Standard")
                        else:
                            sysex_types.append(f"GM/Universal ({sub:02X})")
                    elif manufacturer == 0x41:
                        if len(sysex_data) > 2 and sysex_data[2] == 0x42:
                            if len(sysex_data) > 3 and sysex_data[3] == 0x12:
                                if len(sysex_data) > 6:
                                    addr = (sysex_data[4], sysex_data[5], sysex_data[6])
                                    if addr[0] == 0x40 and (addr[1] & 0x10):
                                        param = addr[2]
                                        if param == 0x15:
                                            sysex_types.append("GS Drum Channel")
                                        elif param == 0x16:
                                            sysex_types.append("GS Key Shift")
                                        elif param == 0x1C:
                                            sysex_types.append("GS Pan")
                                        elif 0x40 <= param <= 0x4B:
                                            sysex_types.append("GS Scale Tuning")
                                        else:
                                            sysex_types.append(f"GS Channel Param ({param:02X})")
                                    elif addr[0] == 0x40 and (addr[1] & 0x20):
                                        sysex_types.append("GS Modulator Mapping")
                                    elif addr[0] == 0x40 and addr[1] == 0x00:
                                        if addr[2] == 0x7F:
                                            sysex_types.append("GS Reset")
                                        else:
                                            sysex_types.append(f"GS System ({addr[2]:02X})")
                                    elif addr[0] == 0x40 and addr[1] == 0x01:
                                        sysex_types.append(f"GS Global ({addr[2]:02X})")
                                    else:
                                        sysex_types.append(f"GS DT1 ({addr[0]:02X}.{addr[1]:02X}.{addr[2]:02X})")
                                else:
                                    sysex_types.append("GS DT1 (short)")
                            else:
                                sysex_types.append("Roland (non-DT1)")
                        elif len(sysex_data) > 2 and sysex_data[2] == 0x45:
                            sysex_types.append("GS Display")
                        elif len(sysex_data) > 2 and sysex_data[2] == 0x16:
                            sysex_types.append("Roland Master Volume")
                        else:
                            sysex_types.append(f"Roland ({sysex_data[2]:02X})")
                    elif manufacturer == 0x43:
                        if len(sysex_data) > 2 and sysex_data[2] == 0x4C:
                            sysex_types.append("XG Parameter")
                        else:
                            sysex_types.append(f"Yamaha ({sysex_data[2]:02X})")
                    else:
                        sysex_types.append(f"Unknown (mfr={manufacturer:02X})")

                sysex_messages.append(sysex_data[:16].hex())

            elif status & 0x80:
                # Channel message
                running_status = status
                pos += 1
                msg_type = status & 0xF0
                channel = status & 0x0F
                channels_used.add(channel)

                if msg_type == 0x80:  # Note Off
                    event_counts['Note Off'] += 1
                    pos += 2
                elif msg_type == 0x90:  # Note On
                    event_counts['Note On'] += 1
                    pos += 2
                elif msg_type == 0xA0:  # Poly Pressure
                    event_counts['Poly Pressure'] += 1
                    poly_pressure_channels.add(channel)
                    pos += 2
                elif msg_type == 0xB0:  # Control Change
                    cc_num = data[pos]
                    cc_val = data[pos+1] if pos+1 < track_end else 0
                    pos += 2
                    event_counts['Control Change'] += 1
                    cc_counts[cc_num] += 1

                    # Detect NRPN/RPN
                    if cc_num == 99:  # NRPN MSB
                        nrpn_channels.add(channel)
                    elif cc_num == 101:  # RPN MSB
                        rpn_channels.add(channel)

                elif msg_type == 0xC0:  # Program Change
                    prog = data[pos]
                    pos += 1
                    event_counts['Program Change'] += 1
                    program_changes[channel].append(prog)
                elif msg_type == 0xD0:  # Channel Pressure
                    event_counts['Channel Pressure'] += 1
                    aftertouch_channels.add(channel)
                    pos += 1
                elif msg_type == 0xE0:  # Pitch Bend
                    event_counts['Pitch Bend'] += 1
                    pitch_bend_channels.add(channel)
                    pos += 2
                else:
                    pos += 2
            else:
                # Running status
                if running_status:
                    msg_type = running_status & 0xF0
                    channel = running_status & 0x0F
                    channels_used.add(channel)

                    if msg_type == 0x80:
                        event_counts['Note Off'] += 1
                        pos += 1  # already consumed 1 byte
                    elif msg_type == 0x90:
                        event_counts['Note On'] += 1
                        pos += 1
                    elif msg_type == 0xA0:
                        event_counts['Poly Pressure'] += 1
                        poly_pressure_channels.add(channel)
                        pos += 1
                    elif msg_type == 0xB0:
                        cc_num = data[pos-1] if pos > 0 else 0  # This byte is already the data
                        # Actually running status: data[pos] is first data byte
                        # We need to re-read
                        cc_num = status  # status here is the data byte (no 0x80 bit)
                        cc_val = data[pos] if pos < track_end else 0
                        pos += 1
                        event_counts['Control Change'] += 1
                        cc_counts[cc_num] += 1
                        if cc_num == 99:
                            nrpn_channels.add(channel)
                        elif cc_num == 101:
                            rpn_channels.add(channel)
                    elif msg_type == 0xC0:
                        event_counts['Program Change'] += 1
                        program_changes[channel].append(status)
                    elif msg_type == 0xD0:
                        event_counts['Channel Pressure'] += 1
                        aftertouch_channels.add(channel)
                    elif msg_type == 0xE0:
                        event_counts['Pitch Bend'] += 1
                        pitch_bend_channels.add(channel)
                        pos += 1
                    else:
                        pos += 1
                else:
                    pos += 1

        pos = track_end

    # Output
    ch_list = sorted(channels_used)
    print(f"  Channels used: {[c+1 for c in ch_list]} (1-based)")
    has_drum = 9 in channels_used
    print(f"  Has drum channel (ch10): {has_drum}")
    print(f"  Tempo changes: {meta_events.get(0x51, 0)}")

    print(f"\n  Event counts:")
    for event, count in sorted(event_counts.items(), key=lambda x: -x[1]):
        print(f"    {event}: {count}")

    # Special features
    print(f"\n  Special features:")
    if pitch_bend_channels:
        print(f"    Pitch Bend on channels: {sorted([c+1 for c in pitch_bend_channels])}")
    if aftertouch_channels:
        print(f"    Channel Pressure on: {sorted([c+1 for c in aftertouch_channels])}")
    if poly_pressure_channels:
        print(f"    Poly Pressure on: {sorted([c+1 for c in poly_pressure_channels])}")
    if nrpn_channels:
        print(f"    NRPN on channels: {sorted([c+1 for c in nrpn_channels])}")
    if rpn_channels:
        print(f"    RPN on channels: {sorted([c+1 for c in rpn_channels])}")

    # Top CCs
    notable_ccs = {
        0: "Bank Select MSB", 1: "Modulation", 5: "Portamento Time",
        6: "Data Entry MSB", 7: "Volume", 10: "Pan", 11: "Expression",
        32: "Bank Select LSB", 38: "Data Entry LSB", 64: "Sustain",
        65: "Portamento", 66: "Sostenuto", 71: "Resonance",
        72: "Release", 73: "Attack", 74: "Brightness",
        75: "Decay", 76: "Vibrato Rate", 77: "Vibrato Depth",
        78: "Vibrato Delay", 91: "Reverb", 93: "Chorus",
        98: "NRPN LSB", 99: "NRPN MSB", 100: "RPN LSB", 101: "RPN MSB",
        120: "All Sound Off", 121: "Reset All Controllers", 123: "All Notes Off",
    }
    if cc_counts:
        print(f"\n  Control Changes (top):")
        for cc, count in cc_counts.most_common(20):
            name = notable_ccs.get(cc, f"CC{cc}")
            print(f"    CC{cc:3d} ({name}): {count}")

    # SysEx
    if sysex_types:
        sysex_type_counts = Counter(sysex_types)
        print(f"\n  SysEx messages ({len(sysex_types)} total):")
        for st, count in sysex_type_counts.most_common():
            print(f"    {st}: {count}")

    # Programs per channel
    print(f"\n  Programs by channel:")
    for ch in sorted(program_changes.keys()):
        progs = program_changes[ch]
        unique = sorted(set(progs))
        ch_name = f"Ch{ch+1:2d}"
        if ch == 9:
            ch_name += " (Drum)"
        print(f"    {ch_name}: {unique}")


if __name__ == "__main__":
    midi_dir = "/Users/hirama/Downloads/GeneralUser-GS/demo MIDIs"
    files = sorted([f for f in os.listdir(midi_dir) if f.endswith('.mid')])
    for f in files:
        path = os.path.join(midi_dir, f)
        print(f"\n{'='*60}")
        print(f"  {f}")
        print(f"{'='*60}")
        analyze_midi(path)
