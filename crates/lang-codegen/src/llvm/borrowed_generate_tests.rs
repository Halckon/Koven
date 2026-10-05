use super::render_verified_program;
use crate::ssa::borrowed_generate_tests::{Layout, dynamic_fixture, fixture};

#[test]
fn borrowed_generation_prepares_fixed_storage_once_outside_the_loop() {
    for layout in [Layout::Pointer, Layout::Shared, Layout::Owned] {
        for size in [0, 1, 3] {
            let fixture = fixture(layout, size, false);
            let llvm = render_verified_program(&fixture.program)
                .expect("borrowed generator LLVM must verify");
            let make = llvm
                .split("@f1.make()")
                .nth(1)
                .expect("make")
                .split("\n}")
                .next()
                .expect("body");
            let body = make
                .split(".body:")
                .nth(1)
                .expect("loop body")
                .split(".done:")
                .next()
                .expect("done");
            assert!(
                !body.contains("alloca"),
                "loop storage must remain bounded: {llvm}"
            );
            assert_eq!(
                make.matches(".index.storage = alloca i32").count(),
                1,
                "{llvm}"
            );
            assert_eq!(
                make.matches(".environment.storage = alloca").count(),
                usize::from(!matches!(layout, Layout::Pointer)),
                "{llvm}"
            );
            assert!(
                body.contains("call i32"),
                "callback stays in guarded loop: {llvm}"
            );
            assert!(make.contains("icmp ult i64"));

            assert!(make.contains("llvm.umul.with.overflow"));
        }
    }
}

#[test]
fn borrowed_unit_void_generation_calls_each_logical_index_without_buffer_addressing() {
    let fixture = fixture(Layout::Pointer, 3, true);
    let llvm = render_verified_program(&fixture.program).expect("Unit LLVM must verify");
    let make = llvm
        .split("@f1.make()")
        .nth(1)
        .expect("make")
        .split("\n}")
        .next()
        .expect("body");
    assert!(
        make.contains("call void %"),
        "logical Unit callback must be invoked: {llvm}"
    );
    assert!(!make.contains("getelementptr"));
    assert!(!make.contains("call ptr @malloc"));
}

#[test]
fn borrowed_generation_dynamic_length_keeps_checked_length_and_bytes_guards() {
    let fixture = dynamic_fixture(Layout::Pointer);
    let llvm = render_verified_program(&fixture.program).expect("dynamic borrowed generator LLVM");
    let make = llvm
        .split("@f1.make(")
        .nth(1)
        .expect("make")
        .split("\n}")
        .next()
        .expect("body");
    assert!(
        make.contains("icmp slt i32"),
        "negative length rejected before conversion: {llvm}"
    );
    assert!(
        make.contains("zext i32"),
        "nonnegative logical length becomes size_t: {llvm}"
    );
    assert!(
        make.contains("llvm.umul.with.overflow"),
        "physical bytes remain checked: {llvm}"
    );
    let preheader = make.split(".loop:").next().expect("preheader");
    assert!(
        !preheader.contains("call i32 %"),
        "no callback before allocation succeeds: {llvm}"
    );
}
