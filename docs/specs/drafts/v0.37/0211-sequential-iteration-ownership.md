# SPEC-0211：顺序迭代 source/element loan 与退出清理

> **性质**：draft Spec · **状态**：draft（v0.37 已启用，按依赖排期） · **读取时机**：实施或评审对应阶段 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P3-211` |
| 所属 Phase | Phase 3 |
| 语言规范 | [现行 v0.37 §37](../../../guide/12-collections-destructuring.md#37-借用式顺序容器迭代-provider) |
| 批准依据 | 2026-09-19 用户明确启用 v0.37；按持续 Goal 顺序推进 |
| 前置 Spec | SPEC-0029、0030、0032 `done`；SPEC-0179 待完成 |
| 前置 ADR | [ADR-0023](../../../adr/accepted/0023-borrowed-sequential-iteration-provider.md) `accepted` |
| 阻塞项 | SPEC-0179 `done` |
| 影响范围 | `lang-frontend` iteration ownership/loan/liveness/drop/capture facts、fixtures；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.37 iteration lifecycle |

## 2. Goal

完成后，ownership checker 消费 SPEC-0179 typed plan，为整个 `for` 建立 shared source loan、
每轮 element/component Borrow binding、temporary source 延寿和所有正常/提前退出 cleanup facts，
并用既有所有权诊断拒绝从迭代借用中移动或修改 source/element。

## 3. 范围与需求

- named/field source 建立覆盖 provider/body/backedge 的 shared place loan；Borrow source 复用或
  reborrow，Inout source shared-reborrow。进入循环前已 moved source 继续使用 L0131。
- temporary source 成为 hidden owned temporary 并延寿到 `LoopExit(statement)`；不得在 source
  expression 后 drop。source loan 结束后才 drop temporary，元素只由 container owner drop。
- 每轮建立 element shared loan；名称和具名解构 component 是 non-owning shared binding，`_`
  不建 binding。Copyable read 复制，MoveOnly Value/return 使用 L0133，`&binding` 使用 L0134。
- source loan 下的整体 move/drop、replacement、exclusive access 与未来 relocation 使用 L0135；
  shared read、嵌套 shared iteration 合法，不按已知当前索引放宽 root conflict。
- closure capture 复用 L0137/L0138：borrow capture 必须在本轮结束前释放，owned capture 不得从
  Borrow binding 取得 owner；跨线程 delivery 不得绕过既有约束。
- 正常 fallthrough/continue 先逆序析构 body-local owner/结束其派生 loan，再结束 element-derived
  loans 并保留 source loan；break/exhaustion 随后结束 provider/source loan 并处理 temporary。
  return operand 先求值和交付，再按同样的 body-local → element → provider → source → temporary →
  outer-scope 顺序清理，不能为允许 `return source` 而提前结束 loan。
- 每轮 binding 状态重新建立，不把上一轮 moved/error state 合并到下一轮；nested jump 只清理
  最近 loop，所有事实按源码与逆析构顺序确定发布。

## 4. 非目标

- 不生成 SSA/LLVM，不实现 provider cursor/length、runtime function 或新诊断码。
- 不实现 consuming/custom/Map/range/String/IO iteration、borrow-return、一般 NLL 或容器 mutation API。
- 不改变普通 call loan、local destructuring、container replacement 或 closure capture 语义。

## 5. 验收标准

- [ ] named/field/Borrow/Inout/temporary source 产生精确 source loan 与 lifetime facts；循环后 named
  source 可复用，temporary 不早析构且每条退出路径恰好 drop 一次。
- [ ] Copyable binding 普通值使用/return 合法；MoveOnly Value/return 为 L0133，`&binding` 为
  L0134，borrow call 合法。
- [ ] body 内 move/drop/replace source 或 exclusive access 产生 L0135；shared read 与 nested shared
  iteration 合法。
- [ ] 名称与 mixed value-class component borrow、discard、borrowed/owned closure capture 形成正确
  L0137/L0138 与无多余 binding/drop。
- [ ] normal/continue/break/exhaustion/return/nested loop 的 cleanup facts 精确锁定 body-local owner、
  derived loan、element/component loan、provider、source loan、temporary 与外围 scope 的顺序；
  return source 在 operand Span 产生 L0135，abort 不发布 unwind cleanup。
- [ ] liveness 不再把 provider 使用的 named source 在 source expression 后提前 drop；重复运行
  结果确定，受影响契约回归通过，Architecture 与实现事实同步。

## 6. 技术方案与边界

新增 statement-keyed `IterationOwnershipPlan` 与职责明确的 iteration loan owner/category，不把
持久 loan 伪装成同步 call。复用 `OwnershipPlace`、dynamic element identity、non-owning binding、
closure checks 和 `DropPoint::ControlTransfer/LoopExit`；liveness 必须认识 hidden provider use。
Phase 4 只消费 validated cleanup 序列，不重新从 jump AST 推导生命周期。

## 7. 实施计划

1. [ ] 建立 source/temporary provider lifetime 与冲突 → 验证：source category/L0131/L0135 矩阵。
2. [ ] 建立 element/component Borrow binding 与 capture → 验证：Copyable/MoveOnly/closure 矩阵。
3. [ ] 接全部 exit cleanup 与 liveness → 验证：normal/jump/nested/temporary drop 矩阵及受影响的共享契约测试。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | lifecycle facts、诊断复用、测试与完成文档 | `feat(frontend): own sequential iteration (SPEC-0211)` |

## 9. 未决问题

- 无；provider SSA 表示由 ADR-0023/SPEC-0212 独立封闭。

## 10. 验证记录

实施前按[分层验收](../../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap/实现审计 | 通过 | 当前 checker 仅 Read source + maybe-loop；drop planner 可能在真实 provider 使用前析构 named source |


2026-09-19 候选重基核对：保留现行 v0.36 的 grammar、nullable/Nothing、所有权与常量契约，
拟议版本取代关系见 proposal。仅更新基线与状态前置，不改变本 Spec 的阶段范围、验收条目或
批准状态；guide 启用与 ADR 接受仍是实施前置。未运行 Rust 测试（本次仅文档）。

2026-09-19 启用记录：v0.37 已启用、ADR-0023 accepted；temporary source 纳入首轮 native。
上方重基时的未启用说明是历史记录，不再是当前阻塞项；实现/验收尚未完成。
