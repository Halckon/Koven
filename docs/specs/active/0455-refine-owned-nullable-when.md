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
| take 参数后续 operand 提前离开 | Node `takenReturn` | 按原参数顺序先 take；已提取 inner 作为 pending Value，后续退出精确清理，仅成功调用提交时解除 obligation |

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

- [x] R1：生产未改时，原成功能力与正常入口路由分开取证；新增目标路由期望真实 RED。
- [x] R2：冻结函数正常入口进入 refine；完整 typed SSA / ownership / LLVM 验证通过。
- [x] R3：原 view/take、单次 subject、generation、pending call、ASAP 与所有退出检查保持；
  三个完整普通文档实际 emit/link/run，完整 stdout 与原 golden 相同，保留真实 linker 输出合同。
- [x] R4：原负例精确 stage/kind/code/span；nullable-if/Map/borrow/receiver 邻居和 forced 控制不扩张。
- [x] R5：single/project、正逆输入、同源 SSA/LLVM 确定性及无关 sibling 路由/执行隔离；
  默认与 feature 的直接行为、共享 CFG/nullable/proof/receipt 消费者通过，CI-only 不本地伪造。
- [x] R6：fmt、适用 workspace check、双 all-target Clippy -D warnings、docs/依赖/尺寸/空白；
  独立上下文审查具体问题，修复后重审；Architecture 与真实账本同步。
- [x] R7：独立 Draft PR 的精确 head/base 必需双宿主、producer/consumer 与真实 CI-only 验收；
  合法过滤/复用单独归因，不自动合并或归档，不把 timeout/ignored/list 当作执行通过。

## 6. 技术方案与职责

先在完整 signature/body/drop/loop admission 中接入现有 nullable When descriptor，仍使用
最终家族触发条件，不绕开其他检查。复用已有 refine 条件分支、ROp/RTerm NullableBranch /
NullableTake、view retirement、一般 predecessor/merge、call operand 与退出清理机制。

When-specific control/generation provenance 留 nullable-control 职责；class/Box/Rc 共用同一
流程，不建类型×CFG 组合模块。以当前 view/normal-if 产品与 when-use 产品的真实区别设计
内部接线，不用假 if descriptor 或事后改写 SSA 补证。缺失事实 fail loud，不 fallback 掩盖。
新代码保持单一职责与 1000 行软上限；三份原 source 逐字节抽为共用 fixture，
source/support 的语义、stdout、负例与 helper 职责不变，不作薄壳打包或清理无关欠账。

## 7. 顺序实施与检查点

1. [x] 冻结本合同/索引与原能力输入 → R1 基线、正反例与实际数量。
2. [x] 新增正常入口路由 RED，再最小接线 → R1–R2；先基础 view/take，再按矩阵验证退出。
3. [x] 完成 generation/cleanup/call 交接与同源/模式一致性 → R3–R5；失败先归因，不继续堆功能。
4. [x] 独立审查、最小充分门禁与事实文档 → R6；源字节与诊断不弱化。
5. [x] 普通推送独立 Draft，精确头 CI 及验收账本 → R7；Spec 保持 in-progress 到用户批准后续状态。

## 8. 提交计划

| 顺序 | 边界 | 提交 |
|---|---|---|
| 1 | 明确合同/库存/roadmap 与真实基线 | `docs(spec-0455): freeze owned nullable when migration` |
| 2 | 可构建的实现、路由与直接契约测试、当前事实 | `feat(codegen): refine owned nullable when (SPEC-0455)` |
| 3 | 具体审查/CI 修复与真实验收 | 按发现记录，不压缩历史失败 |

## 9. 未决问题

无已确认语言、ABI 或未合代码依赖。冻结矩阵已完成本地验证；R6 门禁及 R7 精确
代码候选官方 CI 已闭合。carrier-return 普通 producer 实验先被前端来源权限拒绝，不将其
计为 Phase 4 guard 执行覆盖，也不伪造标准来源补验。Spec 保持 in-progress，不自动归档。

## 10. 单一验收账本

本地 Cargo 顺序复用 `/tmp/koven-0453-target`；Rust 1.96.0 / LLVM21 / macOS AArch64。
原始日志留仓库外；基线为 main `51a3aad3`，最终本地输入的 codegen tree 为
`9b672292885f310a9d07841590e127c802fa0b71`；后续仅文档变化不冒称重跑 Rust。
默认不跑 frontend 全量。

| 验收 / 目标 | 实际结果 | 边界 |
|---|---|---|
| 原 nullable When 家族 12 项基线选择 | 首次 240s 编译超时，未开始测试；第二次 600s 总期限，编译 9m48s 后只观察到 9 passed，退出 124 | 两次均不是 suite pass；保留超时，未清 target/改 profile |
| 同输入剩余 3 项精确目标 | **3 passed / 0 failed / 0 ignored**，退出 0 | 包含三完整文档实际 native 及两个 sibling 契约；与中断前 9 项不合并冒称完整套件通过 |
| R1 正常 Unit 路由 RED | **0 passed / 1 failed / 0 ignored**，退出 101 | `ssa::refine_subset_tests::nullable_when::owned_pointer_nullable_when_views_and_takes_refine_through_the_normal_unit_entry`；class view 原成功 lowering、SSA/ownership/LLVM 均已通过，精确失败为必须 refine；fail-fast 后另五个 case 未执行；生产未改 |
| 初期原契约接线 | 原 12 项先 7/5，再 10/2；修复 control origin、receiver/loop 拒绝边界、携带列和 While 退出源位置 | 失败不覆盖；原断言与源码不削弱 |
| 首轮扩大默认消费者 | **782 passed / 1 failed / 0 ignored**，退出 101 | 原 exiting-call-prefix native 失败：take 全局删 binding，null 续行的 scope cleanup 找不到 owner；不是缺前端事实 |
| 路径局部 take 修复 | **21/0/0**，退出 0 | 保留 binding，复用 dead；原失败 native 完整 stdout `drop\\nafter\\nok\\n` 保持；不与历史 782 项拼为一次 suite pass |
| R2–R5 最终默认 / feature | **676/0/0** / **694/0/0**，均退出 0 | 最终同一 codegen tree；nullable、refine、lower_frontend、parent/child、prefix、loan/receiver receipt 直接消费者；非无关全量 |
| 正常入口及模式矩阵 | 三族九个 view/take/take-then-return case；完整原 entry 的 24 函数；三文档两布局；输入正逆与 String/Long sibling 精确 SSA/LLVM/routes | SSA/ownership/LLVM 及原三文档 native 保持，不复制原 source 或扩大 observation 可见性 |
| CI-only 库存 | **62 listed only，none executed**，退出 0 | 本地只 compile/list；实际 fault 执行留 R7 官方 CI |
| R6 独立审查 | core 首审及 guard、path-local take 两次复核；最终无具体剩余发现 | 首次“borrow 见证可越权”静态判断被实测否定并撤回：实际早在 signature `borrow` span 40..46 拒绝；恢复 guard 是防御保留旧合同 |
| 审查负例与实验边界 | 新正常 Unit borrow-return 负例保持 UnsupportedNode / 40..46；原 17 负例字节不变 | 初次误期望 When span 及 carrier L0164 实验均非产品 RED；后者未到 Phase 4，已移除无效 fixture |
| R6 Cargo 与静态门禁 | fmt、workspace check、默认/feature workspace all-target Clippy `-D warnings` 通过；依赖、尺寸、空白通过 | 2300 手写 Rust、36 个既有超限，无新增；docs 948 Markdown、尺寸 policy 47 测试通过 |
| R7 精确代码候选 | [Draft PR #427](https://github.com/Halckon/Koven/pull/427)，head `29ba77f3bc7d57e590421da7fac7c71bcda5c815`，base `28a3c29029ac7d2a29b1cad7a0473cbf17648461` | 分支创建后 main 经 #425 推进；未混入未合候选，未擅自合并 main 到本分支 |
| 官方合并候选身份 | `b57e99207e7cacb5c599ee8e7678b62331ee31f4`，父提交依次为上述 base/head | 合并候选 codegen tree 与本地 `9b672292885f310a9d07841590e127c802fa0b71` 完全相同；CI 包含新 main frontend 修复 |
| R7 PR workflow | [43595895147](https://github.com/Halckon/Koven/actions/runs/43595895147) **completed success** | 双宿主 Check/Clippy/Targeted Tests、两个 Preview producer 及独立 consumer 实际成功；Editor lexical 按过滤 skipped，不计执行 |
| 官方 Rust 执行 | frontend lib 两宿主各 **848/0/12 ignored**；codegen Linux **3221/0/55 ignored**、macOS **3218/0/56 ignored** | ignored 不计通过；CI 实际覆盖与本地定向选择分开归因 |
| 官方 CI-only 与其他协议 | 两宿主各 **62 fault 全部且恰好执行一次**；独立 parent/child receipt、generated resource 步骤成功 | conformance gate 无 regression，137/154 cap probes，23 known-unimplemented、programs 0/6，不宣称全语言符合；Linux sanitizer 12 模式分类验证成功 |
| 独立 package 消费者 | 对应 producer artifact ID 的安装/运行与管理验证成功 | 损坏 checksum 拒绝、current 保持是预期负控；不将 mismatch 文本误报为安装失败 |
| Push 与观察器 | [43595887819](https://github.com/Halckon/Koven/actions/runs/43595887819) success；Rust/Preview skipped | 不代替 PR 运行；两次有限低频观察器 deadline 124 只代表观察停止，随后权威 run JSON 为 completed success |
| 文档跟进与状态 | 本次仅更新账本，不改变已验收 codegen tree；最新检查见 PR | Rust/Preview 执行归因于上述代码候选，文档过滤/合法复用不得冒称重跑；保持 Draft、in-progress，不合并或归档 |
