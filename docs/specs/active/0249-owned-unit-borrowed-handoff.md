# SPEC-0249：普通 owned-unit 封闭借用交接

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施或验收普通 compilation-unit 的 frontend→codegen 交接时 · **唯一真源**：本 Spec 的有界合同与验收账本

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-249` |
| 所属 Phase | Phase 3→4 交接与 Phase 6 CLI 直接消费者；治理 P3a |
| 语言规范 | [现行 v0.40](../../guide/README.md) |
| 批准依据 | 2026-10-02 已批准[整体治理计划 §7](../../development/engineering-governance-plan.md#p3-封闭交接与共享编排-l-分两片)及本片实施授权 |
| 基线 / 分支 | main `7b3ac11fe1770339f2c5170981839477dae8cbf5` / `feature/spec-0249` |
| 前置 Spec | SPEC-0197、0198、0199 `done` |
| 前置 ADR | [ADR-0020](../../adr/accepted/0020-multifile-compilation-unit.md) `accepted` |
| 阻塞项 | 设计无未决语义；实现与验收状态逐项见 §6，不预先宣称通过 |
| 影响范围 | frontend 普通 owned-unit 工厂与 provenance helper、codegen native/lower/planner 转接、CLI ordinary 分支、契约测试及stage选集、当前架构文档 |
| 语言语义变更 | 否；不改变 ABI、const 能力、Guide 或长期架构决定，不新建 ADR |

## 1. Goal

普通 compilation-unit 的六项 frontend 输入，经一次完整身份校验形成不可伪造的只读借用
view；后端和 CLI 普通路径消费此证明，不重复构建交接 index，同时保留旧入口的签名、
可见性、错误顺序、输出原子性与已有 facts/SSA/native/资源行为。

[PR29](https://github.com/Halckon/Koven/pull/29)已合并到本片固定基底；其
[八项合同基线](../../development/unit-handoff-contract-baseline.md)及既有24条 exact、
7项 basic/const compile-fail 是回归起点，不是本片新 API 或新 head 的通过证据。

## 2. 封闭工厂与身份合同

frontend 的私有 `ownership_checking/handoff.rs` 定义并由 ownership 门面重导出：

```rust
pub struct OwnedCompilationUnitView<'view, 'parsed: 'view> { /* 私有字段 */ }
pub enum OwnedCompilationUnitViewError { MismatchedSource, MismatchedAnalysis }
pub fn owned_compilation_unit_view<'view, 'parsed: 'view>(
    sources: &'view SourceMap,
    inputs: &'view [SourceUnitInput<'parsed>],
    names: &'view ValidatedCompilationUnitNames,
    environment: &'view TypeEnvironment,
    typed: &'view ValidatedCompilationUnitTypes,
    owned: &'view ValidatedCompilationUnitOwnership,
) -> Result<OwnedCompilationUnitView<'view, 'parsed>, OwnedCompilationUnitViewError>;
```

- 六字段全部是私有不可变借用；`_environment` 保留环境借用，不提供 environment getter。
  五个只读 getter 为 `sources()`、`inputs()`、`names()`、`types()`、`ownership()`；后两者
  返回既有 raw facts 的只读引用。禁止 unchecked、公开 index constructor、mutable access、
  recovery 升级或 basic/const 转换；不为方便测试无关扩大 SourceMap API。
- `'parsed` 覆盖 ParsedFile 及 root/path；`'view` 覆盖临时 inputs slice 与阶段借用，
  `'parsed: 'view`。调用方拥有产物、在调用栈建立 view；不复制 ParsedFile、不引入 Arc
  万能上下文、不把 view 存回自引用 snapshot，也不引入 generation ID。
- 工厂严格顺序：`index_compilation_unit(sources, inputs)` 恰一次，失败映射
  `MismatchedSource`；rebuilt 与 names index 比较；signatures 校验 rebuilt/saved index、
  names/saved index、environment owner、names owners 长度及逐项身份；最后 owned 校验
  typed body-analysis owner。后三类失配为 `MismatchedAnalysis`。
- 新增唯一 frontend `pub(crate)` helper `CompilationUnitSignatures::is_compatible_with_index`；
  旧 public `is_compatible_with` 仍自己构建一次 index 后转交 helper。工厂经 signatures 调用，
  不改 typed bodies helper。成功后丢弃 rebuilt，下游读取 names 中已有 index，不缓存第二份。
- 合法 clone 保留身份；同 T0 重做 ownership 合法；fresh T1 与 O0 失配；完整新 names/typed/
  owned 匹配链仍合法。不得额外要求两次 ownership pass 的 analysis owner 相同。

## 3. 兼容转接与副作用顺序

- 新公开 `emit_native_owned_unit_object(&view, entry: impl Into<NativeUnitEntry>, output)`
  消费封闭 view。旧 `emit_native_unit_object` 八参签名不变，首先执行调用方 `entry.into()`
  恰一次，再 factory，最后转新入口；传具体 NativeUnitEntry 的 identity Into 不重复调用用户转换。
- 旧七参 `lower_scalar_unit_with_entry` 和 planner adapter 保留各自 `pub(crate)`/`pub(super)`
  可见性，分别 factory 一次。新 crate-private `lower_owned_unit_with_entry` 消费 view；
  ordinary native→view lower→原私有 `lower_unit_from_facts`→`plan_unit_instances_from_facts`，
  不回调旧校验 gate，不新增无消费者的 public SSA/planner API。
- `From<OwnedCompilationUnitViewError> for LoweringError` 保留旧内部错误与 `span == None`；
  native 复用原 `map_lowering_error`。对 NativeObjectError 的 From 供 CLI factory 继续走
  `ProjectBuildError::Codegen`，不复制错误文本或扩大私有后端 API。
- CLI 只迁 `project_build.rs` 的 ordinary 分支；既有 entry selection 在前，const fallback
  顺序、解析/阶段诊断分流不变。const 保留独立 capability/gate 并共享既有 raw-facts driver。
- 顺序保持 factory→entry shape→plan/lower/SSA verify→native entry plan→reserve→
  LLVM/layout/object emission→atomic commit。前半失败 reserve delta=0；LLVM/layout 不被
  描述为 reserve 前检查。`SiblingObject::reserve/commit/Drop` 算法不改，发布尾部逐句核对。
- N1 只证明既有 commit 失败清理，不当作真正 LLVM emission-failure 注入；H1/H2 是宿主
  邻层证据。若扩大/重构发布尾部，则补独立 emission-failure 证据后再验收。

## 4. 非目标与尺寸边界

不迁 const、单文件、LSP/session，不扩 unit-for、generic、语言能力、ABI、runtime 或 dependency；
不引入新 crate、长期缓存、公共可变 context、通用 trait 框架或新 ADR。0182 与整体 P2/P3b/P4/P5
不因本片完成而关闭；整体计划完成后的外部审计继续排队。

新 frontend handoff、type model/provenance、codegen unit_lower/handoff 独立小模块承担各自职责。
既有超限 model/unit_lower/unit_plan 通过搬出对应职责或删除重复 gate 净减少；不增长超限
ownership compilation_unit、typed bodies、native unit_tests 入口。生产、测试和 helper 适用
1000 PLOC 软上限；policy 只允许必要额度收紧，不重生成历史 baseline、不压行或机械拆场景。
新增 `owned_compilation_unit_view`、`owned_unit_view_compile_contracts` 两个frontend integration
target，分别承载运行时身份与外部rustc能力/借用合同；通过既有stage脚本纳入双宿主选集，
不因此扩为frontend全量。选择policy先证明未接线0命中失败，再验证两target各出现恰一次。

## 5. 实施与提交边界

1. 建立本 Spec 与 active inventory/索引/生成图，固定 PR29 基线及本片验收合同。
2. 先补新 factory/双路转接的失败证据，再实施最小工厂、消费者和领域测试；验证统一记入 §6。
3. 依据真实实现同步 ownership/pipeline/SSA 快照及治理执行账本；代码与文档分提交。
4. 本地适用验收和独立 review 后创建 Draft PR，核 exact-head 双宿主 CI；完成账本后按生命周期
   归档并更新 inventory/图。PR 最终 CI 留在 PR，不倒填历史或自动合并。

回退单位为本 Spec 差异，保留旧入口和其他变更；不得通过删断言、扩大 ignored 或更改 expected
规避失败。提交信息包含 `SPEC-0249`；当前 Spec 的 in-progress 不是实现完成证明。

## 6. 唯一验收账本

本表同时记录验收标准、目标与实际结果；未执行不得由静态审阅、配置存在或基底 CI 替代。
所有 Cargo 验收由本片统一串行调度；同一成功证据按相同源码/依赖/feature/toolchain复用，
不按条目重复跑。基线24条 F/S/N/C/H 的完整名与旧7项 doctest命令以
[合同基线](../../development/unit-handoff-contract-baseline.md#既有24条身份基线)为真源。

| ID / 验收项 | 目标、过滤器或精确 oracle | 实际结果 / 待测边界 |
|---|---|---|
| A1 工厂与只读边界 | `cargo test --locked --offline -p lang-frontend --test owned_compilation_unit_view`；六借用、一次index、两种错误分别断言；旧 compatibility 直接消费者 F1–F6 | 新API编译红E0432（`red-factory.log`），实现后同target 1 passed/0 failed/0 ignored/0 filtered；不是旧行为bug。F1–F4在index11/provenance2完整target通过；F5/F6在typed bodies模块5 passed/182 filtered通过 |
| A2 原八项双路保全 | `cargo test --locked --offline -p lang-codegen --lib native::unit_tests::handoff_contracts`；原八项完整身份不变，六维替换分别走旧native及factory→新native | 新API编译红E0432（`red-native.log`）后9 passed/0 failed/0 ignored/730 filtered，原8项身份＋Into；双路80失配。完整codegen739亦通过；补fresh链后同filter再9 passed/0 failed/0 ignored/730 filtered |
| A3 错误逐字与优先级 | 两路 `MismatchedAnalysis`/None；foreign sources与duplicate inputs为 `native object MismatchedAnalysis: frontend lowering failed with MismatchedSource`；changed-root/missing及其他身份错链为 `native object MismatchedAnalysis: frontend lowering failed with MismatchedAnalysis` | A2同次9项通过，逐case两路锁kind/span/Display与identity先于非法entry；不是复用基线kind/span推定 |
| A4 旧 native Into | 局部计数自定义Into，成功和身份失败均恰一次，失败仍先转换；保留旧签名、adapter可见性及顺序 | A2新增 `legacy_entry_conversion_occurs_once_before_identity_validation` 通过，成功/失配均恰一次 |
| A5 成功等价 | 合法clone分别/全部、重建/重排inputs、same-T0重做ownership；两路facts、verified SSA、entry、object bytes、stdout `handoff-界\n`/空stderr/exit0 | A2初版16组通过；补第9组合首次因fixture新name environment配旧type environment触发 `MismatchedNameEnvironment`，修正匹配pair后9组合×2路18组真实object/link/run通过，生产不改 |
| A6 隔离 reserve | 原 exact 子进程计数覆盖两路；失配及匹配非法entry各delta0，匹配成功各delta1，核新总数 | A2同次exact子进程通过：80失配delta0，12成功各delta1，非法entry delta0，终值12 |
| A7 封闭性 compile-fail | 外部literal/update；const typed与owned分别；recovery names/typed/owned分别；临时inputs与引用产物两类逃逸；indexed helper外部调用 | 首轮5 passed/2 failed为E0308诊断lifetime文本oracle失配；修正后完整target 7 passed/0 failed/0 ignored/0 filtered（含各类成功对照），未降低能力拒绝合同 |
| A8 能力与邻层回归 | 保留旧7项basic/const compile-fail；F7/F8、S1–S3、C1–C8、N1/N2、H1/H2；受影响native unit全部消费者 | 旧7项在frontend12/codegen4 doctests中全部通过；24条基线由对应完整targets、typed bodies5、codegen739与CLI66实际覆盖，未重复逐条启动exact；N1仅证明commit清理 |
| A9 资源 oracle | N3 exact：28 alloc/28 free及live-pointer身份；不改变原fixture、assertions或ignore | N3在完整codegen739中通过，保留28 alloc/28 free和live-pointer oracle |
| A10 动态 index | 同 fixture 扣除 setup：旧 native 目标4→1、旧 lower 2→1、factory 1、预建 view 后 lower/native 增量0 | 两 fixture 动态实测：旧 native 4→1、旧 lower 独立2→1、factory 1、预建 view native/lower 各0；入口/构造器双计数一致。native/factory 14 exact及独立lower 16 exact进程全成功；原libtest738/739仅filtered。原值见[测量页](../../development/owned-unit-handoff-measurement.md) |
| A11 受控性能 | 与基底相同fixture/profile/host/toolchain，配对记录原值、噪声、采样限制及退化调查 | 同期同协议104进程全成功（24 warmup/80 measured）；内部emit中位 small3.154→3.048ms、32文件28.683→28.598ms，配对差值样本范围跨0且setup噪声触发，不能宣称提速/回归/等价；完整原值与限制见[测量页](../../development/owned-unit-handoff-measurement.md) |
| A12 工程检查 | fmt；workspace all-targets check；frontend/codegen/CLI all-targets strict Clippy；frontend/codegen docs；完整codegen/CLI适用套件和CLI build | fmt/check/三crate strict Clippy通过；frontend9 targets共253、typed bodies5（182 filtered）、frontend docs12；完整codegen739＋docs4、CLI66（48＋3format＋9native＋6project）及CLI build通过，除注明filtered外全0 failed/ignored/filtered |
| A13 文档与尺寸治理 | `python3 scripts/check_docs.py`；`python3 -m unittest discover -s scripts/tests -v`；尺寸护栏固定真实base；`git diff --check`；architecture与实现一致 | Spec建立时docs470、policy96/96、diff通过；stage接线后policy97/97。三份架构按实现同步；尺寸682手写/48超限/0生成通过，model1435→1404、lower1297→1282、plan3061→3035，仅收紧三额度；metadata131 targets（129＋2）。archive仅生成图变更 |
| A14 交付 | 独立review、Draft PR exact-head双宿主逐项实际CI、Spec终态和归档路径/inventory一致 | stage已加两个新target，选择policy先红（未接线0命中）后绿；并未本地跑整个stage。生产、原始测量及仓内摘要/JSON独立窄核均无finding；Draft PR尚未发布，双宿主CI待执行，Linux不能替代macOS |

实现采用本Spec的三个私有职责模块；原typed bodies/const和reserve算法不改。新增From使
`unit_lower/cfg.rs`的一处collect需要显式 `LoweringError`，只消除类型推断歧义，不改变算法。
旧lower重导出保留原路径；非test构建对该单一兼容re-export使用有reason的
`cfg_attr(not(test), expect(unused_imports, ...))`，不全域关闭lint。
首轮 API 红、compile-contract oracle 失败和 fresh-chain fixture 配错环境的失败保留为真实历史，
不伪称生产行为 bug 或删除失败记录。fresh-chain 修正后重跑 native 9 项、fmt、workspace
all-targets check、codegen strict Clippy 全过，日志 `final-native-and-gates-fixed.log`；
此后其他源码、依赖、feature 与工具链未变的完整 suite 证据按同一合同复用。

动态计数与同期性能的固定 source tree、完整 counts/samples JSON、噪声、产物 hash 和
独立 lower 补测边界统一记录于[有界测量页](../../development/owned-unit-handoff-measurement.md)。
本地候选 commit 与远端发布 SHA 可能不同，必须核对 tree/源码映射；不把本地 SHA 冒充远端链接。

本地受支持宿主为x86_64 Linux/glibc；Cargo验收沿用Rust/Cargo1.96.0、LLVM/Clang21.1.8，
使用 `--locked --offline` 串行执行。选定frontend命令为 `cargo test -p lang-frontend --no-fail-fast`
加九个 `--test`：`compilation_unit_index`（11）、`multifile_constant_facts`（6）、
`multifile_constant_ownership`（20）、`multifile_ownership_checking`（72）、
`multifile_two_phase_borrows`（27）、`multifile_type_checking`（107）、
`multifile_type_signature_provenance`（2）、`owned_compilation_unit_view`（1）、
`owned_unit_view_compile_contracts`（7）。typed bodies使用既有 `--lib` 模块过滤器。
下游执行 `cargo test -p lang-codegen`、`cargo test -p lang-cli`、frontend `--doc`及CLI build；
详细日志为 `frontend-selected.log`、`downstream-complete.log`，工程检查见 `engineering-gates.log`。
本地没有运行整个stage、frontend全量、workspace全量tests或macOS；双宿主CI仍待精确head证明。

## 7. 未决问题

无阻断实施的语义或架构决定。生命周期推断、strict clippy/missing_docs、compile-fail错误原因、
两路细节与计数及性能噪声都是实施验收项，完成前保持上表待测，不以设计审阅代替执行。
