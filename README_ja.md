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

> **ステータス：v0.7.0 公開済み**（検証済みの2段階合成route primitive、
> route接続性の検証、実コーパス駆動の検索正確性修正。詳細はCHANGELOG.md
> 参照）。
> [crates.io](https://crates.io/crates/gugen) /
> [docs.rs](https://docs.rs/gugen) /
> [v0.7.0リリース](https://github.com/kent-tokyo/gugen/releases/tag/v0.7.0)。
> v0.7.0では`search_two_step_routes`/`SynthesisRoute`
> （前駆体→中間体→目標という化学量論的に連結したroute）を追加し、実
> コーパステストで発見された`search_precursor_sets`の偽陽性
> identity-reaction受理バグを修正し、手書きの中間体候補grammarのための
> 任意・明示的にexperimentalな`experimental_grammar`feature（デフォルト
> 無効）を追加しました。**これは多段階合成の精度が全般的に向上した
> という主張ではありません**。grammar featureがrecallを改善するという
> 主張でもなく（実測では、単純なfrequency priorを上回りませんでした）、
> いずれかのrouteが実験的に検証済みだという主張でもありません。詳細と
> 誠実な限界については[`CHANGELOG.md`](CHANGELOG.md)を参照してください。

## ブラウザで試す

実在文献に基づく無機合成例を
[gugen Playground](https://kent-tokyo.github.io/gugen/)
で試せます。

WebAssemblyによりブラウザ内だけで動作し、アカウント、
サーバー、外部API、データ送信はありません。採用された計画、
棄却候補、未解決条件を表示しますが、実験成功を予測するもの
ではありません。

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

### 多段階合成route

`search_two_step_routes` は `search_precursor_sets` を2回連結し、budget内
では1段階で到達できない目標に対して、化学量論的に連結したroute（前駆体→
中間体→目標）を組み立てます。これは精度向上の主張ではなく **primitive**
です。`intermediate_candidates` は常に呼び出し側が用意するもので（gugen
自身が提案・取得することはありません）、各routeは接続性のみを検証します
（各stageの反応物は、ベース前駆体または前のstageの生成物のいずれかで
説明できる）——実際の合成手順と一致するかどうかは検証しません。また
`Planner` がこれを自動的に呼び出すこともありません。実際の408行の文献
holdoutで測定したところ、1段階では到達不可能と確認された294件の目標
のうち、12件（4.08%）を新たに回収しました。詳細な方法論と誠実な限界
については
[`docs/phase31_pr2_two_step_arity_recall.md`](docs/phase31_pr2_two_step_arity_recall.md)
を参照してください。実行可能なソース全体は`search_two_step_routes`自身の
[rustdoc例](https://docs.rs/gugen/latest/gugen/fn.search_two_step_routes.html)
にあります。

任意・デフォルト無効の`experimental_grammar`featureは、化学量論のみから
中間体候補組成を提案する少数の手書き分解grammar
（`src/transformation_grammar.rs`、例：`MCO3 -> MO + CO2`）を追加します。
これは明示的にexperimentalです。同じholdoutで測定したところ、単純な
コーパスfrequency priorがすでに到達している範囲を超えて目標を回収する
ことはありませんでした（詳細は
[`docs/phase31_pr3_transformation_grammar_audit.md`](docs/phase31_pr3_transformation_grammar_audit.md)
を参照）。APIは`0.x`のどのリリースでも変更される可能性があり、
`Planner`には接続されていません。

gugen Playgroundは現在、多段階routeを可視化しません——単一段階のplanのみ
表示します。

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

### Commercial Precursor Catalog

デフォルトoffのoptional feature `commercial_catalog`：既存の`SynthesisPlan`
の前駆体を、呼び出し側が用意したcommercial offerカタログ（価格・純度・
包装サイズ・供給元）と突き合わせる、planning後の別stageです。planの
score・confidence・reaction・process stepsには一切触れません。厳密rational
arithmeticによるcanonicalかつscale-invariantな組成比でmatchします（`Fe2O3`
と`Fe4O6`は同一物質の異なるformula-unit scaleとしてmatchする一方、
hydrateとanhydrous、異なる化合物は引き続き別物として扱われ、代替品推論は
行いません）。CSV/JSON形式の
カタログimportはaccepted/rejectedの構造化load reportを返します。化学量論
に基づく必要量計算、純度補正後の購入質量、包装個数の丸め、通貨安全な
checked-arithmeticによるcost合計、購入組み合わせの全体に対するbounded
searchを備えます。CSV/JSONの完全なschema・matching policy・数量計算の
規則は[`docs/commercial_precursor_catalog.md`](docs/commercial_precursor_catalog.md)
を、実行可能な使用例は`examples/commercial_catalog_assessment.rs`
（`cargo run --example commercial_catalog_assessment --features
commercial_catalog`）を参照してください。実在のカタログデータはgugenに
同梱されません。gugenはcommercialデータを保証しません（価格・在庫状況は
提供された推定値であり、保証ではありません）。

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

`gugen`バイナリ自体が`serde`・`clap`両featureを必須とします（`Cargo.toml`の
`[[bin]]`）。`--features`を指定しない`cargo install gugen`はバイナリを
一切installしません（default featuresは`[]`のため、libraryのみinstallされ
ます）。CLIを得るには：

```
cargo install gugen --features serde,clap
```

もしくはチェックアウトから `cargo build --features serde,clap --bin gugen`。
サブコマンド（AGENTS.md §19）：

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

`commercial-plan`（商用調達カタログとの照合、価格・純度・納期での比較）は
さらに`commercial_catalog` featureを必要とします：

```
cargo install gugen --features serde,clap,commercial_catalog
```

```
gugen commercial-plan target.json --catalog precursors.json \
  --commercial-catalog offers.csv \
  [--commercial-catalog-column-map column_map.json] \
  [--ranking-policy balanced|cost-first|lead-time-first|purity-first|minimum-unresolved-data|pareto] \
  [--min-purity 0.99] [--max-lead-time-days 30] [--max-total-cost 50000 --currency USD] \
  [--format json|markdown|csv]
```

`commercial_catalog`を有効にしない場合、`commercial-plan`サブコマンド自体が
存在しません（実行時エラーではなく、そのfeatureなしでbuildされたバイナリ
には組み込まれません）。オプション全体は
`docs/commercial_precursor_catalog.md`を、実世界の非標準ヘッダCSVに対応する
`--commercial-catalog-column-map`の宣言的な列名マッピングについても同ドキュ
メントを参照してください。

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
