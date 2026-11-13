# SPEC-0445: `loan_region_ssa_tests` 下临时借用、绑定凭证、父子探针、多临时乘积及 `range_lowering_tests/view_source` 壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：done · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0445 |
| 标题 | `loan_region_ssa_tests` 下临时借用、绑定凭证、父子探针、多临时乘积及 `range_lowering_tests/view_source` 壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | done |
| 创建日期 | 2026-04-17 |
| 完成日期 | 2026-11-13 |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0444 |

---

## 1. 背景与动机

在 [SPEC-0444](../../archive/specs/0444-loan-observer-receiver-named-temporary-and-finite-literal-tests-shells-retirement.md) 完成 `loan_region_ssa_tests` 首批 8 个薄壳文件收敛后，同域及 `range_lowering_tests` 下仍残留一批 138–338 行的薄壳测试文件：

1. `ssa/temporary_loan_route_tests.rs`（138 行）：收敛入同域宿主 `ssa/parent_child_coverage_tests.rs`。
2. `ssa/temporary_loan_receipt_tests.rs`（190 行）：收敛入同域宿主 `ssa/parent_child_coverage_tests.rs`。
3. `ssa/binding_causal_links_tests.rs`（243 行）：收敛入同域宿主 `ssa/parent_child_coverage_tests.rs`。
4. `ssa/binding_coverage_tests.rs`（213 行）：收敛入同域宿主 `ssa/parent_child_causal_tests.rs`。
5. `ssa/binding_receipt_tests.rs`（256 行）：收敛入同域宿主 `ssa/parent_child_causal_tests.rs`。
6. `ssa/parent_child_lifecycle_probe_tests.rs`（176 行）：收敛入同域宿主 `ssa/parent_child_causal_tests.rs`。
7. `ssa/multi_temporary_loan_products_tests.rs`（338 行）：收敛入同域宿主 `ssa/parent_child_drop_dependency_tests.rs`。
8. `ssa/range_lowering_tests/view_source.rs`（298 行）：收敛入宿主 `ssa/parent_child_drop_dependency_tests.rs`。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/temporary_loan_route_tests.rs` | 138 | `ssa/parent_child_coverage_tests.rs` | 475 |
| `ssa/temporary_loan_receipt_tests.rs` | 190 | `ssa/parent_child_coverage_tests.rs` | 668 |
| `ssa/binding_causal_links_tests.rs` | 243 | `ssa/parent_child_coverage_tests.rs` | 914 |
| `ssa/binding_coverage_tests.rs` | 213 | `ssa/parent_child_causal_tests.rs` | 531 |
| `ssa/binding_receipt_tests.rs` | 256 | `ssa/parent_child_causal_tests.rs` | 790 |
| `ssa/parent_child_lifecycle_probe_tests.rs` | 176 | `ssa/parent_child_causal_tests.rs` | 969 |
| `ssa/multi_temporary_loan_products_tests.rs` | 338 | `ssa/parent_child_drop_dependency_tests.rs` | 590 |
| `ssa/range_lowering_tests/view_source.rs` | 298 | `ssa/parent_child_drop_dependency_tests.rs` | 891 |

---

## 3. 验收标准

1. 上述 8 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `2093` 降至 `2085`。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 8 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：8/8 个路径不存在 |
| 内容并入声明的合并目标 | 通过 | 8/8 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 23 个 Spec 已 squash 为 `430a0de2`，无法从历史分离本 Spec 的计数，不虚报该数字；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 引用全量结果（默认 3055 / feature 3117），本 Spec 定向 `
running 390 tests
test ssa::loan_region_ssa_tests::binding_lifecycle_probe::original_binding_lifecycle_records_real_actions_and_ssa ... ok
test native_tests::boxed_enum_tests::unit_source::owner_multi_root_header_tests::ci_only_checked_expression_receipt_requires_actual_edges_and_operands ... ok
test native_tests::boxed_enum_tests::unit_source::owner_multi_root_header_tests::multi_root_receipt_binds_actual_local_guard_slot ... ok
test native_tests::boxed_enum_tests::unit_source::owner_multi_root_header_tests::ci_only_multi_root_actual_receipt_cannot_be_skipped_or_rebound ... ok
test ssa::loan_region_ssa_tests::multi_temporary::instances::tests::generic_instance_budget_has_an_explicit_boundary ... ok
test ssa::loan_region_ssa_tests::certified_receiver_region_has_real_unit_ssa_lifecycle ... ok
test ssa::loan_region_ssa_tests::finite_literal::finite_literal_caller_requires_real_refine_route ... ok
test ssa::loan_region_ssa_tests::multi_temporary::two_literal_call_requires_real_production_receipt ... ok
test ssa::loan_region_ssa_tests::finite_literal::finite_literal_call_requires_real_production_receipt ... ok
test ssa::loan_region_ssa_tests::certified_consumer_region_has_real_unit_ssa_lifecycle ... ok
test native_tests::boxed_enum_tests::unit_source::owner_replace_break_downstream_tests::owner_loan_receipt_sibling_tests::bounded_owner_and_protocol4_siblings_keep_receipts_and_drops_separate ... ok
test native_tests::boxed_enum_tests::unit_source::owner_multi_root_header_tests::multi_root_receipt_preserves_completed_prefix_before_root_reinitialization ... ok
test ssa::loan_region_ssa_tests::multi_temporary::two_literal_caller_requires_real_refine_route ... ok
test ssa::loan_region_ssa_tests::named_literal_mapping::named_literal_caller_requires_real_refine_route ... ok
test ssa::loan_region_ssa_tests::forward_lifecycle_probe::original_expression_wrapper_records_live_handoff_and_actual_lowering ... ok
test ssa::loan_region_ssa_tests::named_literal_mapping::named_literal_call_requires_real_production_receipt ... ok
test ssa::loan_region_ssa_tests::parent_child_coverage::temporary_route::temporary_borrow_caller_uses_refine_without_migrating_callee ... ok
test ssa::loan_region_ssa_tests::parent_child_coverage::temporary_route::temporary_borrow_requires_real_production_receipt ... ok
test ssa::loan_region_ssa_tests::parent_child_coverage::temporary_route::non_target_temporary_shapes_preserve_routes_and_no_receipts ... ok
test native_tests::boxed_enum_tests::unit_source::owner_multi_root_header_tests::multi_root_receipt_substitutes_sparse_entry_catalog_by_root ... ok
test ssa::loan_region_ssa_tests::parent_child_drop_dependency::range_view_source::range_view_source_verifier_rejects_premature_collection_release_after_parent_end ... ok
test ssa::loan_region_ssa_tests::parent_child_drop_dependency::range_view_source::range_view_source_verifier_rejects_a_forwarded_sibling_root ... ok
test ssa::loan_region_ssa_tests::receipts::int_consumer_with_neighboring_plain_calls_has_local_receipt ... ok
test ssa::loan_region_ssa_tests::receipts::named_route::named_borrow_requires_real_production_receipt ... ok
test ssa::loan_region_ssa_tests::receipts::named_route::named_borrow_caller_uses_refine_without_migrating_callee ... ok
test ssa::loan_region_ssa_tests::parent_child_drop_dependency::range_view_source::range_view_source_nested_temporary_chain_is_consumed_synchronously ... ok
test ssa::loan_region_ssa_tests::parent_child_drop_dependency::range_view_source::range_view_source_take_single_and_unit_end_parent_before_child_use ... ok
test ssa::loan_region_ssa_tests::receipts::partial_live_source_preserves_legacy_lowering_without_receipt ... ok
test ssa::loan_region_ssa_tests::named_literal_mapping::rc_nullable_while_presence_identity::presence_rc_nullable_while_preserves_owner_cleanup_identity ... ok
test ssa::loan_region_ssa_tests::receipts::sibling_callables_keep_catalog_and_ssa_receipts_local ... ok
test ssa::loan_region_ssa_tests::receipts::named_route::neighboring_named_shapes_preserve_legacy_routes_without_receipts ... ok
test ssa::loan_region_ssa_tests::receipts::named_siblings::named_receiver_consumer_and_all_owner_siblings_keep_receipts_separate ... ok
test ssa::loan_region_ssa_tests::receipts::receiver_route::neighboring_receiver_shapes_keep_existing_routes ... ok
test ssa::loan_region_ssa_tests::receiver_receipt_siblings::receiver_consumer_and_owner_siblings_keep_production_receipts_separate ... ok
test ssa::loan_region_ssa_tests::temporary_siblings::temporary_named_receiver_and_owner_siblings_keep_receipts_separate ... ok
test ssa::range_lowering_tests::abort_predecessor_range_end_source_loan_then_both_abort ... ok
test ssa::range_lowering_tests::call_prefix::range_call_prefix_named_list_later_argument_return_is_legal ... ok
test ssa::range_lowering_tests::call_prefix::range_call_prefix_named_view_later_argument_return_is_legal ... ok
test ssa::range_lowering_tests::call_prefix::range_call_prefix_temporary_list_later_argument_return_is_legal ... ok
test ssa::range_lowering_tests::call_prefix::range_call_prefix_temporary_view_later_argument_return_is_legal ... ok
test ssa::range_lowering_tests::drop_algorithms::range_drop_algorithms_reuse_arbitrary_named_dynamic_boundary_producers ... ok
test ssa::range_lowering_tests::drop_algorithms::range_drop_algorithms_single_and_unit_share_the_descriptor_abi ... ok
test ssa::range_lowering_tests::local_control::boundary::range_local_control_keeps_unproved_alias_control_gates ... ok
test ssa::range_lowering_tests::local_control::boundary::range_local_control_keeps_unselected_binary_gates ... ok
test ssa::range_lowering_tests::local_control::boundary::range_local_control_rejects_abort_arm ... ok
test ssa::range_lowering_tests::local_control::boundary::range_local_control_rejects_carrier_call_results ... ok
test ssa::range_lowering_tests::local_control::boundary::range_local_control_rejects_different_roots_in_the_same_scope ... ok
test ssa::range_lowering_tests::local_control::boundary::range_local_control_rejects_hidden_cfg_in_condition_or_argument ... ok
test ssa::range_lowering_tests::local_control::boundary::range_local_control_rejects_if_nested_in_a_loop ... ok
test ssa::range_lowering_tests::local_control::boundary::range_local_control_rejects_if_without_else ... ok
test ssa::range_lowering_tests::local_control::boundary::range_local_control_rejects_nested_if_at_inner_expression ... ok
test ssa::range_lowering_tests::local_control::boundary::range_local_control_rejects_new_binding_inside_an_arm ... ok
test ssa::range_lowering_tests::local_control::boundary::range_local_control_rejects_old_call_prefix_exit_inside_a_new_if ... ok
test ssa::range_lowering_tests::local_control::boundary::range_local_control_rejects_value_context_if ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_after_reversed ... ok
test ssa::loan_region_ssa_tests::parent_child_drop_dependency::range_view_source::range_view_source_parent_child_and_elements_coexist_until_their_own_last_uses ... ok
test ssa::range_lowering_tests::drop_algorithms::range_drop_algorithms_single_and_reversed_unit_run_unmodified_llvm_natively ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_after_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_after_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_after_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_after_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_after_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_before_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_before_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_before_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_before_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_before_unit ... ok
test ssa::loan_region_ssa_tests::parent_child_drop_dependency::range_view_source::range_view_source_native_single_and_unit_preserve_collection_and_element_identities ... ok
test ssa::loan_region_ssa_tests::parent_child_drop_dependency::range_view_source::range_view_source_offsets_and_forwarded_return_preserve_absolute_root_coordinates ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_before_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_last_read_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_last_read_reversed_constant ... ok
test native_tests::boxed_enum_tests::unit_source::owner_multi_root_header_tests::multi_root_receipt_consumes_actual_aliased_entry has been running for over 60 seconds
test ssa::loan_region_ssa_tests::parent_child_drop_dependency::range_view_source::range_view_source_nested_temporary_native_releases_each_original_collection_once has been running for over 60 seconds
test ssa::range_lowering_tests::call_prefix::range_call_prefix_native_restores_roots_and_releases_each_identity_on_both_branches has been running for over 60 seconds
test ssa::range_lowering_tests::call_prefix::range_call_prefix_owned_arguments_transfer_only_after_the_later_argument_completes has been running for over 60 seconds
test ssa::range_lowering_tests::local_control::boundary::range_local_control_accepts_int_comparisons_with_ordinary_root_alias has been running for over 60 seconds
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_last_read_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_last_read_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_last_read_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_last_read_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_live_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_live_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_live_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_live_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_live_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ge_live_unit_constant ... ok
test native_tests::boxed_enum_tests::unit_source::owner_multi_root_header_tests::multi_root_receipt_consumes_actual_aliased_entry ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_after_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_after_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_after_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_after_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_after_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_after_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_before_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_before_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_before_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_before_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_before_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_before_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_last_read_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_last_read_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_last_read_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_last_read_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_last_read_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_last_read_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_live_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_live_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_live_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_live_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_live_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_gt_live_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_after_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_after_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_after_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_after_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_after_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_after_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_before_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_before_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_before_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_before_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_before_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_before_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_last_read_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_last_read_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_last_read_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_last_read_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_last_read_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_last_read_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_live_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_live_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_live_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_live_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_live_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_le_live_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_after_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_after_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_after_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_after_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_after_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_after_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_before_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_before_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_before_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_before_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_before_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_before_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_last_read_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_last_read_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_last_read_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_last_read_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_last_read_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_last_read_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_live_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_live_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_live_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_live_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_live_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::range_comparisons_ne_live_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_move_only_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_move_only_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_move_only_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_move_only_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_move_only_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_move_only_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_resource_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_resource_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_resource_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_resource_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_resource_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_resource_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_string_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_string_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_string_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_string_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_string_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ge_string_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_move_only_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_move_only_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_move_only_single ... ok
test ssa::range_lowering_tests::call_prefix::range_call_prefix_owned_arguments_transfer_only_after_the_later_argument_completes ... ok
test ssa::range_lowering_tests::call_prefix::range_call_prefix_native_restores_roots_and_releases_each_identity_on_both_branches ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_move_only_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_move_only_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_move_only_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_resource_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_resource_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_resource_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_resource_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_resource_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_resource_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_string_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_string_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_string_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_string_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_string_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_gt_string_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_move_only_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_move_only_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_move_only_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_move_only_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_move_only_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_move_only_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_resource_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_resource_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_resource_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_resource_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_resource_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_resource_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_string_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_string_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_string_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_string_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_string_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_le_string_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_move_only_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_move_only_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_move_only_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_move_only_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_move_only_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_move_only_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_resource_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_resource_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_resource_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_resource_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_resource_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_resource_unit_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_string_reversed ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_string_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_string_single ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_string_single_constant ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_string_unit ... ok
test ssa::range_lowering_tests::local_control::comparisons::selection::range_comparison_selection_ne_string_unit_constant ... ok
test ssa::range_lowering_tests::local_control::ownership::range_carrier_direct_return_after_scope ... ok
test ssa::range_lowering_tests::local_control::ownership::range_carrier_direct_return_in_binding_scope ... ok
test ssa::loan_region_ssa_tests::parent_child_drop_dependency::range_view_source::range_view_source_nested_temporary_native_releases_each_original_collection_once ... ok
test ssa::range_lowering_tests::local_control::ownership::range_carrier_direct_return_skips_unreachable_use ... ok
test ssa::range_lowering_tests::local_control::ownership::range_carrier_direct_value_return_reads_live_child ... ok
test ssa::range_lowering_tests::local_control::ownership::range_carrier_gate_child_ends_before_outer_if ... ok
test ssa::range_lowering_tests::local_control::ownership::range_carrier_gate_child_is_live_across_return ... ok
test ssa::range_lowering_tests::local_control::boundary::range_local_control_accepts_int_comparisons_with_ordinary_root_alias ... ok
test ssa::range_lowering_tests::local_control::ownership::range_carrier_gate_composes_return_and_normal_scope_end ... ok
test ssa::range_lowering_tests::local_control::ownership::range_carrier_gate_descriptor_begins_after_outer_if ... ok
test ssa::range_lowering_tests::local_control::ownership::range_local_control_move_only_siblings_single ... ok
test ssa::range_lowering_tests::local_control::ownership::range_local_control_move_only_siblings_unit ... ok
test ssa::range_lowering_tests::local_control::ownership::range_local_control_move_only_siblings_unit_reversed ... ok
test ssa::range_lowering_tests::local_control::ownership::range_local_control_resource_siblings_single ... ok
test ssa::range_lowering_tests::local_control::ownership::range_local_control_resource_siblings_unit ... ok
test ssa::range_lowering_tests::local_control::ownership::range_local_control_resource_siblings_unit_reversed ... ok
test ssa::range_lowering_tests::local_control::ownership::range_local_control_string_siblings_single ... ok
test ssa::range_lowering_tests::local_control::ownership::range_local_control_string_siblings_unit ... ok
test ssa::range_lowering_tests::local_control::ownership::range_local_control_string_siblings_unit_reversed ... ok
test ssa::range_lowering_tests::local_control::range_local_control_eq_after_single ... ok
test ssa::range_lowering_tests::local_control::range_local_control_eq_after_unit ... ok
test ssa::range_lowering_tests::local_control::range_local_control_eq_after_unit_reversed ... ok
test ssa::range_lowering_tests::local_control::range_local_control_eq_before_single ... ok
test ssa::range_lowering_tests::local_control::range_local_control_eq_before_unit ... ok
test ssa::range_lowering_tests::local_control::range_local_control_eq_before_unit_reversed ... ok
test ssa::range_lowering_tests::local_control::range_local_control_eq_live_single ... ok
test ssa::range_lowering_tests::local_control::range_local_control_eq_live_unit ... ok
test ssa::range_lowering_tests::local_control::range_local_control_eq_live_unit_reversed ... ok
test ssa::range_lowering_tests::local_control::range_local_control_if_both_arms_single ... ok
test ssa::range_lowering_tests::local_control::range_local_control_if_both_arms_unit ... ok
test ssa::range_lowering_tests::local_control::range_local_control_if_both_arms_unit_reversed ... ok
test ssa::range_lowering_tests::local_control::range_local_control_lt_after_single ... ok
test ssa::range_lowering_tests::local_control::range_local_control_lt_after_unit ... ok
test ssa::range_lowering_tests::local_control::range_local_control_lt_after_unit_reversed ... ok
test ssa::range_lowering_tests::local_control::range_local_control_lt_before_single ... ok
test ssa::range_lowering_tests::local_control::range_local_control_lt_before_unit ... ok
test ssa::range_lowering_tests::local_control::range_local_control_lt_before_unit_reversed ... ok
test ssa::range_lowering_tests::local_control::range_local_control_lt_live_single ... ok
test ssa::range_lowering_tests::local_control::range_local_control_lt_live_unit ... ok
test ssa::range_lowering_tests::local_control::range_local_control_lt_live_unit_reversed ... ok
test ssa::range_lowering_tests::local_control::ownership::range_carrier_gate_descriptor_ends_before_outer_if ... ok
test ssa::range_lowering_tests::local_control::ownership::range_carrier_gate_descriptor_ends_on_explicit_return_arm ... ok
test ssa::range_lowering_tests::local_control::ownership::range_carrier_gate_descriptor_ends_on_unbraced_return ... ok
test ssa::range_lowering_tests::local_control::ownership::range_carrier_gate_return_precedes_child_acquisition ... ok
test ssa::range_lowering_tests::local_control::short_circuit::boundary::range_short_circuit_boundary_loop_lambda_and_control_exit ... ok
test ssa::range_lowering_tests::local_control::short_circuit::boundary::range_short_circuit_boundary_nested_and_hidden_cfg ... ok
test ssa::range_lowering_tests::local_control::short_circuit::boundary::range_short_circuit_boundary_outer_call_effects_and_new_metadata_prefix ... ok
test ssa::range_lowering_tests::local_control::selection::range_local_control_eq_if_selection_single ... ok
test ssa::range_lowering_tests::local_control::short_circuit::boundary::range_short_circuit_boundary_owning_and_inout_calls ... ok
test ssa::range_lowering_tests::local_control::short_circuit::boundary::range_short_circuit_boundary_range_creation_and_aliases ... ok
test ssa::range_lowering_tests::local_control::short_circuit::boundary::range_short_circuit_boundary_receiver_effects ... ok
test ssa::range_lowering_tests::local_control::short_circuit::boundary::range_short_circuit_frontend_nothing_and_borrow_result_diagnostics ... ok
test ssa::range_lowering_tests::local_control::short_circuit::comparison_boundary::range_comparison_boundary_concat ... ok
test ssa::range_lowering_tests::local_control::short_circuit::boundary::range_short_circuit_boundary_scope_and_roots ... ok
test ssa::range_lowering_tests::local_control::short_circuit::comparison_boundary::range_comparison_boundary_metadata_alias ... ok
test ssa::range_lowering_tests::local_control::selection::range_local_control_eq_if_selection_unit ... ok
test ssa::range_lowering_tests::local_control::short_circuit::comparison_boundary::range_comparison_boundary_non_int ... ok
test ssa::range_lowering_tests::local_control::selection::range_local_control_eq_if_selection_unit_reversed ... ok
test ssa::range_lowering_tests::local_control::selection::range_local_control_lt_if_selection_single ... ok
test ssa::range_lowering_tests::local_control::selection::range_local_control_lt_if_selection_unit ... ok
test ssa::range_lowering_tests::local_control::selection::range_local_control_lt_if_selection_unit_reversed ... ok
test ssa::range_lowering_tests::local_control::selection::range_local_control_observable_selection_single ... ok
test ssa::range_lowering_tests::local_control::selection::range_local_control_observable_selection_unit ... ok
test ssa::range_lowering_tests::local_control::selection::range_local_control_observable_selection_unit_reversed ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_argument_reversed ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_argument_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_argument_single ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_argument_unit ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_argument_unit_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_condition_reversed ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_condition_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_condition_single ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_condition_unit ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_condition_unit_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_copyable_value_argument_single ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_local_reversed ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_local_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_local_single ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_local_unit ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_and_local_unit_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_fixed_reversed ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_fixed_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_fixed_single ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_fixed_single_constant_fixture ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_fixed_unit ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_argument_reversed ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_argument_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_argument_single ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_fixed_unit_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_argument_unit ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_argument_unit_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_condition_reversed ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_condition_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_condition_single ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_condition_unit ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_condition_unit_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_local_reversed ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_migrated_plain_operands has been running for over 60 seconds
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_local_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_local_single ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_local_unit ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_or_local_unit_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_scalar_reversed ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_scalar_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_scalar_single ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_migrated_plain_operands ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_pending_prefix_reversed ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_pending_prefix_reversed_constant ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_scalar_unit ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_scalar_unit_constant ... ok
test ssa::range_lowering_tests::range_list_producer_size_uses_metadata_and_source_loan ... ok
test ssa::range_lowering_tests::range_metadata_borrow_is_separate_from_constructing_a_new_descriptor ... ok
test ssa::range_lowering_tests::range_single_and_unit_for_deliver_only_shared_elements ... ok
test ssa::range_lowering_tests::range_single_and_unit_keep_the_same_new_descriptor_abi ... ok
test ssa::range_lowering_tests::range_single_and_unit_named_for_transport_descriptor_and_ancestor_loans ... ok
test ssa::range_lowering_tests::range_temporary_call_single_and_unit_preserve_root_until_metadata_ends ... ok
test ssa::range_lowering_tests::range_temporary_for_single_and_unit_preserve_root_until_metadata_ends ... ok
test ssa::range_lowering_tests::range_verifier_preserves_descriptor_root_pairing_through_loop_edges ... ok
test ssa::range_lowering_tests::range_verifier_rejects_erasure_and_premature_source_end_without_emitting_ir ... ok
test ssa::range_lowering_tests::range_verifier_rejects_metadata_end_while_an_element_loan_is_active ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_readonly_receiver_and_copyable_value_argument ... ok
test ssa::range_lowering_tests::scalar_counts::range_scalar_counts_keep_metadata_alias_boundary ... ok
test ssa::range_lowering_tests::scalar_counts::range_scalar_counts_keep_non_int_binary_and_wider_cfg_gates ... ok
test ssa::range_lowering_tests::scalar_counts::range_scalar_counts_keep_value_if_in_return_outside_gate ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_pending_prefix_single ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_pending_prefix_unit ... ok
test ssa::range_lowering_tests::local_control::short_circuit::range_short_circuit_pending_prefix_unit_constant ... ok
test ssa::range_lowering_tests::scalar_counts::range_scalar_counts_compose_with_ordinary_root_alias ... ok
test ssa::range_lowering_tests::scalar_counts::range_scalar_counts_pending_temporary_source_and_descriptor_evaluate_once ... ok
test ssa::range_lowering_tests::scalar_counts::range_scalar_counts_selected_boolean_short_circuit_local ... ok
test ssa::range_lowering_tests::temporary::range_temporary_sequential_uses_and_loop_exits_keep_correlated_ssa_facts ... ok
test ssa::range_lowering_tests::temporary::range_temporary_verifier_rejects_premature_root_or_descriptor_end ... ok
test ssa::refine::bounded_receipt::emitted::ci_tests::ci_only_bounded_receipt_publication_rolls_back_before_original_verifier_success ... ok
test ssa::refine::bounded_receipt::emitted::ci_tests::ci_only_bounded_receipt_rejects_foreign_ids_missing_parent_and_wrong_flag ... ok
test ssa::refine::branch_receipt::emitted::ci_tests::ci_only_both_return_receipt_rejects_tail_topology_and_missing_exit ... ok
test ssa::refine::branch_receipt::emitted::ci_tests::ci_only_branch_receipt_publication_is_atomic_on_unit_failure ... ok
test ssa::refine::branch_receipt::emitted::ci_tests::ci_only_branch_receipt_rejects_corrupt_identity_and_incomplete_paths ... ok
test ssa::refine::build::condition::receiver_ci_tests::ci_only_foreign_catalog_receipt_is_rejected ... ok
test ssa::refine::forward_receipt::emitted::ci_tests::ci_only_forward_receipt_rejects_missing_duplicate_and_corrupt_roles ... ok
test ssa::refine::loan_receipt::emitted::ci_tests::ci_only_foreign_catalog_receipt_is_rejected ... ok
test ssa::refine::multi_receipt::emitted::ci_tests::ci_only_multi_receipt_publication_rolls_back_before_original_verifier_success ... ok
test ssa::refine::multi_receipt::emitted::ci_tests::ci_only_multi_receipt_rejects_foreign_ids_missing_parent_and_wrong_flag ... ok
test ssa::refine::named_receipt::emitted::ci_tests::ci_only_foreign_catalog_receipt_is_rejected ... ok
test ssa::refine::nested_receipt::emitted::cascade_ci_tests::ci_only_cascade_receipt_publication_rolls_back_with_failed_unit ... ok
test ssa::refine::nested_receipt::emitted::cascade_ci_tests::ci_only_cascade_receipt_rejects_replaced_origins_and_final_ssa ... ok
test ssa::refine::nested_receipt::emitted::ci_tests::ci_only_balanced_receipt_rejects_cross_subtree_identity_and_incomplete_closure ... ok
test ssa::refine::nested_receipt::emitted::ci_tests::ci_only_mirror_receipt_rejects_wrong_topology_missing_false_parent_and_flag ... ok
test ssa::refine::nested_receipt::emitted::ci_tests::ci_only_nested_receipt_publication_rolls_back_before_original_verifier_success ... ok
test ssa::refine::nested_receipt::emitted::ci_tests::ci_only_nested_receipt_rejects_foreign_ids_missing_parent_and_wrong_flag ... ok
test ssa::refine::nested_receipt::emitted::tail_ci_tests::ci_only_tail_receipt_rejects_fold_and_continuation_identity_substitution ... ok
test ssa::refine::single_receipt::emitted::ci_tests::ci_only_single_receipt_publication_rolls_back_before_original_verifier_success ... ok
test ssa::refine::single_receipt::emitted::ci_tests::ci_only_single_receipt_rejects_foreign_ids_missing_parent_and_wrong_flag ... ok
test ssa::refine::temporary_ci_tests::ci_only_foreign_catalog_receipt_is_rejected ... ok
test ssa::range_lowering_tests::local_control::short_circuit::receiver::range_short_circuit_value_receiver_constant_unit has been running for over 60 seconds
test ssa::range_lowering_tests::local_control::short_circuit::receiver::range_short_circuit_value_receiver_constant_unit_reversed has been running for over 60 seconds
test ssa::range_lowering_tests::local_control::short_circuit::receiver::range_short_circuit_value_receiver_single has been running for over 60 seconds
test ssa::range_lowering_tests::local_control::short_circuit::receiver::range_short_circuit_value_receiver_unit has been running for over 60 seconds
test ssa::range_lowering_tests::local_control::short_circuit::receiver::range_short_circuit_value_receiver_unit_reversed has been running for over 60 seconds
test ssa::range_lowering_tests::scalar_counts::range_scalar_counts_single_and_unit_transport_live_parent_child_roots ... ok
test ssa::range_lowering_tests::scalar_counts::range_scalar_counts_normal_source_arithmetic_and_negative_counts_abort has been running for over 60 seconds
test ssa::range_lowering_tests::scalar_counts::range_scalar_counts_selected_int_comparisons_bind_boolean_locals has been running for over 60 seconds
test ssa::range_lowering_tests::local_control::short_circuit::receiver::range_short_circuit_value_receiver_constant_unit ... ok
test ssa::range_lowering_tests::local_control::short_circuit::receiver::range_short_circuit_value_receiver_constant_unit_reversed ... ok
test ssa::range_lowering_tests::local_control::short_circuit::receiver::range_short_circuit_value_receiver_single ... ok
test ssa::range_lowering_tests::temporary::range_temporary_native_single_and_unit_release_string_move_only_and_resource_identities has been running for over 60 seconds
test ssa::range_lowering_tests::local_control::short_circuit::receiver::range_short_circuit_value_receiver_unit ... ok
test ssa::range_lowering_tests::local_control::short_circuit::receiver::range_short_circuit_value_receiver_unit_reversed ... ok
test ssa::range_lowering_tests::scalar_counts::range_scalar_counts_selected_int_comparisons_bind_boolean_locals ... ok
test ssa::range_lowering_tests::scalar_counts::range_scalar_counts_normal_source_arithmetic_and_negative_counts_abort ... ok
test ssa::range_lowering_tests::temporary::range_temporary_native_single_and_unit_release_string_move_only_and_resource_identities ... ok

test result: ok. 390 passed; 0 failed; 0 ignored; 0 measured; 2666 filtered out; finished in 737.53s = `390 passed / 0 failed`` |
