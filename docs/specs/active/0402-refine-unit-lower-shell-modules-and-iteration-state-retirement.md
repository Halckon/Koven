# SPEC-0402：`unit_lower` 遗留壳模块、单变体 `IterationState` 与死辅助入口退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `unit_lower` 遗留壳模块（`map_borrow`、`map_consume`、`while_proof`）、单变体 `IterationState` 及无调用辅助方法退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0402` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认继续推进下一批旧路径退役 |
| 基线 | [SPEC-0401](0401-refine-unit-lower-rc-if-and-field-root-retirement.md)（`6ceafb47`） |
| 分支 | `feature/spec-0402-refine-unit-lower-shell-modules-and-iteration-state-retirement` |
| 前置 | [SPEC-0395](0395-refine-nullable-while-retirement.md)、[SPEC-0396](0396-refine-map-iteration-retirement.md)、[SPEC-0397](0397-refine-nullable-map-and-closure-retirement.md)、[SPEC-0401](0401-refine-unit-lower-rc-if-and-field-root-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

清理 `SPEC-0395`、`SPEC-0396`、`SPEC-0397` 与 `SPEC-0399` 退役可空 `while`、`Map` 迭代与可空 `Map` 消费/分支旧路径后遗留在 `crates/lang-codegen/src/ssa/unit_lower` 中的三个单函数壳模块（`map_borrow.rs`、`map_consume.rs`、`while_proof.rs`）、顺序容器 `for` 迭代上下文中的单变体 `IterationState` 包装枚举，以及无调用方的死辅助入口（`NullableProof::birth_loan`、`CallableAbi::view`、`checked_operation`、`lower_tail_value_body`）与 `aggregate.rs` 中的不可达分支。

## 2. 范围与设计

### 2.1 退役遗留壳模块清单

删除以下仅剩单个跨模块辅助函数、原主责家族已整体迁入 `ssa::refine` 的壳模块文件：

- `crates/lang-codegen/src/ssa/unit_lower/map_borrow.rs`（将 `map_borrow_condition` 收归 `control/nullable_if.rs`）
- `crates/lang-codegen/src/ssa/unit_lower/map_consume.rs`（将兼容委托接收者借用辅助 `lower_delegated_call_receiver` 收归 `field_borrow.rs`）
- `crates/lang-codegen/src/ssa/unit_lower/while_proof.rs`（将可空证明退出核验辅助 `ensure_no_while_proof_carriers`、`ensure_while_proof_non_call_carriers`、`end_nullable_proof` 与 `validate_nullable_proof_retirement` 收归 `nullable_proof.rs`）

### 2.2 展平 `IterationState` 与移除死辅助入口

- **展平单变体 `IterationState`**（自 `SPEC-0396` 退役 `unit_lower` 中 `Map` 迭代后仅剩 `Borrowed` 单一变体）：
  - 在 `crates/lang-codegen/src/ssa/unit_lower/loop_control/iteration.rs` 与 `iteration_cleanup.rs` 中移除 `enum IterationState`，将 `start`、`element`、`step` 与 `int_type` 直接内联至 `IterationContext`。
- **移除无调用方的死辅助方法与不可达分支**：
  - 从 `crates/lang-codegen/src/ssa/unit_lower/nullable_proof.rs` 移除无调用方的 `NullableProof::birth_loan`；
  - 从 `crates/lang-codegen/src/ssa/unit_lower/callable_abi.rs` 移除无调用方的 `CallableAbi::view`；
  - 从 `crates/lang-codegen/src/ssa/unit_lower/checked.rs` 移除无调用方的 `UnitExpressionLowerer::checked_operation`；
  - 从 `crates/lang-codegen/src/ssa/unit_lower/control/branch.rs` 移除无调用方的 `UnitExpressionLowerer::lower_tail_value_body`；
  - 从 `crates/lang-codegen/src/ssa/unit_lower/aggregate.rs::lower_aggregate_projection` 移除已被前置 `current_receiver_field` 完全覆盖的不可达 `explicit_receiver.is_none()` 分支。

## 3. 验收标准

- [x] 删除 `map_borrow.rs`、`map_consume.rs` 与 `while_proof.rs` 三个遗留壳模块，并完成 `IterationState` 展平及死辅助方法清理。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。
