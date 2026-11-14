//! Unified single-run selection of existing source contracts; no driver or identity fault tests.
#![cfg(feature = "d10-catalog-experiment")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{d10_catalog_experiment as experiment, owned_compilation_unit_view},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
const BEFORE: &str = r#"fun run(count: Int, childCount: Int) {
var a = "first"; var b = "kept"; var outer = count;
while(outer > 0) { val parentOld = replace(&a, "parent"); var inner = childCount;
while(inner > 0) { val old = replace(&a, "next"); inner -= 1 }; outer -= 1 };
println(a); println(b) }
fun entry() { val p = 1; val c = 2; run(p, c) }"#;
#[test]
fn named_entry_selects_one_original_bound_from_one_final_owned() {
    use experiment::{NamedEntryOrder, NamedEntryView};
    let after = BEFORE
        .replace("val parentOld = replace(&a, \"parent\"); ", "")
        .replace(
            "}; outer -= 1",
            "}; val parentOld = replace(&a, \"parent\"); outer -= 1",
        );
    for (text, expected) in [
        (BEFORE.to_owned(), Some(NamedEntryOrder::BeforeChild)),
        (after.clone(), Some(NamedEntryOrder::AfterChild)),
        (BEFORE.replace("run(p, c)", "run(1, 2)"), None),
        (after.replace("run(p, c)", "run(1, 2)"), None),
        (
            BEFORE.replace("run(p, c)", "run(p, c); println(\"extra\")"),
            None,
        ),
        (format!("{BEFORE}\nfun unused() {{}}"), None),
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("named-entry.ko", &text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "named-entry.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let selected =
                experiment::analyze_named_entry(&sources, &inputs, &names, &types, &typed).unwrap();
            assert_eq!(
                selected.as_ref().map(|bound| bound.order()),
                expected,
                "{text}"
            );
            let Some(bound) = selected else {
                continue;
            };
            let unit = owned_compilation_unit_view(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                bound.ownership(),
            )
            .unwrap();
            assert!(bound.matches(&unit));
            assert!(std::ptr::eq(
                unit.ownership(),
                bound.ownership().ownership()
            ));
            assert_eq!(unit.ownership().drops().len(), 4);
            assert_eq!(unit.ownership().loans().len(), 6);
            assert_eq!(unit.ownership().direct_call_loan_ends().len(), 2);
            assert_eq!(unit.ownership().ownership_primitives().len(), 2);
            match bound.view() {
                NamedEntryView::BeforeChild(original) => {
                    assert_eq!(expected, Some(NamedEntryOrder::BeforeChild));
                    assert!(original.matches(&unit));
                    assert!(std::ptr::eq(original.ownership(), bound.ownership()));
                    assert_eq!(original.entry(), bound.entry());
                }
                NamedEntryView::AfterChild(original) => {
                    assert_eq!(expected, Some(NamedEntryOrder::AfterChild));
                    assert!(original.matches(&unit));
                    assert!(std::ptr::eq(original.ownership(), bound.ownership()));
                    assert_eq!(original.entry(), bound.entry());
                }
            }
        }
    }
}
