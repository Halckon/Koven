//! 普通借用实际来源与 owned 交付拒绝的 single/unit 同步证据。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, OwnershipCheckedFile, check_compilation_unit_ownership,
        check_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn checked(text: &str) -> (OwnershipCheckedFile, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("borrow_origins.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let single = check_ownership(&sources, &file, &names, &typed).unwrap();
    let inputs = [SourceUnitInput::new(
        "root",
        "borrow_origins.ko",
        source,
        &file,
    )];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    (single, unit)
}

#[test]
fn direct_projected_nullable_and_named_wrapper_returns_publish_actual_origins() {
    for (text, count) in [
        ("fun <T> view(source: T): borrow T from source = source", 1),
        (
            "fun view(source: String): borrow String from source = source",
            1,
        ),
        (
            "fun view(source: String?): borrow String? from source = source",
            1,
        ),
        (
            "class Record(val text: String)\nfun view(source: Record): borrow String from source = source.text",
            1,
        ),
        (
            "fun view(aux: Int, source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source = source, aux = 0)",
            2,
        ),
    ] {
        let (single, unit) = checked(text);
        assert!(
            single.diagnostics().is_empty(),
            "{text}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics().is_empty(),
            "{text}: {:?}",
            unit.diagnostics()
        );
        assert_eq!(single.borrow_return_origins().len(), count);
        assert_eq!(unit.borrow_return_origins().len(), count);
        assert!(single.deferred().is_empty());
        assert!(unit.deferred().is_empty());
    }
}

#[test]
fn unproved_caller_bodyless_and_conditional_paths_remain_closed() {
    for text in [
        "fun view(source: String): borrow String from source = source\nfun run(source: String) { view(source) }",
        "fun view(source: String): borrow String from source = source\nfun observe(item: String) {}\nfun run(source: String) { observe(view(source)) }",
        "fun view(source: String): borrow String from source",
        "fun view(source: String, flag: Boolean): borrow String from source = if (flag) { source } else { source }",
    ] {
        let (single, unit) = checked(text);
        assert!(
            single
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0164"),
            "{text}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0164"),
            "{text}: {:?}",
            unit.diagnostics()
        );
        assert!(single.borrow_return_origins().is_empty());
        assert!(unit.borrow_return_origins().is_empty());
        assert!(single.drops().is_empty());
        assert!(unit.drops().is_empty());
    }
}

#[test]
fn foreign_local_temporary_and_wrong_wrapper_sources_are_rejected_atomically() {
    for text in [
        "fun view(source: String, other: String): borrow String from source = other",
        "fun view(source: String): borrow String from source = \"temporary\"",
        "fun view(source: String): borrow String from source { val local = \"local\"; return local }",
        "fun view(aux: String, source: String): borrow String from source = source\nfun wrap(source: String, other: String): borrow String from source = view(source = other, aux = source)",
        "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(\"temporary\")",
    ] {
        let (single, unit) = checked(text);
        assert!(
            single
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0162"),
            "{text}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0162"),
            "{text}: {:?}",
            unit.diagnostics()
        );
        assert!(single.borrow_return_origins().is_empty());
        assert!(unit.borrow_return_origins().is_empty());
    }
}

#[test]
fn owned_binding_return_and_argument_reject_borrow_results_even_when_copyable() {
    for body in [
        "fun run(source: Int) { val item = view(source); observe(item) }",
        "fun run(source: Int): Int = view(source)",
        "fun run(source: Int) { observe(view(source)) }",
    ] {
        let text = format!(
            "fun view(source: Int): borrow Int from source = source\nfun observe(own item: Int) {{}}\n{body}"
        );
        let (single, unit) = checked(&text);
        assert!(
            single
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0163"),
            "{body}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0163"),
            "{body}: {:?}",
            unit.diagnostics()
        );
        assert!(single.borrow_return_origins().is_empty());
        assert!(unit.borrow_return_origins().is_empty());
    }
}

#[test]
fn source_and_owned_delivery_diagnostics_preserve_real_primary_and_origin_labels() {
    use lang_frontend::diagnostic::DiagnosticDetail;
    for (text, code, primary, label) in [
        (
            "fun view(source: String, other: String): borrow String from source = other",
            "L0162",
            "other",
            "source",
        ),
        (
            "fun view(source: Int): borrow Int from source = source\nfun observe(own item: Int) {}\nfun run(source: Int) { observe(view(source)) }",
            "L0163",
            "view(source)",
            "borrow",
        ),
    ] {
        let (single, unit) = checked(text);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            let diagnostic = diagnostics
                .iter()
                .find(|d| d.code().to_string() == code)
                .unwrap();
            let span = diagnostic.primary_span();
            assert_eq!(&text[span.start()..span.end()], primary);
            assert!(
                diagnostic.details().iter().any(|detail| {
                    let DiagnosticDetail::Label(detail) = detail else {
                        return false;
                    };
                    let span = detail.span();
                    &text[span.start()..span.end()] == label
                }),
                "{diagnostic:?}"
            );
        }
    }
}
