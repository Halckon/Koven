//! Copy primitives must publish independent valid String owners and reject invalid storage input.
use super::{model::*, unit_lower_test_support as support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

fn lower_probe(body: &str) -> (Program, FunctionId) {
    let mut sources = SourceMap::new();
    let source = format!("package koven.text\nfun probe(): Unit {{ {body} }}");
    let (text_id, text) = support::parsed(&mut sources, "text.ko", &source);
    let (app_id, app) = support::parsed(
        &mut sources,
        "app.ko",
        "package app\nimport koven.text.probe\nfun main(): Unit { probe() }",
    );
    let inputs = [
        SourceUnitInput::new("std", "koven/text/text.ko", text_id, &text),
        SourceUnitInput::new("app", "app/Main.ko", app_id, &app),
    ];
    let (environment, mut types) = standard_environments();
    types.authorize_text_source(&sources, text_id).unwrap();
    let (names, typed, owned) = support::analyze(&sources, &inputs, &environment, &types);
    let result = super::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        support::declaration(&names, "app", "main"),
    )
    .unwrap();
    super::verify::verify_program(&result.0).unwrap();
    result
}

#[test]
fn text_copies_own_their_bytes_and_keep_input_storage_alive() {
    let (program, entry) = lower_probe(
        r#"
        val bytes = Array<UByte>(3, { index -> if (index == 0) 195 else if (index == 1) 169 else 0 })
        val text = textCopyBytes(bytes)
        val slice = textCopySlice(text, 0, 2)
        println(slice)
        println(text)
        if (bytes.size != 3) { error("array consumed") }
        println(textCopyBytes(Array<UByte>(0, { index -> 0 })))
        println(textCopySlice("é", 2, 2))
    "#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run =
        crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[2, 1, 0]);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"\xc3\xa9\n\xc3\xa9\0\n\n\n");
}

#[test]
fn text_copy_primitives_abort_before_publishing_invalid_strings() {
    use std::os::unix::process::ExitStatusExt;
    for (body, message, source_allocations) in [
        (
            "println(textCopyBytes(Array<UByte>(1, { index -> 128 })))",
            "invalid UTF-8",
            1,
        ),
        (
            "println(textCopySlice(\"é\", -1, 0))",
            "invalid byte range",
            0,
        ),
        (
            "println(textCopySlice(\"é\", 0, 3))",
            "invalid byte range",
            0,
        ),
        (
            "println(textCopySlice(\"é\", 2, 1))",
            "invalid byte range",
            0,
        ),
        (
            "println(textCopySlice(\"é\", 1, 1))",
            "not a scalar boundary",
            0,
        ),
        (
            "println(textCopySlice(\"é\", 0, 1))",
            "not a scalar boundary",
            0,
        ),
        (
            "println(textCopySlice(\"é\", 1, 2))",
            "not a scalar boundary",
            0,
        ),
    ] {
        let (program, entry) = lower_probe(body);
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(
            &llvm,
            source_allocations,
        );
        assert_eq!(run.status.signal(), Some(6), "{body}: {run:?}");
        assert!(
            String::from_utf8_lossy(&run.stderr).contains(message),
            "{body}: {run:?}"
        );
    }
}
