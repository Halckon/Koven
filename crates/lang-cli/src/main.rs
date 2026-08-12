//! `kovenc` 编译器命令行入口。

// SPEC-0003 先建立纯 renderer；CLI 流水线接入前只由本 target 的邻近测试调用。
#[allow(
    dead_code,
    reason = "the Phase 0 renderer precedes CLI pipeline wiring"
)]
mod diagnostic_renderer;

fn main() {}
