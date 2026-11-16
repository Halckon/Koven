//! Real shared-analysis split-source handoff, no independently rerun checker or nonce injection.
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
println(a); println(b) }"#;
const WRAPPER: &str = "fun entry() { val p = 1; val c = 2; run(p, c) }";
fn check(texts: &[(&str, &str)], expected: Option<experiment::NamedEntryOrder>) {
    let mut sources = SourceMap::new();
    let parsed: Vec<_> = texts
        .iter()
        .map(|(path, text)| {
            let source = sources.add_source(*path, *text).unwrap();
            parse_file(&sources, &lex(&sources, source).unwrap()).unwrap()
        })
        .collect();
    let inputs: Vec<_> = texts
        .iter()
        .zip(&parsed)
        .map(|((path, _), p)| SourceUnitInput::new("root", path, p.source_id(), p))
        .collect();
    let (env, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let bound =
        experiment::analyze_split_named_entry(&sources, &inputs, &names, &types, &typed).unwrap();
    assert_eq!(bound.as_ref().map(|p| p.order()), expected, "{texts:?}");
    assert!(
        experiment::analyze_named_entry(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .is_none(),
        "old singleton gate remains closed"
    );
    let Some(bound) = bound else { return };
    assert_ne!(bound.entry().source_unit(), bound.callee().source_unit());
    assert_ne!(bound.wrapper_roots()[0], bound.wrapper_roots()[1]);
    assert_eq!(bound.call().source_unit(), bound.entry().source_unit());
    assert!(
        bound
            .initializers()
            .iter()
            .all(|i| i.source_unit() == bound.entry().source_unit())
    );
    let target = &names.names().index().declarations()[bound.target().index()];
    assert_eq!(target.source_unit(), bound.callee().source_unit());
    assert_eq!(target.root(), bound.callee().item());
    let owned = bound.ownership().ownership();
    assert_eq!(
        (
            owned.loans().len(),
            owned.drops().len(),
            owned.direct_call_loan_ends().len(),
            owned.call_argument_contracts().len()
        ),
        (6, 4, 2, 8)
    );
    let view =
        owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, bound.ownership())
            .unwrap();
    assert!(bound.matches(&view));
    let wrapper_first = bound.entry().source_unit().index() < bound.callee().source_unit().index();
    let first_source = if wrapper_first {
        bound.entry().source_unit()
    } else {
        bound.callee().source_unit()
    };
    assert_eq!(owned.loans()[0].call().source_unit(), first_source);
}
#[test]
fn split_entry_binds_original_complete_shared_analysis_in_canonical_source_order() {
    let after = BEFORE
        .replace("val parentOld = replace(&a, \"parent\"); ", "")
        .replace(
            "}; outer -= 1",
            "}; val parentOld = replace(&a, \"parent\"); outer -= 1",
        );
    for (callee, order) in [
        (BEFORE, experiment::NamedEntryOrder::BeforeChild),
        (after.as_str(), experiment::NamedEntryOrder::AfterChild),
    ] {
        for callee_first in [false, true] {
            let (cp, wp) = if callee_first {
                ("a.ko", "z.ko")
            } else {
                ("z.ko", "a.ko")
            };
            for reverse in [false, true] {
                let mut texts = [(cp, callee), (wp, WRAPPER)];
                if reverse {
                    texts.reverse();
                }
                check(&texts, Some(order));
            }
        }
    }
}
#[test]
fn split_entry_rejects_valid_but_uncovered_source_closures() {
    for wrapper in [
        WRAPPER.replace("run(p, c)", "run(1, 2)"),
        WRAPPER.replace("run(p, c)", "run(p, p)"),
        WRAPPER.replace("run(p, c)", "run(p, c); println(\"extra\")"),
        format!("{WRAPPER}\nfun extra() {{}}"),
    ] {
        check(&[("a.ko", BEFORE), ("z.ko", &wrapper)], None);
    }
    for callee in [
        BEFORE.replace("println(b)", "println(b); println(\"extra\")"),
        BEFORE.replace("var inner = childCount", "var inner = count"),
    ] {
        check(&[("a.ko", &callee), ("z.ko", WRAPPER)], None);
    }
    check(
        &[
            ("a.ko", BEFORE),
            ("z.ko", WRAPPER),
            ("third.ko", "fun extra() {}"),
        ],
        None,
    );
}
