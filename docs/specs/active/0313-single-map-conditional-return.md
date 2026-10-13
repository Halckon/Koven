# SPEC-0313: single Map owner 条件 return 清理

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或验收 single Map 条件 return 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0313` |
| 所属 Phase | Phase 3 事实消费、Phase 4 single SSA/native |
| 语言规范 | [Guide12](../../guide/12-collections-destructuring.md) |
| 批准依据 | 2026-10-13 用户持续里程碑实施，并要求审核/CI 等待时推进独立工作 |
| 前置 Spec | [SPEC-0311](../../archive/specs/0311-single-map-conditional-continue.md) |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md) |
| 阻塞项 | 红测已冻结；最小实现、定向验收、独立审阅与远端交付待完成 |
| 影响范围 | single 外部具名非 Resource Map 的一个条件 return；temporary/Borrow Sequential source |
| 语言语义变更 | 否 |

## 1. 输入与边界

从真实 main `8e0ce49daea9c17960669aea6d467342508021f6`（tree
`68e6fc8b44930c709a9dac5faf5da6b09790c25a`）建立 `feature/spec-0313`。
本地库存/refs、远端 branch search 与 main 库存均未占用 0313。
0312 尚未合入此基线，不复制其 source proof；等真实合入后正常 merge 与定向回归。
本片不修改 Guide、frontend facts、SSA model/verifier、LLVM、unit 生产代码。

一个条件 Return、真实互补 Fallthrough、唯一 Exhaustion。Return 是完整终止出口，
没有 Header/Exit phi incoming；Header 恰 Entry/Fallthrough。初始 Map 只接受原 constructor
或 own 参数 seed；同型 Map 按 symbol/owner/slot 身份分别证明。
返回Unit/Copyable值，并自然复用独立body-local String/MoveOnly value-class的已有返回交付，
含透明Group；不改transfer/ABI。当前Map直返含Group两格基线原成功，不算新增drop-carrier
证明；active source/借用MoveOnly element保留L0135/L0133。owned nullable remove返回及
在return operand里更新Map的更宽形状仍明确拒绝，不由本片顺手扩大。
不扩多 return、混合 break/continue、无 fallthrough、capture、替换 seed、跨 loop phi、
RangeView 或一般 Resource/presence/ABI。

## 2. 证明与验收

静态 Drop 集合精确封闭：return Header presence∧return path；正常耗尽保留 live Exit 或
消耗 dead Header。返回 operand 先 lower，再保存当前 Map binding；按 frontend 完整动作
顺序清理，记录精确 DropFact/block/value，并核实际终结 Return block。return 无回边、
无 Exit 合流；当前值必须来自本轮 MapPut/MapRemove 运输且消费一次，非 seed/同型其他 owner。
正常 exhaustion/merge 和 Fallthrough owner 校验不放宽。

| 验收项 | 精确目标/过滤器 | 实际结果 |
|---|---|---|
| tests-only 基线 | `9ae5d86b`；`cargo test --offline --locked -p lang-codegen --lib 'loop_owner_tests::conditional_return::' -- --nocapture` | 6命中，single Map live/dead 2红：UnsupportedNode，Span23..28；unit basic/constant × Map live/dead 4绿，verified LLVM成功 |
| 前端真实事实 | 同上 ordinary source | 无诊断；3 exits/3 incoming，Return无incoming |
| 扩展tests-only基线 | `15776b1c`，`loop_owner_tests::conditional_return::source_matrix::` | 与最终fixture字节相同的5矩阵全红，准确Map名UnsupportedNode；生产仍与actual-main相同 |
| 0/早/晚/never、put/remove、双Map/own seed/三provider、两body Resource、独立owned结果 | `::conditional_return::`（SSA/LLVM及原版native） | `365911d2` 25项全绿；后续精确String loan oracle新增后待重跑，不能复用旧结果覆盖新断言 |
| owned source/返回值独立对照 | tests-only `82e0bd02`、current普通controls | owned source local/own×live/dead无Map四格全旧拒绝；Map组合current仍在keys精确Span拒绝。无Map String/value×Group四格与直接Map返回两格原成功；Map+独立owned四格转绿，removed两格仍红 |
| 前端非法source/element | ordinary source controls | active source return与mutable source变异L0135，MoveOnly element return L0133；未进入SSA |
| fmt/clippy/docs/diff、共享调用方回归 | 待最小实现后执行 | 未运行 |
| 独立审阅、PR双宿主CI、归档合并/actual-main | 远端唯一publisher交付 | 未运行 |

首轮过滤器 `conditional_return` 额外命中一个现有普通unit native源码测试（成功，未插桩）；
随后以上完整模块路径重跑隔离6格。原始日志/源码恢复材料在仓库外保留，不以stdout代替
唯一owner、loan与终结CFG证明。云禁止故障注入、计数插桩、sanitizer、mock和全量discover。

## 3. 保留的修正与限制

首批cleanup fixture把外围Resource也混入；它在有/无Map两格均旧UnsupportedNode，已保存
独立boundary源码。return operand更新Map的nullable控制表达式仍在cache Span拒绝。
核心cleanup只去掉这两个独立边界，保留两个body Resource、String source与Copyable结果。
owned结果native最初用temporary/具名MoveOnly字段读取，分别遇到既有BlockingDeferred/
UnsupportedNode，改为已支持的消费式解构读取返回payload；不修改字段lowering。
编译过程E0308、E0433、E0425与上述失败日志均保留；一个不完整exact过滤器零命中不计通过，
随后正确模块过滤实际5红。初版未冻结运行只作探索，不计最终验收。
返回proof支持cleanup block变化的静态闭合检查；当前成功fixture的cleanup是直线SSA，
不宣称有多块cleanup实例的动态覆盖。尺寸门禁保留38项历史债；driver仅增2行接线，旧上限不变。
