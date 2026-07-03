## 実験用ツール

tools フォルダに実験用ツールをおく。venv を activate してから使用する。

```bash
source venv/bin/activate
```

### tools/analyze_midi.py

MIDIファイルのイベント種別・構造を解析する。

```bash
python tools/analyze_midi.py <midi_file>
```

### tools/compare_fft.py

2つのWAVファイルをFFTで周波数領域比較する。

```bash
python tools/compare_fft.py <wav1> <wav2>
```

### tools/measure_analysis.py

MIDIの小節単位でWAVの差分を分析し、差異が大きい箇所のMIDIイベントと相関を取る。

```bash
python tools/measure_analysis.py <ts_wav> <rs_wav> <midi_file>
```

### tools/segment_analysis.py

時間セグメント単位でWAVの差分を分析し、MIDIイベントとの相関を取る。

```bash
python tools/segment_analysis.py <ts_wav> <rs_wav> <midi_file>
```

### tools/extract_channel.py

MIDIファイルから特定チャンネルのみを抽出する。チャンネル番号は0-based。

```bash
python tools/extract_channel.py <input.mid> <output.mid> <channel> [channel2 ...]
# 例: ch5(Strings) + ch9(Drums) を抽出
python tools/extract_channel.py input.mid output.mid 5 9
```

### tools/isolate_channel.py

特定チャンネル以外をミュートする（NoteOn velocity=0）。チャンネル番号は0-based。

```bash
python tools/isolate_channel.py <input.mid> <output.mid> --keep <channel> [channel2 ...]
python tools/isolate_channel.py <input.mid> <output.mid> --mute <channel> [channel2 ...]
# 例: ch5とch9のみ残す
python tools/isolate_channel.py input.mid output.mid --keep 5 9
```

### tools/gain_ratio_analysis.py

2つのディレクトリ内の同名WAVペアについて、Rust/TSのゲイン比を計算する。

```bash
python tools/gain_ratio_analysis.py result/ts result/rust
```

## midiチャンネル番号について

* 基本的に、0 base で扱うこと. ch0-ch15. ドラム=ch9
* どうしても、1 baseで記述する場合、(1base)を後につけること。例: ch10(1base)。ファイル名に使う場合は、_1baseをつけても良い。
* 間違いやすいので、どちらを使っているか確認すること

