# SPEC-0205：非空断言 extraction typed facts

> **性质**：实施 Spec · **状态**：approved · **读取时机**：实施或评审 v0.35 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `approved` |
| Goal ID | `KOV-P2-205` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.35 空安全](../../guide/09-nullability-errors.md)与[阶段边界](../../guide/15-conformance-and-staging.md) |
| 批准依据 | 2026-09-08 用户明确启用 v0.35，按持续推进 Goal 分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0019、0022、0067 `done` |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0017 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` type checking model/checker/tests；Architecture |
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

- [ ] 同名 error 声明不改变 assertion Abort identity；显式 error 调用仍按普通名称解析。

- [ ] Copyable/MoveOnly、owned/Borrow/Inout root、temporary/field/element operand descriptor 矩阵通过。
- [ ] assertion Abort effect 唯一、可回滚且不依赖 `error` 名称或普通 callable selection。
- [ ] 非 nullable/Error/Deferred、nested postfix 与单次求值 Span/identity 稳定。
- [ ] overload trial 完整回滚，L0085 与既有 type suite 回归。
- [ ] Architecture 与实现事实同步。

## 6. 技术方案与边界

在现有 `Expression::NonNullAssert` type path 发布专用 descriptor，复用 `ExpressionCategory`、
place facts 和条件 `Copyable` 查询；ownership 阶段是唯一消费该候选并决定合法性的阶段。

## 7. 实施计划

1. [ ] 增加 descriptor/API/trial snapshot → 验证：model/rollback 测试。
2. [ ] 接 type checker 与 category 矩阵 → 验证：type-checking 窄测试。
3. [ ] 同步验收与 Architecture → 验证：按[分层验收](../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | typed facts、测试与完成文档 | `feat(frontend): describe non-null extraction (SPEC-0205)` |

## 9. 未决问题

- 无语义未决项；R1–R3 已随 v0.35 启用，前置依赖见元数据。

## 10. 验证记录

实施前按[分层验收](../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | `!!` 已类型检查但没有 extraction descriptor，ownership 当前把 operand 当普通 Read |
