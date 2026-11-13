# SPEC-0449: 其余测试域叶子薄壳子模块批量退役（42 个）

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0449 |
| 标题 | 其余测试域叶子薄壳子模块批量退役（42 个） |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0448 |

---

## 1. 背景与动机

在 [SPEC-0448](../../archive/specs/0448-map-provider-exits-pipeline-leaf-tests-batch-retirement.md) 完成 `map_provider_exits_pipeline_tests` 域 132 个叶子薄壳批量收敛后，`crates/lang-codegen/src/ssa/` 其余测试域（`map_iteration_stage_gate_tests`、`map_provider_pipeline_tests`、`nullable_map_consume_lowering_tests`、`while_flow_tests`、`unit_plan_tests`、`closure_operation_tests`、`numeric_iteration_tests`、`string_operation_tests`、`integer_range_value_tests`、`lower_frontend_tests`、`lower_frontend_map_loop_tests` 等）仍残留 42 个可直接内联回直接父模块的叶子薄壳文件（各宿主合并后均严格低于 985 行）。本 Spec 一次性将其全部收敛回各自父模块。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/closure_operation_tests/escape_tests/shared_retain_tests.rs` | 234 | `ssa/closure_operation_tests/escape_tests.rs` | 780 |
| `ssa/closure_operation_tests/escape_tests/root_content_tests/capture_transport_tests.rs` | 393 | `ssa/closure_operation_tests/escape_tests/root_content_tests.rs` | 618 |
| `ssa/integer_range_value_tests/llvm_contract.rs` | 85 | `ssa/integer_range_value_tests.rs` | 678 |
| `ssa/lower_frontend_map_unit_iteration_tests.rs` | 290 | `ssa/lower_frontend_map_loop_tests.rs` | 532 |
| `ssa/lower_frontend_diagnostic_tests.rs` | 262 | `ssa/lower_frontend_tests.rs` | 981 |
| `ssa/map_iteration_stage_gate_tests/roots.rs` | 187 | `ssa/map_iteration_stage_gate_tests.rs` | 366 |
| `ssa/map_iteration_stage_gate_tests/unit.rs` | 243 | `ssa/map_iteration_stage_gate_tests.rs` | 610 |
| `ssa/map_iteration_stage_gate_tests/facts.rs` | 259 | `ssa/map_iteration_stage_gate_tests.rs` | 870 |
| `ssa/map_iteration_stage_gate_tests/group/fixture.rs` | 25 | `ssa/map_iteration_stage_gate_tests/group.rs` | 145 |
| `ssa/map_iteration_stage_gate_tests/group/ast.rs` | 80 | `ssa/map_iteration_stage_gate_tests/group.rs` | 226 |
| `ssa/map_iteration_stage_gate_tests/group/cleanup.rs` | 120 | `ssa/map_iteration_stage_gate_tests/group.rs` | 347 |
| `ssa/map_iteration_stage_gate_tests/group/unit.rs` | 273 | `ssa/map_iteration_stage_gate_tests/group.rs` | 621 |
| `ssa/map_iteration_stage_gate_tests/group/facts.rs` | 297 | `ssa/map_iteration_stage_gate_tests/group.rs` | 919 |
| `ssa/map_provider_exits_pipeline_tests/local_reassignment/facts.rs` | 345 | `ssa/map_provider_exits_pipeline_tests/local_reassignment.rs` | 845 |
| `ssa/map_provider_exits_pipeline_tests/local_resource_future/facts.rs` | 438 | `ssa/map_provider_exits_pipeline_tests/local_resource_future.rs` | 727 |
| `ssa/map_provider_exits_pipeline_tests/local_resources/facts.rs` | 461 | `ssa/map_provider_exits_pipeline_tests/local_resources.rs` | 828 |
| `ssa/map_provider_exits_pipeline_tests/scope_lambda_body/facts.rs` | 348 | `ssa/map_provider_exits_pipeline_tests/scope_lambda_body.rs` | 675 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/conditional_owner_observer/facts.rs` | 493 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/conditional_owner_observer.rs` | 862 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/int_storage/facts.rs` | 358 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/int_storage.rs` | 708 |
| `ssa/map_provider_exits_pipeline_tests/sequential/identity.rs` | 589 | `ssa/map_provider_exits_pipeline_tests/sequential.rs` | 805 |
| `ssa/map_provider_pipeline_tests/query_test.rs` | 122 | `ssa/map_provider_pipeline_tests.rs` | 257 |
| `ssa/map_provider_pipeline_tests/identity.rs` | 158 | `ssa/map_provider_pipeline_tests.rs` | 416 |
| `ssa/map_provider_pipeline_tests/fixture.rs` | 183 | `ssa/map_provider_pipeline_tests.rs` | 600 |
| `ssa/map_provider_pipeline_tests/pipeline.rs` | 211 | `ssa/map_provider_pipeline_tests.rs` | 812 |
| `ssa/nullable_map_consume_fixture_tests/inputs.rs` | 302 | `ssa/nullable_map_consume_fixture_tests.rs` | 361 |
| `ssa/nullable_map_consume_lowering_tests/read_cross_join.rs` | 38 | `ssa/nullable_map_consume_lowering_tests.rs` | 337 |
| `ssa/nullable_map_consume_lowering_tests/sources.rs` | 257 | `ssa/nullable_map_consume_lowering_tests.rs` | 595 |
| `ssa/nullable_map_consume_lowering_tests/consume_identity/llvm.rs` | 395 | `ssa/nullable_map_consume_lowering_tests/consume_identity.rs` | 969 |
| `ssa/nullable_map_consume_lowering_tests/consume_identity/unit/owner.rs` | 79 | `ssa/nullable_map_consume_lowering_tests/consume_identity/unit.rs` | 921 |
| `ssa/nullable_map_consume_lowering_tests/resource/sources.rs` | 128 | `ssa/nullable_map_consume_lowering_tests/resource.rs` | 432 |
| `ssa/nullable_map_consume_lowering_tests/resource/memory.rs` | 351 | `ssa/nullable_map_consume_lowering_tests/resource.rs` | 784 |
| `ssa/nullable_map_consume_lowering_tests/resource/contract/transfer.rs` | 340 | `ssa/nullable_map_consume_lowering_tests/resource/contract.rs` | 684 |
| `ssa/nullable_map_consume_lowering_tests/resource/formed_locals/query.rs` | 226 | `ssa/nullable_map_consume_lowering_tests/resource/formed_locals.rs` | 363 |
| `ssa/nullable_map_consume_lowering_tests/resource/formed_locals/inputs.rs` | 286 | `ssa/nullable_map_consume_lowering_tests/resource/formed_locals.rs` | 650 |
| `ssa/nullable_map_consume_lowering_tests/resource/formed_locals/sources.rs` | 324 | `ssa/nullable_map_consume_lowering_tests/resource/formed_locals.rs` | 975 |
| `ssa/nullable_map_consume_lowering_tests/unit/layout.rs` | 78 | `ssa/nullable_map_consume_lowering_tests/unit.rs` | 723 |
| `ssa/numeric_iteration_tests/cfg_contract.rs` | 376 | `ssa/numeric_iteration_tests.rs` | 473 |
| `ssa/string_operation_tests/text_reads.rs` | 295 | `ssa/string_operation_tests.rs` | 935 |
| `ssa/unit_plan_tests/canonical_callables.rs` | 106 | `ssa/unit_plan_tests.rs` | 251 |
| `ssa/unit_plan_tests/entry_identity.rs` | 133 | `ssa/unit_plan_tests.rs` | 385 |
| `ssa/unit_plan_tests/body_type_budget.rs` | 191 | `ssa/unit_plan_tests.rs` | 577 |
| `ssa/unit_plan_tests/instances.rs` | 240 | `ssa/unit_plan_tests.rs` | 818 |

---

## 3. 验收标准

1. 上述 42 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `1937` 降至 `1895`。
