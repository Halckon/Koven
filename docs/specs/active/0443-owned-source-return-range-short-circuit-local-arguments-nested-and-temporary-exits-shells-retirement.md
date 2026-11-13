# SPEC-0443: `owned_source_return`、`range_lowering_tests`、`local_arguments`、`nested` 及 `temporary_exits` 子壳模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0443 |
| 标题 | `owned_source_return`、`range_lowering_tests`、`local_arguments`、`nested` 及 `temporary_exits` 子壳模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0442 |

---

## 1. 背景与动机

在 [SPEC-0442](../../archive/specs/0442-range-container-unit-plan-borrowed-generate-and-source-forms-shells-retirement.md) 完成 `range_lowering_tests`、`unit_lower_container_tests`、`unit_plan_tests`、`borrowed_generate_tests` 及 `source_forms` 子壳模块收敛后，`crates/lang-codegen/src/ssa/` 的测试子目录下仍残留一批可直接内联回直接父模块的薄壳文件：

1. `ssa/lower_frontend_owned_source_return_structure_tests.rs`（338 行）：直接内联回 `ssa/lower_frontend_owned_source_return_tests.rs`。
2. `ssa/lower_frontend_owned_source_return_boundary_tests.rs`（264 行）：直接内联回 `ssa/lower_frontend_owned_source_return_tests.rs`。
3. `ssa/range_lowering_tests/local_comparison_selection.rs`（325 行）：直接内联回 `ssa/range_lowering_tests/local_comparisons.rs`。
4. `ssa/range_lowering_tests/local_short_circuit_receiver.rs`（82 行）：直接内联回 `ssa/range_lowering_tests/local_short_circuit.rs`。
5. `ssa/map_provider_exits_pipeline_tests/local_arguments/generic_graph.rs`（275 行）：直接内联回 `ssa/map_provider_exits_pipeline_tests/local_arguments/generic.rs`。
6. `ssa/map_provider_exits_pipeline_tests/local_arguments/wrapper.rs`（294 行）：直接内联回 `ssa/map_provider_exits_pipeline_tests/local_arguments/generic.rs`。
7. `ssa/map_provider_exits_pipeline_tests/nested/query_inputs_unit.rs`（611 行）：直接内联回 `ssa/map_provider_exits_pipeline_tests/nested/query_inputs.rs`。
8. `ssa/map_provider_exits_pipeline_tests/temporary_exits/query_guards.rs`（106 行）：直接内联回 `ssa/map_provider_exits_pipeline_tests/temporary_exits/query_inputs.rs`。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/lower_frontend_owned_source_return_structure_tests.rs` | 338 | `ssa/lower_frontend_owned_source_return_tests.rs` | 708 |
| `ssa/lower_frontend_owned_source_return_boundary_tests.rs` | 264 | `ssa/lower_frontend_owned_source_return_tests.rs` | 972 |
| `ssa/range_lowering_tests/local_comparison_selection.rs` | 325 | `ssa/range_lowering_tests/local_comparisons.rs` | 937 |
| `ssa/range_lowering_tests/local_short_circuit_receiver.rs` | 82 | `ssa/range_lowering_tests/local_short_circuit.rs` | 583 |
| `ssa/map_provider_exits_pipeline_tests/local_arguments/generic_graph.rs` | 275 | `ssa/map_provider_exits_pipeline_tests/local_arguments/generic.rs` | 573 |
| `ssa/map_provider_exits_pipeline_tests/local_arguments/wrapper.rs` | 294 | `ssa/map_provider_exits_pipeline_tests/local_arguments/generic.rs` | 867 |
| `ssa/map_provider_exits_pipeline_tests/nested/query_inputs_unit.rs` | 611 | `ssa/map_provider_exits_pipeline_tests/nested/query_inputs.rs` | 784 |
| `ssa/map_provider_exits_pipeline_tests/temporary_exits/query_guards.rs` | 106 | `ssa/map_provider_exits_pipeline_tests/temporary_exits/query_inputs.rs` | 368 |

---

## 3. 验收标准

1. 上述 8 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `2109` 降至 `2101`。
