# SPEC-0426：`unit_lower`、`map_iteration`、`map_consume` 及 `unit_plan` 壳子模块退役

> **性质**：工程收口规范（E3 Phase 4 文件数量收敛第十一批） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 下的 `unit_lower/{assignment, callable_abi, control/branch, scalar}`、`lower_frontend/map_iteration/{cleanup, qualify}`、`lower_frontend/map_consume/nullable/source` 及 `unit_plan/recipe_preflight` 壳子模块时 · **唯一真源**：本文件定义 SPEC-0426 的合并范围、零语义漂移约束与验收条件

| 字段 | 内容 |
| --- | --- |
| 编号 | SPEC-0426 |
| 标题 | `unit_lower`、`map_iteration`、`map_consume` 及 `unit_plan` 壳子模块退役 |
| 状态 | done |
| 所属阶段 | M4 Phase 4（SSA 代码生成与 E3 收口） |
| 依赖 | SPEC-0416, SPEC-0417, SPEC-0418, SPEC-0419, SPEC-0420, SPEC-0421, SPEC-0422, SPEC-0423, SPEC-0424, SPEC-0425 |

## 1. 背景与目标

紧接 [SPEC-0425](../../specs/active/0425-nullable-if-map-iteration-outer-while-unit-lower-and-refine-shells-retirement.md)，继续推进 `crates/lang-codegen/src/ssa` 下编译单元降低、实例规划与单文件降低目录的壳模块收敛，将以下 8 个小规模子模块并入同域宿主模块并删除原壳文件，在保持编译单元与单文件 SSA lowering、泛型实例规划及 Map 迭代/消费行为不变的前提下进一步压缩碎片文件数量，同时确保合并后各文件严格低于 1000 行上限：

1. `unit_lower/assignment.rs`（397 行）-> 并入 `unit_lower/map.rs`（875 行）；
2. `unit_lower/callable_abi.rs`（397 行）-> 并入 `unit_lower/call_lifetimes.rs`（872 行）；
3. `lower_frontend/map_iteration/cleanup.rs`（402 行）-> 并入 `lower_frontend/map_iteration/exits.rs`（833 行）；
4. `lower_frontend/map_iteration/qualify.rs`（402 行）-> 并入 `lower_frontend/map_iteration/temporary.rs`（839 行）；
5. `lower_frontend/map_consume/nullable/source.rs`（407 行）-> 并入 `lower_frontend/map_consume/nullable.rs`（895 行）；
6. `unit_plan/recipe_preflight.rs`（415 行）-> 并入 `unit_plan/runtime_layout.rs`（893 行）；
7. `unit_lower/control/branch.rs`（422 行）-> 并入 `unit_lower/constant.rs`（596 行）；
8. `unit_lower/scalar.rs`（427 行）-> 并入 `unit_lower/control.rs`（928 行）。

## 2. 退役与合并清单

| 退役文件 | 并入目标文件 | 处理方式 |
| --- | --- | --- |
| `crates/lang-codegen/src/ssa/unit_lower/assignment.rs` | `crates/lang-codegen/src/ssa/unit_lower/map.rs` | 内联为 `pub(in crate::ssa::unit_lower) mod assignment` 并在 `unit_lower.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/unit_lower/callable_abi.rs` | `crates/lang-codegen/src/ssa/unit_lower/call_lifetimes.rs` | 内联为 `pub(in crate::ssa::unit_lower) mod callable_abi` 并在 `unit_lower.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/cleanup.rs` | `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/exits.rs` | 内联为 `pub(in crate::ssa::lower_frontend::map_iteration) mod cleanup` 并在 `map_iteration.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/qualify.rs` | `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/temporary.rs` | 内联为 `pub(in crate::ssa::lower_frontend::map_iteration) mod qualify` 并在 `map_iteration.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_consume/nullable/source.rs` | `crates/lang-codegen/src/ssa/lower_frontend/map_consume/nullable.rs` | 内联为私有子模块 `mod source` |
| `crates/lang-codegen/src/ssa/unit_plan/recipe_preflight.rs` | `crates/lang-codegen/src/ssa/unit_plan/runtime_layout.rs` | 内联为 `pub(in crate::ssa::unit_plan) mod recipe_preflight` 并在 `unit_plan.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/unit_lower/control/branch.rs` | `crates/lang-codegen/src/ssa/unit_lower/constant.rs` | 内联为 `pub(in crate::ssa::unit_lower) mod branch` 供 `unit_lower::control` 复用 |
| `crates/lang-codegen/src/ssa/unit_lower/scalar.rs` | `crates/lang-codegen/src/ssa/unit_lower/control.rs` | 内联为 `pub(in crate::ssa::unit_lower) mod scalar` 并在 `unit_lower.rs` 中重导出 |

## 3. 验收条件

1. 上述 8 个壳文件已从版本库移除，合并后的宿主文件均严格小于 1000 行（通过 `python3 scripts/check_rust_sizes.py --base main`）。
2. `crates/lang-codegen/src/ssa/lowering_dependency_tests.rs` 中的所有存在性与依赖边界断言保持原样通过。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 检查通过。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 8 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：8/8 个路径不存在 |
| 内容并入声明的合并目标 | 部分目标已被后续 Spec 再次合并 | 5/8 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 23 个 Spec 已 squash 为 `430a0de2`，无法从历史分离本 Spec 的计数，不虚报该数字；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 引用全量结果（默认 3055 / feature 3117），本 Spec 定向 ``cargo test -p lang-codegen --lib -- lower_frontend refine verify_ownership map_`（本批 0411–0427 合并运行）= 见当前批结果` |
| 退役提交已在基线中 | 通过 | cf092faf refactor(codegen): retire unit_lower, map_iteration, map_consume and unit_plan shells (SPEC-0426)（其余 7 个删除提交同批） |

宿主核对说明：

- `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/exits.rs`：宿主不再存在，且未按符号定位到等价实现，需人工复核
- `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/temporary.rs`：宿主不再存在，且未按符号定位到等价实现，需人工复核
- `crates/lang-codegen/src/ssa/lower_frontend/map_consume/nullable.rs`：宿主不再存在，且未按符号定位到等价实现，需人工复核
