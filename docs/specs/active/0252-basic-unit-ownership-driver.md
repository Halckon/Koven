# SPEC-0252：unit 基础所有权共享推进

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施或评审 unit basic ownership 纯阶段推进时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P3-252` |
| 所属 Phase | Phase 2→3 基础所有权推进与 Phase 6 宿主编排；治理 P3b 有界切片 |
| 语言规范 | [现行 v0.40](../../guide/README.md) |
| 前置 Spec | SPEC-0249、0250、0251 `done` |
| 前置 ADR | [ADR-0020](../../adr/accepted/0020-multifile-compilation-unit.md)、[ADR-0021](../../adr/accepted/0021-lsp-explicit-source-set-protocol.md) `accepted` |
| 阻塞项 | 无；实施与新head验证进行中 |
| 语言语义变更 | 否 |

## 1. Goal、批准与范围

2026-10-02 按获批[治理计划 P3b](../../development/engineering-governance-plan.md#p3-封闭交接与共享编排-l-分两片)，
批准一个有界 PR 同时迁 CLI project 和 LSP unit，CLI 先、LSP 后。从 PR32 merge
`15dfb13a6d771f66fbcfa21ca8db2fed92e77868` 建立 `feature/spec-0252`；编号在 specs 与远端
heads 未占用。前置 SPEC-0249/0250/0251 已 done，ADR-0020/0021 accepted；现行 v0.40 不变。

只共享 typed basic validation→普通 ownership 的纯推进，并消除 LSP 为 validation 克隆
整份 typed 的需要。宿主保留策略和同轮标准环境配对，不新增整体 snapshot/context/trait/mode，
不改变 legacy/bootstrap、checker、const 生产链、0249 view、entry、诊断协议或能力支持。
不声称性能改善、完整 driver、P3b/P3 完成或开展整体计划后的外部审计。

## 2. API 与顺序合同

`analysis::analyze_basic_unit_ownership(sources, inputs, names, environment, typed)` 消费
`CompilationUnitTypes`，返回 `Result<BasicOwnershipOutcome, OwnershipCheckingError>`。
Outcome 字段私有，消费式 `into_result()` 返回
`Result<(ValidatedCompilationUnitTypes, CompilationUnitOwnership), Box<CompilationUnitTypes>>`。

- 严格先 `typed.validate()`；成功只调用原 basic checker，返回 validated typed 与 raw ownership
- 失败原 Box 原样交还；内层 Err 是 NotBasic 分流，不是内部错误，也不证明输入身份
- 不额外核验身份、不尝试 const、不 owned.validate、不聚合排序诊断、不选 entry/native
- NotBasic+foreign environment 仍 NotBasic；basic+foreign context 保持 checker 原错误
- Outcome 按值拥有 facts/Arc provenance，不借 inputs，可跨其借用期持有；不伪造逃逸限制
- 同 facts 不等于同 provenance；合法 clone/move 与同 typed 重做 ownership 保持，fresh typed
  与旧 owned 仍拒绝。原0249 view继续真实六借用，不新增 self-reference

CLI 在调用前保留全部非空 typed diagnostics gate；NotBasic 后原 const gate 不变。
ownership 诊断→IncompleteOwnership→entry→view/native 顺序不变。
LSP 先收集 typed diagnostics；成功以 `into_types()` 恢复原 raw typed，失败保存 `*boxed`。
ownership 有诊断或无诊断 deferred 都保存 Some(raw owned)，不 validate；内部失败整轮失败。
const 仅声明也阻止整个 unit 的 basic ownership，即使另一函数 use-after-move 也不报 L0131；
CLI const ownership 则保留此诊断。宿主 Error variant/Display/source 与 last-good 保持。

## 3. 实施和交付

先冻结旧主干宿主 oracle，再新 API 编译红→最小实现→CLI→LSP。新 Rust domain/helper
各≤1000 PLOC，超限文件不增长，不重生 baseline，不扩大例外。所有 Cargo 串行使用同一 target。

新增 runtime/compile targets 接现有双宿主 stage，selection policy 红→绿；保持既有 native
与宿主断言。独立 code review 后 Draft PR；首轮 exact-head 双宿主通过后有界归档，归档
窄 review 与新 head CI，再由维护者决定 merge并核 main CI。各阶段事实在下表追加，不倒填。

## 4. 唯一验收账本

| ID / 合同 | 目标与证据 | 实际结果 |
|---|---|---|
| A1 完整facts/身份 | `basic_unit_ownership`：旧validate/checker同源完整Eq、空unit、map子集、多root、Unicode/CRLF/输入置换、合法clone/recheck、fresh typed拒绝 | 新API E0432编译红后最小实现；完整target 6/6通过，完整Eq与独立provenance断言通过 |
| A2 recovery与顺序 | 同target：typed errors、unused/used const、raw诊断/deferred、NotBasic foreign环境/全部context、basic provenance原错误 | A1同次通过；foreign environment/new names/foreign source-input/changed-root/duplicate逐项保留MismatchedCompilationUnitTypes的Debug/Display/source；NotBasic外来上下文仍原facts/identity |
| A3 Rust能力边界 | `basic_unit_ownership_compile_contracts`：封闭字段、按值移动/输入期后持有正例、basic/const/recovery错类型精确compile-fail；原view合同复跑 | 完整4/4通过；每项negative配positive，E0451/E0616字段封闭、E0308错能力且仅预期错误；按值跨全fixture输入期合法；原view编译合同7/7 |
| A4 CLI完整协议 | `project_basic_ownership_cli` 与完整CLI/build；完整human/JSON/exit、typed gate、basic/const diagnostics/deferred早于entry、native/临时输出保全 | 干净main旧CLI build＋112次进程完整输出冻结；新oracle旧生产9/9，CLI迁移后18/18；最终完整CLI78/78（48 bin＋3format＋9native＋9原project＋9新oracle），build通过 |
| A5 LSP完整宿主 | 完整`lang-lsp`；冻结manual全链、全部publication字段/UTF16 Locations、const+move、deferred Some、prepare/drop/sendfailure/lastgood | 新3项先在旧生产完整40/40通过；迁移后完整40/40再次通过，含原37与新3；同SourceMap手工全链raw Eq、跨source const+move、deferred Some、全UTF16和所有publication字段及last-good通过 |
| A6 相邻合同 | snapshot/view/index/provenance、multifile types/ownership/const选集、frontend docs、native unit | 12个完整frontend targets247/247（含新10），docs12/12；native unit93/93、646 filtered；均0 failed/ignored；细分与命令见下 |
| A7 工程门禁 | fmt、workspace all-targets check、frontend/CLI/LSP严格Clippy、docs/全部Python policy/尺寸/diff | 全部通过；Clippy -D warnings；docs474、policy99/99；695手写/48历史超限/0生成物，无增长或例外变更；API52行、CLI575、LSP486，新测试均≤339 |
| A8 双宿主交付 | 两新targets未过滤各恰一次；Draft/归档exact-head双宿主逐名与9 jobs；merge/main另验 | selection policy零选择真红→15/15绿；双宿主配置接线已核，独立code review进行中，Draft与新head CI尚未开始，不复用基底CI冒充 |

本地环境 x86_64 Linux、Rust/Cargo1.96.0、LLVM/Clang21.1.8；Cargo 追加 `--locked --offline`。
frontend全量、workspace全量tests、性能/分配测量不在验收声明内；macOS由远端实跑另验。

## 5. P3b 退出条件与明确剩余

本片仅完成 project/unit_session 的名称前缀与 basic 推进共用。
后续一只有界单文件 Spec 同时覆盖 bootstrap 与 legacy analysis 纯门面，保留各宿主策略；
四宿主 parity、公开能力/身份合同及 exact-head 双宿主证据齐全才可结项 P3b。
批准计划中的 const owned 交接另列 P3a 剩余项，不以 P3b 结项推定整个 P3 完成。
不把单文件→unit 内核合并、basic/const合并或新整体snapshot加入 P3b 必做项。
0182、P2/P4/P5与整体计划后的外部审计不被本片自动改变。


## 6. 本地执行与失败历史

所有下列Cargo命令串行，现有共享target不clean、不另建副本。frontend完整target计数：
basic runtime6/compile4、snapshot7/compile4、view1/compile7、index11、type provenance2、
multifile types107/ownership72、constant facts6/ownership20，共247；每target0 filtered。

```sh
cargo test --locked --offline -p lang-frontend --no-fail-fast --test basic_unit_ownership --test basic_unit_ownership_compile_contracts --test unit_name_snapshot --test unit_name_snapshot_compile_contracts --test owned_compilation_unit_view --test owned_unit_view_compile_contracts --test compilation_unit_index --test multifile_type_signature_provenance --test multifile_type_checking --test multifile_ownership_checking --test multifile_constant_facts --test multifile_constant_ownership
cargo test --locked --offline -p lang-frontend --doc
cargo test --locked --offline -p lang-cli
cargo build --locked --offline -p lang-cli
cargo test --locked --offline -p lang-lsp
cargo test --locked --offline -p lang-codegen --lib native::unit_tests
cargo fmt --all -- --check
cargo check --locked --offline --workspace --all-targets
cargo clippy --locked --offline -p lang-frontend -p lang-cli -p lang-lsp --all-targets -- -D warnings
python3 scripts/check_docs.py
python3 -m unittest discover -s scripts/tests -v
python3 scripts/check_rust_sizes.py --base 15dfb13a6d771f66fbcfa21ca8db2fed92e77868
git diff --check
```

新API不存在时得到准确E0432后加最小实现，两新完整target首轮通过；不是既有生产bug。
selection policy在未接线时准确1!=0，添加两target后15项CI policy通过。旧CLI捕获初次未带
LLVM动态库路径而返回127，补齐既有activate环境后全部重跑；该启动失败不算语言行为证据。
文档首次缺Spec状态table、临时ownership架构增补超200行曾由docs门禁拒绝；补metadata并把
事实只放已有tooling架构后通过，不缩写旧合同或放宽门禁。没有失败fixture倒改生产语义。

生产提交：`671ac28` frontend、`a2bfa28` CLI、`408bc51` LSP；之前`450f184`建立有界合同。
独立review、Draft首CI、归档窄review/最终CI和merge/main闭环另行追加，不预称完成。
