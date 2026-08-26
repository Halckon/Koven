# SPEC-0203：nullable `when` view 与 extraction 所有权

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P3-203` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 v0.32；候选 [v0.35 §35](../guide/01-design-decisions.md#35-nullable-when-剩余域与-所有权v035-候选未启用) |
| 批准依据 | 无；v0.35 尚未启用 |
| 前置 Spec | SPEC-0028、0029 `done`；SPEC-0202 待完成 |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0017 |
| 阻塞项 | v0.35 明确启用；SPEC-0202 `done` |
| 影响范围 | `lang-frontend` ownership/drop facts/tests；Roadmap/Architecture |
| 语言语义变更 | 否；实施启用后的 v0.35 ownership 契约 |

## 2. Goal

完成后，ownership checker 消费 SPEC-0202 plan，为 nullable `when` 发布 non-owning proof view、
Copy/Consume extraction 与每分支 wrapper/inner drop facts。

## 3. 范围与需求

- null 判别只读 subject；non-null edge view 绑定同一 root/place/loan identity，不产生
  copy/retain/owner obligation。
- non-consuming use 复用 view；Value use 对 Copyable inner 复制，对 MoveOnly inner 只允许消费
  整个合法 nullable root/temporary，并拒绝 MoveOnly Borrow/Inout、field/element partial extraction。
- 每条 branch 精确跟踪 subject 未提取、已提取、null 与 diverging 状态；正常 join、return、
  break/continue 与 abort 不重复 drop wrapper/inner。
- MoveOnly Borrow/Inout/field/element extraction 分别使用 L0133/L0132/L0136，active loan 使用
  L0135，成功 consume 后再次使用为 L0131；Copyable inner 从这些 source copy 仍合法。facts/
  diagnostics 顺序确定，失败不发布半成品 validated ownership plan。

## 4. 非目标

- 不 lower inline nullable SSA/LLVM ABI；不实现 `!!`、borrow-return 或新的 loan lifetime。

## 5. 验收标准

- [ ] 一般 frontend nullable 的 read/view、Copyable copy 与 MoveOnly whole-root extraction 通过，
  覆盖 scalar/value/enum/String 及 class/Box/Rc 类型层事实而不依赖 LLVM 表示。
- [ ] MoveOnly Borrow/Inout、field、container element、active loan 与重复 move 反例分别产生
  稳定 L0133/L0132/L0136/L0135/L0131。
- [ ] null/non-null/else、comma alternatives、branch join 与所有控制转移的 drop facts 精确。
- [ ] no retain/no duplicate owner/validated marker 与现有 ownership suite 回归。
- [ ] Architecture/Roadmap 与 workspace 基线同步。

## 6. 技术方案与边界

ownership 只消费 0202 typed plan及现有 place/loan/drop machinery；不自行重算 `when` coverage。
proof view 与 ADR-0017 语义对齐，但 frontend facts 保持 LLVM 无关。

## 7. 实施计划

1. [ ] 建立 branch proof/extraction ownership facts → 验证：model 正反测试。
2. [ ] 接入 checker/drop planner → 验证：move/loan/control-transfer 矩阵。
3. [ ] 同步验收与 Architecture → 验证：frontend、workspace、fmt/clippy 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | ownership facts、测试与完成文档 | `feat(frontend): own nullable when branches (SPEC-0203)` |

## 9. 未决问题

- 无；状态门禁由元数据表达。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | 当前 checker 只按一般 `when` 读 subject，未发布 nullable branch view/extraction/drop plan |
