# SPEC-0404：`unit_lower` 剩余单入口壳模块、重复辅助函数与测试专用 `unit_plan` 接口退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `unit_lower` 剩余单入口壳模块（`statement`、`rc`、`checked`）、重复辅助函数收敛与测试专用 `unit_plan` / `integer_range` 接口门控时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0404` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认合入 `main` 最新修改并继续推进下一批旧路径退役 |
| 基线 | [SPEC-0403](0403-refine-unit-lower-dead-fields-and-provider-retirement.md)（`d5af2081`，含 `main` `03d70377`） |
| 分支 | `feature/spec-0404-refine-unit-lower-shell-modules-and-duplicate-helpers-retirement` |
| 前置 | [SPEC-0359](../../archive/specs/0359-refine-rc.md)、[SPEC-0371](0371-refine-integer-range-text.md)、[SPEC-0402](0402-refine-unit-lower-shell-modules-and-iteration-state-retirement.md)、[SPEC-0403](0403-refine-unit-lower-dead-fields-and-provider-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0402` 与 `SPEC-0403` 基础上，继续清理 `crates/lang-codegen/src/ssa/unit_lower` 中仅剩单一适配实现的三个壳模块（`statement.rs`、`rc.rs`、`checked.rs`），收敛 `unit_lower` 与 `unit_plan` 之间重复定义的签名查询、span 包含判定及整数内建类型判定辅助函数，并为仅由测试路径调用的 `integer_range::operation` 与 `unit_plan` 测试入口添加 `#[cfg(test)]` 门控。

## 2. 范围与设计

### 2.1 退役 `unit_lower` 剩余单入口壳模块

删除以下仅剩单一适配或单函数入口的壳模块，将其内聚到对应的核心模块中：

- `crates/lang-codegen/src/ssa/unit_lower/statement.rs`（将 `impl StatementLowering for UnitExpressionLowerer<'_>` 收归已承载 `StatementLowering for NullTail` 的 `control/branch.rs`）
- `crates/lang-codegen/src/ssa/unit_lower/rc.rs`（将 `lower_rc_construction` 收归统一负责构造描述符与字段求值的 `construction.rs`）
- `crates/lang-codegen/src/ssa/unit_lower/checked.rs`（将 `CheckedRoles`、`UnitExpressionLowerer::checked` 与 `impl CheckedLowering for UnitExpressionLowerer<'_>` 收归 `scalar.rs`）

### 2.2 收敛重复辅助函数

- **统一 `unit_callable_signature`**：将 `crates/lang-codegen/src/ssa/unit_plan.rs::unit_callable_signature` 提升为 `pub(crate)`，删除 `crates/lang-codegen/src/ssa/unit_lower.rs` 中完全重复的私有 `fn unit_callable_signature`。
- **统一 `span_contains` 与 `strictly_contains`**：在 `crates/lang-codegen/src/ssa/unit_lower.rs` 中统一提供 `pub(super) fn span_contains` 与 `pub(super) fn strictly_contains`，删除 `call.rs`、`closure.rs` 与 `type_plan.rs` 中重复定义的同名私有函数。
- **统一 `is_integer_builtin`**：在 `crates/lang-codegen/src/ssa/unit_lower/type_plan.rs` 中直接复用 `super::scalar::is_integer_builtin`，删除文件内重复定义的 `const fn is_integer_builtin`。

### 2.3 门控仅测试使用的 `integer_range` 与 `unit_plan` 入口

- 在 `crates/lang-codegen/src/ssa/integer_range.rs` 中，为仅由 `#[cfg(test)] mod lower_frontend;` 调用的 `operation` 函数添加 `#[cfg(test)]` 门控（生产路径已由 `ssa::refine` 承载）。
- 在 `crates/lang-codegen/src/ssa/unit_plan.rs` 中，为仅由单元测试消费的 `plan_unit_instances`、`plan_unit_instances_with_limit`、`UnitInstancePlan::runtime_type_demand`、`Deref for UnitInstancePlan` 与 `IntoIterator for UnitInstancePlan` 添加 `#[cfg(test)]` 门控（生产路径统一通过 `plan_unit_instances_from_facts` 与 `into_parts` 消费）。

## 3. 验收标准

- [x] 删除 `statement.rs`、`rc.rs` 与 `checked.rs` 三个壳模块，完成重复辅助函数收敛及测试专用接口门控。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。
