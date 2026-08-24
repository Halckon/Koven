//! Target-independent typed SSA model、验证与确定性调试表示。

mod closure;
mod lower_frontend;
pub(crate) mod model;
mod render;
mod types;
pub(crate) mod verify;
mod verify_operation;
mod verify_ownership;
mod verify_types;

#[cfg(test)]
mod aggregate_operation_tests;
#[cfg(test)]
mod closure_operation_tests;
#[cfg(test)]
mod container_operation_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod type_tests;

#[cfg(test)]
mod verify_tests;

#[cfg(test)]
mod verify_ownership_tests;

#[cfg(test)]
mod verify_scalar_tests;

#[cfg(test)]
mod lower_frontend_tests;
