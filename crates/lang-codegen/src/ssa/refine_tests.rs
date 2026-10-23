//! SPEC-0350 refinement layer: inline nullable functions lower through slots and liveness.
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{SsaTypeKind, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

fn run(source: &str) -> (String, Option<i32>) {
    let output = crate::native_tests::emit_link_and_run("refine.ko", source, "main");
    assert!(
        output.stderr.is_empty() || !output.status.success(),
        "{output:?}"
    );
    (
        String::from_utf8(output.stdout).expect("utf-8 stdout"),
        output.status.code(),
    )
}

/// Lowers a single-source unit twice (forward and reversed inputs are identical here)
/// and returns the rendered program.
fn render(source: &str) -> String {
    let mut sources = SourceMap::new();
    let text = format!("package p\n{source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "p", "main");
    let lower = || {
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("refined program lowers and verifies")
    };
    let (program, _) = lower();
    let (again, _) = lower();
    let rendered = render_program(&program);
    assert_eq!(rendered, render_program(&again), "deterministic output");
    let module = &program.modules[0];
    assert!(
        module
            .types
            .iter()
            .any(|kind| matches!(kind, SsaTypeKind::InlineNullable { .. })),
        "{rendered}"
    );
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.blocks.iter().any(|block| matches!(
                block.terminator.as_ref().map(|terminator| &terminator.kind),
                Some(TerminatorKind::NullableBranch { .. })
            ))),
        "{rendered}"
    );
    rendered
}

#[test]
fn nullable_int_local_smart_cast_runs() {
    let source = r#"fun main(): Unit { val a: Int? = 5; if (a != null) { if (a == 5) { println("nint") } } }"#;
    render(source);
    assert_eq!(run(source), ("nint\n".to_owned(), Some(0)));
}

#[test]
fn nullable_int_while_reassignment_carries_loop_state() {
    let source = r#"fun main(): Unit {
        var x: Int? = 5
        var total = 0
        while (x != null) { total = total + x; x = null }
        if (total == 5) { println("while-nullable") }
    }"#;
    render(source);
    assert_eq!(run(source), ("while-nullable\n".to_owned(), Some(0)));
}

#[test]
fn nullable_join_return_and_short_circuit_proof() {
    let source = r#"fun pick(c: Boolean): Int? = if (c) 7 else null
    fun main(): Unit {
        val x = pick(true)
        if (x != null && x == 7) { println("join-nullable") }
        val y = pick(false)
        if (y == null || y == 0) { println("absent") }
    }"#;
    render(source);
    assert_eq!(run(source), ("join-nullable\nabsent\n".to_owned(), Some(0)));
}

#[test]
fn nullable_value_parameter_and_null_comparison_value() {
    let source = r#"fun describe(own n: Int?): Boolean { val present = n != null; return present }
    fun main(): Unit {
        if (describe(3)) { println("some") }
        if (!describe(null)) { println("none") }
    }"#;
    render(source);
    assert_eq!(run(source), ("some\nnone\n".to_owned(), Some(0)));
}

#[test]
fn checked_arithmetic_in_refined_function_aborts_with_message() {
    let source = r#"fun main(): Unit {
        var x: Int? = 2147483647
        if (x != null) { val y = x + 1; if (y == 0) { println("unreachable") } }
    }"#;
    let output = crate::native_tests::emit_link_and_run("refine.ko", source, "main");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("abort: integer overflow in +\n  at "),
        "{stderr:?}"
    );
}
