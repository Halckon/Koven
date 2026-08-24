//! Koven SSA 与本机代码生成的公共 crate 边界。

// SPEC-0033 只建立 crate 内部 SSA；SPEC-0034 接入 lowering 后移除该暂时的未使用门禁。
#[allow(dead_code)]
mod ssa;
