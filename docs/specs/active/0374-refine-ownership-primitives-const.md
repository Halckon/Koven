# SPEC-0374：所有权原语与 `const val` 物化迁入所有权细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的所有权原语（`replace`/`swap`）与 `const val` 常量物化细化层迁移时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0374` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权、借用与析构](../../guide/10-ownership-borrowing-drop.md)、[声明与可调用体](../../guide/05-declarations-callables.md) |
| 批准依据 | 用户确认按推荐优先级依次完成 E3 后续切片 |
| 基线 | `feature/spec-0373-refine-destructuring-range-for` `9f13fb86` |
| 分支 | `feature/spec-0374-refine-ownership-primitives-const` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[SPEC-0373](0373-refine-destructuring-range-for.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将局部可变根（`var`）上的所有权原语 `replace(&root, value)` 与 `swap(&left, &right)`（含 Copyable 与 MoveOnly 已支持存储类型）以及 `const val` 常量使用点物化（`Boolean`、`Byte`..`ULong` 的 `InlineCopy` 与 `String` 的 `StringTemporary`）迁入 `ssa::refine`，使已迁入家族内的可变根原子置换/交换与编译期常量读取统一由所有权细化层构建。

## 2. 实施内容

1. **细化层 IR 与发射（`refine/body.rs`、`refine/emit.rs`）**：
   - 新增 `ROp::Copy`、`ROp::RootReplace` 与 `ROp::RootSwap`，并在 `emit.rs` 中映射为 `Operation::Copy`、`Operation::RootReplace` 与 `Operation::RootSwap`。
2. **`const val` 物化（`refine/subset.rs`、`refine/subset/scalar_text.rs`、`refine/build.rs`、`refine/build/scalar_text.rs`、`refine/build/callable.rs`、`refine/build/when.rs`）**：
   - 在 `Checker::expression` 与 `Builder::expression_inner` 中接入 `constant_materialization`，将 `InlineCopy`（`Boolean`、八种整数）映射为 `ROp::Constant`，将 `StringTemporary` 映射为 `ROp::StringLiteral` 并登记为临时值以按 `CallReturn`/`AfterExpression` 事实释放。
   - 在 `unspecialized` 中放行 `UnitShortCircuitRhs::Conditional` 的动态短路计划，并在借用参数与 `when` 主语探针中跳过常量物化临时值。
3. **可变根 `replace` / `swap`（`refine/subset/inout.rs`、`refine/build/inout.rs`）**：
   - 新增 `Checker::ownership_primitive` 与 `Builder::ownership_primitive`：校验 `UnitOwnershipPrimitiveDescriptor`、`UnitOwnershipPrimitivePlan`、独占借用事实与值交付方式；对 Copyable 根与新值先发出 `ROp::Copy` 以隔离 SSA 值身份，经 `ROp::RootPlace` 与 `ROp::BorrowBegin(Exclusive)` 后发出 `ROp::RootReplace` / `ROp::RootSwap` 并更新局部绑定槽位。

## 3. 验收标准

- [x] `refine_tests::ownership_primitives_and_constants_lower_in_refine` 验证 `const val`（`Int`/`String`）物化与局部可变根 `replace`/`swap` 经 `ssa::refine` lowering 并通过原生计数执行验证。
- [x] `unit_root_primitive_tests`（16/16）、`unit_constant_tests`（27/27）与 `unit_lower::constant::tests` 全部通过。
- [x] `cargo fmt --all -- --check`、`cargo clippy`、`scripts/check_docs.py`、`scripts/check_rust_sizes.py --base main` 通过。
