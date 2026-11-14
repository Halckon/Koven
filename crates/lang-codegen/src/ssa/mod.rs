#[deny(dead_code)]
mod carry;
mod closure;
use closure::integer;
mod constant_materialization;
mod expression_core;
#[cfg(test)]
mod lower_frontend;
#[cfg(test)]
mod map_source;
#[deny(dead_code)]
mod refine;
#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use nullable_map_consume_fixture_tests::refine_string_when_tests;
#[cfg(test)]
pub(crate) use refine::loan_receipt::observe as observe_loan_receipts;
#[cfg(test)]
pub(crate) use refine::{
    OwnerFault, observe_owner_actions, observe_owner_calls, observe_owner_drops,
    observe_owner_returns, with_owner_fault,
};
#[cfg(test)]
mod refine_subset_tests;
#[cfg(test)]
mod refine_tests;
#[cfg(test)]
pub(crate) use lower_frontend::orchestrate::lower_scalar_file_with_entry;
#[cfg(test)]
pub(crate) use refine::with_per_family_lowering;
mod lowering_support;
pub(crate) use lowering_support::{LoweringError, LoweringErrorKind};
#[deny(dead_code)]
pub(crate) mod model;
pub(crate) mod provider;
#[cfg(test)]
mod render;
mod statement_core;
#[cfg(test)]
pub(crate) fn render_program(program: &model::Program) -> String {
    render::render_program(program)
}
#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use text_extension_tests::borrow_call_lowering_tests;
#[cfg(test)]
mod borrow_memory_test_support;
#[cfg(test)]
mod borrow_result_contract_tests;
mod borrow_result_support;
#[cfg(test)]
mod borrow_storage_contract_tests;
#[cfg(test)]
pub(in crate::ssa) mod loan_region_ssa_tests;
#[cfg(test)]
mod map_consume_fixture_tests;
#[cfg(test)]
pub(in crate::ssa) mod map_consume_lowering_tests;
#[cfg(test)]
pub(in crate::ssa) mod map_promotion_contract_tests;
#[cfg(test)]
pub(in crate::ssa) mod nullable_map_consume_fixture_tests;
#[cfg(test)]
pub(in crate::ssa) mod nullable_map_consume_lowering_tests;
#[cfg(test)]
pub(crate) use nullable_map_consume_lowering_tests::ordinary_nullable_consume_sources;
mod types;
#[deny(dead_code)]
pub(crate) mod unit_lower;
#[deny(dead_code)]
pub(crate) mod unit_plan;
pub(crate) mod verify;
mod verify_operation;
use verify_operation::verify_borrow_result;
mod verify_ownership;
mod verify_types;

#[cfg(test)]
mod aggregate_operation_tests;
#[cfg(test)]
pub(in crate::ssa) mod borrowed_container_lowering_tests;
#[cfg(test)]
pub(crate) mod borrowed_generate_tests;
#[cfg(test)]
mod closure_operation_tests;
#[cfg(test)]
mod container_lowering_tests;
#[cfg(test)]
mod container_operation_tests;
#[cfg(test)]
pub(in crate::ssa) mod mutable_list_delivery_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(in crate::ssa) mod type_tests;
#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use text_extension_tests::unit_generic_container_tests;
#[cfg(test)]
pub(in crate::ssa) mod unit_llvm_tests;
#[cfg(test)]
mod unit_lower_aggregate_tests;
#[cfg(test)]
pub(in crate::ssa) mod unit_lower_assignment_tests;
#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use map_consume_lowering_tests::unit_lower_borrow_string_tests;
#[cfg(test)]
mod unit_lower_borrow_tests;
#[cfg(test)]
mod unit_lower_closure_tests;
#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use owned_map_borrow_tests::unit_lower_container_element_tests;
#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use type_tests::unit_lower_container_size_tests;
#[cfg(test)]
mod unit_lower_container_tests;
#[cfg(test)]
pub(in crate::ssa) mod unit_lower_control_result_tests;
#[cfg(test)]
mod unit_lower_enum_tests;
#[cfg(test)]
mod unit_lower_loop_tests;
#[cfg(test)]
mod unit_lower_rc_tests;
#[cfg(test)]
mod unit_lower_receiver_tests;
#[cfg(test)]
mod unit_lower_scalar_tests;
#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use map_consume_lowering_tests::unit_lower_straight_line_tests;
#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use unit_field_borrow_tests::unit_lower_short_circuit_tests;
#[cfg(test)]
pub(in crate::ssa) mod unit_lower_string_tests;
#[cfg(test)]
mod unit_lower_test_support;
#[cfg(test)]
mod unit_lower_tests;
#[cfg(test)]
mod unit_lower_type_plan_tests;
#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use mutable_list_delivery_tests::unit_map_tests;
#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use rc_nullable_while_observation_tests::unit_lower_when_tests;
#[cfg(test)]
mod unit_plan_tests;
#[cfg(test)]
pub(in crate::ssa) mod unit_receiver_two_phase_tests;
#[cfg(test)]
mod unit_type_origin_tests;

#[cfg(test)]
mod verify_tests;

#[cfg(test)]
#[deny(dead_code)]
mod verify_ownership_tests;

#[cfg(test)]
mod refine_closure_tests;
#[cfg(test)]
mod verify_callable_view_tests;
#[cfg(test)]
mod verify_loan_mutate_tests;

#[cfg(test)]
mod verify_scalar_tests;

#[cfg(test)]
pub(in crate::ssa) mod lower_frontend_tests;

#[cfg(test)]
mod lower_frontend_scalar_abi_unit_comparison_tests;

#[cfg(test)]
mod lower_frontend_cfg_ownership_unit_comparison_tests;

#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use unit_lower_string_tests::lower_frontend_nullable_unit_comparison_tests;

#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use nullable_map_consume_lowering_tests::lower_frontend_rc_unit_comparison_tests;

#[cfg(test)]
pub(in crate::ssa) mod lower_frontend_borrow_unit_comparison_tests;

#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use unit_lower_assignment_tests::lower_frontend_string_io_unit_comparison_tests;

#[cfg(test)]
mod map_unit_consumer_test_support;

#[cfg(test)]
pub(in crate::ssa) mod borrow_result_boundary_tests;

#[cfg(test)]
mod nullable_operation_tests;
#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use type_tests::shared_owner_operation_tests;
#[cfg(test)]
mod string_operation_tests;

#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use bitwise_lowering_tests::char_constant_tests;

#[cfg(test)]
mod sequential_for_lowering_tests;
#[cfg(test)]
pub(in crate::ssa) mod unit_constant_tests;

#[cfg(test)]
pub(in crate::ssa) mod bitwise_lowering_tests;
#[cfg(test)]
pub(in crate::ssa) mod bitwise_operation_tests;
#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use numeric_iteration_tests::checked_integer_conversion_backend_tests;
#[cfg(test)]
pub(in crate::ssa) mod checked_integer_conversion_contract_tests;
#[cfg(test)]
pub(in crate::ssa) mod checked_integer_conversion_tests;
#[cfg(test)]
mod integer_core_contract_tests;

#[cfg(test)]
pub(in crate::ssa) mod deinit_tests;

#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use rc_nullable_while_bounded_tests::unit_root_primitive_tests;

#[cfg(test)]
pub(in crate::ssa) mod unit_field_borrow_tests;
#[cfg(test)]
mod unit_field_replace_tests;

#[cfg(test)]
pub(in crate::ssa) mod lowering_support_tests;

#[cfg(test)]
mod lowering_entry_contract_tests;

#[cfg(test)]
mod lowering_dependency_tests;

#[cfg(test)]
pub(in crate::ssa) mod range_lowering_tests;

#[cfg(test)]
mod text_copy_tests;
#[cfg(test)]
pub(in crate::ssa) mod text_extension_tests;

#[cfg(test)]
mod owned_null_cleanup_tests;

#[cfg(test)]
pub(in crate::ssa) mod owned_map_borrow_tests;

#[cfg(test)]
pub(in crate::ssa) mod ordered_map_storage_tests;

#[cfg(test)]
pub(in crate::ssa) mod owned_map_borrow_boundary_tests;

#[cfg(test)]
pub(in crate::ssa) mod multi_borrow_range_boundary_tests;
#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use bitwise_lowering_tests::multi_borrow_tests;

#[cfg(test)]
pub(in crate::ssa) mod mixed_borrow_boundary_tests;
#[cfg(test)]
mod mixed_borrow_tests;

#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use lower_frontend_borrow_unit_comparison_tests::nested_borrow_tests;

#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ssa) use while_flow_tests::borrow_value_field_tests;

#[cfg(test)]
mod single_readonly_method_tests;

#[cfg(test)]
pub(in crate::ssa) mod integer_range_contract_tests;

#[cfg(test)]
pub(in crate::ssa) mod integer_range_pending_tests;

#[cfg(test)]
mod integer_range_value_tests;

#[cfg(test)]
pub(in crate::ssa) mod numeric_iteration_tests;

#[cfg(test)]
pub(in crate::ssa) mod while_flow_tests;

#[cfg(test)]
pub(in crate::ssa) mod rc_nullable_while_observation_tests;

#[cfg(test)]
pub(crate) mod rc_nullable_while_bounded_tests;

#[cfg(test)]
mod map_iteration_stage_gate_tests;

#[cfg(test)]
pub(crate) mod map_provider_pipeline_tests;

#[cfg(test)]
pub(crate) mod map_provider_exits_pipeline_tests;

#[cfg(test)]
mod unit_lower_nullable_tests;

#[cfg(test)]
mod unit_lower_nullable_neighbor_tests;

#[cfg(test)]
mod unit_enum_regression_tests;

#[cfg(test)]
mod borrow_argument;

#[cfg(test)]
mod e2_two_source_transport_tests;
