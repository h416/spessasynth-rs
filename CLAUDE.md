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
tsx ./tmp/spessasynth_core-4.2.0/examples/midi_to_wav_node.ts sample/soundfont/GeneralUser-GS.sf2 sample/midi/J-cycle.mid sample/result/ts/J-cycle.wav

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

## テストデータ

- soundfont: sample/soundfont/GeneralUser-GS.sf2 
- midi: sample/midi/


## 実験用ツール

tools フォルダに実験用ツールをおく。

## midiチャンネル番号について

* 基本的に、0 base で扱うこと. ch0-ch15. ドラム=ch9
* どうしても、1 baseで記述する場合、(1base)を後につけること。例: ch10(1base)。ファイル名に使う場合は、_1baseをつけても良い。
* 間違いやすいので、どちらを使っているか確認すること

