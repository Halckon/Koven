# SPEC-0450: 下一层叶子薄壳与同级测试小文件批量收敛（62 个）

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0450 |
| 标题 | 下一层叶子薄壳与同级测试小文件批量收敛（62 个） |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0448, SPEC-0449 |

---

## 1. 背景与动机

在 [SPEC-0448](0448-map-provider-exits-pipeline-leaf-tests-batch-retirement.md) 与 [SPEC-0449](0449-remaining-test-domains-leaf-shells-batch-retirement.md) 完成 174 个第一层与第二层叶子薄壳的直接父模块内联后，`crates/lang-codegen/src/ssa/` 下暴露出一批新的二级叶子宿主（如 `borrowed.rs`、`resource_forms.rs`、`nullable_map_consume_fixture_tests.rs`、`abort_owner_assertions.rs` 等）以及直接父模块行高已接近 985 行但同级兄弟宿主模块仍具充足行宽余量的 62 个纯叶子测试小文件。本 Spec 通过同级宿主内联与原父模块 `pub(in crate::ssa) use` 重导出一次性将其全部收敛。

---

## 2. 设计与收敛方案

### 2.1 收敛后父模块与同级宿主行高

| 模块路径 | 收敛后行高 |
|---|---:|
| `ssa/bitwise_lowering_tests.rs` | 732 |
| `ssa/lower_frontend_borrow_unit_comparison_tests.rs` | 733 |
| `ssa/lower_frontend_map_operand_last_use_tests.rs` | 961 |
| `ssa/lower_frontend_map_return_tests.rs` | 724 |
| `ssa/lower_frontend_map_tests.rs` | 531 |
| `ssa/lower_frontend_resource_deinit_tests.rs` | 940 |
| `ssa/lower_frontend_resource_deinit_tests/direct_closure.rs` | 479 |
| `ssa/lower_frontend_tests.rs` | 980 |
| `ssa/lowering_entry_contract_tests.rs` | 989 |
| `ssa/lowering_entry_contract_tests/carry_join.rs` | 814 |
| `ssa/map_consume_lowering_tests.rs` | 967 |
| `ssa/map_provider_exits_pipeline_tests.rs` | 950 |
| `ssa/map_provider_exits_pipeline_tests/borrowed.rs` | 786 |
| `ssa/map_provider_exits_pipeline_tests/conditional.rs` | 739 |
| `ssa/map_provider_exits_pipeline_tests/conditional/query_cleanup.rs` | 808 |
| `ssa/map_provider_exits_pipeline_tests/local_future.rs` | 738 |
| `ssa/map_provider_exits_pipeline_tests/local_resource_future/graph.rs` | 676 |
| `ssa/map_provider_exits_pipeline_tests/local_resource_future/graph/provider.rs` | 794 |
| `ssa/map_provider_exits_pipeline_tests/local_sources/deep.rs` | 995 |
| `ssa/map_provider_exits_pipeline_tests/local_sources/deep/ssa.rs` | 666 |
| `ssa/map_provider_exits_pipeline_tests/nested.rs` | 753 |
| `ssa/map_provider_exits_pipeline_tests/nested/query_ssa.rs` | 892 |
| `ssa/map_provider_exits_pipeline_tests/nested_exits.rs` | 900 |
| `ssa/map_provider_exits_pipeline_tests/nested_exits/query_body.rs` | 760 |
| `ssa/map_provider_exits_pipeline_tests/nested_exits/query_inputs.rs` | 679 |
| `ssa/map_provider_exits_pipeline_tests/resource.rs` | 787 |
| `ssa/map_provider_exits_pipeline_tests/resource_forms.rs` | 884 |
| `ssa/map_provider_exits_pipeline_tests/scope_lowering.rs` | 822 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births.rs` | 869 |
| `ssa/map_provider_exits_pipeline_tests/temporary.rs` | 858 |
| `ssa/map_provider_exits_pipeline_tests/temporary/query_ssa.rs` | 914 |
| `ssa/map_provider_exits_pipeline_tests/temporary_argument_cfg_query.rs` | 870 |
| `ssa/map_provider_exits_pipeline_tests/temporary_argument_cfg_query/provider.rs` | 853 |
| `ssa/map_provider_exits_pipeline_tests/temporary_exits.rs` | 778 |
| `ssa/map_provider_exits_pipeline_tests/temporary_exits/calls_unit.rs` | 757 |
| `ssa/map_provider_pipeline_tests.rs` | 812 |
| `ssa/map_provider_pipeline_tests/facts.rs` | 955 |
| `ssa/mod.rs` | 969 |
| `ssa/mutable_list_delivery_tests.rs` | 883 |
| `ssa/nullable_map_consume_fixture_tests.rs` | 679 |
| `ssa/nullable_map_consume_lowering_tests.rs` | 939 |
| `ssa/numeric_iteration_tests.rs` | 799 |
| `ssa/owned_map_borrow_tests.rs` | 756 |
| `ssa/rc_nullable_while_bounded_tests.rs` | 867 |
| `ssa/rc_nullable_while_observation_tests.rs` | 931 |
| `ssa/refine_subset_tests.rs` | 989 |
| `ssa/refine_subset_tests/temporary_sequential_for.rs` | 359 |
| `ssa/refine_tests.rs` | 984 |
| `ssa/refine_tests/abort_owner_assertions.rs` | 428 |
| `ssa/sequential_for_lowering_tests.rs` | 799 |
| `ssa/sequential_for_lowering_tests/determinism_tests.rs` | 786 |
| `ssa/text_extension_tests.rs` | 927 |
| `ssa/type_tests.rs` | 865 |
| `ssa/unit_field_borrow_tests.rs` | 847 |
| `ssa/unit_lower_assignment_tests.rs` | 713 |
| `ssa/unit_lower_closure_tests.rs` | 998 |
| `ssa/unit_lower_closure_tests/direct_prefix.rs` | 422 |
| `ssa/unit_lower_string_tests.rs` | 789 |
| `ssa/unit_plan_tests.rs` | 818 |
| `ssa/unit_plan_tests/generic_containers.rs` | 978 |
| `ssa/unit_plan_tests/layout_demand.rs` | 806 |
| `ssa/while_flow_tests.rs` | 781 |
| `ssa/while_flow_tests/borrowed_proof_cases.rs` | 901 |
| `ssa/while_flow_tests/distinct_nested_contract.rs` | 894 |
| `ssa/while_flow_tests/nullable_field_contract.rs` | 851 |
| `ssa/while_flow_tests/proof_cases.rs` | 967 |

### 2.2 退役纯叶子测试薄壳清单（62 个）

- `ssa/borrow_call_lowering_tests.rs`
- `ssa/borrow_value_field_tests.rs`
- `ssa/char_constant_tests.rs`
- `ssa/checked_integer_conversion_backend_tests.rs`
- `ssa/lower_frontend_map_operand_structure_tests.rs`
- `ssa/lower_frontend_map_return_matrix_tests.rs`
- `ssa/lower_frontend_nullable_unit_comparison_tests.rs`
- `ssa/lower_frontend_rc_unit_comparison_tests.rs`
- `ssa/lower_frontend_resource_deinit_tests/field_chain.rs`
- `ssa/lower_frontend_return_tests.rs`
- `ssa/lower_frontend_string_io_unit_comparison_tests.rs`
- `ssa/lowering_entry_contract_tests/receiver.rs`
- `ssa/map_provider_exits_pipeline_tests/ci_contracts.rs`
- `ssa/map_provider_exits_pipeline_tests/conditional/unit.rs`
- `ssa/map_provider_exits_pipeline_tests/formation_runtime.rs`
- `ssa/map_provider_exits_pipeline_tests/history.rs`
- `ssa/map_provider_exits_pipeline_tests/identity.rs`
- `ssa/map_provider_exits_pipeline_tests/local_resource_future/graph/continuation.rs`
- `ssa/map_provider_exits_pipeline_tests/local_sources/deep/unit.rs`
- `ssa/map_provider_exits_pipeline_tests/nested/query_body.rs`
- `ssa/map_provider_exits_pipeline_tests/nested/query_callers.rs`
- `ssa/map_provider_exits_pipeline_tests/nested_exits/query_resources.rs`
- `ssa/map_provider_exits_pipeline_tests/nested_exits/query_ssa.rs`
- `ssa/map_provider_exits_pipeline_tests/pipeline.rs`
- `ssa/map_provider_exits_pipeline_tests/query_cleanup.rs`
- `ssa/map_provider_exits_pipeline_tests/query_graph.rs`
- `ssa/map_provider_exits_pipeline_tests/query_inputs.rs`
- `ssa/map_provider_exits_pipeline_tests/query_map_normal_body.rs`
- `ssa/map_provider_exits_pipeline_tests/query_owner.rs`
- `ssa/map_provider_exits_pipeline_tests/query_temporary_cleanup.rs`
- `ssa/map_provider_exits_pipeline_tests/temporary/ast.rs`
- `ssa/map_provider_exits_pipeline_tests/temporary_argument_cfg_query/argument.rs`
- `ssa/map_provider_exits_pipeline_tests/temporary_exits/query_body.rs`
- `ssa/map_provider_pipeline_tests/query_inputs.rs`
- `ssa/map_provider_pipeline_tests/query_owner.rs`
- `ssa/multi_borrow_tests.rs`
- `ssa/nested_borrow_tests.rs`
- `ssa/refine_string_when_tests.rs`
- `ssa/refine_subset_tests/control_completion.rs`
- `ssa/refine_tests/operand_exit_tests.rs`
- `ssa/sequential_for_lowering_tests/unit_storage_tests.rs`
- `ssa/shared_owner_operation_tests.rs`
- `ssa/unit_generic_container_tests.rs`
- `ssa/unit_lower_borrow_string_tests.rs`
- `ssa/unit_lower_closure_tests/borrow_storage.rs`
- `ssa/unit_lower_closure_tests/shared_capture.rs`
- `ssa/unit_lower_container_element_tests.rs`
- `ssa/unit_lower_container_size_tests.rs`
- `ssa/unit_lower_short_circuit_tests.rs`
- `ssa/unit_lower_straight_line_tests.rs`
- `ssa/unit_lower_when_tests.rs`
- `ssa/unit_map_tests.rs`
- `ssa/unit_plan_tests/error_order.rs`
- `ssa/unit_plan_tests/recipe_cycles.rs`
- `ssa/unit_root_primitive_tests.rs`
- `ssa/while_flow_tests/assignment_contract.rs`
- `ssa/while_flow_tests/direct_contract.rs`
- `ssa/while_flow_tests/owned_field_contract.rs`
- `ssa/while_flow_tests/readonly_nested_contract.rs`
- `ssa/while_flow_tests/rejection_diagnostics.rs`
- `ssa/while_flow_tests/value_argument_contract.rs`
- `ssa/while_flow_tests/written_nested_contract.rs`

---

## 3. 验收标准

1. 上述 62 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `1895` 降至 `1833`。
