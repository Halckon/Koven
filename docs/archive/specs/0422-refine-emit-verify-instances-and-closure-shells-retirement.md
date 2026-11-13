# SPEC-0422：`refine/emit/statements`、`verify_operation`、`verify_ownership/provider_lifetime`、`unit_lower/closure/layout` 及 `lower_frontend` 实例/接收者壳子模块退役

> **性质**：工程收口规范（E3 Phase 4 文件数量收敛第七批） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 下的 `refine/emit/statements`、`single_readonly_method_boundaries`、`unit_lower/closure/layout`、`verify_operation/{map, callable}`、`verify_ownership/provider_lifetime` 及 `lower_frontend/{instances/callable_planner, readonly_receiver}` 壳子模块时 · **唯一真源**：本文件定义 SPEC-0422 的合并范围、零语义漂移约束与验收条件

| 字段 | 内容 |
| --- | --- |
| 编号 | SPEC-0422 |
| 标题 | `refine/emit/statements`、`verify_operation`、`verify_ownership/provider_lifetime`、`unit_lower/closure/layout` 及 `lower_frontend` 实例/接收者壳子模块退役 |
| 状态 | done |
| 所属阶段 | M4 Phase 4（SSA 代码生成与 E3 收口） |
| 依赖 | SPEC-0416, SPEC-0417, SPEC-0418, SPEC-0419, SPEC-0420, SPEC-0421 |

## 1. 背景与目标

紧接 [SPEC-0421](0421-refine-unit-lower-map-source-and-iteration-shells-retirement.md)，`crates/lang-codegen/src/ssa` 下仍有一批薄壳子模块可内联收敛至宿主或同层兄弟模块，且合并后全部非豁免文件均保持在 1000 行阈值以内：

1. `refine/emit/statements.rs`（150 行）-> 并入 `refine.rs`（635 行）；
2. `single_readonly_method_boundaries.rs`（153 行）-> 并入 `single_readonly_method_tests.rs`（645 行）；
3. `unit_lower/closure/layout.rs`（402 行）-> 并入 `unit_lower/closure.rs`（663 行）；
4. `verify_operation/map.rs`（281 行）-> 并入 `verify_operation/integer.rs`（550 行）；
5. `verify_operation/callable.rs`（293 行）-> 并入 `verify_operation/range.rs`（581 行）；
6. `lower_frontend/instances/callable_planner.rs`（304 行）与 `lower_frontend/instances/callable_planner/origins.rs`（350 行）-> 一并并入 `lower_frontend/instances.rs`（916 行）；
7. `verify_ownership/provider_lifetime.rs`（354 行）-> 并入 `verify_ownership/aliases.rs`（626 行）；
8. `lower_frontend/readonly_receiver.rs`（292 行）-> 并入 `lower_frontend/field_replace.rs`（572 行）。

本规范在不改变任何外部 API、诊断合同、SSA 验证语义及 `lowering_dependency_tests.rs` 架构护栏的前提下，一次性退役上述 9 个薄壳文件。

## 2. 退役与合并清单

| 退役文件 | 并入目标文件 | 处理方式 |
| --- | --- | --- |
| `crates/lang-codegen/src/ssa/refine/emit/statements.rs` | `crates/lang-codegen/src/ssa/refine.rs` | 内联 `pub(super) mod statements` 子模块，并在 `refine/emit.rs` 中引用 `super::statements` |
| `crates/lang-codegen/src/ssa/single_readonly_method_boundaries.rs` | `crates/lang-codegen/src/ssa/single_readonly_method_tests.rs` | 内联 `mod boundaries` 测试子模块，移除外部 `#[path]` 壳文件 |
| `crates/lang-codegen/src/ssa/unit_lower/closure/layout.rs` | `crates/lang-codegen/src/ssa/unit_lower/closure.rs` | 内联 `pub(super) mod layout` 子模块并保留 `layout_tests.rs` 挂载 |
| `crates/lang-codegen/src/ssa/verify_operation/map.rs` | `crates/lang-codegen/src/ssa/verify_operation/integer.rs` | 内联 `pub(in crate::ssa::verify_operation) mod map` 子模块，并在 `verify_operation.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/verify_operation/callable.rs` | `crates/lang-codegen/src/ssa/verify_operation/range.rs` | 内联 `pub(in crate::ssa) mod callable` 子模块，并在 `verify_operation.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/instances/callable_planner.rs` | `crates/lang-codegen/src/ssa/lower_frontend/instances.rs` | 内联 `mod callable_planner` 子模块，移除独立文件 |
| `crates/lang-codegen/src/ssa/lower_frontend/instances/callable_planner/origins.rs` | `crates/lang-codegen/src/ssa/lower_frontend/instances.rs` | 合入 `mod callable_planner` 中的 `Planner` 实现块，移除独立文件 |
| `crates/lang-codegen/src/ssa/verify_ownership/provider_lifetime.rs` | `crates/lang-codegen/src/ssa/verify_ownership/aliases.rs` | 内联 `pub(in crate::ssa::verify_ownership) mod provider_lifetime` 子模块，并在 `verify_ownership.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/readonly_receiver.rs` | `crates/lang-codegen/src/ssa/lower_frontend/field_replace.rs` | 内联 `pub(in crate::ssa::lower_frontend) mod readonly_receiver` 子模块，并在 `lower_frontend.rs` 中重导出 |

## 3. 验收条件

1. 上述 9 个壳文件已从版本库移除，合并后的宿主文件均严格小于 1000 行（通过 `python3 scripts/check_rust_sizes.py --base main`）。
2. `crates/lang-codegen/src/ssa/lowering_dependency_tests.rs` 中的所有存在性与依赖边界断言保持原样通过。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 检查通过。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 9 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：9/9 个路径不存在 |
| 内容并入声明的合并目标 | 部分目标已被后续 Spec 再次合并 | 5/9 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 23 个 Spec 已 squash 为 `430a0de2`，无法从历史分离本 Spec 的计数，不虚报该数字；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 引用全量结果（默认 3055 / feature 3117），本 Spec 定向 ``cargo test -p lang-codegen --lib -- lower_frontend refine verify_ownership map_`（0411–0427 合并运行）` |
| 退役提交已在基线中 | 通过 | e9f73142 refactor(codegen): retire refine/emit/statements, verify_operation, provider_lifetime, closure/layout and lower_frontend shells (SPEC-0422)（其余 7 个删除提交同批） |

宿主核对说明：

- `crates/lang-codegen/src/ssa/verify_operation/integer.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/verify_ownership/root_exchange.rs`（`623` 行）
- `crates/lang-codegen/src/ssa/verify_operation/range.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/borrow_result_support.rs`（`696` 行）
- `crates/lang-codegen/src/ssa/lower_frontend/instances.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/unit_plan_tests/callable_identity.rs`（`186` 行）
- `crates/lang-codegen/src/ssa/lower_frontend/instances.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/multi_temporary_loan_tests/instances.rs`（`551` 行）
