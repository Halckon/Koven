# SPEC-0441: `container_lowering_tests` 三子壳及剩余 `lower_frontend` 支撑/清理壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0441 |
| 标题 | `container_lowering_tests` 三子壳及剩余 `lower_frontend` 支撑/清理壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0440 |

---

## 1. 背景与动机

在 [SPEC-0440](0440-rc-nullable-while-queries-same-guard-loop-control-and-instances-shells-retirement.md) 完成 `rc_nullable_while_identity_queries`、`same_guard`、`map_loop/operand_unit_identity` 及 `loop_control`/`instances` 壳子模块收敛后，`crates/lang-codegen/src/ssa/` 下仍残留一批 `lower_frontend` 测试对比支撑壳与 `container_lowering_tests` 目录下的 3 个薄壳文件：

1. `ssa/container_lowering_tests/runtime_helpers.rs`（73 行）：仅向父测试模块提供辅助函数，可内联回 `ssa/container_lowering_tests.rs`。
2. `ssa/container_lowering_tests/runtime_matrix.rs`（58 行）：仅向父测试模块提供矩阵单测，可内联回 `ssa/container_lowering_tests.rs`。
3. `ssa/container_lowering_tests/runtime_matrix_support.rs`（112 行）：仅向父测试模块提供矩阵用例表，可内联回 `ssa/container_lowering_tests.rs`。
4. `ssa/lower_frontend_map_nullable_operand_unit_test_support.rs`（811 行）：可空操作数 Unit 测试支撑，可收敛入 `ssa/unit_constant_tests/control_completion.rs`。
5. `ssa/lower_frontend_map_loop_unit_contract.rs`（928 行）：Map 循环 Unit 合同检验，可收敛入 `ssa/nullable_map_consume_lowering_tests/single.rs`。
6. `ssa/lower_frontend/map_iteration/conditional.rs`（961 行）：测试对比用条件 Map 迭代模块，可收敛入 `ssa/map_provider_exits_pipeline_tests/nested_exits/query_cleanup.rs`。
7. `ssa/lower_frontend/nullable_numeric_cleanup.rs`（970 行）：测试对比用可空数值清理模块，可收敛入 `ssa/map_provider_exits_pipeline_tests/temporary_exits/query_cleanup.rs`。
8. `ssa/lower_frontend/orchestrate/nullable_map_guard_support.rs`（986 行）：可空 Map 守卫测试支撑，可收敛入 `ssa/map_provider_exits_pipeline_tests/local_sources/deep.rs`。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/container_lowering_tests/runtime_helpers.rs` | 73 | `ssa/container_lowering_tests.rs` | 609 |
| `ssa/container_lowering_tests/runtime_matrix.rs` | 58 | `ssa/container_lowering_tests.rs` | 667 |
| `ssa/container_lowering_tests/runtime_matrix_support.rs` | 112 | `ssa/container_lowering_tests.rs` | 779 |
| `ssa/lower_frontend_map_nullable_operand_unit_test_support.rs` | 811 | `ssa/unit_constant_tests/control_completion.rs` | 836 |
| `ssa/lower_frontend_map_loop_unit_contract.rs` | 928 | `ssa/nullable_map_consume_lowering_tests/single.rs` | 945 |
| `ssa/lower_frontend/map_iteration/conditional.rs` | 961 | `ssa/map_provider_exits_pipeline_tests/nested_exits/query_cleanup.rs` | 969 |
| `ssa/lower_frontend/nullable_numeric_cleanup.rs` | 970 | `ssa/map_provider_exits_pipeline_tests/temporary_exits/query_cleanup.rs` | 978 |
| `ssa/lower_frontend/orchestrate/nullable_map_guard_support.rs` | 986 | `ssa/map_provider_exits_pipeline_tests/local_sources/deep.rs` | 995 |

---

## 3. 验收标准

1. 上述 8 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `2125` 降至 `2117`。
