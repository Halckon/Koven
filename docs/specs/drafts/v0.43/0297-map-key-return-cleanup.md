# SPEC-0297: Map 查询与删除的 key return 清理

> **性质**：候选变更合同 · **状态**：draft · **读取时机**：准备 Map key 提前返回修复时 · **唯一真源**：本 Spec 的候选范围与验收账本

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P4-0297` |
| 所属 Phase | Phase 3 事实消费、Phase 4 SSA/native |
| 语言规范 | 现行 [Guide v0.43](../../../guide/README.md)、[所有权](../../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 2026-10-11 已确认实施顺序0297→0300→0299/0298；本页保持draft，由对应独立实现分支激活 |
| 前置 Spec | [SPEC-0294](../../../archive/specs/0294-map-put-control-exit.md) |
| 前置 ADR | [ADR-0016](../../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 关联 ADR | 无新增；若需要改变 ABI，先形成独立决定 |
| 阻塞项 | 更广 receiver/key 清理边界与精确事实核实；独立实现分支激活 |
| 影响范围 | `lang-codegen` Map lowering、直接测试与实现事实 |
| 语言语义变更 | 否；不启用新 Guide |

## 1. Goal

`get`、`contains`、`remove` 的 key 求值提前 return 时，结束实际 receiver/key
借用并清理已求值 owner；完全退出传播 Diverged，正常出口仍执行恰一次 Map 操作。

## 2. 基线与已核实边界

静态基线为 main `e168c485a5379764cf927433452f8b7059f7b334`。
`ssa/lower_frontend/map.rs` 的三操作通过 `lower_map_owner` 要求 key 为 Value；
`lower_frontend.rs::require_value` 将 Diverged 转为 UnsupportedNode。unit 对应
`unit_lower/map.rs::map_owner_operand` / `require_lowered_value` 将其转为 MissingFact。
它们没有 Map.put 已接入的完整退出传播。

single 的 owned receiver 仅保存在 temporary 中，未像 put 一样登记 receiver 的
pending cleanup identity；frontend 已发布的 return loan-end 可能因此在
`lower_frontend/call_lifetimes.rs::emit_control_transfer_cleanup` 缺失。
应按真实 call/argument identity 证明该缺口，不能伪造 SSA loan。

`if (flag) { return } else { key }` 有正常 Value 出口，不等于整个表达式 Diverged。
unit mixed 不能仅凭代码缺少显式 Diverged 分支就判失败；single/unit 必须分别记录。
既有 `map_operands_follow_argument_control_flow` 只证明无退出 if，不替代 return 验收。
SPEC-0294 明确只修 put，本片不重写其有界归档。

后续使用冻结的0296 CLI（生产实现 `ae65b2ce`，非上述main二进制）执行
[普通源码矩阵](../../../archive/migrations/pr72-74-followup-audit.md#冻结cli的map-key-return矩阵)：
owned named `MutableMap<Int, Int>` 的三操作在single中，mixed、反向mixed、双return
均以MissingFact拒绝；unit两种mixed均编译运行成功，双return为MissingFact。
六个无return对照均成功。CLI未提供错误Span或内部事实，不能从该输出单独定位
具体MissingFact调用点；single的完整Diverged转换仍是后续静态边界，不是本次诊断。

## 3. 范围与非目标

- 三操作各覆盖 mixed、反向 mixed、双 return、直接 return；receiver 精确求值一次。
- 先使用 owned named receiver；Borrow receiver、temporary receiver 按 Guide 合法性
  分别建立对照，不能把其中一种结果扩大到所有 receiver。
- String key 使用 named、grouped 和 temporary 形状；退出前已求值 owner 按事实清理，
  未求值后缀不运行；正常查询不得消费 Borrow key，remove 仅移交被删除 value。
- 沿用 frontend 清理事实、既有 carry slots 和 verifier；不按源码名称重推语义。
- 不扩普通调用、Elvis、Map withValue/requireValue 或 nullable V 的 owned 三态表示；
  不把本片扩大为一般 CFG 重构，也不处理 SPEC-0296 的最后判空提前 drop。

`break` / `continue` 参数退出只登记为未批准的后继实施候选，不纳入本片 return Goal。
它们的语言语义并未禁用。启动前须另定范围，先证明目标 loop identity、前缀 temporary
恢复、provider/source loan 在 continue 保留及 break 退出、嵌套 loop 只清理最近目标，
再以两入口普通源码和正常 native 验证；不能把 put-return 通过视作这些能力已接通。

## 4. 验收标准

- [ ] 同一合法源码先通过 typed/ownership，再分别记录三操作 × 四种 return 的
  single/unit 结果、错误 kind/Span 或健康 SSA；已有成功用例保留成功。
- [ ] mixed 两种动态输入保持正常 Map 结果或提前返回；双/直接 return 无 Map 操作、
  无后缀调用，verifier 与未改写 LLVM 均通过。
- [ ] String/Resource 前缀正常 native 验证求值顺序、Borrow key 可复用、清理一次及
  未执行后缀；普通输出不冒充逐指针计数、sanitizer 或完整内存证明。
- [ ] 原 put-return 与无退出三操作回归保持；非法 borrowed move 的诊断不放宽。
- [ ] 按实际改动运行 fmt、受影响 crate clippy、尺寸与文档检查，更新 Architecture。

## 5. 实施与提交

按已确认顺序启动时从届时 main 新建 `fix/spec-0297`，顺序为普通源码红绿边界、最小修复、直接
SSA/native 回归、当前事实与验收账本。每步验证上一节对应项，单独提交并包含
`SPEC-0297`；发布、PR、CI 与合并按当时授权独立闭环。

## 6. 未决问题与验证记录

已取得上述有界普通源码红绿结果；直接return、String/Resource前缀及其他receiver
仍未执行。具体修复只在红测与已发布事实支持后确定，不把unit mixed已有成功列为待修。

| 检查 | 结果 | 边界 |
|---|---|---|
| 静态源码、Guide 与原测试核对 | 已完成 | 只证明上述接线差异，不证明每个 mixed 都失败 |
| 冻结0296 CLI普通源码 | 18个return格：6成功、12拒绝；6个无return对照成功 | 版本、命令和有界结果见审计摘要；原始记录留原本地链；无分配计数或完整内存证明 |
| 起草检查点的 `check_docs.py`、checker 单测、DAG 与 diff | 612 Markdown、37 tests 通过；diff clean | 后续静态复查见审计账本；不作为功能验收 |
| Cargo、内部SSA事实查询、注入/校准/聚合驱动 | 本分支未运行 | 普通native仅限上述CLI矩阵；本草案尚未激活 |
