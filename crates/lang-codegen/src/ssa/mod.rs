//! Target-independent typed SSA model、验证与确定性调试表示。

mod carry;
mod closure;
mod constant_materialization;
mod deinit;
mod expression_core;
mod integer;
mod integer_range;
mod integer_range_type;
mod lower_frontend;
mod map_result;
mod map_source;
#[cfg(test)]
pub(crate) use lower_frontend::orchestrate::lower_scalar_file_with_entry;
mod lowering_support;
pub(crate) use lowering_support::{LoweringError, LoweringErrorKind};
pub(crate) mod model;
// SPEC-0182 将在接入真实 for lowering 时消费本阶段验证的构造器。
#[allow(dead_code)]
pub(crate) mod provider;
mod render;
mod statement_core;
#[cfg(test)]
pub(crate) fn render_program(program: &model::Program) -> String {
    render::render_program(program)
}
#[cfg(test)]
mod borrow_call_lowering_tests;
#[cfg(test)]
mod borrow_result_contract_tests;
mod borrow_result_support;
#[cfg(test)]
mod borrow_storage_contract_tests;
#[cfg(test)]
mod map_consume_fixture_tests;
#[cfg(test)]
mod map_consume_lowering_tests;
#[cfg(test)]
mod map_promotion_contract_tests;
#[cfg(test)]
mod nullable_map_consume_fixture_tests;
#[cfg(test)]
mod nullable_map_consume_lowering_tests;
#[cfg(test)]
pub(crate) use nullable_map_consume_lowering_tests::ordinary_nullable_consume_sources;
mod types;
pub(crate) mod unit_lower;
pub(crate) mod unit_plan;
mod unit_source_query;
#[cfg(test)]
mod unit_source_query_tests;
pub(crate) mod verify;
mod verify_borrow_result;
mod verify_operation;
mod verify_ownership;
mod verify_types;

#[cfg(test)]
mod aggregate_operation_tests;
#[cfg(test)]
mod borrowed_container_lowering_tests;
#[cfg(test)]
pub(crate) mod borrowed_generate_tests;
#[cfg(test)]
mod closure_operation_tests;
#[cfg(test)]
mod container_lowering_tests;
#[cfg(test)]
mod container_operation_tests;
#[cfg(test)]
mod mutable_list_delivery_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod type_tests;
#[cfg(test)]
mod unit_generic_container_tests;
#[cfg(test)]
mod unit_llvm_tests;
#[cfg(test)]
mod unit_lower_aggregate_tests;
#[cfg(test)]
mod unit_lower_assignment_tests;
#[cfg(test)]
mod unit_lower_borrow_string_tests;
#[cfg(test)]
mod unit_lower_borrow_tests;
#[cfg(test)]
mod unit_lower_closure_tests;
#[cfg(test)]
mod unit_lower_container_element_tests;
#[cfg(test)]
mod unit_lower_container_size_tests;
#[cfg(test)]
mod unit_lower_container_tests;
#[cfg(test)]
mod unit_lower_control_result_tests;
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
mod unit_lower_short_circuit_tests;
#[cfg(test)]
mod unit_lower_string_tests;
#[cfg(test)]
mod unit_lower_test_support;
#[cfg(test)]
mod unit_lower_tests;
#[cfg(test)]
mod unit_lower_type_plan_tests;
#[cfg(test)]
mod unit_lower_when_tests;
#[cfg(test)]
mod unit_map_tests;
#[cfg(test)]
mod unit_plan_tests;
#[cfg(test)]
mod unit_receiver_two_phase_tests;

#[cfg(test)]
mod verify_tests;

#[cfg(test)]
mod verify_ownership_tests;

#[cfg(test)]
mod verify_scalar_tests;

#[cfg(test)]
mod lower_frontend_tests;

#[cfg(test)]
mod borrow_result_boundary_tests;

#[cfg(test)]
mod nullable_operation_tests;
#[cfg(test)]
mod shared_owner_operation_tests;
#[cfg(test)]
mod string_operation_tests;

#[cfg(test)]
mod char_constant_tests;

#[cfg(test)]
mod sequential_for_lowering_tests;
#[cfg(test)]
mod unit_constant_tests;

#[cfg(test)]
mod bitwise_lowering_tests;
#[cfg(test)]
mod bitwise_operation_tests;
#[cfg(test)]
mod checked_integer_conversion_backend_tests;
#[cfg(test)]
mod checked_integer_conversion_contract_tests;
#[cfg(test)]
mod checked_integer_conversion_tests;
#[cfg(test)]
mod integer_core_contract_tests;

#[cfg(test)]
mod deinit_tests;

#[cfg(test)]
mod unit_root_primitive_tests;

#[cfg(test)]
mod unit_field_borrow_tests;
#[cfg(test)]
mod unit_field_replace_tests;

#[cfg(test)]
mod lowering_support_tests;

#[cfg(test)]
mod lowering_entry_contract_tests;

#[cfg(test)]
mod lowering_dependency_tests;

#[cfg(test)]
mod range_lowering_tests;

#[cfg(test)]
mod owned_null_cleanup_tests;

#[cfg(test)]
mod owned_map_borrow_tests;

#[cfg(test)]
mod ordered_map_storage_tests;

#[cfg(test)]
mod owned_map_borrow_boundary_tests;

#[cfg(test)]
mod multi_borrow_range_boundary_tests;
#[cfg(test)]
mod multi_borrow_tests;

#[cfg(test)]
mod mixed_borrow_boundary_tests;
#[cfg(test)]
mod mixed_borrow_tests;

#[cfg(test)]
mod nested_borrow_tests;

#[cfg(test)]
mod borrow_value_field_tests;

#[cfg(test)]
mod single_readonly_method_tests;

#[cfg(test)]
mod integer_range_contract_tests;

#[cfg(test)]
mod integer_range_pending_tests;

#[cfg(test)]
mod integer_range_value_tests;

#[cfg(test)]
mod numeric_iteration_tests;

#[cfg(test)]
mod while_flow_tests;

#[cfg(test)]
mod rc_nullable_while_observation_tests;

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
