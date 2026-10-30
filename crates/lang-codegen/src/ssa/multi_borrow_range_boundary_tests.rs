//! Ordinary sources using the real trusted standard-library provider; no malformed IR.
use super::LoweringErrorKind;
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use lang_frontend::{
    name_resolution::{SourceUnitInput, SourceUnitKey, SymbolKind},
    source::SourceMap,
    type_checking::standard_environments,
};
const SOURCE: &str = "fun view(source: String, other: View<String>): borrow String from source = source\nfun wrap(source: String, items: List<String>): borrow String from source = view(source, take(items, 1))\nfun entry() { val source = \"kept\".clone(); val items = listOf(\"other\"); borrow val item = wrap(source, items); println(item) }";
const STANDARD: &str = include_str!("../../../lang-std/koven/algorithms/ranges.ko");

#[test]
fn multi_parameter_borrow_single_short_range_argument_remains_unsupported() {
    let text = format!(
        "{}\n{SOURCE}",
        STANDARD.strip_prefix("package koven.algorithms\n").unwrap()
    );
    let analysis =
        UnitSource::analyze_with_type_environment("trusted.ko", &text, |sources, source, types| {
            types.authorize_range_source(sources, source).unwrap();
            types
                .authorize_range_extension_source(sources, source)
                .unwrap();
        })
        .expect("the original trusted source passes strict Unit admission");
    analysis.assert_frontend_clean();
    assert!(analysis.ownership().deferred().is_empty());
    let [source] = analysis.names().names().index().source_units() else {
        panic!("one original MemoryDocument");
    };
    assert_eq!(source.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(source.source_id(), analysis.parsed().source_id());
    assert_eq!(
        analysis.sources().source_name(source.source_id()).unwrap(),
        "trusted.ko"
    );
    assert_eq!(
        analysis.sources().source_text(source.source_id()).unwrap(),
        text
    );
    let entry = analysis.declaration("entry", SymbolKind::Function);
    let error = analysis
        .lower_entry(entry)
        .err()
        .expect("the original short range argument remains unsupported");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    let span = error
        .span
        .expect("the original rejection has a source span");
    assert_eq!(span.source_id(), source.source_id());
    assert_eq!(
        analysis.sources().slice(span).unwrap(),
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
