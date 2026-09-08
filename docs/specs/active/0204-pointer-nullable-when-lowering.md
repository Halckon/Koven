# SPEC-0204：pointer-like nullable `when` lowering

> **性质**：实施 Spec · **状态**：approved · **读取时机**：实施或评审 v0.35 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `approved` |
| Goal ID | `KOV-P4-204` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.35 空安全](../../guide/09-nullability-errors.md)与[阶段边界](../../guide/15-conformance-and-staging.md) |
| 批准依据 | 2026-09-08 用户明确启用 v0.35，按持续推进 Goal 分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0034、0184、0196 `done`；SPEC-0202/0203 `done` |
| 前置 ADR | ADR-0017 `accepted` |
| 阻塞项 | 无 |
| 影响范围 | `lang-codegen` frontend lowering/SSA/LLVM/native tests；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.35 pointer-like lowering |

## 2. Goal

完成后，owned whole-root/temporary 的 class/Box/Rc nullable `when` 消费 frontend
typed/ownership plans，经 verified SSA、LLVM、object/link/run 执行 null、non-null、else 与
extraction 分支。

## 3. 范围与需求

- 只消费 0202/0203 descriptor，不按 AST spelling 重算 remaining domain、copy/move 或 drop。
- 复用 ADR-0017 `NullableBranch` non-null view及必要的 `NullableTake`，保持 subject 一次求值。
- 所有正常/diverging edge 精确交付 block parameter 与 owner obligation；null drop no-op，non-null
  未提取 owner conditional drop，已提取 inner 只由新 owner负责。
- 首轮只接受 descriptor 标记为 owned whole-root/temporary 的 class、Box、Rc subject；Borrow/
  Inout/field/element subject 保持确定性 unsupported，等待 nullable-place/loan branch ADR。
- native 覆盖 MoveOnly view/consume/drop；IR/LLVM 验证无 wrapper allocation/tag/隐式 retain。

## 4. 非目标

- 不实现 inline/tagged nullable、`!!`、Elvis、safe call 或新的 nullable ABI。

## 5. 验收标准

- [ ] native 覆盖只读 when 后复用 named subject、循环复用及提取结果恰好析构一次。
- [ ] nullable when 用作后续实参时，消费 `LoanEndFact::point()` 的 ControlTransfer 结束边，
  在 return/break/continue cleanup 前发出 BorrowEnd；较早 Borrow 实参的 root/temporary
  在正常调用前保持存活，abort 不产生 unwind。用 SSA 与 native 反例锁定时序。

- [ ] SSA/verifier 正反矩阵覆盖 proof source、edge、take、drop 与重复 owner。
- [ ] owned-root/temporary class/Box/Rc 的 `null`+`else`、non-consuming view 与 MoveOnly extraction
  native 运行正确；不虚构 pointer-like Copyable 类型。
- [ ] subject 一次求值、Rc retain/release 计数与 conditional drop 被 IR/native 测试锁定。
- [ ] Borrow/Inout/field/element 与 inline nullable 继续确定性 unsupported；既有 nullable `if`
  的受影响契约回归。
- [ ] Architecture 同步。

## 6. 技术方案与边界

扩展现有 `lower_when` 分派，复用 nullable type/terminator/LLVM adapter；不新增 ABI ADR或并行
nullable IR 类型。

SPEC-0203 的匹配边 drop 与精确 loan-end 事实必须在对应 CFG 边消费；不能只查询正常
`loans_ending_at(call)`，也不能在 active loan 下直接消费 ControlTransfer drop。

## 7. 实施计划

1. [ ] 接 typed/ownership plan 到 SSA CFG → 验证：lowering/verifier 窄测试。
2. [ ] 接 LLVM/native class/Box/Rc → 验证：object/link/run 与 IR 断言。
3. [ ] 同步验收与 Architecture → 验证：按[分层验收](../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | SSA/LLVM/native 与完成文档 | `feat(codegen): lower nullable when branches (SPEC-0204)` |

## 9. 未决问题

- 无语义未决项；R1–R3 已随 v0.35 启用，前置依赖见元数据。

## 10. 验证记录

实施前按[分层验收](../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | ADR-0017/SPEC-0196 基元已存在，当前 `when` lowering 仍拒绝 pointer-like nullable subject |
