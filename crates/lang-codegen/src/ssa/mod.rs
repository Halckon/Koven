//! Target-independent typed SSA model、验证与确定性调试表示。

mod model;
mod render;
mod verify;
mod verify_operation;
mod verify_ownership;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod verify_tests;

#[cfg(test)]
mod verify_ownership_tests;

#[cfg(test)]
mod verify_scalar_tests;
