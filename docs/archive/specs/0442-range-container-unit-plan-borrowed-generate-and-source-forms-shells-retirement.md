# SPEC-0442: `range_lowering_tests`、`unit_lower_container_tests`、`unit_plan_tests`、`borrowed_generate_tests` 及 `source_forms` 子壳模块退役

> **性质**：正式开发规范（Spec） · **状态**：done · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0442 |
| 标题 | `range_lowering_tests`、`unit_lower_container_tests`、`unit_plan_tests`、`borrowed_generate_tests` 及 `source_forms` 子壳模块退役 |
| 版本目标 | v0.46 |
| 状态 | done |
| 创建日期 | 2026-04-17 |
| 完成日期 | 2026-11-13 |
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

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 8 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：8/8 个路径不存在 |
| 内容并入声明的合并目标 | 部分目标已被后续 Spec 再次合并 | 7/8 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 23 个 Spec 已 squash 为 `430a0de2`，无法从历史分离本 Spec 的计数，不虚报该数字；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 引用全量结果（默认 3055 / feature 3117），本 Spec 定向 `
running 411 tests
test llvm::container_tests::list_form_length_guard_prevents_invalid_header_lengths ... ok
test llvm::container_tests::container_drop_walks_move_only_elements_in_reverse_and_frees_once ... ok
test llvm::container_tests::move_only_zst_drop_preserves_logical_count_without_address_or_free ... ok
test llvm::container_tests::copyable_element_drop_skips_element_loop_but_releases_non_empty_buffer ... ok
test llvm::container_tests::replacement_commits_new_owner_before_dropping_the_old_element ... ok
test llvm::container_tests::checked_element_place_aborts_before_address_formation ... ok
test llvm::borrowed_generate_tests::borrowed_generation_dynamic_length_keeps_checked_length_and_bytes_guards ... ok
test llvm::container_tests::zst_container_uses_aligned_sentinel_without_allocation_or_addressing ... ok
test llvm::borrowed_generate_tests::borrowed_unit_void_generation_calls_each_logical_index_without_buffer_addressing ... ok
test llvm::container_tests::runtime_length_generation_calls_initializer_in_an_explicit_loop ... ok
test llvm::container_tests::fixed_headers_and_list_construction_use_one_checked_continuous_allocation ... ok
test llvm::unit_storage_tests::unit_container_element_storage_accessor_has_no_unknown_or_opaque_fallback ... ok
test llvm::unit_storage_tests::container_unit_storage_does_not_enable_aggregate_fields_or_shared_payloads ... ok
test llvm::unit_storage_tests::unit_container_value_parameter_and_explicit_ssa_return_remain_unsupported ... ok
test llvm::unit_storage_tests::empty_unit_container_verified_ssa_lowers_to_llvm ... ok
test llvm::unit_storage_tests::container_unit_storage_does_not_enable_general_value_callable_abi ... ok
test llvm::unit_storage_tests::unit_storage_matches_target_datalayout_without_changing_container_headers ... ok
test llvm::container_tests::provider_cfg::borrowed_provider_cfg_lowers_length_once_and_cleanup_exits ... ok
test llvm::unit_storage_tests::unit_container_ir_keeps_sentinel_void_abi_and_signed_bounds_without_element_memory ... ok
test llvm::borrowed_generate_tests::borrowed_generation_prepares_fixed_storage_once_outside_the_loop ... ok
test llvm::object_tests::borrowed_container_length_builds_links_and_runs_with_int_result ... ok
test native::unit_tests::container_clear_tests::unit_container_clear_native_empty_list ... ok
test native::unit_tests::container_append_tests::unit_container_append_native_move_only_elements_deinit_in_reverse_order ... ok
test native::unit_tests::container_clear_tests::unit_container_clear_native_multiple_cycles_buffer_reuse ... ok
test native::unit_tests::container_append_tests::unit_container_append_native_nonempty_start ... ok
test native::unit_tests::container_clear_tests::unit_container_clear_native_move_only_elements_reverse_deinit ... ok
test native::unit_tests::container_clear_tests::unit_container_clear_native_primitive_and_reappend ... ok
test native::unit_tests::container_append_tests::unit_container_append_native_growth_and_ordering ... ok
test native::unit_tests::container_insert_at_tests::unit_container_insert_at_native_move_only_elements_lifecycle ... ok
test native::unit_tests::container_insert_at_tests::unit_container_insert_at_native_out_of_bounds_beyond_size_aborts ... ok
test native::unit_tests::container_insert_at_tests::unit_container_insert_at_native_out_of_bounds_negative_aborts ... ok
test native::unit_tests::container_insert_at_tests::unit_container_insert_at_native_primitive_sequential ... ok
test native::unit_tests::container_remove_at_tests::unit_container_remove_at_native_move_only_elements_lifecycle ... ok
test native::unit_tests::container_remove_at_tests::unit_container_remove_at_native_out_of_bounds_aborts ... ok
test native::unit_tests::container_remove_at_tests::unit_container_remove_at_native_primitive_sequential ... ok
test native::unit_tests::container_remove_at_tests::unit_container_remove_at_native_shrink_to_empty ... ok
test native::unit_tests::container_remove_first_tests::unit_container_remove_first_native_empty_list_aborts ... ok
test native::unit_tests::container_remove_first_tests::unit_container_remove_first_native_move_only_elements_lifecycle ... ok
test native::unit_tests::container_remove_first_tests::unit_container_remove_first_native_pop_and_empty_abort ... ok
test native::unit_tests::container_remove_first_tests::unit_container_remove_first_native_primitive_sequential ... ok
test native::unit_tests::container_size_tests::generic_containers::generic_bodies::unit_generic_body_native_nullable_int_keeps_control_rejection ... ok
test native::unit_tests::container_remove_last_tests::unit_container_remove_last_native_empty_list_aborts ... ok
test native::unit_tests::container_remove_last_tests::unit_container_remove_last_native_move_only_elements_lifecycle ... ok
test native::unit_tests::container_remove_last_tests::unit_container_remove_last_native_pop_and_empty_abort ... ok
test native::unit_tests::container_size_tests::generic_containers::generic_body_budgets::unit_generic_body_native_production_limit_rejects_growth_preserving_outputs ... ok
test native::unit_tests::container_remove_last_tests::unit_container_remove_last_native_primitive_sequential ... ok
test native::unit_tests::container_size_tests::generic_containers::generic_bodies::unit_generic_body_native_nongeneric_nullable_int_control ... ok
test native::unit_tests::container_size_tests::generic_containers::unit_generic_container_native_rejections_preserve_outputs_and_canonical_arena ... ok
test llvm::unit_storage_tests::unit_container_native_bounds_abort_before_marker_and_valid_indices_read ... ok
test native::unit_tests::container_size_tests::generic_containers::generic_body_budgets::unit_generic_body_native_unselected_growth_seeds_preserve_normal_entry ... ok
test llvm::synthetic_zst_tests::synthetic_zst_container_drop_counts_logical_elements_and_storage_separately has been running for over 60 seconds
test native::unit_tests::container_size_tests::generic_containers::generic_bodies::unit_generic_body_native_deinit_only_closed_helper_seed ... ok
test native::unit_tests::container_size_tests::generic_containers::generic_bodies::unit_generic_body_native_multiple_container_demands ... ok
test native::unit_tests::container_size_tests::generic_containers::generic_bodies::unit_generic_body_native_nullable_resource_and_empty_container ... ok
test native::unit_tests::container_size_tests::generic_containers::generic_bodies::unit_generic_body_native_resource_relay_drops_once_in_reverse ... ok
test native::unit_tests::container_size_tests::generic_containers::unit_generic_container_native_direct_argument_accepts_concrete_nested_list ... ok
test native::unit_tests::container_size_tests::generic_containers::generic_bodies::unit_generic_body_native_scalar_and_string_relay_lengths has been running for over 60 seconds
test native::unit_tests::container_size_tests::generic_containers::unit_generic_container_native_resource_borrow_and_pass_drop_once_in_reverse ... ok
test native::unit_tests::container_size_tests::unit_container_size_native_assignment_rhs_keeps_old_owner_until_replacement ... ok
test native::unit_tests::container_size_tests::generic_containers::unit_generic_container_native_scalar_and_string_lengths_preserve_owners has been running for over 60 seconds
test native::unit_tests::container_size_tests::runtime_constructor_tests::runtime_constructor_native_unit_cross_file_helper_preserves_element_and_environment has been running for over 60 seconds
test native::unit_tests::container_size_tests::unit_container_size_native_borrow_survives_cfg_and_reuse ... ok
test native::unit_tests::container_size_tests::unit_container_size_native_covers_empty_nonempty_and_reuses_owners ... ok
test native::unit_tests::container_size_tests::runtime_constructor_tests::runtime_constructor_native_unit_nothing_operands_do_not_generate ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_array_local_dead_basic ... ok
test native::unit_tests::container_size_tests::runtime_constructor_tests::runtime_constructor_native_unit_evaluates_operands_once_in_order ... ok
test native::unit_tests::container_size_tests::unit_container_size_native_temporary_resources_drop_once_after_header_read ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_array_local_dead_constant ... ok
test native::unit_tests::container_size_tests::runtime_constructor_tests::runtime_constructor_native_unit_orders_callbacks_elements_and_environment_cleanup ... ok
test native::unit_tests::container_size_tests::generic_containers::generic_bodies::unit_generic_body_native_scalar_and_string_relay_lengths ... ok
test native::unit_tests::container_size_tests::runtime_constructor_tests::runtime_constructor_native_unit_matrix_executes_logical_callbacks has been running for over 60 seconds
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_array_local_live_basic ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_array_local_live_constant ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_array_own_dead_basic ... ok
test native::unit_tests::container_size_tests::runtime_constructor_tests::runtime_constructor_native_unit_storable_helper_elements has been running for over 60 seconds
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_array_own_dead_constant ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_array_own_live_basic ... ok
test native::unit_tests::container_size_tests::generic_containers::unit_generic_container_native_scalar_and_string_lengths_preserve_owners ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_array_own_live_constant ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_list_local_dead_basic ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_list_local_dead_constant ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_list_local_live_basic ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_list_local_live_constant ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_list_own_dead_basic ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_list_own_dead_constant ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_list_own_live_basic ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_list_own_live_constant ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_mutable_list_local_dead_basic ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_mutable_list_local_dead_constant ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_mutable_list_local_live_basic ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_mutable_list_local_live_constant ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_mutable_list_own_dead_basic ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_mutable_list_own_dead_constant ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_mutable_list_own_live_basic ... ok
test native::unit_tests::map_loop_owner_tests::owned_source_return::owned_source_return_unit_native_mutable_list_own_live_constant ... ok
test native::unit_tests::string_clone_tests::string_clone_cross_file_borrow_container_and_return_reach_native ... ok
test native_map_provider_tests::local_arguments::single_generic_wrapper_sources_keep_exact_native_order_and_cleanup ... ok
test native_map_provider_tests::local_arguments::unit_generic_wrapper_sources_keep_exact_native_order_and_cleanup ... ok
test native_map_provider_tests::local_arguments::single_local_argument_sources_keep_exact_native_order_and_cleanup has been running for over 60 seconds
test native_map_provider_tests::nested_exits_tests::single_map_provider_nested_exit_sources_exit_normally ... ok
test native_map_provider_tests::local_arguments::unit_local_argument_sources_keep_exact_native_order_and_cleanup has been running for over 60 seconds
test native_map_provider_tests::nested_exits_tests::unit_map_provider_nested_exit_sources_exit_normally ... ok
test native_map_provider_tests::resource_forms::single_existing_resource_representations_exit_normally has been running for over 60 seconds
test native_map_provider_tests::resource_forms::unit_existing_resource_representations_exit_normally has been running for over 60 seconds
test native_map_provider_tests::local_arguments::single_local_argument_sources_keep_exact_native_order_and_cleanup ... ok
test native_map_provider_tests::source_forms::single_owned_mutable_and_group_sources_exit_normally has been running for over 60 seconds
test native_map_provider_tests::source_forms::unit_owned_mutable_and_group_sources_exit_normally has been running for over 60 seconds
test llvm::synthetic_zst_tests::synthetic_zst_container_drop_counts_logical_elements_and_storage_separately ... ok
test native_map_provider_tests::temporary_exits_tests::single_map_provider_temporary_exit_sources_exit_normally has been running for over 60 seconds
test native_map_provider_tests::temporary_exits_tests::single_map_provider_temporary_exit_sources_exit_normally ... ok
test native_map_provider_tests::local_arguments::unit_local_argument_sources_keep_exact_native_order_and_cleanup ... ok
test native_tests::boxed_enum_tests::unit_source::pending_construction_tests::container_pending_prefix_facts_and_verified_ssa_preserve_transfer ... ok
test native_tests::abort_output_tests::indirect_nothing_container_prefix_keeps_size_loan_off_the_bypass ... ok
test native_tests::int_container_construction_links_and_runs_with_size_t_header ... ok
test native_tests::sequential_for_tests::temporary_container_iteration_runs_natively ... ok
test native_tests::string_clone_tests::string_clone_borrows_temporary_container_element ... ok
test native_map_provider_tests::temporary_exits_tests::unit_map_provider_temporary_exit_sources_exit_normally has been running for over 60 seconds
test native_tests::string_clone_tests::string_clone_copies_owned_container_elements_without_moving_them ... ok
test native_map_provider_tests::temporary_exits_tests::unit_map_provider_temporary_exit_sources_exit_normally ... ok
test ssa::bitwise_lowering_tests::single_file_integer_index_reads_then_inverts_without_consuming_container ... ok
test ssa::borrowed_container_lowering_tests::source_borrowed_array_string_index_lowers_to_checked_element_loan ... ok
test ssa::borrowed_container_lowering_tests::verifier_accepts_active_shared_container_loan_and_rejects_wrong_or_ended_loan ... ok
test ssa::borrowed_generate_tests::borrowed_generation_accepts_pointer_shared_and_owned_without_consuming_initializer ... ok
test ssa::borrowed_generate_tests::borrowed_generation_accepts_unit_void_as_one_logical_element ... ok
test ssa::borrowed_generate_tests::borrowed_generation_invalid_unused_type_is_structurally_rejected ... ok
test ssa::borrowed_generate_tests::borrowed_generation_keeps_shared_capture_dependency_until_callback_drop ... ok
test ssa::borrowed_generate_tests::borrowed_generation_operand_registration_enforces_definition_order ... ok
test ssa::borrowed_generate_tests::borrowed_generation_preserves_initializer_and_capture_dependencies_across_cfg_transfer ... ok
test ssa::borrowed_generate_tests::borrowed_generation_rejects_a_non_returning_unit_initializer ... ok
test ssa::borrowed_generate_tests::borrowed_generation_rejects_exclusive_and_noncallable_initializer ... ok
test ssa::borrowed_generate_tests::borrowed_generation_rejects_expired_initializer_and_owner_drop ... ok
test ssa::borrowed_generate_tests::borrowed_generation_rejects_hidden_initializer_loan_across_cfg ... ok
test ssa::borrowed_generate_tests::borrowed_generation_rejects_initializer_defined_only_in_successor ... ok
test ssa::borrowed_generate_tests::borrowed_generation_rejects_wrong_index_mode_arity_and_element_return ... ok
test ssa::borrowed_generate_tests::borrowed_generation_uses_rebound_active_capture_loan_in_successor ... ok
test ssa::borrowed_generate_tests::root_storage_tests::borrowed_generation_accepts_external_borrow_callable_without_local_owner ... ok
test ssa::borrowed_generate_tests::root_storage_tests::borrowed_generation_accepts_projected_function_pointer_without_capture_proof ... ok
test ssa::borrowed_generate_tests::root_storage_tests::borrowed_generation_checks_current_root_capture_after_replacement ... ok
test ssa::borrowed_generate_tests::root_storage_tests::borrowed_generation_keeps_concrete_closure_root_replace_rejected ... ok
test ssa::borrowed_generate_tests::root_storage_tests::borrowed_generation_keeps_concrete_closure_root_swap_rejected ... ok
test ssa::borrowed_generate_tests::root_storage_tests::borrowed_generation_normalizes_current_capture_at_a_conditional_join ... ok
test ssa::borrowed_generate_tests::root_storage_tests::borrowed_generation_reads_current_root_contents_after_take ... ok
test ssa::borrowed_generate_tests::root_storage_tests::borrowed_generation_rebinds_current_replacement_capture_across_cfg ... ok
test ssa::borrowed_generate_tests::root_storage_tests::borrowed_generation_rejects_unknown_projected_capture_contents ... ok
test ssa::borrowed_generate_tests::root_storage_tests::borrowed_generation_sequential_overwrite_forgets_ended_previous_capture ... ok
test ssa::borrowed_generate_tests::root_storage_tests::helper_calls::borrowed_generation_helper_checks_current_root_capture_at_call_boundary ... ok
test ssa::borrowed_generate_tests::root_storage_tests::helper_calls::borrowed_generation_pointer_helper_checks_current_root_capture_at_call_boundary ... ok
test ssa::borrowed_generate_tests::shared_reference_follow_tests::shared_reference_follow_does_not_turn_reference_slot_into_owned_content_proof ... ok
test ssa::borrowed_generate_tests::shared_reference_follow_tests::shared_reference_follow_llvm_loads_the_slot_pointer_before_reading_the_capture ... ok
test ssa::borrowed_generate_tests::shared_reference_follow_tests::shared_reference_follow_preserves_parent_dependency_after_cfg_rebinding ... ok
test ssa::borrowed_generate_tests::shared_reference_follow_tests::shared_reference_follow_reads_capture_and_index_in_a_real_thunk ... ok
test ssa::borrowed_generate_tests::shared_reference_follow_tests::shared_reference_follow_rejects_exclusive_or_nonreference_source_and_wrong_results ... ok
test ssa::borrowed_generate_tests::shared_reference_follow_tests::shared_reference_follow_requires_active_source_and_blocks_parent_or_ancestor_end ... ok
test ssa::borrowed_generate_tests::shared_reference_follow_tests::shared_reference_follow_uses_target_contents_for_borrowed_generation ... ok
test ssa::closure_operation_tests::escape_tests::borrowed_closure_escape_rejects_entry_container_return_at_delivery_origin ... ok
test ssa::closure_operation_tests::escape_tests::delivery_tests::borrowed_closure_escape_rejects_container_construct ... ok
test ssa::closure_operation_tests::escape_tests::delivery_tests::borrowed_closure_escape_rejects_container_element_mutate ... ok
test ssa::closure_operation_tests::escape_tests::delivery_tests::borrowed_closure_escape_rejects_container_element_mutate_cfg_alias ... ok
test ssa::closure_operation_tests::escape_tests::delivery_tests::borrowed_closure_escape_rejects_container_replace ... ok
test ssa::container_lowering_tests::lowers_string_list_forms_in_source_order_and_drops_each_container_once ... ok
test ssa::container_lowering_tests::moves_a_string_value_into_a_returned_list_without_a_second_owner_drop ... ok
test ssa::container_lowering_tests::mutable_list_add_single_file_lowers_to_container_append ... ok
test ssa::container_lowering_tests::mutable_list_clear_single_file_lowers_to_container_clear ... ok
test ssa::container_lowering_tests::mutable_list_insert_at_single_file_lowers_to_container_insert_at ... ok
test ssa::container_lowering_tests::mutable_list_remove_at_single_file_lowers_to_container_remove_at ... ok
test ssa::container_lowering_tests::mutable_list_remove_first_single_file_lowers_to_container_remove_first ... ok
test ssa::container_lowering_tests::mutable_list_remove_last_single_file_lowers_to_container_remove_last ... ok
test ssa::container_lowering_tests::runtime_generator_single_source_accepts_pointer_and_shared_initializers ... ok
test ssa::container_lowering_tests::runtime_helpers::runtime_generator_single_generic_helper_preserves_three_callable_environments ... ok
test ssa::container_lowering_tests::runtime_helpers::runtime_generator_single_pointer_factory_keeps_return_identity ... ok
test ssa::container_lowering_tests::runtime_length_container_helper_borrows_selected_pointer_initializer ... ok
test ssa::container_lowering_tests::runtime_matrix::runtime_generator_single_matrix_preserves_element_capture_and_storage_contracts ... ok
test ssa::container_operation_tests::borrowed_container_length_requires_an_active_shared_container_loan ... ok
test ssa::container_operation_tests::container_construct_length_place_replace_and_drop_verify_together ... ok
test ssa::container_operation_tests::container_element_place_requires_koven_int_index ... ok
test ssa::container_operation_tests::container_types_preserve_kind_element_identity_and_move_only_ownership ... ok
test ssa::container_operation_tests::generated_container_requires_int_to_element_initializer_contract ... ok
test ssa::container_operation_tests::immutable_list_replacement_fails_operation_contract ... ok
test ssa::container_operation_tests::move_after_drop_and_replacement_during_element_loan_are_rejected ... ok
test ssa::container_operation_tests::provider_cfg_carries_owner_and_source_loan_and_ends_each_element_loan ... ok
test ssa::container_operation_tests::provider_element_loan_can_end_after_a_block_edge ... ok
test ssa::container_operation_tests::verifier_rejects_corrupted_structural_container_identity_cycle ... ok
test ssa::integer_range_value_tests::range_values_cross_fields_containers_and_consumed_map_results ... ok
test ssa::lower_frontend_tests::map_result_tests::loop_owner_tests::conditional_return::operand_last_use::source_matrix::owned_source_return_distinct_maps_structure ... ok
test ssa::lower_frontend_tests::map_result_tests::loop_owner_tests::conditional_return::operand_last_use::source_matrix::owned_source_return_independent_results_structure ... ok
test ssa::lower_frontend_tests::map_result_tests::loop_owner_tests::conditional_return::operand_last_use::source_matrix::owned_source_return_operand_cleanup_structure ... ok
test ssa::lower_frontend_tests::map_result_tests::loop_owner_tests::conditional_return::ordinary_controls::ordinary_source_controls_owned_source_return ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::boundaries::owned_source_return_alias_seed_remains_bounded ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::boundaries::owned_source_return_fresh_inner_exact_source_structure ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::boundaries::owned_source_return_fresh_inner_instance ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::boundaries::owned_source_return_group_and_same_type_decoy ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::boundaries::owned_source_return_mixed_continue_remains_bounded ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::boundaries::owned_source_return_outer_owner_with_inner_loop ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::boundaries::owned_source_return_replacement_seed_remains_bounded ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::boundaries::owned_source_return_resource_source_remains_bounded ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::boundaries::owned_source_return_same_source_nested ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::boundaries::owned_source_return_same_source_sequential ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::boundaries::owned_source_return_source_loan_rejects_body_moves_and_mutation ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::boundaries::owned_source_return_two_returns_remain_bounded ... ok
test native_tests::string_containers_clean_up_on_normal_path_and_early_return ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_array_local_dead_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_array_local_dead_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_array_local_live_map ... ok
test native_tests::sequential_for_tests::owned_source_tests::owned_and_borrow_sources_cover_three_containers_sizes_and_exits has been running for over 60 seconds
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_array_local_live_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_array_own_dead_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_array_own_dead_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_array_own_live_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_array_own_live_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_distinct_maps ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_fresh_inner_instances ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_independent_results ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_list_local_dead_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_list_local_dead_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_list_local_live_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_list_local_live_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_list_own_dead_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_list_own_dead_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_list_own_live_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_list_own_live_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_moveonly_patterns ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_mutable_list_local_dead_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_mutable_list_local_dead_no_map ... ok
test native::unit_tests::container_size_tests::runtime_constructor_tests::runtime_constructor_native_unit_cross_file_helper_preserves_element_and_environment ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_mutable_list_local_live_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_mutable_list_local_live_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_mutable_list_own_dead_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_mutable_list_own_dead_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_mutable_list_own_live_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_mutable_list_own_live_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_array_local_dead_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_array_local_dead_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_array_local_live_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_array_local_live_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_array_own_dead_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_array_own_dead_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_array_own_live_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_array_own_live_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_list_local_dead_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_list_local_dead_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_list_local_live_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_list_local_live_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_list_own_dead_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_list_own_dead_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_list_own_live_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_list_own_live_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_mutable_list_local_dead_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_mutable_list_local_dead_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_mutable_list_local_live_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_mutable_list_local_live_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_mutable_list_own_dead_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_mutable_list_own_dead_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_mutable_list_own_live_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_single_mutable_list_own_live_no_map ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_array_local_dead_basic ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_array_local_dead_constant ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_array_local_live_basic ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_array_local_live_constant ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_array_own_dead_basic ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_array_own_dead_constant ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_array_own_live_basic ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_array_own_live_constant ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_list_local_dead_basic ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_list_local_dead_constant ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_list_local_live_basic ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_list_local_live_constant ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_list_own_dead_basic ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_list_own_dead_constant ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_list_own_live_basic ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_list_own_live_constant ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_mutable_list_local_dead_basic ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_mutable_list_local_dead_constant ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_mutable_list_local_live_basic ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_mutable_list_local_live_constant ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_mutable_list_own_dead_basic ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_mutable_list_own_dead_constant ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_mutable_list_own_live_basic ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::owned_source_return_unit_mutable_list_own_live_constant ... ok
test ssa::map_provider_exits_pipeline_tests::local_arguments::generic::concrete_return_generic_wrapper_keeps_all_three_shapes ... ok
test ssa::map_provider_exits_pipeline_tests::local_arguments::selected_argument_factory_forms_named_owner_in_both_pipelines ... ok
test ssa::map_provider_exits_pipeline_tests::nested_exits::map_and_mutable_map_nested_inner_exits_reach_verified_llvm ... ok
test ssa::map_provider_exits_pipeline_tests::nested_exits::query_test::nested_inner_exits_preserve_full_caller_guard_and_edge_identity ... ok
test ssa::map_provider_exits_pipeline_tests::resource_forms::existing_resource_representations_reach_verified_llvm ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_operand_cleanup ... ok
test ssa::map_provider_exits_pipeline_tests::temporary_exits::map_and_mutable_map_temporary_exits_reach_verified_llvm ... ok
test ssa::map_provider_exits_pipeline_tests::temporary_exits::query_test::temporary_map_exits_preserve_factory_owner_and_all_edge_identity ... ok
test ssa::map_source::canonical_callable_tests::canonical_callable_single_specialization_keeps_modes_move_and_container_return ... ok
test ssa::owned_map_borrow_tests::unit_lower_container_element_tests::field_backed_elements_and_temporary_compound_remain_atomic_boundaries ... ok
test ssa::owned_map_borrow_tests::unit_lower_container_element_tests::lowers_cross_file_container_element_read_borrow_and_replacement_deterministically ... ok
test ssa::owned_map_borrow_tests::unit_lower_container_element_tests::named_nested_element_borrow_passes_p3_but_keeps_the_p4_receiver_boundary ... ok
test ssa::refine_subset_tests::container_append_admission_does_not_perturb_or_miscompile_sibling_functions ... ok
test ssa::refine_subset_tests::known_function_unit_container_and_nested_value_projection_route_to_refine ... ok
test ssa::refine_subset_tests::mixed_temporary_container_and_map_with_closures_route_to_refine ... ok
test ssa::refine_subset_tests::numeric_for_runtime_container_and_map_diverging_keys_route_to_refine ... ok
test ssa::refine_subset_tests::sequential_for_temporary_container_scope_has_field_replace_routing_pairs ... ok
test ssa::refine_subset_tests::temporary_container_size_and_map_get_single_eval_and_drop_points ... ok
test ssa::map_provider_exits_pipeline_tests::source_forms::owned_mutable_and_group_sources_preserve_verified_programs ... ok
test ssa::lower_frontend_tests::owned_source_return_tests::native::owned_source_return_native_post_loop_permissions ... ok
test ssa::refine_tests::container_tests::box_and_map_lower_in_refine ... ok
test native_map_provider_tests::resource_forms::single_existing_resource_representations_exit_normally ... ok
test ssa::refine_tests::container_tests::map_cross_function_routing_is_isolated ... ok
test ssa::refine_tests::container_tests::class_fields_rc_and_smart_cast_receivers_lower_in_refine ... ok
test native_map_provider_tests::resource_forms::unit_existing_resource_representations_exit_normally ... ok
test ssa::refine_tests::container_tests::container_for_and_runtime_generate_lower_in_refine ... ok
test native::unit_tests::container_size_tests::runtime_constructor_tests::runtime_constructor_native_unit_storable_helper_elements ... ok
test ssa::refine_tests::container_tests::deinit_nested_fields_box_payloads_and_move_receivers_lower_in_refine ... ok
test ssa::refine_tests::map_condition_tests::retired_known_function_shared_capture_callable_view_and_container_element_boundary_tests ... ok
test ssa::refine_tests::map_condition_tests::retired_numeric_for_runtime_container_map_get_and_consume_boundary_tests ... ok
test ssa::sequential_for_lowering_tests::determinism_tests::unit_storage_tests::unit_container_fresh_frontend_chains_render_identically ... ok
test ssa::sequential_for_lowering_tests::determinism_tests::unit_storage_tests::unit_container_nine_source_cases_preserve_factory_and_iteration_identities ... ok
test ssa::text_extension_tests::unit_generic_container_tests::unit_generic_container_array_signatures_preserve_concrete_ownership ... ok
test ssa::text_extension_tests::unit_generic_container_tests::unit_generic_container_borrow_size_uses_current_loan_after_cfg ... ok
test ssa::text_extension_tests::unit_generic_container_tests::unit_generic_container_list_signatures_preserve_concrete_ownership ... ok
test ssa::text_extension_tests::unit_generic_container_tests::unit_generic_container_mutable_list_signatures_preserve_concrete_ownership ... ok
test ssa::type_tests::unit_lower_container_size_tests::unit_container_size_borrow_uses_current_loan_after_cfg ... ok
test ssa::type_tests::unit_lower_container_size_tests::unit_container_size_reads_all_three_kinds_without_consuming_parameters ... ok
test ssa::type_tests::unit_lower_container_size_tests::unit_container_size_temporary_receiver_is_evaluated_once_and_cleanup_is_after_read ... ok
test ssa::unit_lower_container_tests::lowers_cross_file_container_construction_and_owner_transfer_deterministically ... ok
test ssa::unit_lower_container_tests::lowers_cross_file_mutable_list_add_to_container_append ... ok
test ssa::unit_lower_container_tests::mutable_list_clear_unit_lowers_to_container_clear ... ok
test ssa::unit_lower_container_tests::mutable_list_insert_at_compilation_unit_lowering_creates_container_insert_at ... ok
test ssa::unit_lower_container_tests::mutable_list_remove_at_compilation_unit_lowering_creates_container_remove_at ... ok
test ssa::unit_lower_container_tests::mutable_list_remove_first_compilation_unit_lowering_creates_container_remove_first ... ok
test ssa::unit_lower_container_tests::mutable_list_remove_last_compilation_unit_lowering_creates_container_remove_last ... ok
test ssa::unit_lower_container_tests::runtime_generator_unit_source_accepts_pointer_and_shared_initializers ... ok
test ssa::unit_lower_container_tests::runtime_helpers::runtime_generator_unit_caller_first_pointer_factory_keeps_return_identity ... ok
test ssa::unit_lower_container_tests::runtime_helpers::runtime_generator_unit_cross_file_generic_helper_preserves_three_callable_environments ... ok
test ssa::unit_lower_container_tests::runtime_helpers::runtime_generator_unit_nothing_operand_stops_before_borrow_and_initializer ... ok
test ssa::unit_lower_container_tests::runtime_helpers::runtime_generator_unit_unreachable_initializers_do_not_demand_callable_layouts ... ok
test ssa::unit_lower_container_tests::runtime_length_container_calls_a_view_parameter_initializer ... ok
test ssa::unit_lower_container_tests::runtime_matrix::runtime_generator_unit_matrix_preserves_element_capture_and_storage_contracts ... ok
test ssa::unit_lower_container_tests::unsupported_float_element_remains_an_explicit_boundary ... ok
test ssa::unit_plan::concrete_types::verify_map_tests::map_get_rejects_an_owned_copy_of_a_move_only_value ... ok
test ssa::unit_plan::concrete_types::verify_map_tests::map_get_requires_registered_result_identity_instead_of_bare_payload ... ok
test ssa::unit_plan::concrete_types::verify_map_tests::map_owned_result_requires_conditional_consuming_extraction ... ok
test ssa::unit_plan::concrete_types::verify_map_tests::map_queries_accept_a_shared_string_key_and_reject_exclusive_key ... ok
test ssa::unit_plan::concrete_types::verify_map_tests::map_result_registry_checks_presence_and_preserves_payload_ownership ... ok
test ssa::unit_plan::concrete_types::verify_map_tests::map_type_verifier_rejects_a_non_hashable_key ... ok
test ssa::unit_plan::recipe_validation::direct_field_tests::sibling_proof_does_not_allow_duplicate_exclusive_field_loans ... ok
test ssa::unit_plan::recipe_validation::direct_field_tests::sibling_proof_preserves_reborrow_but_never_root_or_owner_consumption ... ok
test ssa::unit_plan::recipe_validation::direct_field_tests::sibling_proof_rejects_mixed_cfg_paths_and_unseeded_cycles ... ok
test ssa::unit_plan_tests::body_type_budget::unit_generic_body_budget_later_sibling_recipe_precedes_zero_one_two_limit ... ok
test ssa::unit_plan_tests::body_type_budget::unit_generic_body_budget_zero_one_two_keep_pure_limit_template_spans ... ok
test ssa::unit_plan_tests::callable_identity::factory_return_queries::unit_callable_return_query_direct_constructor_demands_returned_named_source_body ... ok
test ssa::unit_plan_tests::callable_identity::factory_return_queries::unit_callable_return_query_direct_constructor_freezes_one_factory_owned_pointer ... ok
test ssa::unit_plan_tests::callable_identity::unit_callable_plan_dispatches_a_nothing_callee_but_skips_its_outer_call ... ok
test ssa::unit_plan_tests::callable_identity::unit_callable_plan_factory_memo_and_routes_share_the_returned_pointer_token ... ok
test ssa::unit_plan_tests::callable_identity::unit_callable_plan_frozen_named_routes_preserve_sparse_slots_and_owner_tokens ... ok
test ssa::unit_plan_tests::callable_identity::unit_callable_plan_function_valued_capture_has_no_selected_environment_layout ... ok
test ssa::unit_plan_tests::callable_identity::unit_callable_plan_keeps_static_routes_inside_lambda_and_hidden_deinit_bodies ... ok
test ssa::unit_plan_tests::callable_identity::unit_callable_plan_known_function_reference_requires_the_selected_source_body ... ok
test ssa::unit_plan_tests::callable_identity::unit_callable_plan_named_arguments_use_declaration_slots_instead_of_evaluation_order ... ok
test ssa::unit_plan_tests::callable_identity::unit_callable_plan_named_unknown_reports_only_the_actual_callback_operand ... ok
test ssa::unit_plan_tests::callable_identity::unit_callable_plan_pointer_factory_return_identity_is_memoized_per_source_instance ... ok
test ssa::unit_plan_tests::callable_identity::unit_callable_plan_preserves_earlier_helper_error_before_later_recipe_frontier ... ok
test ssa::unit_plan_tests::callable_identity::unit_callable_plan_same_origin_alias_and_recursive_parameter_reuse_one_instance ... ok
test ssa::unit_plan_tests::callable_identity::unit_callable_plan_three_environments_have_three_full_keys_and_stable_input_order ... ok
test ssa::unit_plan_tests::callable_identity::unit_callable_plan_three_parameter_forwarders_preserve_each_actual_environment ... ok
test ssa::unit_plan_tests::callable_identity::unit_callable_plan_unknown_legal_cfg_join_does_not_guess_a_pointer_environment ... ok
test ssa::unit_plan_tests::canonical_callables::canonical_callable_unit_missing_canonical_target_is_structured_and_readonly ... ok
test ssa::unit_plan_tests::canonical_callables::canonical_callable_unit_specialization_keeps_modes_move_and_container_return ... ok
test ssa::unit_plan_tests::delegation_routes::bodyful_requirement_chain_prefers_the_nested_route_over_an_inherited_default ... ok
test ssa::unit_plan_tests::delegation_routes::nested_delegate_local_override_terminates_the_outer_route ... ok
test ssa::unit_plan_tests::delegation_routes::plans_bodyful_delegation_from_frontend_effective_targets ... ok
test ssa::unit_plan_tests::delegation_routes::plans_concrete_override_instead_of_abstract_requirement ... ok
test ssa::unit_plan_tests::delegation_routes::plans_delegate_implementation_from_validated_forwarder_route ... ok
test ssa::unit_plan_tests::delegation_routes::plans_frontend_authorized_nested_generic_delegate_layout ... ok
test ssa::unit_plan_tests::delegation_routes::plans_identity_changing_delegation_chain_to_the_direct_endpoint ... ok
test ssa::unit_plan_tests::delegation_routes::plans_parameter_independent_generic_delegate_field ... ok
test ssa::unit_plan_tests::delegation_routes::plans_parameter_independent_generic_outer_receiver ... ok
test ssa::unit_plan_tests::delegation_routes::plans_same_requirement_delegation_chain_to_the_direct_implementation ... ok
test ssa::unit_plan_tests::delegation_routes::rejects_delegation_cycle_before_planning_a_partial_instance ... ok
test ssa::unit_plan_tests::delegation_routes::remaps_generic_delegation_owner_prefix_and_callable_suffix ... ok
test ssa::unit_plan_tests::delegation_routes::remaps_identity_changing_next_hop_owner_prefix_and_callable_suffix ... ok
test ssa::unit_plan_tests::entry_identity::nested_runtime_layout_requires_the_exact_frontend_owner_descriptor ... ok
test ssa::unit_plan_tests::entry_identity::recovery_owner_without_a_layout_cannot_become_executable_ssa ... ok
test ssa::unit_plan_tests::entry_identity::rejects_non_callable_entries_and_foreign_ownership_products ... ok
test ssa::unit_plan_tests::generic_containers::error_order::keeps_declaration_frontier_before_earlier_source_symbol_recipe ... ok
test ssa::unit_plan_tests::generic_containers::error_order::keeps_delegation_owner_field_error_before_endpoint_recipe_failure ... ok
test ssa::unit_plan_tests::generic_containers::error_order::keeps_earlier_call_error_before_later_recipe_failure ... ok
test ssa::unit_plan_tests::generic_containers::error_order::keeps_earlier_helper_error_before_later_recipe_failure ... ok
test ssa::unit_plan_tests::generic_containers::error_order::keeps_earlier_sibling_instance_limit_before_helper_error ... ok
test ssa::unit_plan_tests::generic_containers::error_order::keeps_earlier_unsupported_constructor_before_later_recipe_cycle ... ok
test ssa::unit_plan_tests::generic_containers::error_order::keeps_first_specialization_error_before_later_specialization_recipe ... ok
test ssa::unit_plan_tests::generic_containers::error_order::keeps_instance_limit_before_concrete_error_in_generic_body ... ok
test ssa::unit_plan_tests::generic_containers::error_order::rejects_later_sibling_recipe_before_current_instance_limit ... ok
test ssa::unit_plan_tests::generic_containers::error_order::selects_stable_recipe_root_across_limit_hit_siblings ... ok
test ssa::unit_plan_tests::generic_containers::unit_generic_container_body_only_uses_frontend_canonical_without_extending_arena ... ok
test ssa::unit_plan_tests::generic_containers::unit_generic_container_direct_parameter_accepts_existing_concrete_nested_argument ... ok
test ssa::unit_plan_tests::generic_containers::unit_generic_container_instances_keep_source_parameters_and_deduplicate ... ok
test ssa::unit_plan_tests::generic_containers::unit_generic_container_missing_substitution_keeps_actual_parameter_span ... ok
test ssa::unit_plan_tests::generic_containers::unit_generic_container_recursive_template_stays_unsupported_with_canonical_present ... ok
test ssa::unit_plan_tests::instances::canonicalizes_generic_instances_and_is_input_order_independent ... ok
test ssa::unit_plan_tests::instances::plans_one_interface_default_instance_per_concrete_static_self ... ok
test ssa::unit_plan_tests::instances::plans_only_cross_file_reachable_functions_and_deduplicates_recursion ... ok
test ssa::unit_plan_tests::instances::plans_reachable_member_instances_with_owner_and_callable_arguments ... ok
test ssa::unit_plan_tests::layout_demand::merges_dependent_inherited_runtime_demand_independent_of_input_order ... ok
test ssa::unit_plan_tests::layout_demand::plans_dependent_inherited_owner_recipe_as_instance_key_only ... ok
test ssa::unit_plan_tests::layout_demand::recipe_cycles::keeps_closed_descriptor_unsupported_out_of_cycle_failures ... ok
test ssa::unit_plan_tests::layout_demand::recipe_cycles::keeps_generic_delegation_local_override_out_of_recipe_failures ... ok
test ssa::unit_plan_tests::layout_demand::recipe_cycles::rejects_fixed_argument_recipe_scc_before_instance_limit ... ok
test ssa::unit_plan_tests::layout_demand::recipe_cycles::rejects_generic_delegation_recipe_across_helper_before_instance_limit ... ok
test ssa::unit_plan_tests::layout_demand::recipe_cycles::rejects_generic_delegation_recipe_before_instance_limit ... ok
test ssa::unit_plan_tests::layout_demand::recipe_cycles::rejects_generic_helper_recipe_before_instance_limit ... ok
test ssa::unit_plan_tests::layout_demand::recipe_cycles::rejects_parameter_growing_recipe_at_delegation_endpoint ... ok
test ssa::unit_plan_tests::layout_demand::recipe_cycles::rejects_parameter_growing_recipe_at_stable_back_edge_across_input_order ... ok
test ssa::unit_plan_tests::layout_demand::upgrades_dependent_inherited_owner_recipe_to_runtime_layout_required ... ok
test ssa::unit_plan_tests::layout_demand::upgrades_dependent_inherited_owner_recipe_used_only_by_enum_payload ... ok
test ssa::unit_plan_tests::layout_demand::upgrades_dependent_inherited_owner_recipe_used_only_by_lambda_abi ... ok
test ssa::unit_plan_tests::owner_recipes::delegated_dispatch_owner_recipe_keeps_unsupported_nested_kinds_closed ... ok
test ssa::unit_plan_tests::owner_recipes::inherited_dispatch_owner_recipe_keeps_dependent_kinds_and_missing_canonical_closed ... ok
test ssa::unit_plan_tests::owner_recipes::remaps_inherited_default_owner_recipe_and_callable_slots ... ok
test ssa::unit_plan_tests::owner_recipes::remaps_list_inherited_owner_recipe_to_the_effective_default ... ok
test ssa::unit_plan_tests::owner_recipes::remaps_requirement_arguments_to_concrete_owner_and_callable_slots ... ok
test unit_container_origin_bridge_tests::container_origin_does_not_steal_its_nominal_element_declaration ... ok
test unit_container_origin_bridge_tests::real_unit_container_origins_bridge_stride_failures_without_nominal_labels ... ok
test unit_container_origin_bridge_tests::view_only_unit_parameter_supplies_its_synthetic_list_layout_origin ... ok
test ssa::refine_tests::container_tests::map_iteration_require_value_range_view_and_control_edges_lower_in_refine ... ok
test ssa::refine_tests::container_tests::map_ops_general_values_and_unit_root_primitives_lower_in_refine ... ok
test ssa::refine_tests::container_tests::mid_construction_exits_and_pending_cleanup_lower_in_refine ... ok
test ssa::refine_tests::container_tests::move_only_reassignment_and_move_enum_when_lower_in_refine ... ok
test ssa::refine_tests::container_tests::sequential_containers_lower_in_refine ... ok
test native_map_provider_tests::source_forms::single_owned_mutable_and_group_sources_exit_normally ... ok
test native_map_provider_tests::source_forms::unit_owned_mutable_and_group_sources_exit_normally ... ok
test native_tests::sequential_for_tests::owned_source_tests::owned_and_borrow_sources_cover_three_containers_sizes_and_exits ... ok
test native::unit_tests::container_size_tests::runtime_constructor_tests::runtime_constructor_native_unit_matrix_executes_logical_callbacks ... ok

test result: ok. 411 passed; 0 failed; 0 ignored; 0 measured; 2645 filtered out; finished in 757.08s = `411 passed / 0 failed`` |

宿主核对说明：

- `ssa/map_provider_exits_pipeline_tests/source_forms/graph.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/native_tests/map_sibling_route_tests.rs`（`11` 行）
