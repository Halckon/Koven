# SPEC-0406：`unit_lower` 入口交接与可空 `when` 清理壳模块收敛及整数算子映射去重

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `unit_lower` 入口交接壳模块（`handoff`）、可空 `when` 清理子模块（`nullable_when/cleanup`）、闭包布局辅助内聚与跨层整数/比较/复合赋值算子映射去重时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0406` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户指示合入 `main` 最新修改并继续推进 E3 收尾清理 |
| 基线 | [SPEC-0405](0405-refine-dead-helpers-and-test-entry-gates-retirement.md)（`708fc6c8`，含 `main` `e659b66a`） |
| 分支 | `feature/spec-0406-refine-unit-lower-shell-modules-and-operator-helpers-retirement` |
| 前置 | [SPEC-0383](0383-refine-final-legacy-retirement.md)、[SPEC-0403](0403-refine-unit-lower-dead-fields-and-provider-retirement.md)、[SPEC-0404](0404-refine-unit-lower-shell-modules-and-duplicate-helpers-retirement.md)、[SPEC-0405](0405-refine-dead-helpers-and-test-entry-gates-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0404` 与 `SPEC-0405` 基础上，继续收敛 `crates/lang-codegen/src/ssa/unit_lower` 中剩余的薄包装壳模块（`handoff.rs` 与 `nullable_when/cleanup.rs`），将仅由 `closure/layout.rs` 消费的 `lambda` 尾表达式与直接引用辅助函数内聚回 `closure/layout.rs`，并在 `crates/lang-codegen/src/ssa/integer.rs` 中统一 `checked_operator`、`assignment_operator` 与 `comparison_operator`，消除 `unit_lower`、`ssa::refine` 与 `lower_frontend` 三处的重复算子映射实现及 `lower_frontend/control.rs` 的无调用死辅助函数。

## 2. 范围与设计

### 2.1 收敛 `unit_lower` 入口交接与 `nullable_when` 清理壳模块

- **退役 `crates/lang-codegen/src/ssa/unit_lower/handoff.rs`**：将 `From<OwnedCompilationUnitViewError> for LoweringError`、`lower_owned_unit_with_entry` 与 `#[cfg(test)] lower_scalar_unit_with_entry` 收归同属编译单元入口包装层的 `crates/lang-codegen/src/ssa/unit_lower/constant.rs`，删除 `handoff.rs`。
- **退役 `crates/lang-codegen/src/ssa/unit_lower/nullable_when/cleanup.rs`**：将 `emit_nullable_when_operand_drops`、`validate_nullable_when_retirement`、`retire_nullable_when_scope`、`retire_nullable_when_transfers` 与 `preflight_nullable_take_drops` 直接内聚到 `crates/lang-codegen/src/ssa/unit_lower/nullable_when.rs`，删除 `nullable_when/cleanup.rs`。
- **内聚闭包布局私有辅助函数**：将 `crates/lang-codegen/src/ssa/unit_lower/closure.rs` 中仅由 `closure/layout.rs` 调用的 `lambda_tail_expression` 与 `direct_reference_symbol` 迁入 `crates/lang-codegen/src/ssa/unit_lower/closure/layout.rs`。

### 2.2 统一跨层整数、比较与复合赋值算子映射

- 在 `crates/lang-codegen/src/ssa/integer.rs` 中统一提供 `checked_operator`、`assignment_operator` 与 `comparison_operator`（与既有 `bitwise_operator` 同级）。
- 在 `crates/lang-codegen/src/ssa/unit_lower/scalar.rs`、`crates/lang-codegen/src/ssa/unit_lower/assignment.rs`、`crates/lang-codegen/src/ssa/refine/build/scalar_text.rs`、`crates/lang-codegen/src/ssa/refine/build/expression.rs`、`crates/lang-codegen/src/ssa/refine/build/assignment.rs` 与 `crates/lang-codegen/src/ssa/lower_frontend.rs` 中统一复用 `ssa::integer` 的算子映射函数，删除各处重复定义。
- 删除 `crates/lang-codegen/src/ssa/lower_frontend/control.rs` 中无任何调用方的 `ExpressionLowerer::add_empty_block`。

## 3. 验收标准

- [x] 退役 `unit_lower/handoff.rs` 与 `unit_lower/nullable_when/cleanup.rs`，完成 `closure/layout.rs` 辅助函数内聚与 `ssa::integer` 算子映射统一。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 2 个退役项已从版本库移除 | 通过 | 逐个确认：2/2 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过 | 2/2 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- refine verify_ownership` 等定向；完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` |

核对说明：

- 退役 `unit_lower/handoff.rs`（收归 `unit_lower/constant.rs`）与 `unit_lower/nullable_when/cleanup.rs`（收归 `unit_lower/nullable_when.rs`），两个宿主均存在
- `unit_lower/closure/layout.rs` 属本 Spec 内部内聚删除，该文件随后被 SPEC-0436 再次合并
