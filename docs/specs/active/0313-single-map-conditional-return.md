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
返回 Unit/Copyable 值；返回当前 Map、active source、借用 MoveOnly element 不获新证明。
独立 owned 返回与 owned named source 返回先普通源码对照，保持原有能力或明确拒绝。
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
| 0/早/晚退出与尾部可达、最新 owner/双Map、provider/seed | 待选定普通源码/结构/native | 未运行 |
| owned source 与独立 owned 返回对照、非法源诊断 | 待选定普通源码 | 未运行 |
| fmt/clippy/docs/diff、共享调用方回归 | 待最小实现后执行 | 未运行 |
| 独立审阅、PR双宿主CI、归档合并/actual-main | 远端唯一publisher交付 | 未运行 |

首轮过滤器 `conditional_return` 额外命中一个现有普通unit native源码测试（成功，未插桩）；
随后以上完整模块路径重跑隔离6格。原始日志/源码恢复材料在仓库外保留，不以stdout代替
唯一owner、loan与终结CFG证明。云禁止故障注入、计数插桩、sanitizer、mock和全量discover。
