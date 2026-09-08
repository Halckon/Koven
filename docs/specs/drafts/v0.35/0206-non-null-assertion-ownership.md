# SPEC-0206：非空断言 Copy/Consume 所有权

> **性质**：draft Spec · **状态**：draft（等待前置 Spec） · **读取时机**：实施或评审 v0.35 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P3-206` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.35 空安全](../../../guide/09-nullability-errors.md)与[阶段边界](../../../guide/15-conformance-and-staging.md) |
| 批准依据 | 2026-09-08 用户明确启用 v0.35，按持续推进 Goal 分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0028、0029 `done`；SPEC-0205 待完成 |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0017 |
| 阻塞项 | SPEC-0205 `done` |
| 影响范围 | `lang-frontend` ownership/drop facts/tests；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.35 ownership 契约 |

## 2. Goal

完成后，ownership checker 消费 SPEC-0205 descriptor，为 `!!` 发布 Copyable copy 或 MoveOnly
whole-root consumption、non-null inner transfer、null Abort edge 与精确 drop facts。

## 3. 范围与需求

- Copyable inner 复制且 place 可继续使用；MoveOnly inner 只允许整体消费 owned root/temporary。
- MoveOnly Borrow/Inout、field、element extraction 分别使用 L0133、L0132、L0136；active loan
  使用 L0135。Copyable inner 从相同 source copy 合法；成功 MoveOnly continuation 上原 binding
  moved，后续使用 L0131。
- non-null edge凭 proof consume nullable并把 inner obligation交付结果；null edge不伪造 take/
  consume，直接进入 0205 的 diverging Abort effect，无正常 successor 或 unwind cleanup。
- drop/liveness 与 nested return/call/assignment 保持 ASAP 和一次求值。

## 4. 非目标

- 不生成 SSA/LLVM；不提供 place-preserving borrow unwrap、borrow-return 或 inline nullable ABI。

## 5. 验收标准

- [ ] Copyable place 保留、MoveOnly root/temporary move 与 use-after-move 矩阵通过。
- [ ] MoveOnly Borrow/Inout/field/element/active loan 与重复使用分别产生稳定
  L0133/L0132/L0136/L0135/L0131 与精确 Span/labels。
- [ ] null Abort、non-null transfer、nested control-flow/drop facts 无泄漏或双析构。
- [ ] validated ownership plan、determinism 与现有 ownership/drop suite 回归。
- [ ] Architecture 与实现事实同步。

## 6. 技术方案与边界

替换当前把 `NonNullAssert` operand 一律当 `Read` 的临时处理，复用 place、Value delivery、move、
loan 与 drop planner；frontend facts不引用 SSA operation。

## 7. 实施计划

1. [ ] 建立 assertion extraction/abort ownership facts → 验证：model 正反测试。
2. [ ] 接 checker、liveness、drop planner → 验证：move/loan/drop 矩阵。
3. [ ] 同步验收与 Architecture → 验证：按[分层验收](../../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | ownership facts、测试与完成文档 | `feat(frontend): own non-null extraction (SPEC-0206)` |

## 9. 未决问题

- 无语义未决项；R1–R3 已随 v0.35 启用，前置依赖见元数据。

## 10. 验证记录

实施前按[分层验收](../../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | checker/drop planner 当前均把 `!!` operand 当 Read，不能证明 MoveOnly extraction |
