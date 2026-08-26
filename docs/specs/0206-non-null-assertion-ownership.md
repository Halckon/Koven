# SPEC-0206：非空断言 Copy/Consume 所有权

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P3-206` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 v0.32；候选 [v0.35 §35](../guide/01-design-decisions.md#35-nullable-when-剩余域与-所有权v035-候选未启用) |
| 批准依据 | 无；v0.35 尚未启用 |
| 前置 Spec | SPEC-0028、0029 `done`；SPEC-0205 待完成 |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0017 |
| 阻塞项 | v0.35 明确启用；SPEC-0205 `done` |
| 影响范围 | `lang-frontend` ownership/drop facts/tests；Roadmap/Architecture |
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
- [ ] Architecture/Roadmap 与 workspace 基线同步。

## 6. 技术方案与边界

替换当前把 `NonNullAssert` operand 一律当 `Read` 的临时处理，复用 place、Value delivery、move、
loan 与 drop planner；frontend facts不引用 SSA operation。

## 7. 实施计划

1. [ ] 建立 assertion extraction/abort ownership facts → 验证：model 正反测试。
2. [ ] 接 checker、liveness、drop planner → 验证：move/loan/drop 矩阵。
3. [ ] 同步验收与 Architecture → 验证：frontend、workspace、fmt/clippy 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | ownership facts、测试与完成文档 | `feat(frontend): own non-null extraction (SPEC-0206)` |

## 9. 未决问题

- 无；状态门禁由元数据表达。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | checker/drop planner 当前均把 `!!` operand 当 Read，不能证明 MoveOnly extraction |
