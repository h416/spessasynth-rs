#!/usr/bin/env python3
"""Isolate specific MIDI channels by muting all others.

Mutes channels by converting their NoteOn velocity to 0 (effectively NoteOff).
Preserves all CC/PitchBend events to maintain controller state.

Channels are 0-based (0-15, e.g. 9 for drums).

Usage:
    python tools/isolate_channel.py <input.mid> <output.mid> --keep 5 9
    python tools/isolate_channel.py <input.mid> <output.mid> --mute 9
"""
import argparse
import struct
import sys


def read_varlen(data, pos):
    value = 0
    while pos < len(data):
        b = data[pos]
        pos += 1
        value = (value << 7) | (b & 0x7F)
        if not (b & 0x80):
            break
    return value, pos


def write_varlen(value):
    if value < 0:
        value = 0
    result = []
    result.append(value & 0x7F)
    value >>= 7
    while value > 0:
        result.append((value & 0x7F) | 0x80)
        value >>= 7
    result.reverse()
    return bytes(result)


def isolate_channels_in_track(track_data, keep_channels):
    """Mute NoteOn for channels not in keep_channels by setting velocity=0."""
    pos = 0
    output = bytearray()
    running_status = 0
    muted_notes = 0

    while pos < len(track_data):
        delta, pos = read_varlen(track_data, pos)

        if pos >= len(track_data):
            output.extend(write_varlen(delta))
            break

        byte = track_data[pos]

        if byte == 0xFF:
            # Meta event - copy as-is
            event_start = pos
            pos += 1
            if pos >= len(track_data):
                output.extend(write_varlen(delta))
                break
            pos += 1  # meta type
            length, pos = read_varlen(track_data, pos)
            event_end = pos + length
            output.extend(write_varlen(delta))
            output.extend(track_data[event_start:event_end])
            pos = event_end

        elif byte == 0xF0 or byte == 0xF7:
            # SysEx - copy as-is
            event_start = pos
            pos += 1
            length, pos = read_varlen(track_data, pos)
            event_end = pos + length
            output.extend(write_varlen(delta))
            output.extend(track_data[event_start:event_end])
            pos = event_end

        elif byte & 0x80:
            # New status byte
            status = byte
            running_status = status
            pos += 1
            status_high = status & 0xF0
            channel = status & 0x0F  # 0-based

            if status_high in (0x80, 0x90, 0xA0, 0xB0, 0xE0):
                # 2 data bytes
                d1 = track_data[pos]
                d2 = track_data[pos + 1]
                output.extend(write_varlen(delta))
                output.append(status)
                if status_high == 0x90 and channel not in keep_channels and d2 > 0:
                    # Mute NoteOn by setting velocity to 0
                    output.append(d1)
                    output.append(0)
                    muted_notes += 1
                else:
                    output.append(d1)
                    output.append(d2)
                pos += 2
            elif status_high in (0xC0, 0xD0):
                # 1 data byte
                output.extend(write_varlen(delta))
                output.append(status)
                output.append(track_data[pos])
                pos += 1
            else:
                output.extend(write_varlen(delta))
                output.append(status)

        else:
            # Running status
            status_high = running_status & 0xF0
            channel = running_status & 0x0F  # 0-based

            if status_high in (0x80, 0x90, 0xA0, 0xB0, 0xE0):
                d1 = track_data[pos]
                d2 = track_data[pos + 1]
                output.extend(write_varlen(delta))
                if status_high == 0x90 and channel not in keep_channels and d2 > 0:
                    output.append(d1)
                    output.append(0)
                    muted_notes += 1
                else:
                    output.append(d1)
                    output.append(d2)
                pos += 2
            elif status_high in (0xC0, 0xD0):
                output.extend(write_varlen(delta))
                output.append(track_data[pos])
                pos += 1
            else:
                output.extend(write_varlen(delta))
                output.append(track_data[pos])
                pos += 1

    return bytes(output), muted_notes


def isolate_channels(input_path, output_path, keep_channels):
    with open(input_path, 'rb') as f:
        data = f.read()

    if data[:4] != b'MThd':
        print("Error: Not a valid MIDI file")
        sys.exit(1)

    header_len = struct.unpack('>I', data[4:8])[0]
    fmt, num_tracks, division = struct.unpack('>HHH', data[8:14])

    print(f"Input: {input_path}")
    print(f"Format: {fmt}, Tracks: {num_tracks}, Division: {division}")
    print(f"Keeping channels: {sorted(keep_channels)} (0-based)")

    output = bytearray()
    output.extend(data[:8 + header_len])

    pos = 8 + header_len
    total_muted = 0

    for track_num in range(num_tracks):
        if pos >= len(data) or data[pos:pos + 4] != b'MTrk':
            break

        track_len = struct.unpack('>I', data[pos + 4:pos + 8])[0]
        track_data = data[pos + 8:pos + 8 + track_len]

        new_track_data, muted = isolate_channels_in_track(track_data, keep_channels)
        total_muted += muted

        output.extend(b'MTrk')
        output.extend(struct.pack('>I', len(new_track_data)))
        output.extend(new_track_data)

        pos += 8 + track_len

    with open(output_path, 'wb') as f:
        f.write(output)

    print(f"Muted {total_muted} NoteOn events")
    print(f"Output: {output_path}")


def main():
    parser = argparse.ArgumentParser(description="Isolate MIDI channels")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--keep", type=int, nargs="+",
                       help="Channels to keep (0-based, others muted)")
    group.add_argument("--mute", type=int, nargs="+",
                       help="Channels to mute (0-based, others kept)")
    parser.add_argument("input", help="Input MIDI file")
    parser.add_argument("output", help="Output MIDI file")

    args = parser.parse_args()

    if args.keep:
        keep_channels = set(args.keep)
    else:
        # mute mode: keep everything except muted channels
        # We don't know all channels, so use 0-15
        keep_channels = set(range(0, 16)) - set(args.mute)

    isolate_channels(args.input, args.output, keep_channels)


if __name__ == '__main__':
    main()
