# SPEC-0204：pointer-like nullable `when` lowering

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P4-204` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 v0.32；候选 [v0.35 §35](../guide/01-design-decisions.md#35-nullable-when-剩余域与-所有权v035-候选未启用) |
| 批准依据 | 无；v0.35 尚未启用 |
| 前置 Spec | SPEC-0034、0184、0196 `done`；SPEC-0202/0203 待完成 |
| 前置 ADR | ADR-0017 `accepted` |
| 阻塞项 | v0.35 明确启用；SPEC-0202/0203 `done` |
| 影响范围 | `lang-codegen` frontend lowering/SSA/LLVM/native tests；Roadmap/Architecture |
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
  与 workspace 基线回归。
- [ ] Architecture/Roadmap 同步。

## 6. 技术方案与边界

扩展现有 `lower_when` 分派，复用 nullable type/terminator/LLVM adapter；不新增 ABI ADR或并行
nullable IR 类型。

## 7. 实施计划

1. [ ] 接 typed/ownership plan 到 SSA CFG → 验证：lowering/verifier 窄测试。
2. [ ] 接 LLVM/native class/Box/Rc → 验证：object/link/run 与 IR 断言。
3. [ ] 同步验收与 Architecture → 验证：codegen、workspace、fmt/clippy 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | SSA/LLVM/native 与完成文档 | `feat(codegen): lower nullable when branches (SPEC-0204)` |

## 9. 未决问题

- 无；inline nullable 由新 ABI ADR/后继 Spec 承接。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | ADR-0017/SPEC-0196 基元已存在，当前 `when` lowering 仍拒绝 pointer-like nullable subject |
