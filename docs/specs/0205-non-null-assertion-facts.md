# SPEC-0205：非空断言 extraction typed facts

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P2-205` |
| 所属 Phase | Phase 2 |
| 语言规范 | 起草基线 v0.32；候选 [v0.35 §35](../guide/01-design-decisions.md#35-nullable-when-剩余域与-所有权v035-候选未启用) |
| 批准依据 | 无；v0.35 尚未启用，且尚未显式重基到现行 v0.34 |
| 前置 Spec | SPEC-0019、0022、0067 `done` |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0017 |
| 阻塞项 | 明确 v0.35 对现行 v0.34 的重基与取代关系；v0.35 启用 |
| 影响范围 | `lang-frontend` type checking model/checker/tests；Roadmap/Architecture |
| 语言语义变更 | 否；实施启用后的 v0.35 typed extraction 契约 |

## 2. Goal

完成后，type checker 为每个有效 `e!!` 发布 operand identity/category、nullable/inner 类型、
Copyable 条件、一次求值的 extraction descriptor 与 compiler-bound assertion Abort effect。

## 3. 范围与需求

- 保持现有 `T? -> T` 与 L0085；descriptor 精确关联 assertion、operand、operator Span 与类型。
- 保存 operand 的 place/temporary 及 binding category，标明 Copyable-copy 或 MoveOnly extraction
  候选，但不在 Phase 2 决定 loan/move/drop 成功。
- Abort effect 是由 `NonNullAssert` AST identity 直接产生的封闭 compiler-bound fact，不执行
  源码名称查找、不生成普通 `CallDescriptor`，也不受同名 `error` 声明遮蔽。
- descriptor 参加 overload/lambda trial snapshot/rollback，顺序确定且不写回 AST。

## 4. 非目标

- 不执行所有权检查，不生成 abort/SSA/LLVM；不实现 nullable borrow unwrap 或其他 control form。

## 5. 验收标准

- [ ] Copyable/MoveOnly、owned/Borrow/Inout root、temporary/field/element operand descriptor 矩阵通过。
- [ ] assertion Abort effect 唯一、可回滚且不依赖 `error` 名称或普通 callable selection。
- [ ] 非 nullable/Error/Deferred、nested postfix 与单次求值 Span/identity 稳定。
- [ ] overload trial 完整回滚，L0085 与既有 type suite 回归。
- [ ] Architecture/Roadmap 与 workspace 基线同步。

## 6. 技术方案与边界

在现有 `Expression::NonNullAssert` type path 发布专用 descriptor，复用 `ExpressionCategory`、
place facts 和条件 `Copyable` 查询；ownership 阶段是唯一消费该候选并决定合法性的阶段。

## 7. 实施计划

1. [ ] 增加 descriptor/API/trial snapshot → 验证：model/rollback 测试。
2. [ ] 接 type checker 与 category 矩阵 → 验证：type-checking 窄测试。
3. [ ] 同步验收与 Architecture → 验证：frontend、workspace、fmt/clippy 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | typed facts、测试与完成文档 | `feat(frontend): describe non-null extraction (SPEC-0205)` |

## 9. 未决问题

- 无；状态门禁仅为 v0.35 尚未启用。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | `!!` 已类型检查但没有 extraction descriptor，ownership 当前把 operand 当普通 Read |
