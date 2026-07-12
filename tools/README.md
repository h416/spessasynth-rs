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

### tools/windowed_corr.py

2つのWAVを時間窓に分割し、窓ごとの相関と最大サンプル差を表示する。
**WAV乖離の切り分けの起点。** 特定の窓だけ相関が急落→1.0に回復するなら、その窓で
鳴っている特定楽器/イベントの乖離(FP累積ではない)。単調減少ならリバーブ等の累積系。

```bash
python tools/windowed_corr.py <ts.wav> <rs.wav> [--win 10] [--thresh 0.9999]
# 例: 5秒窓で乖離窓を洗い出す
python tools/windowed_corr.py sample/result/ts430/EarthDay.wav sample/result/rust/EarthDay.wav --win 5
```

その後 `isolate_channel.py` でチャンネルを絞り、`gen_single_note.py` で楽器を最小再現する。

### tools/gen_single_note.py

指定 program を1チャンネルで1音だけ鳴らす最小MIDIを生成する。特定楽器の合成を
最小再現して TS と比較するのに使う(program 番号は 0-based の GM 番号)。

```bash
python tools/gen_single_note.py <out.mid> --program <0-127> [--note 60] [--velocity 100] \
    [--channel 0] [--hold-beats 8] [--tail-beats 4]
# 例: トランペット(program 56)を1音生成して比較
python tools/gen_single_note.py /tmp/mini56.mid --program 56
cargo run --release --example midi_to_wav -- <sf2> /tmp/mini56.mid /tmp/mini56_rs.wav
tsx tmp/spessasynth_core-4.3.0/examples/midi_to_wav_node.ts <sf2> /tmp/mini56.mid /tmp/mini56_ts.wav
python tools/windowed_corr.py /tmp/mini56_ts.wav /tmp/mini56_rs.wav --win 0.5
```

## midiチャンネル番号について

* 基本的に、0 base で扱うこと. ch0-ch15. ドラム=ch9
* どうしても、1 baseで記述する場合、(1base)を後につけること。例: ch10(1base)。ファイル名に使う場合は、_1baseをつけても良い。
* 間違いやすいので、どちらを使っているか確認すること

