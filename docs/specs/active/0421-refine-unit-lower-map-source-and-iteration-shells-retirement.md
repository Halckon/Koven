# SPEC-0421：`unit_lower` 可空证明/分支、`map_source` 交接、`verify/module` 及 `lower_frontend` 迭代/循环壳子模块退役

> **性质**：工程收口规范（E3 Phase 4 文件数量收敛第六批） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 下的 `unit_lower` 可空分支/证明、`map_source` 交接运行时、`verify/module` 模块验证及 `lower_frontend` 闭包/迭代/可空 `while` 壳子模块时 · **唯一真源**：本文件定义 SPEC-0421 的合并范围、零语义漂移约束与验收条件

| 字段 | 内容 |
| --- | --- |
| 编号 | SPEC-0421 |
| 标题 | `unit_lower` 可空证明/分支、`map_source` 交接、`verify/module` 及 `lower_frontend` 迭代/循环壳子模块退役 |
| 状态 | in-progress |
| 所属阶段 | M4 Phase 4（SSA 代码生成与 E3 收口） |
| 依赖 | SPEC-0416, SPEC-0417, SPEC-0418, SPEC-0419, SPEC-0420 |

## 1. 背景与目标

紧接 [SPEC-0420](0420-refine-support-verify-receipt-and-drops-shells-retirement.md)，`crates/lang-codegen/src/ssa` 下仍有一批薄壳子模块可内联收敛至宿主或相邻兄弟模块，且合并后全部文件均保持在 1000 行阈值以内：

1. `unit_lower/control/nullable_if.rs`（239 行）-> 并入 `unit_lower/control/conditional.rs`（525 行）；
2. `unit_lower/nullable_proof.rs`（278 行）-> 并入 `unit_lower/construction.rs`（568 行）；
3. `lower_frontend/map_iteration/conditional/retained.rs`（265 行）-> 并入 `lower_frontend/map_iteration/conditional.rs`（483 行）；
4. `lower_frontend/source_closure/formation.rs`（289 行）-> 并入 `lower_frontend/source_closure.rs`（723 行）；
5. `map_source/handoff_runtime.rs`（240 行）-> 并入 `map_source.rs`（646 行）；
6. `lower_frontend/nullable_borrowed_cleanup/runtime.rs`（267 行）-> 并入 `lower_frontend/nullable_borrowed_cleanup.rs`（694 行）；
7. `lower_frontend/loop_control/nullable_while/entry.rs`（279 行）-> 并入 `lower_frontend/loop_control/nullable_while.rs`（635 行）；
8. `verify/module.rs`（184 行）-> 并入 `verify_types.rs`（760 行）。

本规范在不改变任何外部 API、诊断合同、SSA 验证语义及 `lowering_dependency_tests.rs` 架构护栏的前提下，一次性退役上述 8 个薄壳文件。

## 2. 退役与合并清单

| 退役文件 | 并入目标文件 | 处理方式 |
| --- | --- | --- |
| `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if.rs` | `crates/lang-codegen/src/ssa/unit_lower/control/conditional.rs` | 内联 `pub(in crate::ssa::unit_lower) mod nullable_if` 子模块，保留 `nullable_if/field_tests.rs` 与 `nullable_if/heap.rs` 路径映射，并在 `unit_lower/control.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/unit_lower/nullable_proof.rs` | `crates/lang-codegen/src/ssa/unit_lower/construction.rs` | 内联 `pub(super) mod nullable_proof` 子模块，并在 `unit_lower.rs` 中通过 `use construction::nullable_proof;` 维持模块路径不变 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/conditional/retained.rs` | `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/conditional.rs` | 内联 `pub(super) mod retained` 子模块，移除独立文件 |
| `crates/lang-codegen/src/ssa/lower_frontend/source_closure/formation.rs` | `crates/lang-codegen/src/ssa/lower_frontend/source_closure.rs` | 内联 `ExpressionLowerer` 闭包构造与捕获视图绑定实现，移除 `mod formation;` |
| `crates/lang-codegen/src/ssa/map_source/handoff_runtime.rs` | `crates/lang-codegen/src/ssa/map_source.rs` | 内联 `pub(super) mod handoff_runtime` 子模块，移除独立文件 |
| `crates/lang-codegen/src/ssa/lower_frontend/nullable_borrowed_cleanup/runtime.rs` | `crates/lang-codegen/src/ssa/lower_frontend/nullable_borrowed_cleanup.rs` | 内联 `mod runtime` 子模块并保留 `#[path = "nullable_borrowed_cleanup/tests.rs"]` 测试挂载 |
| `crates/lang-codegen/src/ssa/lower_frontend/loop_control/nullable_while/entry.rs` | `crates/lang-codegen/src/ssa/lower_frontend/loop_control/nullable_while.rs` | 内联 `mod entry` 子模块，移除独立文件 |
| `crates/lang-codegen/src/ssa/verify/module.rs` | `crates/lang-codegen/src/ssa/verify_types.rs` | 内联 `pub(in crate::ssa) mod module` 子模块，并在 `verify.rs` 中通过 `use super::verify_types::module;` 重导出 |

## 3. 验收条件

1. 上述 8 个壳文件已从版本库移除，合并后的宿主文件均严格小于 1000 行（通过 `python3 scripts/check_rust_sizes.py --base main`）。
2. `crates/lang-codegen/src/ssa/lowering_dependency_tests.rs` 中的所有存在性与依赖边界断言保持原样通过。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 检查通过。
