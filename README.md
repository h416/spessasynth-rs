# spessasynth-rs

A Rust port of [SpessaSynth](https://github.com/spessasus/SpessaSynth).
Offline WAV rendering from SoundFont2 (.sf2) and MIDI files.

## Quick start

```bash
git clone https://github.com/h416/spessasynth-rs
cd spessasynth-rs
curl -OL https://github.com/mrbumpy409/GeneralUser-GS/raw/refs/heads/main/GeneralUser-GS.sf2
curl -OL https://github.com/mrbumpy409/GeneralUser-GS/raw/refs/heads/main/demo%20MIDIs/J-cycle.mid

cargo run --release --example midi_to_wav -- GeneralUser-GS.sf2 J-cycle.mid J-cycle.wav
```

## Differences from the original

This is a partial port of spessasynth_core v4.1.0. The following limitations apply:

- **MIDI to WAV offline rendering only** — real-time playback and other features are not included
- **No SoundFont3 (SF3 / Vorbis) support** — only SoundFont2 (.sf2) is supported
- **No XMF support**

## Build

```bash
cargo build --release
```

## MIDI to WAV (example)

```bash
cargo run --release --example midi_to_wav -- <soundfont.sf2> <input.mid> <output.wav>
```

### Options

| Option | Description |
|---|---|
| `-g`, `--gain <value>` | Master gain (default: 1.0) |
| `--no-normalize` | Disable audio normalization |
| `-h`, `--help` | Show help |

```bash
# Gain 0.5 without normalization
cargo run --release --example midi_to_wav -- -g 0.5 --no-normalize font.sf2 song.mid out.wav
```

### Generate test MIDI

```bash
cargo run --release --example midi_to_wav -- --gen-test-midi test.mid
```

## Using as a library

`Cargo.toml`:

```toml
[dependencies]
spessasynth-rs = { path = "../spessasynth-rs" }
```

Example:

```rust
use spessasynth_rs::{render_midi_file_to_wav, RenderOptions};

fn main() {
    // Simple: render with default options
    render_midi_file_to_wav("font.sf2", "song.mid", "output.wav", None, None);

    // With options and progress callback
    render_midi_file_to_wav(
        "font.sf2",
        "song.mid",
        "output.wav",
        Some(RenderOptions {
            gain: 0.8,
            normalize: true,
            ..Default::default()
        }),
        Some(&|current, total| {
            println!("Rendered {} / {}", current, total);
        }),
    );
}
```

## Credits

- [SpessaSynth](https://github.com/spessasus/SpessaSynth) by spessasus - Original TypeScript implementation (spessasynth_core v4.1.0)
- Rust port by h416
- Ported with the assistance of [Claude Code](https://claude.ai/code)

## License

Apache-2.0
