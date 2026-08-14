# gugen（具現）

説明可能な材料合成・プロセス計画ライブラリ（Rust製）。

目標材料の組成（および任意で目標結晶構造）を入力として受け取り、候補となる
前駆体の組み合わせ、バランスの取れた反応式、固相合成のプロセス計画を、根拠
（evidence）・仮定（assumption）・未確定条件（unresolved）を明示したまま、
機械可読な形で返します。実験の成功を保証するものではありません。

> **ステータス：v0.1 リリース候補。** 全9フェーズが完了（アーキテクチャ
> 設計、基盤型定義、厳密な反応式バランス、bounded前駆体探索、固相合成プロ
> セステンプレート、plan scoring・confidence、これらを一気通貫で統括する
> `Planner`、CLI、文献に基づくfixtureによる検証一式 --
> [`docs/benchmark_report.md`](docs/benchmark_report.md) 参照 -- および
> v0.1リリース準備)。オプションの`mikiwame`機能で構造診断結果を取り込め
> ますが、`chematic-crystal`アダプタは同クレート未公開のため保留中です。
> 未公開・`main`未マージです（候補状態に達したこと自体はmerge/publishの
> 許可を意味せず、それは所有者が明示的に判断します）。フェーズごとの詳細は
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
1 Ba:1, O:1 + 1 O:2, Ti:1 -> 1 Ba:1, O:3, Ti:1
```

### Bounded前駆体探索

`search_precursor_sets` は前駆体カタログに対して決定的かつbudget制約付きの
探索を行い、採用された前駆体セット（各々にバランス済み反応式付き）と、却下
された候補すべてに理由コードを付けて返します。採用候補だけを返すことはあり
ません。具体例は [`src/precursor.rs`](src/precursor.rs) のテストを参照して
ください。

### 固相合成プロセステンプレート

`conventional_solid_state_template` は採用済み前駆体セットを、秤量・混合・
粉砕・成形・焼成・冷却・中間確認のstep列に変換します。各stepには
`Required`/`Recommended`/`Optional`/`Unresolved`のいずれかが付与されます。
すべての材料へ同一のtemplateを適用することはありません。byproductを放出
する経路（例：炭酸塩経路でCO₂を放出）には、酸化物のみの経路にはない仮焼
stepが追加されます。温度・時間・昇温速度・雰囲気は、根拠なく推測せず
`None`（未確定）のままとします。gugenは現時点でthermodynamic/文献evidence
providerを持たないためです。

### Plan scoring・confidence

`score_plan` はplanごとに`PlanScoreBreakdown`と`ConfidenceAssessment`を
算出します。単一の数値に潰すことはありません。thermodynamicデータの欠損
は失敗扱いにせずスコアから除外し、evidenceのないplanはevidenceのある
planより低いスコアになります。`total_ranking_score`は候補を比較するため
の序数的・説明可能なスコアであり、成功確率ではありません。v0.1では
route familyが1つのみでthermodynamic providerも存在しないため、実質的に
`process_simplicity`という単一の指標だけが結果を左右します（内訳の詳細は
[`PlanScoreBreakdown`のdocコメント](src/score.rs)を参照）。
gugenは現時点でhazard/safetyデータsourceを持たないため、すべてのplanで
`manual_review_required: true`となります。

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

出力（`serde_json::to_string_pretty`、1フィールド1行）：

```json
[
  {
    "reactants": [
      {
        "composition": {
          "Ba": 1.0,
          "O": 1.0
        },
        "coefficient": 1
      },
      {
        "composition": {
          "O": 2.0,
          "Ti": 1.0
        },
        "coefficient": 1
      }
    ],
    "products": [
      {
        "composition": {
          "Ba": 1.0,
          "O": 3.0,
          "Ti": 1.0
        },
        "coefficient": 1
      }
    ]
  }
]
```

CLIのビルドは `cargo build --features serde,clap --bin gugen`。サブコマンド
（AGENTS.md §19）：

```
gugen balance reaction.json
gugen plan target.json --catalog precursors.json [--output report.json] [--format json|markdown]
gugen explain report.json --plan plan-001
gugen validate-target target.json
gugen doctor
gugen batch input.json --catalog precursors.json [--output out.json]
```

`target.json`・`precursors.json`・`input.json` は、CLI専用の形式を新設せず
gugen自身の公開JSON形式（`TargetSpecification`、`PrecursorCandidate`の
JSON配列、`TargetSpecification`のJSON配列）をそのまま再利用しています。
`gugen batch` は各targetを独立に計画し、一件の失敗が残りを止めることは
ありません。

### 実例：合成計画レポート全体

```
$ gugen plan target.json --catalog precursors.json --format markdown
```

`target.json`（BaTiO3）と`precursors.json`（BaCO3 + TiO2という標準的な
固相合成ルート）：

```json
{
  "composition": {"Ba": 1.0, "Ti": 1.0, "O": 3.0},
  "structure": null,
  "desired_phase": null,
  "constraints": {"forbidden_elements": []}
}
```

```json
[
  {"id": "BaCO3", "composition": {"Ba": 1.0, "C": 1.0, "O": 3.0}, "availability": null},
  {"id": "TiO2", "composition": {"Ti": 1.0, "O": 2.0}, "availability": null}
]
```

出力（実際の`gugen plan`の出力そのもの。`tests/fixtures/batio3_report.md`の
golden snapshotと同一内容だが、紙面の都合でunresolved一覧とrejected
candidatesの節を省略。両方とも完全な出力ファイルには含まれる）：

```markdown
# Synthesis Planning Report (schema v1)

**Target:** Ba:1, O:3, Ti:1

**Applicability:** PartiallyInDomain -- formula-only target, no structure provided (AGENTS.md §16's own example for this level)

## Plan plan-a702f5b0380d3716 (score 0.062)

- Target: Ba:1, O:3, Ti:1
- Route family: ConventionalSolidState
- Reaction: 1x(Ba:1, C:1, O:3) + 1x(O:2, Ti:1) -> 1x(Ba:1, O:3, Ti:1) + 1x(C:1, O:2)
- Manual review required: true
- Applicability: PartiallyInDomain -- formula-only target, no structure provided (AGENTS.md §16's own example for this level)

### Steps

- [Required] Weigh: BaCO3 x1, TiO2 x1
- [Required] Mix (DryMixing)
- [Required] Grind (MortarAndPestle), duration=unresolved
- [Optional] Form (UniaxialPressing), pressure=unresolved
- [Required] Heat (Calcination): temperature=unresolved, duration=unresolved, atmosphere=unresolved, ramp=unresolved
- [Recommended] Grind (MortarAndPestle), duration=unresolved
- [Required] Heat (Sintering): temperature=unresolved, duration=unresolved, atmosphere=unresolved, ramp=unresolved
- [Required] Cool (FurnaceCooling)
- [Recommended] Characterize (Xrd): verify target-phase formation

### Evidence

- [Weak/ProcessTemplate] weigh/mix/grind/form are the fixed opening sequence of the v0.1 conventional solid-state template
- [Strong/StoichiometricBalance] balanced reaction releases a byproduct beyond the target, indicating a decomposition (calcination) step is needed before the final firing step
- [Weak/ProcessTemplate] AGENTS.md §11's template outline places a regrind between calcination and final firing

### Warnings

- [Caution] temperature, duration, ramp rate, and atmosphere are unresolved for every heating step: gugen has no thermodynamic or literature evidence provider wired in yet (AGENTS.md §4.1)
- [Severe] no hazard or safety data source is wired in yet: safety_penalty carries no real safety information, and this is not a safety clearance (AGENTS.md §15 "unknown hazardを安全と扱わない")
```

仮焼（Calcination）と再粉砕のstepがあるのは、バランス済み反応式がCO2を
放出しているためです（Evidenceの2番目の項目を参照）。すべてのplanに同じ
templateが適用されるわけではなく、炭酸塩を経由しないルートにはこのstepは
付きません。完全なレポートには、plan単位のスコア内訳、confidence評価
（1つに潰した数値ではなく5つの独立した副指標）、assumptions一覧、却下
された単一前駆体候補すべてが理由コード付きで含まれます。

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
です（2026-08-14時点で未公開。詳細は [`docs/integration.md`](docs/integration.md)）。
それまでは最小限のtrait境界を独自に持って動作します。`mikiwame`は公開済みで、
optionalな`mikiwame`機能（`cargo build --features mikiwame`、デフォルト無効）
として構造診断結果をgugen側の警告・confidenceに変換できますが、`Planner::plan`
には未接続です（gugenの`TargetStructure`がまだ実構造データを持たないため）。
gugenは`renkin`（分子逆合成）に依存せず、その
アルゴリズムも流用しません。gugenはrenkinの移植版ではなく、材料領域における
姉妹プロジェクトです。

## 開発

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --no-default-features
cargo test --no-default-features --features mikiwame
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
cargo build --features serde,clap --bin gugen
cargo check --target wasm32-unknown-unknown
cargo check --target wasm32-unknown-unknown --features mikiwame
cargo audit
```

仕様全体：[`AGENTS.md`](AGENTS.md)。設計判断の詳細：[`docs/`](docs/)。
フェーズごとの進捗：[`tasks/todo.md`](tasks/todo.md)。

## ライセンス

[Apache License, Version 2.0](LICENSE-APACHE) または
[MIT license](LICENSE-MIT) のいずれかを選択できます。

English version: [README.md](README.md)
