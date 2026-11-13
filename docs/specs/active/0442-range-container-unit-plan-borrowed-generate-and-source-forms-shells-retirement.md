# SPEC-0442: `range_lowering_tests`、`unit_lower_container_tests`、`unit_plan_tests`、`borrowed_generate_tests` 及 `source_forms` 子壳模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0442 |
| 标题 | `range_lowering_tests`、`unit_lower_container_tests`、`unit_plan_tests`、`borrowed_generate_tests` 及 `source_forms` 子壳模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0441 |

---

## 1. 背景与动机

在 [SPEC-0441](../../archive/specs/0441-container-lowering-and-remaining-lower-frontend-shells-retirement.md) 完成 `container_lowering_tests` 三子壳及剩余 `lower_frontend` 支撑/清理壳子模块收敛后，`crates/lang-codegen/src/ssa/` 的测试子目录下仍残留一批可直接内联回直接父模块的薄壳文件：

1. `ssa/range_lowering_tests/drop_algorithms.rs`（56 行）：直接内联回 `ssa/range_lowering_tests.rs`。
2. `ssa/range_lowering_tests/temporary.rs`（125 行）：直接内联回 `ssa/range_lowering_tests.rs`。
3. `ssa/range_lowering_tests/call_prefix.rs`（226 行）：直接内联回 `ssa/range_lowering_tests.rs`。
4. `ssa/unit_lower_container_tests/runtime_helpers.rs`（271 行）：直接内联回 `ssa/unit_lower_container_tests.rs`。
5. `ssa/unit_lower_container_tests/runtime_matrix.rs`（67 行）：直接内联回 `ssa/unit_lower_container_tests.rs`。
6. `ssa/unit_plan_tests/factory_return_queries.rs`（106 行）：直接内联回 `ssa/unit_plan_tests/callable_identity.rs`。
7. `ssa/borrowed_generate_tests/root_storage_tests/helper_calls.rs`（152 行）：直接内联回 `ssa/borrowed_generate_tests/root_storage_tests.rs`。
8. `ssa/map_provider_exits_pipeline_tests/source_forms/entries.rs`（151 行）：直接内联回 `ssa/map_provider_exits_pipeline_tests/source_forms/graph.rs`。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/range_lowering_tests/drop_algorithms.rs` | 56 | `ssa/range_lowering_tests.rs` | 639 |
| `ssa/range_lowering_tests/temporary.rs` | 125 | `ssa/range_lowering_tests.rs` | 764 |
| `ssa/range_lowering_tests/call_prefix.rs` | 226 | `ssa/range_lowering_tests.rs` | 990 |
| `ssa/unit_lower_container_tests/runtime_helpers.rs` | 271 | `ssa/unit_lower_container_tests.rs` | 894 |
| `ssa/unit_lower_container_tests/runtime_matrix.rs` | 67 | `ssa/unit_lower_container_tests.rs` | 961 |
| `ssa/unit_plan_tests/factory_return_queries.rs` | 106 | `ssa/unit_plan_tests/callable_identity.rs` | 939 |
| `ssa/borrowed_generate_tests/root_storage_tests/helper_calls.rs` | 152 | `ssa/borrowed_generate_tests/root_storage_tests.rs` | 978 |
| `ssa/map_provider_exits_pipeline_tests/source_forms/entries.rs` | 151 | `ssa/map_provider_exits_pipeline_tests/source_forms/graph.rs` | 453 |

---

## 3. 验收标准

1. 上述 8 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `2117` 降至 `2109`。
