gugen 開発指示書
Explainable Materials Synthesis and Process Planning in Rust

あなたは新規Rustライブラリ gugen（具現） を開発してください。

gugenは、目標材料の組成・結晶構造・制約を入力として受け取り、

どの前駆体を使うか
反応式をどう組み立てるか
どのような工程系列が考えられるか
どの条件が既知・推定・未確定なのか
なぜその計画を候補として選んだのか
どの点に失敗リスクや不確実性があるのか

を、機械可読かつ説明可能な形で返す、材料合成・プロセス計画ライブラリです。

名称は確定済みです。変更提案は不要です。

依存対象の名称は必ず chematic と記述してください。schematicではありません。

1. プロダクトの位置づけ

エコシステム上の関係は次の通りです。

                       chematic-crystal
               periodic structure foundation
                             │
                ┌────────────┴────────────┐
                │                         │
             mikiwame                  gugen
     explainable diagnostics    synthesis/process planning
                │                         │
                └──────── optional ───────┘

役割を混同しないでください。

chematic-crystal
lattice
periodic structure
composition
CIF
PBC
neighbor search
symmetryなどの構造基盤
mikiwame
入力構造の異常
配位環境
distortion
occupancy
applicability
diagnostic findings
gugen
precursor selection
reaction balancing
route-family selection
process-step planning
conditionsの根拠付き提案
alternative plan generation
planning uncertainty
risksieve
将来的な選択的予測・棄却・リスク制御
veridict
planner更新前後の統計的評価

依存方向は次を守ってください。

gugen → chematic-crystal
gugen → mikiwame   optional feature only

次は禁止します。

chematic-crystal → gugen
mikiwame → gugen
gugen → renkin

gugenはrenkinの材料版ではありますが、分子逆合成のアルゴリズムや型をそのまま流用してはいけません。

2. gugenが答える問い

gugenが答える問いは次です。

この目標無機材料を作る候補として、どの前駆体、反応、工程系列、条件範囲が考えられ、その根拠と不確実性は何か？

v0.1では、主として次を扱います。

target composition / structure
        ↓
precursor candidates
        ↓
balanced reaction candidates
        ↓
solid-state process template
        ↓
mixing / grinding / pelletizing
        ↓
calcination / annealing
        ↓
atmosphere / temperature range / duration range
        ↓
ranked synthesis plans

ただし、次を保証してはいけません。

実験が成功する
目標相が得られる
単相になる
指定温度で反応が完了する
収率が高い
安全に実行できる
特許性がある
工業的に量産できる

gugenの出力は候補計画であり、実験SOPや成功保証ではありません。

3. v0.1の対象範囲
対象

v0.1では対象を次に限定してください。

無機結晶材料
bulk material
主としてsolid-state synthesis
明示的な目標組成
任意で目標結晶構造
既知の無機前駆体候補
混合、粉砕、成形、仮焼、焼成、アニール
空気、不活性、酸化性、還元性雰囲気の抽象表現
常圧を中心とする工程
複数の代替計画
rule-basedまたは外部データに基づく説明可能なranking
対象外

v0.1では次を実装してはいけません。

有機合成
分子逆合成
MOF/COF合成計画
ポリマー合成
薄膜、CVD、PVD、ALD
電析
高圧合成
水熱・ソルボサーマル
mechanochemical synthesisの詳細条件
molten-salt synthesis
自動実験装置の直接制御
DFTそのもの
molecular dynamics
kinetic simulation
反応速度定数予測
収率予測
成功確率予測
文献の自動スクレイピング
LLMによる根拠なしのrecipe生成
特許調査
市場性評価

これらは将来のroute-family plugin候補です。v0.1へ混ぜないでください。

4. 最重要原則
4.1 根拠のない温度や時間を生成しない

次のような出力は禁止です。

800°Cで12時間焼成してください

その値の出典・ルール・類似例・適用範囲がない場合は生成してはいけません。

代わりに、次のように表現します。

Calcination temperature:
  suggested range: 700–900 °C
  evidence:
    - route template prior
    - decomposition temperature constraint
    - user-provided precedent
  confidence: low
  limitations:
    - no target-specific experimental precedent

根拠がなければ、

temperature: unresolved

としてください。

空欄を、もっともらしい数値で埋めてはいけません。

4.2 成功確率とranking scoreを混同しない

v0.1のscoreは候補の順位付けにのみ使用します。

pub struct RankingScore(f64);

これを、

成功確率 82%

のように表示してはいけません。

確率として表示できるのは、外部検証とcalibrationが成立した後だけです。

4.3 thermodynamicsだけで合成可能性を断定しない

反応エネルギーが有利でも、次の理由で実験は失敗します。

kinetic barrier
competing phases
precursor passivation
gas transport
particle size
diffusion distance
atmosphere mismatch
volatilization
crucible reaction
metastability

したがって、

thermodynamically favorable

と、

experimentally likely to succeed

を分離してください。

4.4 新規性と実現可能性を混同しない

未知の組成、データベースにない構造、OODな材料は、合成可能とは限りません。

noveltyは必要なら補助情報として保持しますが、v0.1のplanning scoreへ直接混ぜないでください。

5. 並行開発への対応

chematic-crystalとmikiwameは並行開発中である可能性があります。

そのため、gugenの開発をこれらの完成待ちにしてはいけません。

chematic-crystalが利用可能な場合

以下を直接利用してください。

PeriodicStructure
Lattice
PeriodicSite
composition
structure provenance
まだ利用できない場合

最小のtrait境界を定義してください。

概念例：

pub trait TargetMaterialView {
    fn composition(&self) -> CompositionView<'_>;
    fn structure_metadata(&self) -> Option<StructureMetadataView<'_>>;
}

ただし、gugen内へ独自の巨大な結晶構造実装を作ってはいけません。

chematic-crystal利用可能後にadapterを差し替えられる構成にしてください。

mikiwame連携

mikiwame連携はoptional featureにします。

[features]
mikiwame = ["dep:mikiwame"]

連携例：

InvalidInputならplanningを停止
severe site overlapならplanningを停止
oxidation-state ambiguityを計画分岐へ反映
low applicabilityならconfidenceを下げる
structural anomalyをwarningとして保持

mikiwameの診断を、gugen内部で重複実装してはいけません。

6. 公開データモデル

単一の文章recipeを返してはいけません。

中心となる公開型を、概ね次のように設計してください。

pub struct SynthesisPlanningReport {
    pub schema_version: u32,
    pub target: TargetSummary,
    pub applicability: ApplicabilityAssessment,
    pub plans: Vec<SynthesisPlan>,
    pub rejected_candidates: Vec<RejectedCandidate>,
    pub unresolved: Vec<UnresolvedRequirement>,
    pub warnings: Vec<PlanningWarning>,
    pub provenance: PlanningProvenance,
}
TargetSpecification
pub struct TargetSpecification {
    pub composition: Composition,
    pub structure: Option<TargetStructure>,
    pub desired_phase: Option<PhaseRequirement>,
    pub constraints: PlanningConstraints,
}

structureがないformula-only planningを許可しても構いませんが、applicabilityを下げてください。

SynthesisPlan
pub struct SynthesisPlan {
    pub plan_id: PlanId,
    pub route_family: RouteFamily,
    pub precursors: Vec<PrecursorSelection>,
    pub balanced_reaction: Option<BalancedReaction>,
    pub steps: Vec<ProcessStep>,
    pub score: PlanScoreBreakdown,
    pub confidence: ConfidenceAssessment,
    pub applicability: ApplicabilityAssessment,
    pub evidence: Vec<PlanningEvidence>,
    pub warnings: Vec<PlanningWarning>,
    pub assumptions: Vec<PlanningAssumption>,
    pub unresolved: Vec<UnresolvedRequirement>,
}
RouteFamily

v0.1は次だけで十分です。

pub enum RouteFamily {
    ConventionalSolidState,
}

将来のvariantを先回りして大量追加しないでください。

ProcessStep
pub enum ProcessStep {
    Weigh {
        materials: Vec<MaterialAmount>,
    },
    Mix {
        method: MixingMethod,
    },
    Grind {
        method: GrindingMethod,
        duration: Option<DurationRange>,
    },
    Form {
        method: FormingMethod,
        pressure: Option<PressureRange>,
    },
    Heat {
        purpose: HeatingPurpose,
        temperature: Option<TemperatureRange>,
        duration: Option<DurationRange>,
        atmosphere: Option<Atmosphere>,
        ramp: Option<RampRateRange>,
    },
    Cool {
        mode: CoolingMode,
    },
    IntermediateCharacterization {
        method: CharacterizationMethod,
        purpose: String,
    },
}

工程の意味をenumで保持し、最終表示だけ文章化してください。

Conditions

条件は単一点よりrangeを優先します。

pub struct TemperatureRange {
    pub min_celsius: f64,
    pub max_celsius: f64,
}

pub struct DurationRange {
    pub min_hours: f64,
    pub max_hours: f64,
}

次を保証してください。

min ≤ max
finite
物理的に不正な値を拒否
unitを曖昧にしない
unknownをOptionで表現
NaNをpublic resultへ漏らさない
7. Evidenceモデル

すべての提案へ根拠を紐付けてください。

pub enum EvidenceKind {
    StoichiometricBalance,
    RuleBased,
    ThermodynamicData,
    UserProvidedPrecedent,
    CuratedLiteratureRecord,
    SimilarComposition,
    SimilarStructure,
    ProcessTemplate,
    SafetyConstraint,
}
pub struct PlanningEvidence {
    pub kind: EvidenceKind,
    pub source_id: Option<String>,
    pub statement: String,
    pub strength: EvidenceStrength,
    pub applicable_to: EvidenceScope,
    pub limitations: Vec<String>,
}
文献情報

DOI、論文名、特許番号、URLを創作してはいけません。

外部evidence providerが返した情報だけを利用します。

出典がない場合は、

source: none
evidence kind: heuristic

と明示します。

Provenance

最低限、次を保存してください。

gugen version
commit SHAまたはbuild identifier
schema version
chematic-crystal version
mikiwame version
precursor catalog version
thermodynamic provider version
process-template version
ranking-config digest
execution timestamp
deterministic seed
enabled features
8. Provider設計

外部データへ密結合しないでください。

最低限、次のprovider traitを検討してください。

pub trait PrecursorCatalog {
    fn candidates_for(
        &self,
        target: &Composition,
        constraints: &PlanningConstraints,
    ) -> Result<Vec<PrecursorCandidate>, ProviderError>;
}
pub trait ThermodynamicProvider {
    fn reaction_energy(
        &self,
        reaction: &BalancedReaction,
        conditions: &ThermodynamicConditions,
    ) -> Result<Option<ReactionEnergy>, ProviderError>;
}
pub trait ProcessEvidenceProvider {
    fn precedents(
        &self,
        target: &TargetSpecification,
        precursors: &[PrecursorSelection],
    ) -> Result<Vec<ProcessPrecedent>, ProviderError>;
}

v0.1では、

in-memory provider
JSON/JSONL provider
fixture provider

を優先します。

ネットワークアクセスはcore libraryに入れないでください。

オンラインMaterials Project等との接続は別adapterまたは将来crateにします。

9. 前駆体候補生成
原則

前駆体候補は、目標組成を構成する元素を含む既知化合物から生成します。

例：

target: A-B-O system

candidate precursors:
- A oxide
- A carbonate
- B oxide
- B carbonate
- mixed precursor

ただし、全組合せを無制限に列挙してはいけません。

フィルタ候補
target元素を被覆するか
禁止元素を含まないか
user constraintsに反しないか
前駆体数
gaseous byproduct数
redox compatibility
atmosphere compatibility
availability metadata
toxicity/hazard metadata
volatilization warning
targetへ残らない元素の除去可能性
stoichiometric balanceが存在するか
探索

v0.1では、決定的なbounded searchを採用してください。

候補：

combinations with explicit maximum precursor count
beam search
branch-and-bound
deterministic best-first search

探索budgetを設定可能にし、budget exhaustionをreportへ残してください。

pub struct SearchBudget {
    pub max_precursor_sets: usize,
    pub max_precursors_per_plan: usize,
    pub max_plans_returned: usize,
}

budget不足を「候補なし」と混同してはいけません。

10. 反応式バランス

反応式バランスはgugenの中心機能です。

要件
整数係数
元素保存
必要に応じて電荷保存
gcdで正規化
決定的
overflowを安全に処理
複数解がある場合に曖昧性を保持
ゼロ係数を除去
非物理的な負係数を拒否
target側係数の正規化規則を文書化

浮動小数点近似だけで反応式を決めてはいけません。

可能なら、有理数または整数行列を用いたnull-space計算を採用してください。

副生成物

v0.1では、許可する副生成物をcurated setとして管理します。

候補例：

CO₂
H₂O
O₂

ただし、curated setへ根拠なく化学種を追加してはいけません。

副生成物の仮定はreportへ明示してください。

11. Process Template

v0.1では、固相合成の抽象templateを実装します。

概念例：

1. precursorsを秤量
2. 混合
3. 粉砕
4. 任意で成形
5. 仮焼
6. 再粉砕
7. 本焼成
8. 冷却
9. XRD等による中間確認

ただし、すべての材料へ同じtemplateを適用してはいけません。

各stepは、

required
optional
unresolved

を区別してください。

pub enum StepRequirement {
    Required,
    Recommended,
    Optional,
    Unresolved,
}

条件が不明な工程を削除するのではなく、Unresolvedとして残します。

12. Atmosphereとredox

雰囲気は文字列だけで表現しないでください。

pub enum Atmosphere {
    Air,
    OxygenRich,
    Inert {
        gas: InertGas,
    },
    Reducing {
        agent: Option<ReducingAgent>,
    },
    Vacuum,
    Controlled {
        description: String,
    },
}

ただし、v0.1で精密なoxygen partial pressureを予測しないでください。

酸化状態に関する推論は、

formal oxidation-state model
atmosphere compatibility heuristic
target/precursor redox mismatch

として扱い、実際の相平衡を保証しないでください。

mikiwameが酸化状態の曖昧性を返した場合は、その曖昧性を保持してください。

13. Plan ranking

v0.1のrankingは説明可能な多項目評価にしてください。

pub struct PlanScoreBreakdown {
    pub stoichiometric_validity: Score01,
    pub precursor_coverage: Score01,
    pub thermodynamic_support: Option<Score01>,
    pub process_simplicity: Score01,
    pub evidence_strength: Score01,
    pub safety_penalty: Score01,
    pub uncertainty_penalty: Score01,
    pub total_ranking_score: Score01,
}

各項目のweightを設定可能にします。

pub struct RankingWeights {
    pub stoichiometric_validity: f64,
    pub precursor_coverage: f64,
    pub thermodynamic_support: f64,
    pub process_simplicity: f64,
    pub evidence_strength: f64,
    pub safety_penalty: f64,
    pub uncertainty_penalty: f64,
}

要件：

weightをprovenanceに保存
各寄与を出力
default weightの根拠を文書化
validation corpusを見て恣意的に調整しない
holdoutを見てweight変更しない
total scoreを成功確率と呼ばない
missing thermodynamic dataを自動的に失敗扱いしない
evidenceなしのplanはconfidenceを下げる
14. RejectedCandidate

採用した計画だけでなく、不採用理由も返してください。

pub struct RejectedCandidate {
    pub precursors: Vec<PrecursorId>,
    pub reason_codes: Vec<RejectionCode>,
    pub explanation: String,
}

候補となるreason code：

NO_STOICHIOMETRIC_BALANCE
MISSING_TARGET_ELEMENT
FORBIDDEN_ELEMENT_PRESENT
PRECURSOR_COUNT_EXCEEDED
UNSUPPORTED_BYPRODUCT_REQUIRED
ATMOSPHERE_CONFLICT
USER_CONSTRAINT_VIOLATION
HAZARD_POLICY_BLOCKED
THERMODYNAMIC_DATA_UNAVAILABLE
SEARCH_BUDGET_EXHAUSTED
DUPLICATE_PLAN

THERMODYNAMIC_DATA_UNAVAILABLEだけでplanを必ずrejectする必要はありません。設定に応じてwarningまたは低confidenceにしてください。

15. Safety設計

gugenは安全審査ツールではありませんが、危険情報を無視してはいけません。

必須
precursor hazard metadataを保持可能にする
toxic gas generation warning
volatile component warning
high-temperature warning
reducing/oxidizing atmosphere warning
pressureを伴う工程はv0.1対象外
unknown hazardを安全と扱わない
manual review requiredを明示
institutional safety reviewを代替しない
禁止
危険性の高い工程を安全と断定する
PPEや設備要件を推測だけで確定する
自動実験機器へ直接送信する
safety warningをranking向上のために消す
危険な前駆体を、安価という理由だけで上位にする

v0.1のJSON planには必ず、

pub manual_review_required: bool,

または同等の表現を含めてください。

16. ApplicabilityとConfidence

次を分離してください。

Applicability

このplannerが対象を扱えるか。

pub enum ApplicabilityLevel {
    InDomain,
    PartiallyInDomain,
    OutOfDomain,
}

例：

bulk inorganic solid-state：InDomain
formula-only：PartiallyInDomain
MOF：OutOfDomain
thin film：OutOfDomain
severe disorder：PartiallyInDomain
Confidence

個々のplanに対する根拠の強さ。

pub struct ConfidenceAssessment {
    pub overall: Score01,
    pub stoichiometry: Score01,
    pub precursor_selection: Score01,
    pub process_conditions: Score01,
    pub evidence_coverage: Score01,
}

条件未確定でも反応式が確実なケースがあります。単一confidenceに潰さないでください。

17. 推奨crate構成

最初から多数のcrateへ分割しないでください。

v0.1は単一crateを基本とします。

gugen/
├── Cargo.toml
├── README.md
├── README_ja.md
├── AGENTS.md
├── CHANGELOG.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── docs/
│   ├── architecture.md
│   ├── scientific_scope.md
│   ├── planning_contract.md
│   ├── evidence_model.md
│   ├── validation.md
│   ├── safety.md
│   ├── competitors.md
│   └── integration.md
├── src/
│   ├── lib.rs
│   ├── error.rs
│   ├── config.rs
│   ├── target.rs
│   ├── composition.rs
│   ├── precursor.rs
│   ├── reaction.rs
│   ├── balance.rs
│   ├── process.rs
│   ├── evidence.rs
│   ├── provider.rs
│   ├── planner.rs
│   ├── ranking.rs
│   ├── rejection.rs
│   ├── safety.rs
│   ├── report.rs
│   ├── provenance.rs
│   └── adapters/
│       ├── mod.rs
│       ├── chematic.rs
│       └── mikiwame.rs
├── src/bin/
│   └── gugen.rs
├── fixtures/
├── tests/
├── benches/
└── tasks/
    ├── todo.md
    └── lessons.md

規模が実際に必要になるまで、gugen-core、gugen-data、gugen-cliへ分割しないでください。

18. 公開API

概念的には次の利用形を目指してください。

use gugen::{
    Planner,
    PlanningConfig,
    TargetSpecification,
};

let planner = Planner::new(
    precursor_catalog,
    process_evidence_provider,
    thermodynamic_provider,
    PlanningConfig::default(),
);

let report = planner.plan(&target)?;

for plan in &report.plans {
    println!(
        "{}: score={}, confidence={}",
        plan.plan_id,
        plan.score.total_ranking_score,
        plan.confidence.overall
    );
}

providerがなくても最低限のstoichiometric planningを実行できる構成を検討してください。

let planner = Planner::offline_minimal(catalog, config);

ただし、providerなしで条件を創作してはいけません。

19. CLI

最低限、次を提供してください。

gugen plan target.json \
  --catalog precursors.json \
  --output report.json

gugen plan target.json \
  --format markdown

gugen balance reaction.json

gugen explain report.json \
  --plan plan-001

gugen validate-target target.json

gugen doctor

将来CIF adapterが利用可能になった場合：

gugen plan target.cif \
  --catalog precursors.json
doctor出力
gugen version
schema version
chematic-crystal version
mikiwame integration status
enabled route families
precursor catalog version
thermodynamic provider
process evidence provider
ranking config digest
deterministic mode
supported domain
known limitations
20. JSON schema

JSON schemaには必ずversionを持たせてください。

{
  "schema_version": 1,
  "target": {},
  "applicability": {},
  "plans": [],
  "rejected_candidates": [],
  "unresolved": [],
  "warnings": [],
  "provenance": {}
}

要件：

round-trip可能
unknown field方針を文書化
enum表現を安定させる
public resultへNaN/Infinityを出さない
plan IDを決定的にする
schema変更時のcompatibility方針を記載
README例は実際の出力から生成する
21. テスト戦略
21.1 Reaction balancing

最低限：

単純な1対1反応
carbonateからoxide材料＋CO₂
酸素を副生成物または反応物として含む反応
複数前駆体
解なし
複数解
gcd正規化
元素保存
permutation invariance
大きい係数
overflow処理
21.2 Precursor generation
target元素をすべて被覆
不要元素を含む候補の除外
最大前駆体数
forbidden precursor
duplicate elimination
deterministic ordering
search budget exhaustion
availability metadata欠損
21.3 Planning

検証用候補として、出典を確認した上で代表的な無機材料を採用してください。

候補例：

perovskite oxide
spinel oxide
phosphate
simple binary oxide
carbonate precursor route

具体的なfixture反応は、信頼できる文献またはcurated datasetで確認してから採用してください。記憶だけで作成してはいけません。

21.4 Metamorphic tests

結果は原則として次に不変であるべきです。

target元素順序
precursor catalog順序
equivalent formula normalization
provider返却順序
unrelated precursor追加
JSON field order

変化してよい項目は文書化してください。

21.5 Provider failure
provider timeout相当
missing entry
malformed record
partial thermodynamic coverage
duplicated evidence
inconsistent units
unavailable provider

一つのprovider失敗で、可能なplanning全体を必ず失敗させないでください。

21.6 Snapshot tests

MarkdownおよびJSON出力は、schemaが意図せず変わらないようsnapshotまたはgolden testを用意してください。

22. 検証dataset

ライセンスが明確なデータだけを使用してください。

データを次に分離します。

development
validation
holdout
adversarial
out-of-domain
評価指標

単一のaccuracyではなく、次を個別に測定します。

valid reaction generation rate
element-balance exactness
known precursor-set top-k recovery
exact precursor match
partial precursor match
route-family coverage
process-step coverage
condition evidence coverage
unresolved condition rate
false confident plan rate
rejected-candidate reason correctness
deterministic reproducibility
planning throughput
search-budget exhaustion rate
out-of-domain abstention rate

温度評価を行う場合、単純なMAEだけでなく、

predicted rangeがreferenceを含む率
evidence付き条件のcoverage
unsupported exact-value generation rate

を測定してください。

23. Differential validation

可能なら既存研究コードやPython実装と比較します。

比較対象候補：

inorganic synthesis pathway planning code
phase-diagram-based precursor selection
curated solid-state synthesis datasets
reaction balancing implementations

ただし、他実装の出力を盲目的な正解とみなさないでください。

不一致を次に分類します。

definition difference
catalog difference
provider data difference
valid alternative plan
gugen bug
reference implementation bug
insufficient evidence

比較用Pythonコードを本番依存へ含めないでください。scripts/またはbenchmarks/へ隔離します。

24. 競合との差別化

gugenの価値は、単にprecursor候補を一つ返すことではありません。

差別化は次です。

pure Rust中心
reaction balanceが厳密
複数の代替計画
process stepsが機械可読
evidenceとassumptionを分離
confidenceとapplicabilityを分離
unresolved条件を隠さない
rejected candidateの理由を返す
deterministic
provider交換可能
chematic-crystalと連携
mikiwameの診断をplanningへ安全に引き継ぐ
失敗しそうな理由を説明する
実験成功を過剰主張しない

「Rust版の既存Python planner」をそのまま作るのではなく、planning contractと説明可能性を製品の中心にしてください。

25. Rust品質要件
Rust 2024 edition
MSRVは依存するchematic ecosystemと整合
#![forbid(unsafe_code)]
typed error
deterministic by default
network accessなし
panicを通常入力の処理に使用しない
public outputへNaNを漏らさない
integer overflowを処理
optional serde
coreは可能な限りWASM互換
thread数で結果が変わらない
random処理を使うならseed必須
public itemを文書化
examplesをcompile test
dependency licenseを確認
巨大依存を安易に導入しない

最低品質ゲート：

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
cargo audit

可能なら：

cargo check --target wasm32-unknown-unknown
cargo test --doc
cargo deny check
26. 実装フェーズ
Phase 0 — Landscape and Architecture

コードを書く前に次を行ってください。

existing inorganic synthesis planners調査
precursor-selection研究の整理
phase-diagram planningとの境界整理
solid-state synthesis dataset調査
chematic-crystal API確認
mikiwame API確認
crates.io/GitHub上の名称衝突確認
データライセンス確認
scientific scope確定
report schema設計
provider境界設計
reaction balancing方式決定

成果物：

docs/architecture.md
docs/scientific_scope.md
docs/competitors.md
docs/evidence_model.md
docs/integration.md
tasks/todo.md

名称衝突が見つかっても、勝手に製品名を変更しないでください。package名の代替案と影響をstop-and-reportしてください。

Phase 1 — Foundation
crate初期化
config
typed errors
validated numeric types
composition
target specification
report schema
provenance
provider traits
JSON round-trip
CI

診断・planningロジックを増やしすぎないでください。

Phase 2 — Reaction Balancing
exact composition representation
integer/rational balancing
byproduct model
multiple solutions
normalization
exhaustive tests
CLI balance

ここを独立commitにしてください。

Phase 3 — Precursor Enumeration
in-memory precursor catalog
candidate generation
constraints
bounded search
rejection reasons
deterministic ordering
budget diagnostics
Phase 4 — Solid-State Process Template
route family
process steps
unresolved conditions
atmosphere abstraction
basic evidence
warnings
Phase 5 — Ranking and Explanation
score breakdown
confidence
applicability
evidence strength
assumptions
alternatives
rejected candidate explanations
Phase 6 — Integration
chematic-crystal adapter
optional mikiwame adapter
invalid target handling
composition/structure handoff
feature-gated builds
Phase 7 — CLI and Batch
gugen plan
gugen balance
gugen explain
gugen validate-target
gugen doctor
gugen batch

batchでは一件の失敗で全体を失敗させないでください。

Phase 8 — Validation
curated fixtures
known-route recovery
adversarial examples
false-confidence audit
provider failure tests
reproducibility
benchmark report
limitations更新
Phase 9 — v0.1 Release Preparation
README実例を実出力と同期
README_ja
changelog
docs.rs
package内容確認
dependency/license audit
schema audit
semver audit
release checklist

所有者の明示的許可なくpublishしないでください。

27. 自律開発ルール
独立branchまたはworktreeで作業する
親エージェントのworking treeを変更しない
小さなfocused commitを作る
unrelated refactorを混ぜない
既存failureと今回failureを切り分ける
調査可能な問題で直ちに質問しない
承認待ち作業は後回しにし、独立作業を継続する
科学的根拠がないheuristicを追加しない
fixtureだけ通す特殊処理を入れない
benchmarkを見てholdoutへ過適合しない
READMEへ未検証の精度を記載しない
CIがgreenでも勝手にmergeしない
publishしない
draft PRとして提出する
28. Stop-and-report条件

次の場合は勝手に範囲を拡張せず、報告してください。

gugen package名が利用不能
chematic-crystal APIが未確定で重大な密結合が必要
mikiwame APIが未確定で直接依存が危険
使用候補datasetのライセンスが不明
precursor catalogの再配布条件が不明
thermodynamic dataの利用条件が不明
exact reaction balancingに大きな外部依存が必要
unsafeまたはC/C++ FFIが必要
public schemaの破壊的変更が必要
temperature/atmosphere条件に妥当な根拠を持てない
validation corpusでfalse confident plansが多い
plannerが特定fixtureだけに過適合している
solid-state synthesis以外を実装しないとv0.1が成立しない
自動実験装置との直接接続が必要
危険性の高いrouteを安全に扱えない
version bump、merge、publishが必要

報告形式：

判明した事実
なぜ問題か
最小解決案
代替案
推奨案
作業量
影響範囲
安全に継続できる作業
29. v0.1完了条件

以下を満たした時点でv0.1候補とします。

Rust libraryとして公開APIがある
target compositionを受け取れる
precursor catalogから候補生成できる
reaction equationを厳密にbalanceできる
複数候補planを生成できる
conventional solid-state routeを表現できる
process stepsが機械可読
unknown conditionsをunresolvedとして保持できる
evidenceとassumptionを分離できる
ranking breakdownを返せる
scoreを成功確率と表現していない
confidenceとapplicabilityが分離されている
rejected candidateの理由を返せる
safety warningがある
manual review requirementがある
providerが交換可能
JSON schema versionがある
provenanceがある
deterministic
batch APIとCLIがある
known-route validationがある
false-confidence auditがある
out-of-domain inputを棄却できる
chematic-crystal連携境界がある
mikiwame連携がoptional
README例が実出力と一致
fmt/clippy/test/doc/auditが通る
draft PRがある
merge/publishしていない
working treeがclean
30. 最終報告形式
実装内容
crate構成
公開型
provider
reaction balancing
precursor search
process template
CLI
科学的範囲
対象route family
対象材料
何を予測しているか
何を予測していないか
Planning contract
evidence
assumptions
unresolved
confidence
applicability
rejection reasons
検証結果
fixture数
reaction-balance exactness
precursor top-k recovery
plan generation rate
false-confidence audit
out-of-domain abstention
runtime
deterministic再現性
chematic/mikiwame連携
使用したAPI
adapter
feature flag
未解決点
将来の移行計画
安全性
hazard metadata
warnings
manual review
対象外工程
品質確認

実行したコマンドとpass/fail。

Git状態
branch
commits
draft PR
CI
working tree
次の推奨作業

最大3件に限定してください。

優先順位は原則として、

外部recipe corpusによる検証
false-confidence削減
thermodynamic/provider coverage拡大

とし、単純な機能数の増加を優先しないでください。

最重要原則

gugenの目的は、AIがもっともらしい合成レシピを書くことではありません。

目標材料を現実の実験候補へ落とし込み、何が分かっていて、何が仮定で、何がまだ分からないのかを保ったまま、検証可能な計画として具現化すること

この原則を、機能数、見栄え、デモの派手さより優先してください。