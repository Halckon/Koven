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
        val bytes = Array<UByte>(3, { index -> if (index == 0) 195u else if (index == 1) 169u else 0u })
        val text = textCopyBytes(bytes)
        val slice = textCopySlice(text, 0, 2)
        println(slice)
        println(text)
        if (bytes.size != 3) { error("array consumed") }
        println(textCopyBytes(Array<UByte>(0, { index -> 0u })))
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
            "println(textCopyBytes(Array<UByte>(1, { index -> 128u })))",
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

fn lower_public_text_program(source: &str) -> (Program, FunctionId) {
    try_lower_public_text_program(source)
        .unwrap_or_else(|error| panic!("public text lowering: {error:?}; source={source}"))
}

fn try_lower_public_text_program(
    source: &str,
) -> Result<(Program, FunctionId), super::LoweringError> {
    let mut sources = SourceMap::new();
    let (result_id, result) = support::parsed(
        &mut sources,
        "result.ko",
        include_str!("../../../lang-std/koven/core/result.ko"),
    );
    let (text_id, text) = support::parsed(
        &mut sources,
        "text.ko",
        include_str!("../../../lang-std/koven/text/text.ko"),
    );
    let (app_id, app) = support::parsed(&mut sources, "app.ko", source);
    let inputs = [
        SourceUnitInput::new("std", "koven/core/result.ko", result_id, &result),
        SourceUnitInput::new("std", "koven/text/text.ko", text_id, &text),
        SourceUnitInput::new("app", "app/Main.ko", app_id, &app),
    ];
    let (mut names, mut types) = standard_environments();
    names.authorize_builtin_source(&sources, result_id).unwrap();
    types.authorize_text_source(&sources, text_id).unwrap();
    let (names, typed, owned) = support::analyze(&sources, &inputs, &names, &types);
    super::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        support::declaration(&names, "app", "main"),
    )
}

#[test]
fn public_text_field_receiver_cleanup_and_result_storage_are_counted() {
    let source = r#"package app
import koven.text.*
class Holder(val text: String)
fun field(flag: Boolean): Unit {
    val holder = Holder("field".clone())
    val joined = (holder.text).checkedPlus(if (flag) { "!" } else { "?" })
    if (joined != null) { println(joined) }
    println(holder.text)
}
fun transfer(flag: Boolean): Result<String, TextError> {
    val result = "early".sliceBytes(0, 5)
    if (flag) { return result }
    return result
}
fun transferred(flag: Boolean): Unit {
    val result = transfer(flag)
    when (result) {
        is Ok -> println(result.success)
        is Err -> error("transfer failed")
    }
}
fun main(): Unit {
    field(true)
    field(false)
    transferred(true)
    transferred(false)
    val results = Array<Result<String, TextError>>(2, { index -> "stored".sliceBytes(0, 6) })
    if (results.size != 2) { error("container size") }
    println("container")
}
"#;
    let (program, entry) = lower_public_text_program(source);
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    // Two field owners + two class allocations + two joined strings, two returned
    // strings, one array and two stored strings: every live pointer must be freed once.
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 11);
    crate::native_tests::boxed_enum_tests::assert_success(
        &run,
        b"field!\nfield\nfield?\nfield\nearly\nearly\ncontainer\n",
    );
}

#[test]
fn public_text_field_receiver_keeps_sibling_replacement_guard() {
    let source = r#"package app
import koven.text.checkedPlus
class Holder(val text: String, var other: String)
fun main(): Unit {
    val holder = Holder("field".clone(), "old".clone())
    val joined = holder.text.checkedPlus(replace(&holder.other, "new".clone()))
    if (joined != null) { println(joined) }
}

"#;
    let error = try_lower_public_text_program(source)
        .err()
        .expect("shared field conflict");
    assert_eq!(error.kind, super::LoweringErrorKind::UnsupportedNode);
    let span = error.span.unwrap();
    assert_eq!(&source[span.start()..span.end()], "&holder.other");
}

#[test]
fn public_text_field_receiver_retains_existing_argument_return_boundary() {
    let source = r#"package app
import koven.text.checkedPlus
class Holder(val text: String)
fun main(): Unit {
    val holder = Holder("field".clone())
    val joined = holder.text.checkedPlus(if (true) { return } else { "!" })
}
"#;
    let error = try_lower_public_text_program(source)
        .err()
        .expect("receiver control prefix remains closed");
    assert_eq!(error.kind, super::LoweringErrorKind::UnsupportedNode);
    let span = error.span.unwrap();
    assert_eq!(
        &source[span.start()..span.end()],
        "holder.text.checkedPlus(if (true) { return } else { \"!\" })"
    );
}

#[test]
fn public_decode_result_borrows_payload_and_releases_independent_copy() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.decodeUtf8
fun main(): Unit {
    val bytes = Array<UByte>(3, { index -> if (index == 0) 195u else if (index == 1) 169u else 0u })
    val decoded = decodeUtf8(bytes)
    when (decoded) {
        is Ok -> println(decoded.success)
        is Err -> error("valid UTF-8 rejected")
    }
    if (bytes.size != 3) { error("input array consumed") }
}"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run =
        crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[1, 0]);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"\xc3\xa9\0\n");
}
