#!/usr/bin/env python3
"""Strip PitchBend events (0xE0-0xEF) from a MIDI file.

Usage:
    python tools/strip_pitchbend.py <input.mid> <output.mid>
"""
import struct
import sys


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


def write_varlen(value):
    """Encode a value as MIDI variable-length quantity."""
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


def strip_pitchbend_from_track(track_data):
    """Remove PitchBend events from a single MIDI track, preserving timing."""
    pos = 0
    output = bytearray()
    running_status = 0
    pending_delta = 0
    removed = 0

    while pos < len(track_data):
        # Read delta time
        delta, pos = read_varlen(track_data, pos)
        pending_delta += delta

        if pos >= len(track_data):
            break

        # Determine event type
        byte = track_data[pos]

        if byte == 0xFF:
            # Meta event
            event_start = pos
            pos += 1  # 0xFF
            if pos >= len(track_data):
                break
            meta_type = track_data[pos]
            pos += 1
            length, pos = read_varlen(track_data, pos)
            event_end = pos + length
            # Keep meta events
            output.extend(write_varlen(pending_delta))
            output.extend(track_data[event_start:event_end])
            pending_delta = 0
            pos = event_end

        elif byte == 0xF0 or byte == 0xF7:
            # SysEx event
            event_start = pos
            pos += 1
            length, pos = read_varlen(track_data, pos)
            event_end = pos + length
            # Keep SysEx events
            output.extend(write_varlen(pending_delta))
            output.extend(track_data[event_start:event_end])
            pending_delta = 0
            pos = event_end

        elif byte & 0x80:
            # New status byte
            status = byte
            running_status = status
            pos += 1

            status_high = status & 0xF0

            if status_high == 0xE0:
                # PitchBend — skip (2 data bytes)
                pos += 2
                removed += 1
            elif status_high in (0x80, 0x90, 0xA0, 0xB0):
                # 2 data bytes
                output.extend(write_varlen(pending_delta))
                output.append(status)
                output.extend(track_data[pos:pos + 2])
                pending_delta = 0
                pos += 2
            elif status_high in (0xC0, 0xD0):
                # 1 data byte
                output.extend(write_varlen(pending_delta))
                output.append(status)
                output.append(track_data[pos])
                pending_delta = 0
                pos += 1
            else:
                # Unknown status, just copy
                output.extend(write_varlen(pending_delta))
                output.append(status)
                pending_delta = 0

        else:
            # Running status
            status_high = running_status & 0xF0

            if status_high == 0xE0:
                # PitchBend running status — skip (2 data bytes including current)
                pos += 2  # current byte + next byte
                removed += 1
            elif status_high in (0x80, 0x90, 0xA0, 0xB0):
                # 2 data bytes (current + next)
                output.extend(write_varlen(pending_delta))
                output.extend(track_data[pos:pos + 2])
                pending_delta = 0
                pos += 2
            elif status_high in (0xC0, 0xD0):
                # 1 data byte
                output.extend(write_varlen(pending_delta))
                output.append(track_data[pos])
                pending_delta = 0
                pos += 1
            else:
                output.extend(write_varlen(pending_delta))
                output.append(track_data[pos])
                pending_delta = 0
                pos += 1

    return bytes(output), removed


def strip_pitchbend(input_path, output_path):
    with open(input_path, 'rb') as f:
        data = f.read()

    if data[:4] != b'MThd':
        print("Error: Not a valid MIDI file")
        sys.exit(1)

    header_len = struct.unpack('>I', data[4:8])[0]
    fmt, num_tracks, division = struct.unpack('>HHH', data[8:14])

    print(f"Input: {input_path}")
    print(f"Format: {fmt}, Tracks: {num_tracks}, Division: {division}")

    output = bytearray()
    # Copy header
    output.extend(data[:8 + header_len])

    pos = 8 + header_len
    total_removed = 0

    for track_num in range(num_tracks):
        if pos >= len(data) or data[pos:pos + 4] != b'MTrk':
            print(f"  Warning: Track {track_num} header not found")
            break

        track_len = struct.unpack('>I', data[pos + 4:pos + 8])[0]
        track_data = data[pos + 8:pos + 8 + track_len]

        new_track_data, removed = strip_pitchbend_from_track(track_data)
        total_removed += removed

        if removed > 0:
            print(f"  Track {track_num}: removed {removed} PitchBend events")

        # Write track header with new length
        output.extend(b'MTrk')
        output.extend(struct.pack('>I', len(new_track_data)))
        output.extend(new_track_data)

        pos += 8 + track_len

    with open(output_path, 'wb') as f:
        f.write(output)

    print(f"\nTotal PitchBend events removed: {total_removed}")
    print(f"Output: {output_path}")


if __name__ == '__main__':
    if len(sys.argv) != 3:
        print("Usage: python strip_pitchbend.py <input.mid> <output.mid>")
        sys.exit(1)
    strip_pitchbend(sys.argv[1], sys.argv[2])
