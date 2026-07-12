#!/usr/bin/env python3
"""gen_single_note.py - Generate a minimal single-note MIDI for one program.

Emits a valid Standard MIDI File (format 0) that sets one program on one channel
and plays a single sustained note, then releases it. This is the smallest possible
reproduction of a single instrument's synthesis, for isolating a TS-vs-Rust
divergence down to one program (see also tools/isolate_channel.py to find the
channel/program from a real song first).

Usage:
    python tools/gen_single_note.py out.mid --program 56
    python tools/gen_single_note.py out.mid --program 0 --note 60 --velocity 100 \
        --hold-beats 8 --tail-beats 4 --channel 0

Notes:
    * --channel is 0-based (drums = channel 9).
    * Division is 480 ticks/beat; default tempo is the SMF default 120 BPM
      (500000 us/beat), so 1 beat = 0.5 s. hold-beats/tail-beats scale from that.
    * Render with the usual example, e.g.:
        cargo run --release --example midi_to_wav -- font.sf2 out.mid out.wav
"""
import argparse
import struct


def vlq(n: int) -> bytes:
    b = [n & 0x7F]
    n >>= 7
    while n:
        b.append((n & 0x7F) | 0x80)
        n >>= 7
    return bytes(reversed(b))


def ev(delta: int, *data: int) -> bytes:
    return vlq(delta) + bytes(data)


def main():
    ap = argparse.ArgumentParser(description="Generate a single-note MIDI for one program.")
    ap.add_argument("output", help="output .mid path")
    ap.add_argument("--program", type=int, required=True, help="GM program 0-127")
    ap.add_argument("--note", type=int, default=60, help="note number (default 60 = middle C)")
    ap.add_argument("--velocity", type=int, default=100, help="note-on velocity (default 100)")
    ap.add_argument("--channel", type=int, default=0, help="0-based channel (default 0; drums=9)")
    ap.add_argument("--hold-beats", type=float, default=8.0, help="note duration in beats (default 8)")
    ap.add_argument("--tail-beats", type=float, default=4.0, help="silence after note-off in beats (default 4)")
    args = ap.parse_args()

    div = 480
    ch = args.channel & 0x0F
    hold = int(round(args.hold_beats * div))
    tail = int(round(args.tail_beats * div))

    tr = b""
    tr += ev(0, 0xC0 | ch, args.program & 0x7F)          # program change
    tr += ev(0, 0x90 | ch, args.note & 0x7F, args.velocity & 0x7F)  # note on
    tr += ev(hold, 0x80 | ch, args.note & 0x7F, 0)       # note off
    tr += ev(tail, 0xFF, 0x2F, 0)                         # end of track

    head = b"MThd" + struct.pack(">I", 6) + struct.pack(">HHH", 0, 1, div)
    trk = b"MTrk" + struct.pack(">I", len(tr)) + tr
    with open(args.output, "wb") as f:
        f.write(head + trk)
    print(f"wrote {args.output}: program={args.program} note={args.note} "
          f"vel={args.velocity} ch={ch} hold={args.hold_beats}b tail={args.tail_beats}b")


if __name__ == "__main__":
    main()
