# gugen（具現）

説明可能な材料合成・プロセス計画ライブラリ（Rust製）。

目標材料の組成（および任意で目標結晶構造）を入力として受け取り、候補となる
前駆体の組み合わせ、バランスの取れた反応式、固相合成のプロセス計画を、根拠
（evidence）・仮定（assumption）・未確定条件（unresolved）を明示したまま、
機械可読な形で返します。実験の成功を保証するものではありません。

> **ステータス：開発初期、v0.1 開発中。** 全9フェーズ中フェーズ0〜3が完了
> （アーキテクチャ設計、基盤型定義、厳密な反応式バランス、bounded前駆体探索）。
> ranking、プロセステンプレート、`Planner`型、CLIの大部分はまだ存在しません。
> 未公開・`main`未マージ・利用不可な状態です。フェーズごとの詳細は
> [`tasks/todo.md`](tasks/todo.md) を、レビュー中の内容は
> [draft PR](https://github.com/kent-tokyo/gugen/pull/1) を参照してください。

## gugenが保証すること・しないこと

gugenの出力は候補計画であり、実験SOPや成功保証ではありません。次を保証しま
せん：実験の成功、目標相の生成、単相になること、指定温度での反応完了、高収
率、安全な実行可能性、特許性、工業的量産可能性。ranking scoreは候補を並べ替
えるための序数的・説明可能な指標であり、成功確率ではありません。v0.1の対象
範囲の詳細は [`docs/scientific_scope.md`](docs/scientific_scope.md) を、
evidence・assumption・unresolvedの分離方針は
[`docs/evidence_model.md`](docs/evidence_model.md) を参照してください。

## 現在動作するもの

### 反応式バランス

元素×化学種行列に対する厳密有理数のGauss-Jordan消去法を採用しており、浮動
小数点近似は一切使用しません（詳細は [`docs/architecture.md`](docs/architecture.md)）。
このサンプルの実行可能なソース全体は
[`examples/balance_batio3.rs`](examples/balance_batio3.rs) にあります。

```rust
use gugen::{balance, Composition, Element};

let ba = Element::new("Ba")?;
let ti = Element::new("Ti")?;
let o = Element::new("O")?;

let bao = Composition::new([(ba, 1.0), (o, 1.0)])?;
let tio2 = Composition::new([(ti, 1.0), (o, 2.0)])?;
let batio3 = Composition::new([(ba, 1.0), (ti, 1.0), (o, 3.0)])?;

let reactions = balance(&[bao, tio2], &[batio3])?;
```

出力（`cargo run --example balance_batio3`）：

```
1 Ba1O1 + 1 O2Ti1 -> 1 Ba1O3Ti1
```

### Bounded前駆体探索

`search_precursor_sets` は前駆体カタログに対して決定的かつbudget制約付きの
探索を行い、採用された前駆体セット（各々にバランス済み反応式付き）と、却下
された候補すべてに理由コードを付けて返します。採用候補だけを返すことはあり
ません。具体例は [`src/precursor.rs`](src/precursor.rs) のテストを参照して
ください。

### CLI

```
$ gugen balance reaction.json
```

`reaction.json`：

```json
{
  "reactants": [
    {"Ba": 1.0, "O": 1.0},
    {"Ti": 1.0, "O": 2.0}
  ],
  "products": [
    {"Ba": 1.0, "Ti": 1.0, "O": 3.0}
  ]
}
```

出力：

```json
[
  {
    "reactants": [
      { "composition": { "Ba": 1.0, "O": 1.0 }, "coefficient": 1 },
      { "composition": { "O": 2.0, "Ti": 1.0 }, "coefficient": 1 }
    ],
    "products": [
      { "composition": { "Ba": 1.0, "O": 3.0, "Ti": 1.0 }, "coefficient": 1 }
    ]
  }
]
```

CLIのビルドは `cargo build --features serde,clap --bin gugen`。現時点では
`gugen balance` のみ実装済みで、`plan`・`explain`・`validate-target`・
`doctor`・`batch` はPhase 7で実装予定です。

## エコシステム上の位置づけ

```
                       chematic-crystal
               periodic structure foundation
                             │
                ┌────────────┴────────────┐
                │                         │
             mikiwame                  gugen
     explainable diagnostics    synthesis/process planning
```

gugenは`chematic-crystal`が公開され次第、周期構造の型をそこに依存する予定
です（2026-08-13時点で未公開。詳細は [`docs/integration.md`](docs/integration.md)）。
それまでは最小限のtrait境界を独自に持って動作します。`mikiwame`連携は
optionalでデフォルト無効です。gugenは`renkin`（分子逆合成）に依存せず、その
アルゴリズムも流用しません。gugenはrenkinの移植版ではなく、材料領域における
姉妹プロジェクトです。

## 開発

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
cargo audit
```

仕様全体：[`AGENTS.md`](AGENTS.md)。設計判断の詳細：[`docs/`](docs/)。
フェーズごとの進捗：[`tasks/todo.md`](tasks/todo.md)。

## ライセンス

[Apache License, Version 2.0](LICENSE-APACHE) または
[MIT license](LICENSE-MIT) のいずれかを選択できます。

English version: [README.md](README.md)
