//! Target-independent typed SSA model、验证与确定性调试表示。

mod closure;
mod lower_frontend;
pub(crate) use lower_frontend::orchestrate::lower_scalar_file_with_entry;
pub(crate) use lower_frontend::{LoweringError, LoweringErrorKind};
pub(crate) mod model;
mod render;
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
mod unit_lower_loop_tests;
#[cfg(test)]
mod unit_lower_scalar_tests;
#[cfg(test)]
mod unit_lower_short_circuit_tests;
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
