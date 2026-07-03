#!/bin/bash
# Batch WAV comparison: TS vs Rust
# Usage: bash tools/batch_compare.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

SOUNDFONT="sample/soundfont/GeneralUser-GS.sf2"
MIDI_DIR="sample/midi"
TS_DIR="sample/result/ts"
RS_DIR="sample/result/rust"

MIDI_FILES=(
  Bond
  Breakout
  Dance
  EarthDay
  J-cycle
  Jump
  SantaClaus
  TheHYBRIDCollage
  UminoMieruMachi
)

# Ensure output directories exist
mkdir -p "$TS_DIR" "$RS_DIR"

# Step 1: Generate TS WAVs (skip if already exists)
echo "=== Step 1: Generating TS WAVs ==="
for name in "${MIDI_FILES[@]}"; do
  if [ -f "$TS_DIR/${name}.wav" ]; then
    echo "  SKIP: $TS_DIR/${name}.wav (already exists)"
  else
    echo "  Generating: $TS_DIR/${name}.wav"
    tsx tmp/spessasynth_core-4.2.0/examples/midi_to_wav_node.ts \
      "$SOUNDFONT" "$MIDI_DIR/${name}.mid" "$TS_DIR/${name}.wav"
  fi
done

# Step 2: Generate Rust WAVs (always regenerate)
echo ""
echo "=== Step 2: Generating Rust WAVs ==="
for name in "${MIDI_FILES[@]}"; do
  echo "  Generating: $RS_DIR/${name}.wav"
  cargo run --release --example midi_to_wav -- \
    "$SOUNDFONT" "$MIDI_DIR/${name}.mid" "$RS_DIR/${name}.wav"
done

# Step 3: Compare
echo ""
echo "=== Step 3: Comparing WAVs ==="
python3 "$SCRIPT_DIR/batch_compare_impl.py" "$TS_DIR" "$RS_DIR" "${MIDI_FILES[@]}"
