//! Text extensions require host authority and preserve ordinary owned results.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        CallableResultSource, CompilationUnitTypes, TextExtensionKind, TypedFile,
        check_compilation_unit_types, check_types, standard_environments,
    },
};

fn checked(text: &str, authorized: bool) -> (TypedFile, CompilationUnitTypes) {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "koven-std/koven/text/text.ko",
            format!("package koven.text\n{text}"),
        )
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, mut types) = standard_environments();
    if authorized {
        types.authorize_text_source(&sources, source).unwrap();
    }
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let single = check_types(&sources, &parsed, &names, &types).unwrap();
    let inputs = [SourceUnitInput::new(
        "koven-std",
        "koven/text/text.ko",
        source,
        &parsed,
    )];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    (single, unit)
}

#[test]
fn text_authority_belongs_to_the_exact_source_map_and_does_not_grant_range_access() {
    let mut sources = SourceMap::new();
    let trusted = sources.add_source("trusted.ko", "").unwrap();
    let forged = sources
        .add_source("koven-std/koven/text/text.ko", "")
        .unwrap();
    let mut foreign = SourceMap::new();
    let other = foreign.add_source("trusted.ko", "").unwrap();
    let (_, mut types) = standard_environments();
    assert!(types.authorize_text_source(&sources, other).is_err());
    types.authorize_text_source(&sources, trusted).unwrap();
    types.authorize_text_source(&sources, trusted).unwrap();
    assert!(types.is_authorized_text_source(trusted));
    assert!(!types.is_authorized_text_source(forged));
    assert!(!types.is_authorized_text_source(other));
    assert!(!types.is_authorized_range_source(trusted));
    assert!(!types.is_authorized_range_extension_source(trusted));
}

#[test]
fn trusted_text_receiver_uses_the_canonical_callable_and_owned_result() {
    let (single, unit) = checked(
        "borrow fun String.byteLength(): Int = 0\nfun run(text: String): Int = text.byteLength()",
        true,
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    let callable = single
        .callables()
        .iter()
        .find(|c| c.text_extension().is_some())
        .unwrap();
    let binding = callable.text_extension().unwrap();
    assert_eq!(binding.kind(), TextExtensionKind::ByteLength);
    assert_eq!(binding.source(), single.source_id());
    assert_eq!(binding.receiver_type(), callable.receiver().unwrap().ty());
    assert_eq!(callable.result_source(), CallableResultSource::Owned);
    assert!(callable.range_extension().is_none());
    assert!(callable.extension_receiver_symbol().is_some());
    let callable = unit
        .signatures()
        .declarations()
        .iter()
        .filter_map(|d| d.callable())
        .find(|c| c.text_extension().is_some())
        .unwrap();
    let binding = callable.text_extension().unwrap();
    assert_eq!(binding.kind(), TextExtensionKind::ByteLength);
    assert_eq!(binding.callable(), callable.target());
    assert_eq!(callable.result_source(), CallableResultSource::Owned);
    assert!(callable.range_extension().is_none());
    assert!(callable.extension_receiver_symbol().is_some());
    assert!(unit.calls().iter().any(|c| c.receiver().is_some()));
}

#[test]
fn owned_text_results_keep_receiver_identity_inside_the_body() {
    for (declaration, call, kind) in [
        (
            "borrow fun String.toBytes(): Array<UByte> = Array<UByte>(0, { index -> 0u })",
            "fun run(text: String): Array<UByte> = text.toBytes()",
            TextExtensionKind::ToBytes,
        ),
        (
            "borrow fun String.checkedPlus(other: String): String? = this + other",
            "fun run(text: String): String? = text.checkedPlus(text)",
            TextExtensionKind::CheckedPlus,
        ),
    ] {
        let (single, unit) = checked(&format!("{declaration}\n{call}"), true);
        assert!(
            single.diagnostics().is_empty(),
            "{declaration}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics().is_empty(),
            "{declaration}: {:?}",
            unit.diagnostics()
        );
        let single_callable = single
            .callables()
            .iter()
            .find(|c| c.text_extension().is_some())
            .unwrap();
        assert_eq!(single_callable.text_extension().unwrap().kind(), kind);
        assert_eq!(single_callable.result_source(), CallableResultSource::Owned);
        let unit_callable = unit
            .signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.callable())
            .find(|c| c.text_extension().is_some())
            .unwrap();
        assert_eq!(unit_callable.text_extension().unwrap().kind(), kind);
        assert_eq!(unit_callable.result_source(), CallableResultSource::Owned);
    }
}

#[test]
fn a_matching_package_and_path_cannot_authorize_text_extensions() {
    let (single, unit) = checked("borrow fun String.byteLength(): Int = 0", false);
    assert!(
        single
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0164")
    );
    assert!(
        unit.diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0164")
    );
    assert!(
        single
            .callables()
            .iter()
            .all(|c| c.text_extension().is_none())
    );
    assert!(
        unit.signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.callable())
            .all(|c| c.text_extension().is_none())
    );
}

#[test]
fn trusted_text_authority_does_not_admit_other_receivers_names_or_signatures() {
    for declaration in [
        "borrow fun Int.byteLength(): Int = 0",
        "borrow fun String.arbitrary(): Int = 0",
        "borrow fun String.byteLength(value: Int): Int = value",
        "borrow fun String.byteLength(): Long = 0",
        "own fun String.byteLength(): Int = 0",
        "borrow fun <T> String.byteLength(): Int = 0",
        "borrow fun String.toBytes(): Array<Int> = Array<Int>(0, { index -> 0 })",
        "borrow fun String.checkedPlus(own other: String): String? = null",
        "borrow fun String.checkedPlus(other: String): String = other.clone()",
        "borrow fun String.byteLength(): borrow String from this = this",
    ] {
        let (single, unit) = checked(declaration, true);
        assert!(
            single
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0164"),
            "{declaration}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0164"),
            "{declaration}: {:?}",
            unit.diagnostics()
        );
    }
}

fn checked_import(application: &str) -> CompilationUnitTypes {
    let mut sources = SourceMap::new();
    let standard = sources
        .add_source(
            "loaded.ko",
            "package koven.text\nborrow fun String.byteLength(): Int = 0",
        )
        .unwrap();
    let application = sources.add_source("app.ko", application).unwrap();
    let parsed = [standard, application]
        .map(|id| parse_file(&sources, &lex(&sources, id).unwrap()).unwrap());
    assert!(parsed.iter().all(|p| p.diagnostics().is_empty()));
    let (environment, mut types) = standard_environments();
    types.authorize_text_source(&sources, standard).unwrap();
    let inputs = [
        SourceUnitInput::new("std", "koven/text/text.ko", standard, &parsed[0]),
        SourceUnitInput::new("app", "app/Main.ko", application, &parsed[1]),
    ];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap()
}

#[test]
fn explicit_imports_select_the_same_text_declaration_including_aliases() {
    use lang_frontend::type_checking::{UnitCallTarget, UnitCallableTarget};
    for (import, spelling) in [
        ("import koven.text.byteLength", "byteLength"),
        ("import koven.text.byteLength as bytes", "bytes"),
        ("import koven.text.*", "byteLength"),
    ] {
        let unit = checked_import(&format!(
            "package app\n{import}\nfun run(text: String): Int = text.{spelling}()"
        ));
        assert!(
            unit.diagnostics().is_empty(),
            "{import}: {:?}",
            unit.diagnostics()
        );
        let call = unit
            .calls()
            .iter()
            .find(|c| c.receiver().is_some())
            .unwrap();
        let UnitCallTarget::Declaration(id) = call.target() else {
            panic!("expected canonical text declaration")
        };
        let callable = unit
            .signatures()
            .declaration(id)
            .unwrap()
            .callable()
            .unwrap();
        assert_eq!(callable.name(), "byteLength");
        assert_eq!(
            callable.text_extension().unwrap().callable(),
            UnitCallableTarget::Declaration(id)
        );
        assert_eq!(call.result_source(), CallableResultSource::Owned);
    }
}

#[test]
fn loading_text_does_not_bypass_import_or_local_shadowing() {
    for application in [
        "package app\nfun run(text: String): Int = text.byteLength()",
        "package app\nfun byteLength(): Int = 7\nfun run(text: String): Int = text.byteLength()",
        "package app\nimport koven.text.byteLength\nfun run(text: String): Int { val byteLength = 0; return text.byteLength() }",
    ] {
        let unit = checked_import(application);
        // No canonical receiver call may be published, even if recovery remains deferred.
        assert!(unit.calls().iter().all(|c| c.receiver().is_none()));
    }
}

#[test]
fn slice_result_requires_builtin_result_and_authorized_error_identity() {
    for (builtin_authorized, foreign_error, accepted) in [
        (true, false, true),
        (false, false, false),
        (true, true, false),
    ] {
        let mut sources = SourceMap::new();
        let result = sources
            .add_source(
                "result.ko",
                "package koven.core\nenum class Result<T, E> { Ok(success: T), Err(error: E) }",
            )
            .unwrap();
        let error_declaration = if foreign_error {
            "import foreign.TextError"
        } else {
            "enum class TextError { InvalidUtf8(offset: Int), InvalidRange, NotScalarBoundary, EmptySeparator }"
        };
        let text = sources.add_source("text.ko", format!("package koven.text\nimport koven.core.Result\n{error_declaration}\nborrow fun String.sliceBytes(start: Int, end: Int): Result<String, TextError> = Result.Ok(this.clone())")).unwrap();
        let foreign = sources
            .add_source(
                "foreign.ko",
                "package foreign\nenum class TextError { InvalidRange }",
            )
            .unwrap();
        let application = sources.add_source("app.ko", "package app\nimport koven.text.sliceBytes\nfun run(text: String): Unit { val part = text.sliceBytes(0, 0) }").unwrap();
        let parsed = [result, text, foreign, application]
            .map(|id| parse_file(&sources, &lex(&sources, id).unwrap()).unwrap());
        assert!(parsed.iter().all(|p| p.diagnostics().is_empty()));
        let (mut environment, mut types) = standard_environments();
        if builtin_authorized {
            environment
                .authorize_builtin_source(&sources, result)
                .unwrap();
        }
        types.authorize_text_source(&sources, text).unwrap();
        let inputs = [
            SourceUnitInput::new("std", "koven/core/result.ko", result, &parsed[0]),
            SourceUnitInput::new("std", "koven/text/text.ko", text, &parsed[1]),
            SourceUnitInput::new("user", "foreign/Error.ko", foreign, &parsed[2]),
            SourceUnitInput::new("user", "app/Main.ko", application, &parsed[3]),
        ];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let unit = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        let binding = unit
            .signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.callable())
            .find(|c| c.name() == "sliceBytes")
            .unwrap()
            .text_extension();
        assert_eq!(
            binding.is_some(),
            accepted,
            "builtin={builtin_authorized}, foreign={foreign_error}: {:?}",
            unit.diagnostics()
        );
        if accepted {
            assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
            assert_eq!(binding.unwrap().kind(), TextExtensionKind::SliceBytes);
            assert!(unit.calls().iter().any(|c| matches!(c.target(), lang_frontend::type_checking::UnitCallTarget::Declaration(id) if unit.signatures().declaration(id).and_then(|d| d.callable()).is_some_and(|c| c.name() == "sliceBytes"))));
        } else {
            assert!(
                unit.diagnostics()
                    .iter()
                    .any(|d| d.code().to_string() == "L0164")
            );
        }
    }
}

#[test]
fn storage_primitives_require_the_host_source_and_exact_borrow_shapes() {
    use lang_frontend::type_checking::{ParameterMode, TextPrimitiveKind};
    for (source, kind) in [
        (
            "fun run(text: String): Int = textByteLength(text)",
            TextPrimitiveKind::ByteLength,
        ),
        (
            "fun run(text: String, index: Int): UByte = textByteAt(text, index)",
            TextPrimitiveKind::ByteAt,
        ),
        (
            "fun run(bytes: Array<UByte>): Int = textUtf8ErrorOffset(bytes)",
            TextPrimitiveKind::Utf8ErrorOffset,
        ),
        (
            "fun run(bytes: Array<UByte>): String = textCopyBytes(bytes)",
            TextPrimitiveKind::CopyBytes,
        ),
        (
            "fun run(text: String, start: Int, end: Int): String = textCopySlice(text, start, end)",
            TextPrimitiveKind::CopySlice,
        ),
    ] {
        for authorized in [false, true] {
            let (single, unit) = checked(source, authorized);
            if authorized {
                assert!(
                    single.diagnostics().is_empty(),
                    "{source}: {:?}",
                    single.diagnostics()
                );
                assert!(
                    unit.diagnostics().is_empty(),
                    "{source}: {:?}",
                    unit.diagnostics()
                );
                let call = single
                    .calls()
                    .iter()
                    .find(|c| c.text_primitive().is_some())
                    .unwrap();
                assert_eq!(call.text_primitive(), Some(kind));
                assert!(
                    call.arguments()
                        .iter()
                        .all(|a| a.mode() == ParameterMode::Borrow)
                );
                let call = unit
                    .calls()
                    .iter()
                    .find(|c| c.text_primitive().is_some())
                    .unwrap();
                assert_eq!(call.text_primitive(), Some(kind));
                assert!(
                    call.arguments()
                        .iter()
                        .all(|a| a.mode() == ParameterMode::Borrow)
                );
            } else {
                assert!(!single.diagnostics().is_empty());
                assert!(!unit.diagnostics().is_empty());
                assert!(single.calls().iter().all(|c| c.text_primitive().is_none()));
                assert!(unit.calls().iter().all(|c| c.text_primitive().is_none()));
            }
        }
    }
}

#[test]
fn text_storage_rejects_bad_operands_and_does_not_hijack_source_names() {
    for source in [
        "fun run(): Int = textByteLength(1)",
        "fun run(bytes: Array<Int>): String = textCopyBytes(bytes)",
        "fun run(text: String): UByte = textByteAt(text)",
        "fun run(text: String): Int = textByteLength(&text)",
        "fun run(text: String): Int = textByteLength<Int>(text)",
        "fun run(text: String): Int = textByteLength(value = text)",
    ] {
        let (single, unit) = checked(source, true);
        assert!(!single.diagnostics().is_empty(), "{source}");
        assert!(!unit.diagnostics().is_empty(), "{source}");
        assert!(single.calls().iter().all(|c| c.text_primitive().is_none()));
        assert!(unit.calls().iter().all(|c| c.text_primitive().is_none()));
    }
    let (single, unit) = checked(
        "fun textByteLength(value: Int): Int = value\nfun run(): Int = textByteLength(7)",
        false,
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    assert!(single.calls().iter().all(|c| c.text_primitive().is_none()));
    assert!(unit.calls().iter().all(|c| c.text_primitive().is_none()));
}
