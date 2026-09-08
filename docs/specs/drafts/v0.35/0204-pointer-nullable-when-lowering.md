# SPEC-0204：pointer-like nullable `when` lowering

> **性质**：draft Spec · **状态**：draft（blocked by unapproved v0.35 guide） · **读取时机**：评审 v0.35 proposal 或对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P4-204` |
| 所属 Phase | Phase 4 |
| 语言规范 | 起草基线 v0.32；候选 [v0.35 §35](../../../proposals/v0.35-nullable-when-and-non-null-assertion.md) |
| 批准依据 | 无；v0.35 尚未启用，且尚未显式重基到现行 v0.34 |
| 前置 Spec | SPEC-0034、0184、0196 `done`；SPEC-0202/0203 待完成 |
| 前置 ADR | ADR-0017 `accepted` |
| 阻塞项 | 明确 v0.35 对现行 v0.34 的重基与取代关系；v0.35 启用；SPEC-0202/0203 `done` |
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

## 7. 实施计划

1. [ ] 接 typed/ownership plan 到 SSA CFG → 验证：lowering/verifier 窄测试。
2. [ ] 接 LLVM/native class/Box/Rc → 验证：object/link/run 与 IR 断言。
3. [ ] 同步验收与 Architecture → 验证：按[分层验收](../../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | SSA/LLVM/native 与完成文档 | `feat(codegen): lower nullable when branches (SPEC-0204)` |

## 9. 未决问题

- 启用前处理 [v0.34 重基审查](../../../proposals/v0.35-nullable-when-and-non-null-assertion.md#v034-重基审查未启用)中的 R1（named owner liveness）。

- 其余状态门禁由元数据表达。

## 10. 验证记录

实施前按[分层验收](../../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | ADR-0017/SPEC-0196 基元已存在，当前 `when` lowering 仍拒绝 pointer-like nullable subject |
