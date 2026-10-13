//! Ordinary sources using the real trusted standard-library provider; no malformed IR.
use super::LoweringErrorKind;
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, resolve_names},
    ownership_checking::check_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};
const SOURCE: &str = "fun view(source: String, other: View<String>): borrow String from source = source\nfun wrap(source: String, items: List<String>): borrow String from source = view(source, take(items, 1))\nfun entry() { val source = \"kept\".clone(); val items = listOf(\"other\"); borrow val item = wrap(source, items); println(item) }";
const STANDARD: &str = include_str!("../../../lang-std/koven/algorithms/ranges.ko");

#[test]
fn multi_parameter_borrow_single_short_range_argument_remains_unsupported() {
    let mut sources = SourceMap::new();
    let text = format!(
        "{}\n{SOURCE}",
        STANDARD.strip_prefix("package koven.algorithms\n").unwrap()
    );
    let source = sources.add_source("trusted.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, source).unwrap();
    types
        .authorize_range_extension_source(&sources, source)
        .unwrap();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    assert!(names.diagnostics().is_empty());
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let error = super::lower_frontend::orchestrate::lower_scalar_file(
        &sources, &parsed, &names, &typed, &owned,
    )
    .err()
    .unwrap();
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(
        sources.slice(error.span.unwrap()).unwrap(),
        "view(source, take(items, 1))"
    );
}

#[test]
fn multi_parameter_borrow_unit_short_range_argument_remains_unsupported() {
    let mut sources = SourceMap::new();
    let (p, provider) = super::unit_lower_test_support::parsed(&mut sources, "ranges.ko", STANDARD);
    let (q, consumer) = super::unit_lower_test_support::parsed(
        &mut sources,
        "entry.ko",
        &format!("package app\nimport koven.algorithms.take\n{SOURCE}"),
    );
    let inputs = [
        SourceUnitInput::new("std", "koven/algorithms/ranges.ko", p, &provider),
        SourceUnitInput::new("app", "app/entry.ko", q, &consumer),
    ];
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, p).unwrap();
    types.authorize_range_extension_source(&sources, p).unwrap();
    let (names, typed, owned) =
        super::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let error = super::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        super::unit_lower_test_support::declaration(&names, "app", "entry"),
    )
    .err()
    .unwrap();
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(
        sources.slice(error.span.unwrap()).unwrap(),
        "view(source, take(items, 1))"
    );
}
