//! Receiver ABI fixtures, not evidence for the public byte-count or overflow algorithms.
use super::{model::*, unit_lower_test_support as support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

#[test]
fn unit_text_receiver_is_a_shared_first_argument_with_an_owned_result() {
    let mut sources = SourceMap::new();
    let (text_id, text) = support::parsed(
        &mut sources,
        "text.ko",
        "package koven.text\nborrow fun String.byteLength(): Int = 7\nborrow fun String.checkedPlus(other: String): String? = this + other",
    );
    let (app_id, app) = support::parsed(
        &mut sources,
        "app.ko",
        "package app\nimport koven.text.byteLength\nimport koven.text.checkedPlus\nfun main(): Unit { val source = \"a\".clone(); if (source.byteLength() != 7) { error(\"length\") }; println(\"7\"); val joined = source.checkedPlus(\"b\"); if (joined != null) { println(joined) }; println(source) }",
    );
    let inputs = [
        SourceUnitInput::new("std", "koven/text/text.ko", text_id, &text),
        SourceUnitInput::new("app", "app/Main.ko", app_id, &app),
    ];
    let (environment, mut types) = standard_environments();
    types.authorize_text_source(&sources, text_id).unwrap();
    let (names, typed, owned) = support::analyze(&sources, &inputs, &environment, &types);
    let (program, entry) = super::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        support::declaration(&names, "app", "main"),
    )
    .unwrap();
    super::verify::verify_program(&program).unwrap();
    let module = &program.modules[0];
    for spelling in ["byteLength", "checkedPlus"] {
        let function = module
            .functions
            .iter()
            .find(|f| f.name.contains(spelling))
            .unwrap();
        assert!(function.receiver.is_none());
        assert!(function.carrier_return.is_none());
        assert!(function.borrow_return.is_none());
        let entry = function.block(function.entry_block().unwrap()).unwrap();
        let EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        } = function.entity(entry.parameters[0]).unwrap().ty
        else {
            panic!("text receiver must be a shared entry loan");
        };
        assert_eq!(module.types[target.index()], SsaTypeKind::StringOwner);
        let calls = module.functions.iter().flat_map(|f| &f.instructions).filter(|i| matches!(&i.operation, Operation::DirectCall { callee, receiver: None, .. } if *callee == function.id)).count();
        assert_eq!(
            calls, 1,
            "{spelling} uses a static call with an explicit borrowed first slot"
        );
    }
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run =
        crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[1, 0]);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"7\nab\na\n");
}

/// Byte offsets must count UTF-8 bytes and retain embedded NUL, without consuming the owner.
#[test]
fn trusted_text_storage_reads_preserve_utf8_bytes_and_the_source_owner() {
    let mut sources = SourceMap::new();
    let (text_id, text) = support::parsed(
        &mut sources,
        "text.ko",
        r#"package koven.text
borrow fun String.byteLength(): Int = textByteLength(this)
fun inspect(text: String): Unit {
    if (textByteLength(text) != 4) { error("length") }
    val ascii: UByte = 65u
    if (textByteAt(text, 0) != ascii) { error("ascii") }
    val nul: UByte = 0u
    if (textByteAt(text, 1) != nul) { error("nul") }
    val lead: UByte = 195u
    if (textByteAt(text, 2) != lead) { error("lead") }
    val continuation: UByte = 169u
    if (textByteAt(text, 3) != continuation) { error("continuation") }
}
"#,
    );
    let (app_id, app) = support::parsed(
        &mut sources,
        "app.ko",
        r#"package app
import koven.text.inspect
import koven.text.byteLength
fun main(): Unit {
    val text = "A\0é".clone()
    inspect(text)
    if (text.byteLength() != 4) { error("length") }
    println("4")
    println(text)
    if ("".byteLength() != 0) { error("empty") }
    println("0")
}
"#,
    );
    let inputs = [
        SourceUnitInput::new("std", "koven/text/text.ko", text_id, &text),
        SourceUnitInput::new("app", "app/Main.ko", app_id, &app),
    ];
    let (environment, mut types) = standard_environments();
    types.authorize_text_source(&sources, text_id).unwrap();
    let (names, typed, owned) = support::analyze(&sources, &inputs, &environment, &types);
    let (program, entry) = super::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        support::declaration(&names, "app", "main"),
    )
    .unwrap();
    super::verify::verify_program(&program).unwrap();
    let operations: Vec<_> = program.modules[0]
        .functions
        .iter()
        .flat_map(|f| &f.instructions)
        .map(|i| &i.operation)
        .collect();
    assert_eq!(
        operations
            .iter()
            .filter(|op| matches!(op, Operation::StringByteLength { .. }))
            .count(),
        2
    );
    assert_eq!(
        operations
            .iter()
            .filter(|op| matches!(op, Operation::StringByteAt { .. }))
            .count(),
        4
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[0]);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"4\nA\0\xc3\xa9\n0\n");
}

#[test]
fn trusted_utf8_scan_reports_sequence_start_and_preserves_byte_array() {
    let mut sources = SourceMap::new();
    let (text_id, text) = support::parsed(
        &mut sources,
        "text.ko",
        r#"package koven.text
fun checkBytes(): Unit {
    val bytes = Array<UByte>(3, { index -> if (index == 0) 65u else if (index == 1) 226u else 130u })
    if (textUtf8ErrorOffset(bytes) != 1) { error("truncated sequence start") }
    if (bytes.size != 3) { error("source retained") }
    if (textUtf8ErrorOffset(Array<UByte>(0, { index -> 0u })) != -1) { error("empty") }
    if (textUtf8ErrorOffset(Array<UByte>(1, { index -> 0u })) != -1) { error("nul") }
    println("utf8")
}
"#,
    );
    let (app_id, app) = support::parsed(
        &mut sources,
        "app.ko",
        "package app\nimport koven.text.checkBytes\nfun main(): Unit { checkBytes() }",
    );
    let inputs = [
        SourceUnitInput::new("std", "koven/text/text.ko", text_id, &text),
        SourceUnitInput::new("app", "app/Main.ko", app_id, &app),
    ];
    let (environment, mut types) = standard_environments();
    types.authorize_text_source(&sources, text_id).unwrap();
    let (names, typed, owned) = support::analyze(&sources, &inputs, &environment, &types);
    let (program, entry) = super::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        support::declaration(&names, "app", "main"),
    )
    .unwrap();
    super::verify::verify_program(&program).unwrap();
    assert_eq!(
        program.modules[0]
            .functions
            .iter()
            .flat_map(|f| &f.instructions)
            .filter(|i| matches!(i.operation, Operation::Utf8ErrorOffset { .. }))
            .count(),
        3
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert_eq!(
        llvm.matches("define internal i64 @koven.utf8.error_offset(")
            .count(),
        1
    );
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 2);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"utf8\n");
}

#[test]
fn nullable_string_if_borrows_retest_the_current_owner_and_distinguish_empty_from_null() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;
    let source = UnitSource::analyze(
        "nullable-string.ko",
        r#"fun make(flag: Boolean): String? {
    if (flag) { return "value".clone() }
    return null
}
fun show(flag: Boolean): Unit {
    val text = make(flag)
    if (text != null) { println(text); println(text) } else { println("null") }
    if (text == null) { println("absent") } else { println((text)) }
}
fun main(): Unit {
    show(true)
    show(false)
    val empty: String? = ""
    if (empty != null) { println(empty) }
}"#,
    )
    .expect("actual nullable String source passes admission");
    let (program, entry) = super::refine::with_per_family_lowering(|| {
        source.lower_entry(source.declaration("main", SymbolKind::Function))
    })
    .expect("each non-null branch borrows its current String owner");
    super::verify::verify_program(&program).unwrap();
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 1);
    crate::native_tests::boxed_enum_tests::assert_success(
        &run,
        b"value\nvalue\nvalue\nnull\nabsent\n\n",
    );
}

#[test]
fn per_family_nullable_string_if_keeps_nested_and_prefix_carriers_rejected() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;
    for (name, text) in [
        (
            "nested",
            r#"fun entry(own text: String?, own other: String?): Unit {
    if (text != null) {
        println(text)
        if (other != null) { println(other) }
        println(text)
    }
}"#,
        ),
        (
            "prefix",
            r#"fun consume(marker: Int, value: Int): Unit {}
fun entry(own text: String?): Unit {
    consume(1, if (true) { if (text != null) { println(text) }; 2 } else { 3 })
}"#,
        ),
    ] {
        let source =
            UnitSource::analyze(name, text).expect("rejection begins after actual admission");
        // The general refinement entry can support additional source shapes. These
        // negative cases check only the per-family proof path changed in this slice.
        let result = super::refine::with_per_family_lowering(|| {
            source.lower_entry(source.declaration("entry", SymbolKind::Function))
        });
        let error = match result {
            Ok(_) => panic!("{name} must not bypass the existing proof transfer boundary"),
            Err(error) => error,
        };
        assert!(
            matches!(
                error.kind,
                super::LoweringErrorKind::MissingFact | super::LoweringErrorKind::UnsupportedNode
            ),
            "{name}: {error:?}"
        );
    }
}

#[test]
fn per_family_nullable_string_if_returns_the_owned_parameter_after_borrow() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;
    let source = UnitSource::analyze(
        "nullable-owned-return.ko",
        r#"fun entry(own text: String?): String {
    if (text != null) { println(text); return text }
    return "fallback"
}
fun make(flag: Boolean): String? {
    if (flag) { return "value".clone() }
    return null
}
fun main(): Unit {
    val present = make(true)
    println(entry(present))
    val absent = make(false)
    println(entry(absent))
}"#,
    )
    .unwrap();
    let (program, entry) = super::refine::with_per_family_lowering(|| {
        source.lower_entry(source.declaration("main", SymbolKind::Function))
    })
    .unwrap();
    super::verify::verify_program(&program).unwrap();
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 1);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"value\nvalue\nfallback\n");
}
