#!/usr/bin/env python3
"""Extract specific MIDI channel(s) into a new MIDI file.

Usage:
    python tools/extract_channel.py <input.mid> <output.mid> <channel> [channel2 ...]

Channels are 0-based (0-15, e.g. 9 for drums).
Preserves tempo, time signature, and other meta events.
"""

import sys
import mido


def extract_channels(input_path: str, output_path: str, channels: list[int]):
    mid = mido.MidiFile(input_path)
    out = mido.MidiFile(type=mid.type, ticks_per_beat=mid.ticks_per_beat)

    ch_set = set(channels)

    for track in mid.tracks:
        new_track = mido.MidiTrack()
        pending_time = 0
        for msg in track:
            if msg.is_meta:
                # Keep all meta events, accumulate pending delta time
                new_msg = msg.copy(time=msg.time + pending_time)
                new_track.append(new_msg)
                pending_time = 0
            elif hasattr(msg, 'channel') and msg.channel in ch_set:
                new_msg = msg.copy(time=msg.time + pending_time)
                new_track.append(new_msg)
                pending_time = 0
            elif not hasattr(msg, 'channel'):
                # SysEx or other non-channel messages — keep them
                new_msg = msg.copy(time=msg.time + pending_time)
                new_track.append(new_msg)
                pending_time = 0
            else:
                # Filtered out — accumulate delta time for next kept message
                pending_time += msg.time

        out.tracks.append(new_track)

    out.save(output_path)
    ch_str = ', '.join(str(c) for c in channels)
    print(f"Extracted channel(s) {ch_str} from {input_path}")
    print(f"Saved to {output_path}")


def main():
    if len(sys.argv) < 4:
        print("Usage: python extract_channel.py <input.mid> <output.mid> <channel> [channel2 ...] (0-based)")
        sys.exit(1)

    input_path = sys.argv[1]
    output_path = sys.argv[2]
    channels = [int(c) for c in sys.argv[3:]]

    extract_channels(input_path, output_path, channels)


if __name__ == '__main__':
    main()
