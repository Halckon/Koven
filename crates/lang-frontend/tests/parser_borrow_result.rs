//! SPEC-0288 / Guide v0.42 普通借用结果与显式局部绑定。

use lang_frontend::{
    parser::{Item, NameMarker, ParsedFile},
    source::SourceMap,
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

fn parsed(text: &str) -> (SourceMap, ParsedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("borrow_result.ko", text).unwrap();
    let parsed = parser_test_assertions::parse_file_twice(&sources, source, "borrow result");
    (sources, parsed)
}

#[test]
fn ordinary_borrow_return_and_explicit_local_parse() {
    for text in [
        "fun view(source: String): borrow String from source = source\nfun use(source: String) { borrow val item: String = view(source); println(item) }",
        "fun view(source: String?): borrow String? from source = source",
        "class Record(val text: String) { fun view(): borrow String from this = this.text }",
        "fun use(source: String) { val run = { borrow val item = source; println(item) }; run() }",
    ] {
        let (_, result) = parsed(text);
        assert!(
            result.diagnostics().is_empty(),
            "{text}: {:?}",
            result.diagnostics()
        );
    }
}

#[test]
fn borrow_and_from_remain_soft_names() {
    let (_, result) = parsed("fun use() { val borrow = 1; val from = borrow; println(from) }");
    assert!(
        result.diagnostics().is_empty(),
        "{:?}",
        result.diagnostics()
    );
}

#[test]
fn borrow_var_has_a_marker_diagnostic_and_preserves_following_declaration() {
    let text = "fun use(source: String) { borrow var item = source; val after = 1 }";
    let (sources, result) = parsed(text);
    let error = result
        .diagnostics()
        .iter()
        .find(|d| d.code().to_string() == "L0162")
        .expect("borrow var must have a contract diagnostic");
    assert_eq!(sources.slice(error.primary_span()).unwrap(), "borrow");
    assert!(result.ast().items().iter().any(|(_, node)| {
        matches!(node.payload(), Item::Variable { name: NameMarker::Present(span), .. }
            if sources.slice(*span).unwrap() == "after")
    }));
}

#[test]
fn incomplete_or_conditional_borrow_results_are_rejected() {
    for text in [
        "fun view(source: String): borrow String = source",
        "fun view(source: String): borrow? String from source = source",
        "fun use(source: String) { borrow? val item = source }",
        "val item: borrow String = \"x\"",
    ] {
        let (_, result) = parsed(text);
        assert!(!result.diagnostics().is_empty(), "{text} must be rejected");
    }
}
