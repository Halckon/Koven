# SPEC-0179：顺序容器借用迭代 typed plan

> **性质**：draft Spec · **状态**：draft（blocked by unapproved v0.37 guide） · **读取时机**：评审 v0.37 proposal 或对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P2-179` |
| 所属 Phase | Phase 2 |
| 语言规范 | 重基基线 v0.36；候选 [v0.37 §37](../../../proposals/v0.37-sequential-iteration.md) |
| 批准依据 | 无；候选已重基到 v0.36，v0.37 尚未启用 |
| 前置 Spec | SPEC-0016、0018、0019、0020、0022、0023、0178 `done` |
| 前置 ADR | [ADR-0023](../../../adr/proposed/0023-borrowed-sequential-iteration-provider.md) 待 `accepted` |
| 阻塞项 | v0.37 启用；ADR-0023 `accepted` |
| 影响范围 | `lang-frontend` for/type facts、L0159–L0160、fixtures；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.37 typed iteration 契约 |

## 2. Goal

完成后，单文件 type checker 能为 intrinsic `Array<T>`、`List<T>`、`MutableList<T>` 的 `for`
发布确定的 sequential borrowed iteration plan，为名称、discard 与 value-class 解构 binding 分配
精确 element/component 类型，并拒绝没有 compiler-bound provider 的 source。

## 3. 范围与需求

- 以 `StatementId` 为稳定 identity，source 精确检查一次；只有 compiler-bound sequential
  container identity 才发布 provider kind、source/container/element type 和 shared delivery。
- 单名称 binding 设置为 element type；单 `_` 发布 `Discard` 且不创建 symbol。所有真实 binding
  都标记为 loop-scoped Borrow delivery，不在 Phase 2 推导 owned copy/move。
- 解构只接受 concrete `value class` element，按替换实际类型参数后的主构造器字段顺序发布
  component projection；`_` component 保留位置但没有 symbol，其他 component 设置精确类型。
- 解构字段数不等继续使用 L0118；非 value-class/不可结构投影 element 使用 L0160。该 descriptor
  独立于局部 `val` 的 Copy/Consume `DestructuringDescriptor`，不调用 `componentN()`。
- 非 intrinsic source 使用 L0159；Error/Deferred 根因不级联。同名用户类型、interface 或方法
  不取得 provider identity。
- body 在所有 binding 类型发布后检查；计划、诊断和 component 顺序不依赖 hash/input 顺序，
  invalid/poisoned `for` 不向 Phase 3 发布半成品 plan。

## 4. 非目标

- 不检查 source/element loan、move、capture、drop 或 jump cleanup；这些属于 SPEC-0211。
- 不实现 receiver/method selection、runtime provider、SSA/LLVM、Map/range/String/IO、自定义或
  consuming iteration。
- 不开放一般 post-index field expression；本 Spec 的 value-class projection 只属于 compiler-owned
  `for` pattern typed fact。

## 5. 验收标准

- [ ] Array/List/MutableList × 名称/`_`/完整 value-class 解构均发布精确、可查询 typed plan。
- [ ] generic element substitution、Copyable/MoveOnly element 与 mixed component 类型均正确；
  Phase 2 不提前发所有权错误。
- [ ] Boolean、String、普通 class、用户同名 List/Iterable/iterator 方法均在 source Span 产生
  L0159，poisoned source 不级联。
- [ ] 非 value-class 解构产生 L0160；过少/过多 component 产生 L0118，`_` 不创建 symbol。
- [ ] 原允许 `for (item in flag)` 的占位 fixture 改为 compile-fail；jump-target/parser/name suite
  不回归。
- [ ] 重复检查与声明/fixture 顺序置换产生相同 descriptor/diagnostics；受影响契约回归通过，
  Architecture 与实现事实同步。

## 6. 技术方案与边界

在现有 container identity/element extraction 与 value-class field substitution 上增加专用
`SequentialIterationDescriptor`（或等价产物），包含 statement/source/provider/element/delivery
及 `Discard | Name | Destructure` binding。它不写回 AST，不伪造普通 call descriptor，也不复用
局部 owned destructuring mode。后续阶段只消费 validated query，缺 plan 必须 fail loud。

## 7. 实施计划

1. [ ] 建立 provider/source/element descriptor 与 L0159 → 验证：三容器/伪造 identity 矩阵。
2. [ ] 接名称/discard/value-class borrowed projection 与 L0118/L0160 → 验证：binding/type 矩阵。
3. [ ] 收口 poisoned/deterministic product 并同步 Architecture → 验证：本 Spec 验收项及下述分层检查。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | typed plan、诊断、测试与完成文档 | `feat(frontend): type sequential iteration (SPEC-0179)` |

## 9. 未决问题

- 无；状态门禁由元数据表达，其他 provider identity 必须由后续 guide 扩展。

## 10. 验证记录

实施前按[分层验收](../../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap/实现审计 | 通过 | Parser/name/jump 已具备；当前 type checker 只检查 source/body，ForBinding 最终无 element type |


2026-09-19 候选重基核对：保留现行 v0.36 的 grammar、nullable/Nothing、所有权与常量契约，
拟议版本取代关系见 proposal。仅更新基线与状态前置，不改变本 Spec 的阶段范围、验收条目或
批准状态；guide 启用与 ADR 接受仍是实施前置。未运行 Rust 测试（本次仅文档）。
