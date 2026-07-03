#!/usr/bin/env python3
"""Strip specific MIDI events from a MIDI file.

Usage:
    python tools/strip_midi_events.py <input.mid> <output.mid> [options]

Options:
    --strip-pitchbend    Remove PitchBend events (0xE0-0xEF)
    --strip-cc N         Remove CC N events (can be repeated)
    --strip-mod          Shortcut for --strip-cc 1 (Modulation)
    --strip-expr         Shortcut for --strip-cc 11 (Expression)
"""
import argparse
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


def should_strip_event(status, data_bytes, strip_pitchbend, strip_ccs):
    """Check if an event should be stripped."""
    status_high = status & 0xF0

    if strip_pitchbend and status_high == 0xE0:
        return True

    if strip_ccs and status_high == 0xB0 and len(data_bytes) >= 1:
        cc_num = data_bytes[0]
        if cc_num in strip_ccs:
            return True

    return False


def strip_events_from_track(track_data, strip_pitchbend, strip_ccs):
    """Remove specified events from a single MIDI track, preserving timing."""
    pos = 0
    output = bytearray()
    running_status = 0
    pending_delta = 0
    removed_counts = {}

    while pos < len(track_data):
        delta, pos = read_varlen(track_data, pos)
        pending_delta += delta

        if pos >= len(track_data):
            break

        byte = track_data[pos]

        if byte == 0xFF:
            # Meta event - always keep
            event_start = pos
            pos += 1
            if pos >= len(track_data):
                break
            pos += 1  # meta type
            length, pos = read_varlen(track_data, pos)
            event_end = pos + length
            output.extend(write_varlen(pending_delta))
            output.extend(track_data[event_start:event_end])
            pending_delta = 0
            pos = event_end

        elif byte == 0xF0 or byte == 0xF7:
            # SysEx event - always keep
            event_start = pos
            pos += 1
            length, pos = read_varlen(track_data, pos)
            event_end = pos + length
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

            if status_high in (0x80, 0x90, 0xA0, 0xB0, 0xE0):
                data_bytes = track_data[pos:pos + 2]
                if should_strip_event(status, data_bytes, strip_pitchbend, strip_ccs):
                    label = f"PitchBend" if status_high == 0xE0 else f"CC{data_bytes[0]}"
                    removed_counts[label] = removed_counts.get(label, 0) + 1
                    pos += 2
                else:
                    output.extend(write_varlen(pending_delta))
                    output.append(status)
                    output.extend(data_bytes)
                    pending_delta = 0
                    pos += 2
            elif status_high in (0xC0, 0xD0):
                output.extend(write_varlen(pending_delta))
                output.append(status)
                output.append(track_data[pos])
                pending_delta = 0
                pos += 1
            else:
                output.extend(write_varlen(pending_delta))
                output.append(status)
                pending_delta = 0

        else:
            # Running status
            status_high = running_status & 0xF0

            if status_high in (0x80, 0x90, 0xA0, 0xB0, 0xE0):
                data_bytes = track_data[pos:pos + 2]
                if should_strip_event(running_status, data_bytes, strip_pitchbend, strip_ccs):
                    label = f"PitchBend" if status_high == 0xE0 else f"CC{data_bytes[0]}"
                    removed_counts[label] = removed_counts.get(label, 0) + 1
                    pos += 2
                else:
                    output.extend(write_varlen(pending_delta))
                    output.extend(data_bytes)
                    pending_delta = 0
                    pos += 2
            elif status_high in (0xC0, 0xD0):
                output.extend(write_varlen(pending_delta))
                output.append(track_data[pos])
                pending_delta = 0
                pos += 1
            else:
                output.extend(write_varlen(pending_delta))
                output.append(track_data[pos])
                pending_delta = 0
                pos += 1

    return bytes(output), removed_counts


def strip_events(input_path, output_path, strip_pitchbend, strip_ccs):
    with open(input_path, 'rb') as f:
        data = f.read()

    if data[:4] != b'MThd':
        print("Error: Not a valid MIDI file")
        sys.exit(1)

    header_len = struct.unpack('>I', data[4:8])[0]
    fmt, num_tracks, division = struct.unpack('>HHH', data[8:14])

    strip_desc = []
    if strip_pitchbend:
        strip_desc.append("PitchBend")
    for cc in sorted(strip_ccs):
        cc_names = {1: "Modulation", 11: "Expression", 7: "Volume", 10: "Pan"}
        name = cc_names.get(cc, f"CC{cc}")
        strip_desc.append(name)

    print(f"Input: {input_path}")
    print(f"Format: {fmt}, Tracks: {num_tracks}, Division: {division}")
    print(f"Stripping: {', '.join(strip_desc)}")

    output = bytearray()
    output.extend(data[:8 + header_len])

    pos = 8 + header_len
    total_removed = {}

    for track_num in range(num_tracks):
        if pos >= len(data) or data[pos:pos + 4] != b'MTrk':
            print(f"  Warning: Track {track_num} header not found")
            break

        track_len = struct.unpack('>I', data[pos + 4:pos + 8])[0]
        track_data = data[pos + 8:pos + 8 + track_len]

        new_track_data, removed_counts = strip_events_from_track(
            track_data, strip_pitchbend, strip_ccs)

        for label, count in removed_counts.items():
            total_removed[label] = total_removed.get(label, 0) + count

        if removed_counts:
            parts = [f"{label}={count}" for label, count in sorted(removed_counts.items())]
            print(f"  Track {track_num}: removed {', '.join(parts)}")

        output.extend(b'MTrk')
        output.extend(struct.pack('>I', len(new_track_data)))
        output.extend(new_track_data)

        pos += 8 + track_len

    with open(output_path, 'wb') as f:
        f.write(output)

    print(f"\nTotal removed:")
    for label, count in sorted(total_removed.items()):
        print(f"  {label}: {count}")
    print(f"Output: {output_path}")


def main():
    parser = argparse.ArgumentParser(description="Strip specific MIDI events")
    parser.add_argument("input", help="Input MIDI file")
    parser.add_argument("output", help="Output MIDI file")
    parser.add_argument("--strip-pitchbend", action="store_true",
                        help="Remove PitchBend events")
    parser.add_argument("--strip-cc", type=int, action="append", default=[],
                        help="Remove CC N events (can be repeated)")
    parser.add_argument("--strip-mod", action="store_true",
                        help="Shortcut for --strip-cc 1")
    parser.add_argument("--strip-expr", action="store_true",
                        help="Shortcut for --strip-cc 11")

    args = parser.parse_args()

    strip_ccs = set(args.strip_cc)
    if args.strip_mod:
        strip_ccs.add(1)
    if args.strip_expr:
        strip_ccs.add(11)

    if not args.strip_pitchbend and not strip_ccs:
        print("Error: No events to strip. Use --strip-pitchbend, --strip-cc, --strip-mod, or --strip-expr")
        sys.exit(1)

    strip_events(args.input, args.output, args.strip_pitchbend, strip_ccs)


if __name__ == '__main__':
    main()
