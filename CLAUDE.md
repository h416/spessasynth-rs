# CLAUDE.md

このファイルは、Claude Code (claude.ai/code) がこのリポジトリで作業する際のガイダンスを提供します。
返答は日本語で行なってください。

## プロジェクト概要

typescryptで書かれた、spessasynth_core を 
rustのプロジェクトに移植します。

ファイル単位、関数単位で同じになるように移植します。

移植する機能
midiからwavを生成する機能のみです。
その他の機能は不要です。

## wav 生成方法

- typescript
tsx ./tmp/spessasynth_core-4.3.0/examples/midi_to_wav_node.ts sample/soundfont/GeneralUser-GS.sf2 sample/midi/J-cycle.mid sample/result/ts/J-cycle.wav

- rust
cargo run --release --example midi_to_wav -- sample/soundfont/GeneralUser-GS.sf2 sample/midi/J-cycle.mid sample/result/rust/J-cycle.wav

## 移植方針メモ

### `src/midi/xmf_loader.ts` は移植不要

XMFはには対応しません。

### sf3(Vorbis) 関連は移植不要

SoundFont3 (SF3, Vorbis)には対応しません。

### コメント

移植するrustソースコードのコメントは英語で記述する

### rustの予約後

"gen"はrustで予約後なので、"r#gen" を使う

### randomの扱い

random pan のイベントがあると、ts,rustで差異が起こる。
決定的なrandomを使う。typescriptを、rustに合わせる。tschange/ フォルダを参照。

## rust と　typescript 浮動小数点の違い

* typescriptは、Float32Array以外、f64になる
* rustで、[f32],Vec<f32> 配列(バッファ)以外、f64を使うこと。配列に代入するときに初めてf32にキャストする。配列から取り出したらすぐ、f64にキャストする。

## バージョンアップ移植時の注意(WAV乖離の主因)

spessasynth_core のバージョンを上げて移植する時、WAV が TS版と乖離する主因は
**「旧バージョンの定数・デフォルト値・式が残ったまま、新バージョンの変更が反映漏れ」** になること。
`tmp/spessasynth_core-<旧>` と `tmp/spessasynth_core-<新>` の両ソースを直接 `diff` し、
数値定数・デフォルト値・式・制御フローの変更を1つずつ Rust 側と照合すること。

### 4.2.0→4.3.0 で実際にあった乖離(すべて修正済み・再発注意)

1. **ピッチの整数セント切り捨て(最大の犯人)**: `render_voice` で
   `cents_total = (cents + semitones*100) as i32` としていた。新版は整数を
   「再計算するかの判定キー」にのみ使い、`tuningRatio = pow(2, centsTotal/1200)` は
   **フル float** で計算する。切り捨てるとビブラート・fine tune・pitch bend 等の
   サブセント動作を持つ持続音の位相がドリフトし、管弦楽器で顕著に乖離する。
2. **デフォルトコントローラ値の変更漏れ**: `DEFAULT_MIDI_CONTROLLERS`(新版 `channel/reset.ts`)を
   1行ずつ照合する。例: reverb depth(CC91)40→0、NRPN(`DEFAULT_NRPN`)127→0、
   portamento デフォルト削除、CC121(resetRP15)のリセット範囲が8CCのみに限定、など。
3. **機構の削除に伴う経路変更**: 旧版の「custom vibrato」削除に伴い、GS NRPN vibrato は
   `channel_vibrato` フィールドではなく vibrato CC(76/77/78)経由で vibLfoRate 生成子を駆動する。
   旧経路を残すと dead code になり効かない。
4. **エフェクト定数**: chorus/delay/reverb の gain・character係数・damping・send補正
   (`EFX_SENDS_GAIN_CORRECTION` 等)も変更されている。

### WAV乖離の切り分け手法(有効な順)

1. 窓別相関(例 10秒ごと)を見る。「特定の窓だけ急落→1.0に回復」なら FP累積ではなく
   **特定の楽器/イベントの乖離**。単調減少なら累積系。
2. `tools/isolate_channel.py --keep <ch>` でチャンネルを特定。
3. 単音MIDI(program 指定・1ノート持続)を生成して楽器を最小再現。
4. oscillator → filter → volume envelope → effect-send を段階的にダンプして層を特定。
5. 該当層の generator/定数/式を新旧TSと Rust で3者照合。

## テストデータ

- soundfont: sample/soundfont/GeneralUser-GS.sf2 
- midi: sample/midi/


## 実験用ツール

tools フォルダに実験用ツールをおく。

## midiチャンネル番号について

* 基本的に、0 base で扱うこと. ch0-ch15. ドラム=ch9
* どうしても、1 baseで記述する場合、(1base)を後につけること。例: ch10(1base)。ファイル名に使う場合は、_1baseをつけても良い。
* 間違いやすいので、どちらを使っているか確認すること

