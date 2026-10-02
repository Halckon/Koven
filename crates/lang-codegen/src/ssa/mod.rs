//! Target-independent typed SSA model、验证与确定性调试表示。

mod closure;
mod integer;
mod lower_frontend;
pub(crate) use lower_frontend::orchestrate::lower_scalar_file_with_entry;
pub(crate) use lower_frontend::{LoweringError, LoweringErrorKind};
pub(crate) mod model;
// SPEC-0182 将在接入真实 for lowering 时消费本阶段验证的构造器。
#[allow(dead_code)]
pub(crate) mod provider;
mod render;
#[cfg(test)]
pub(crate) fn render_program(program: &model::Program) -> String {
    render::render_program(program)
}
mod types;
pub(crate) mod unit_lower;
pub(crate) mod unit_plan;
pub(crate) mod verify;
mod verify_operation;
mod verify_ownership;
mod verify_types;

#[cfg(test)]
mod aggregate_operation_tests;
#[cfg(test)]
mod borrowed_container_lowering_tests;
#[cfg(test)]
mod closure_operation_tests;
#[cfg(test)]
mod container_lowering_tests;
#[cfg(test)]
mod container_operation_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod type_tests;
#[cfg(test)]
mod unit_llvm_tests;
#[cfg(test)]
mod unit_lower_aggregate_tests;
#[cfg(test)]
mod unit_lower_assignment_tests;
#[cfg(test)]
mod unit_lower_borrow_tests;
#[cfg(test)]
mod unit_lower_closure_tests;
#[cfg(test)]
mod unit_lower_container_element_tests;
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
mod unit_root_primitive_tests;
