# SPEC-0455: owned pointer-like nullable when 迁入细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：迁移既有 owned nullable when 能力时 · **唯一真源**：本 Spec 的范围与验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-E3-0455` |
| 所属 Phase | Phase 4；只消费既有 Phase 2/3 产品 |
| 语言规范 | [Guide v0.47](../../guide/README.md)、[nullable flow/extraction](../../guide/09-nullability-errors.md#nullable-flow-与-extraction) |
| 批准依据 | 用户确认 Goal `mwg35xu4-pl3zpc`：先闭合 0454 候选，再迁移已有 owned pointer-like nullable when，不自动合并 |
| 前置 Spec | 无代码依赖；0454 候选验收已闭合，但未合并，其改动不混入本分支 |
| 前置 ADR | [0017 nullable handle](../../adr/accepted/0017-nullable-handle-ssa-abi.md)、[0032 refinement](../../adr/accepted/0032-ownership-refinement-layer.md) |
| 关联 ADR | [0006 block parameters](../../adr/accepted/0006-typed-ssa-block-parameters.md)、[0016 Borrow ABI](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 分支 / 基线 | `feature/spec-0455`，从最新 main `51a3aad36bd754c268a05c26c8dd0b80940286cb` 新建 |
| 阻塞项 | 无已确认语义缺口；若需要新事实、ABI、权限或未合代码则停止询问 |
| 影响范围 | `lang-codegen::ssa::refine`、直接 Unit/native 消费者及当前事实文档 |
| 语言语义变更 | 否 |

## 1. Goal

使冻结矩阵内原已成功的 owned whole-root / temporary pointer-like nullable `when`，
经正常 Unit 入口进入 refine，保持单次求值、真实 proof/view、唯一 take 和清理/调用交接，
以独立实现、测试、审查与通过必需 CI 的 Draft PR 验收；不自动合并或归档。

## 2. 背景与输入身份

main 的 `unit_lower/nullable_when.rs` 及其 source/proof 职责仍实际生成函数体。
`refine/subset/named_route.rs::subject_when` 接受 scalar/enum/string，但尚未消费 nullable
When descriptor；这不是此前拒绝输入的新能力授权，也不允许移除全局触发门。

原有成功真源为 `native_nullable_when_tests/sources.rs` 的三份独立完整普通文档与 stdout，
及同族 generation/replacement/sibling 的真实 Unit 产品、SSA 与 native 检查。
原 fixture、精确 span、golden 与 helper 职责保持；不从旧 CI-only mixed whole 截段造新见证。

## 3. 冻结范围与矩阵

### 3.1 目标函数与退出行为

三份原文档中的 `positive_functions` 共 **24 个函数 / 30 个 nullable When**：
Node 12/14，Box 6/8，Rc 6/8；正常 `entry` 与可达 helper 仍由原 Unit call/ABI 规划。
Box 的既有完整普通文档为 `Box<Cell>?`，Rc 为 `Rc<Int>?`，不把例名当作全 payload 已验收。

| 来源/行为 | 冻结见证 | 必须保留 |
|---|---|---|
| owned root repeated view / grouped reference / 最后 consume | 三族 `repeated`，每函数三个 When | 同一 root 的真实来源与非消费 view；consume 唯一接管，既有 scalar while 不扩大权限 |
| When 后 nullable 再使用 | 三族 `later` | 合流前不得提前析构，退出时仍按原 ASAP/liveness |
| 嵌套调用 pending Prefix/Outer 与提前 return | 三族 `earlyReturn` | 先求值实参的 owner/loan 清理、精确顺序，不进入应取消的后续调用 |
| loop 内 named nullable local + break/continue | 三族 `earlyBreak` / `earlyContinue` | loop header/exit 和已有权限不变，subject 只求值一次 |
| temporary subject + 普通合流 | 三族 `temporary` | factory 精确一次；临时 owner 到完整表达式边界；无源码 symbol 不伪造 root |
| temporary subject + return/break/continue | Node 三个 `temporary*` 退出函数 | pending operand、call frame、temporary 各按原退出清理 |
| 两臂均离开 | Node `doubleReturn` / `temporaryDoubleReturn` | 不造虚假 merge，不遗漏任一臂的清理 |
| take 参数后续 operand 提前离开 | Node `takenReturn` | 不提前提交 take；无到达的调用不取得 inner owner |

同片保留原 generation/replacement 正例：顺序两个 When 与可变 root 新 generation、替换后
真实 if reproof、当前 owner 的正确 cleanup。它们不是给旧 when proof 续期的权限。
原 sibling 四项还约束 heap nullable-if、borrowed Map 结果与 test-only ForcedPerFamily。
如既有成功路径不能完整保持，必须报告并询问，不能删源码或将失败改成成功拒绝。

### 3.2 产品与身份约束

- 只接同轮 `UnitNullableWhenDescriptor` / `UnitNullableWhenOwnershipPlan`：精确 control、
  subject、entry/body、source category、stable symbol、generation、nullable/inner 类型一致。
- 入口只使用可整体 Value-deliver 的 OwnedRoot 或 Temporary，且 inner 是现有 pointer-like
  class/Box/Rc；不将 Borrow/Inout、field/element 或 This loan 假装为 owner。
- Observe/Consume 以真实 `UnitNullableWhenUseFact` 的 Shared loan / Value delivery 为准，
  对齐 canonical reference 与最外 Group 实参；不按 expected type、名字或裸偏移猜测。
- `NullableBranch` 的唯一 non-null edge 发布同 owner 的 Shared view，take 消费该 active proof；
  join、赋值、退出与 loop 边界退役正确，不复用旧 generation 或错误 control 的 view。
- AST 仅提供原表达式/语句结构；不重新推导 nullable 域或 ownership，不造新前端产品。

## 4. 非目标

- 不新增 Guide 语义、前端事实、SSA operation、LLVM ABI、依赖或公开构造能力。
- 不扩 nullable place/loan branch ABI、inline nullable、receiver caller、E2/D10 receipt 权限。
- 原 out-of-slice fixture 的阶段/code/span 保持；不附带启用 MoveOnly When tail、任意 mixed
  alternatives、借用 root take、proof 跨失效边界或现行 Guide 延后的 Elvis/safe call 等能力。
- 不全局删除迁移预检，不退休共享 Unit/type/ABI 基础设施，不以文件数当 E3 终态。
- 不导入 #422/#423/#424 未合代码；若本片依赖其修复，给出具体证据后请求用户决定。

## 5. 验收标准

- [ ] R1：生产未改时，原成功能力与正常入口路由分开取证；新增目标路由期望真实 RED。
- [ ] R2：冻结函数正常入口进入 refine；完整 typed SSA / ownership / LLVM 验证通过。
- [ ] R3：原 view/take、单次 subject、generation、pending call、ASAP 与所有退出检查保持；
  三个完整普通文档实际 emit/link/run，完整 stdout 与原 golden 相同，保留真实 linker 输出合同。
- [ ] R4：原负例精确 stage/kind/code/span；nullable-if/Map/borrow/receiver 邻居和 forced 控制不扩张。
- [ ] R5：single/project、正逆输入、同源 SSA/LLVM 确定性及无关 sibling 路由/执行隔离；
  默认与 feature 的直接行为、共享 CFG/nullable/proof/receipt 消费者通过，CI-only 不本地伪造。
- [ ] R6：fmt、适用 workspace check、双 all-target Clippy -D warnings、docs/依赖/尺寸/空白；
  独立上下文审查具体问题，修复后重审；Architecture 与真实账本同步。
- [ ] R7：独立 Draft PR 的精确 head/base 必需双宿主、producer/consumer 与真实 CI-only 验收；
  合法过滤/复用单独归因，不自动合并或归档，不把 timeout/ignored/list 当作执行通过。

## 6. 技术方案与职责

先在完整 signature/body/drop/loop admission 中接入现有 nullable When descriptor，仍使用
最终家族触发条件，不绕开其他检查。复用已有 refine 条件分支、ROp/RTerm NullableBranch /
NullableTake、view retirement、一般 predecessor/merge、call operand 与退出清理机制。

When-specific control/generation provenance 留 nullable-control 职责；class/Box/Rc 共用同一
流程，不建类型×CFG 组合模块。以当前 view/normal-if 产品与 when-use 产品的真实区别设计
内部接线，不用假 if descriptor 或事后改写 SSA 补证。缺失事实 fail loud，不 fallback 掩盖。
新代码保持单一职责与 1000 行软上限；原测试 source/support 不搬家打包，不清理无关欠账。

## 7. 顺序实施与检查点

1. [ ] 冻结本合同/索引与原能力输入 → R1 基线、正反例与实际数量。
2. [ ] 新增正常入口路由 RED，再最小接线 → R1–R2；先基础 view/take，再按矩阵验证退出。
3. [ ] 完成 generation/cleanup/call 交接与同源/模式一致性 → R3–R5；失败先归因，不继续堆功能。
4. [ ] 独立审查、最小充分门禁与事实文档 → R6；源字节与诊断不弱化。
5. [ ] 普通推送独立 Draft，精确头 CI 及验收账本 → R7；Spec 保持 in-progress 到用户批准后续状态。

## 8. 提交计划

| 顺序 | 边界 | 提交 |
|---|---|---|
| 1 | 明确合同/库存/roadmap 与真实基线 | `docs(spec-0455): freeze owned nullable when migration` |
| 2 | 可构建的实现、路由与直接契约测试、当前事实 | `feat(codegen): refine owned nullable when (SPEC-0455)` |
| 3 | 具体审查/CI 修复与真实验收 | 按发现记录，不压缩历史失败 |

## 9. 未决问题

无已确认语言或 ABI 缺口；完整矩阵实现尚未证明。任何需新能力或未合依赖的发现按 Goal
阻塞规则报告，不以本合同的 in-progress 提前声称当前实现已具备。

## 10. 单一验收账本

本地 Cargo 顺序复用 `/tmp/koven-0453-target`；Rust 1.96.0 / LLVM21 / macOS AArch64。
原始日志留仓库外；输入为未修改生产的 main `51a3aad3`。默认不跑 frontend 全量。

| 验收 / 目标 | 实际结果 | 边界 |
|---|---|---|
| 原 nullable When 家族 12 项基线选择 | 首次 240s 编译超时，未开始测试；第二次 600s 总期限，编译 9m48s 后只观察到 9 passed，退出 124 | 两次均不是 suite pass；保留超时，未清 target/改 profile |
| 同输入剩余 3 项精确目标 | **3 passed / 0 failed / 0 ignored**，退出 0 | 包含三完整文档实际 native 及两个 sibling 契约；与中断前 9 项不合并冒称完整套件通过 |
| R1 路由 RED | 待执行 | 不把已有成功 lowering 当作已迁入 refine |
| R2–R5 两配置/原契约与新模式矩阵 | 待执行 | 完整 suite 与 feature 尚未接受 |
| R6 审查与门禁 | 待执行 | 结构检查不代替语义 |
| R7 精确候选 CI | 待执行 | 无 PR，无合并/归档授权 |
