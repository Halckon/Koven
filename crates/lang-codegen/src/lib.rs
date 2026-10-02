//! Koven SSA 与本机代码生成的公共 crate 边界。

mod native;
#[cfg(test)]
mod native_tests;
#[cfg(test)]
mod test_support;

pub use native::{
    NativeEntry, NativeObjectError, NativeObjectErrorKind, NativeUnitEntry,
    emit_native_constant_unit_object, emit_native_object, emit_native_owned_unit_object,
    emit_native_unit_object,
};

// SPEC-0034 完成 SSA→LLVM adapter 后移除该暂时的未使用门禁。
#[allow(dead_code)]
mod llvm;

// SPEC-0033 只建立 crate 内部 SSA；SPEC-0034 接入 lowering 后移除该暂时的未使用门禁。
#[allow(dead_code)]
mod ssa;

#[cfg(test)]
mod bitwise_test_support;
