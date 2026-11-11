# SPEC-0405：`ssa` 死辅助入口、测试专用入口门控与跨模块 `Span` 辅助去重

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `ssa` 死辅助函数退役、测试专用 lowering 入口门控与跨模块 `Span` 包含判定去重时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0405` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认 E3 进入收尾阶段并继续推进 SPEC-0405 清理 |
| 基线 | [SPEC-0404](0404-refine-unit-lower-shell-modules-and-duplicate-helpers-retirement.md)（`d3537535`，含 [SPEC-0403](0403-refine-unit-lower-dead-fields-and-provider-retirement.md) `06dc4e80`） |
| 分支 | `feature/spec-0405-refine-dead-helpers-and-test-entry-gates-retirement` |
| 前置 | [SPEC-0383](0383-refine-final-legacy-retirement.md)、[SPEC-0402](0402-refine-unit-lower-shell-modules-and-iteration-state-retirement.md)、[SPEC-0403](0403-refine-unit-lower-dead-fields-and-provider-retirement.md)、[SPEC-0404](0404-refine-unit-lower-shell-modules-and-duplicate-helpers-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0384` 至 `SPEC-0404` 完成 `unit_lower` 各家族旧路径与单入口壳模块退役后，继续清理 `crates/lang-codegen/src/ssa` 中残留的死辅助函数，为仅由单元测试消费的编译单元带入口 lowering 包装与 `UnitCallablePlan` 查询入口补充 `#[cfg(test)]` 门控并移除 `unused_imports` 告警抑制，同时将 `span_contains` / `strictly_contains` 统一收敛到 `unit_lower` 并在 `unit_plan`、`refine/subset/callable` 与 `borrow_result_support` 之间复用。

## 2. 范围与设计

### 2.1 退役 `ssa` 残留死辅助函数

删除全仓库已无任何调用点的死辅助函数：

- `crates/lang-codegen/src/ssa/refine/build/borrow_binding.rs` 中的 `Builder::is_borrow_result_binding`
- `crates/lang-codegen/src/ssa/refine/subset/callable.rs` 中的 `Checker::argument_has_loop_jump`（生产准入已由 `argument_has_bare_loop_jump` 承载）
- `crates/lang-codegen/src/ssa/refine/subset/range_borrow.rs` 中的 `Checker::unit_requires_refine`
- `crates/lang-codegen/src/ssa/verify_operation.rs` 中的 `is_first_class_value`

### 2.2 门控仅测试使用的编译单元入口与 `UnitCallablePlan` 查询

- 在 `crates/lang-codegen/src/ssa/unit_lower/handoff.rs` 与 `crates/lang-codegen/src/ssa/unit_lower.rs` 中，为仅由测试调用的 `lower_scalar_unit_with_entry` 及其重导出添加 `#[cfg(test)]` 门控，并移除 `unit_lower.rs` 上的 `#[cfg_attr(not(test), expect(unused_imports, ...))]` 告警抑制（生产路径统一通过 `lower_scalar_unit_from_facts` 消费）。
- 在 `crates/lang-codegen/src/ssa/unit_lower/constant.rs` 中，为仅由测试调用的 `lower_constant_unit_with_entry` 及其测试专用导入添加 `#[cfg(test)]` 门控（生产路径统一通过 `lower_constant_unit_from_facts` 消费）。
- 在 `crates/lang-codegen/src/ssa/unit_plan/callable_instances.rs` 中，为仅由单元测试调用的 `UnitCallablePlan::borrowed_argument` 添加 `#[cfg(test)]` 门控（生产路径统一通过 `borrowed_arguments` 迭代器消费）。

### 2.3 跨 `ssa` 模块统一 `Span` 包含判定辅助函数

- 将 `crates/lang-codegen/src/ssa/unit_lower.rs` 中的 `span_contains` 与 `strictly_contains` 可见性提升为 `pub(in crate::ssa)`。
- 在 `crates/lang-codegen/src/ssa/unit_plan.rs`、`crates/lang-codegen/src/ssa/refine/subset/callable.rs` 与 `crates/lang-codegen/src/ssa/borrow_result_support.rs` 中直接复用 `unit_lower::{span_contains, strictly_contains}`，删除这三处重复定义的私有函数。

## 3. 验收标准

- [x] 删除 `is_borrow_result_binding`、`argument_has_loop_jump`、`unit_requires_refine` 与 `is_first_class_value` 四个死辅助函数。
- [x] 为 `lower_scalar_unit_with_entry`、`lower_constant_unit_with_entry` 与 `UnitCallablePlan::borrowed_argument` 补充 `#[cfg(test)]` 门控，移除 `unit_lower.rs` 的 `unused_imports` 抑制，并完成跨模块 `span_contains` / `strictly_contains` 去重。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。
