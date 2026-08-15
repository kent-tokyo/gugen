# gugen（具現）

[![Crates.io](https://img.shields.io/crates/v/gugen.svg)](https://crates.io/crates/gugen)
[![docs.rs](https://img.shields.io/docsrs/gugen)](https://docs.rs/gugen)
[![CI](https://github.com/kent-tokyo/gugen/actions/workflows/ci.yml/badge.svg)](https://github.com/kent-tokyo/gugen/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/gugen.svg)](#ライセンス)

[English](README.md) | **日本語**

説明可能な材料合成・プロセス計画ライブラリ（Rust製）。

目標材料の組成（および任意で目標結晶構造）を入力として受け取り、候補となる
前駆体の組み合わせ、バランスの取れた反応式、固相合成のプロセス計画を、根拠
（evidence）・仮定（assumption）・未確定条件（unresolved）を明示したまま、
機械可読な形で返します。実験の成功を保証するものではありません。

> **ステータス：v0.4.0 公開済み。**
> [crates.io](https://crates.io/crates/gugen) /
> [docs.rs](https://docs.rs/gugen) /
> [v0.4.0リリース](https://github.com/kent-tokyo/gugen/releases/tag/v0.4.0)。
> v0.4.0では、ガスを含まない固相系向けの有限温度熱力学プリミティブ、
> DOI間のagreement/conflict分類を備えた大規模文献観測スナップショット
> API、`Planner`のレポートへ参照専用として表示される文献証拠（プロセス
> 条件の自動補完やscore・confidence・rankingへの影響は一切ありません）
> を追加しました。詳細な一覧と既知の制限は
> [`CHANGELOG.md`](CHANGELOG.md)を参照してください。

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
`None`（未確定）のままとします。このworked exampleは`Planner::offline_minimal`
を使っており、providerを一切wireしていないためです。呼び出し側が
`ProcessEvidenceProvider`（例：`InMemoryLiteratureConditionProvider`）を
設定すれば、これらの一部を引用文献から解決できます — `docs/integration.md`
を参照してください。

### Plan scoring・confidence

`score_plan` はplanごとに`PlanScoreBreakdown`と`ConfidenceAssessment`を
算出します。単一の数値に潰すことはありません。thermodynamicデータの欠損
は失敗扱いにせずスコアから除外し、evidenceのないplanはevidenceのある
planより低いスコアになります。`total_ranking_score`は候補を比較するため
の序数的・説明可能なスコアであり、成功確率ではありません。`ThermodynamicProvider`
の設定有無に関わらず`thermodynamic_support`は常に`None`のままです（解決
されたreaction energyはevidenceにはなってもスコアには反映されません —
AGENTS.md §4.3）。そのため実質的に`process_simplicity`という単一の指標
だけが結果を左右します（Phase 12でroute familyが2つになって以降は
route familyごとに計算されます — 詳細は下記）。内訳の詳細は
[`PlanScoreBreakdown`のdocコメント](src/score.rs)を参照してください。
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
golden snapshotと同一内容だが、紙面の都合でscore breakdown/confidence/
assumptions/unresolved一覧とrejected candidatesの節を省略。すべて完全な
出力ファイルには含まれる）：

```markdown
# Synthesis Planning Report (schema v1)

**Target:** Ba:1, O:3, Ti:1

**Applicability:** PartiallyInDomain -- formula-only target, no structure provided (AGENTS.md §16's own example for this level)

## Plan plan-1677d44bfe4dbdc2 (score 0.062)

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

## Plan plan-ee311be9350b7d8b (score 0.062)

- Target: Ba:1, O:3, Ti:1
- Route family: Mechanochemical
- Reaction: 1x(Ba:1, C:1, O:3) + 1x(O:2, Ti:1) -> 1x(Ba:1, O:3, Ti:1) + 1x(C:1, O:2)
- Manual review required: true
- Applicability: PartiallyInDomain -- formula-only target, no structure provided (AGENTS.md §16's own example for this level)

### Steps

- [Required] Weigh: BaCO3 x1, TiO2 x1
- [Required] Grind (BallMilling), duration=unresolved
- [Optional] Form (UniaxialPressing), pressure=unresolved
- [Required] Heat (Annealing): temperature=unresolved, duration=unresolved, atmosphere=unresolved, ramp=unresolved
- [Required] Cool (FurnaceCooling)
- [Recommended] Characterize (Xrd): verify target-phase formation

### Evidence

- [Weak/ProcessTemplate] weigh, then a single high-energy ball-milling step (which performs mixing and grinding together, unlike the separate Mix/Grind steps of the conventional solid-state template) is the fixed opening sequence of the mechanochemical route template
- [Moderate/StoichiometricBalance] balanced reaction releases a byproduct beyond the target; ball milling alone is not reliably sufficient to complete such a reaction at room temperature, so a post-milling anneal is included -- the cited review reports specific byproduct-releasing compounds (e.g. gamma-Al2O3, ZrO2) that formed only after heating the as-milled powder

### Warnings

- [Caution] grinding duration, forming pressure, and (if present) heating temperature/duration/atmosphere/ramp are unresolved: gugen has no thermodynamic or literature evidence provider wired in yet (AGENTS.md §4.1)
- [Severe] no hazard or safety data source is wired in yet: safety_penalty carries no real safety information, and this is not a safety clearance (AGENTS.md §15 "unknown hazardを安全と扱わない")
```

2つのplanが出力されている点に注意してください。Phase 12以降、受理された
前駆体の組み合わせは適用可能な**すべての**route family（現在2つ）のもとで
それぞれplanが生成されます — gugenには特定targetに対してどちらが適切かを
判断するroute-suitability分類器がないため、両方とも常に提示され、独立に
ランク付けされます（AGENTS.md §13）。1つ目のplanの仮焼（Calcination）と
再粉砕のstepがあるのは、バランス済み反応式がCO2を放出しているためです
（Evidenceの2番目の項目を参照）。すべてのplanに同じtemplateが適用される
わけではなく、炭酸塩を経由しないルートにはこのstepは付きません — 2つ目の
mechanochemicalなplanの再粉砕後annealも同じ条件で付与されています。完全な
レポートには、plan単位のスコア内訳、confidence評価（1つに潰した数値では
なく5つの独立した副指標）、assumptions一覧、却下された単一前駆体候補すべて
が理由コード付きで含まれます。

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

設計判断の詳細：[`docs/`](docs/)。

## ライセンス

[Apache License, Version 2.0](LICENSE-APACHE) または
[MIT license](LICENSE-MIT) のいずれかを選択できます。

English version: [README.md](README.md)
