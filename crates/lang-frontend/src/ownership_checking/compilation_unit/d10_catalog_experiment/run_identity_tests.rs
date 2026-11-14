//! Real analysis provenance. Cross-run composition is a CI-only negative control.
use super::*;
use crate::{
    lexer::lex,
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    type_checking::{check_compilation_unit_types, standard_environments},
};
const BEFORE: &str = r#"fun run(count: Int, childCount: Int) {
var a = "first"; var b = "kept"; var outer = count;
while(outer > 0) { val parentOld = replace(&a, "parent"); var inner = childCount;
while(inner > 0) { val old = replace(&a, "next"); inner -= 1 }; outer -= 1 };
println(a); println(b) }"#;
const ENTRY: &str = "fun entry() { val p = 1; val c = 2; run(p, c) }";
fn exercise(cross_run: bool) {
    let after = BEFORE
        .replace("val parentOld = replace(&a, \"parent\"); ", "")
        .replace(
            "}; outer -= 1",
            "}; val parentOld = replace(&a, \"parent\"); outer -= 1",
        );
    for (callee, is_after) in [(BEFORE, false), (after.as_str(), true)] {
        for text in [
            callee.to_owned(),
            format!("{callee}\n{ENTRY}"),
            format!("{ENTRY}\n{callee}"),
        ] {
            let wrapped = text != callee;
            let mut sources = SourceMap::new();
            let source = sources.add_source("run-identity.ko", &text).unwrap();
            let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
            let inputs = [SourceUnitInput::new(
                "root",
                "run-identity.ko",
                source,
                &parsed,
            )];
            let (environment, types) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let analyze = || {
                analysis::analyze_with_catalog(
                    &sources,
                    &inputs,
                    names.names(),
                    &types,
                    typed.types(),
                    false,
                )
                .unwrap()
            };
            let (
                original,
                _,
                _,
                _,
                mut before,
                mut after,
                mut wrapped_before,
                mut wrapped_after,
                _,
            ) = analyze();
            let owned = if cross_run {
                analyze().0
            } else {
                original.clone()
            };
            assert_eq!(original.is_same_analysis(&owned), !cross_run);
            assert!(Arc::ptr_eq(
                &original.provenance.typed_analysis_owner,
                &owned.provenance.typed_analysis_owner
            ));
            let accepted = match (wrapped, is_after) {
                (false, false) => {
                    let p = before.pop().unwrap();
                    assert!(Arc::ptr_eq(
                        &p.analysis_owner,
                        &original.provenance.analysis_owner
                    ));
                    p.bind(owned).is_some()
                }
                (false, true) => {
                    let p = after.pop().unwrap();
                    assert!(Arc::ptr_eq(
                        &p.analysis_owner,
                        &original.provenance.analysis_owner
                    ));
                    p.bind(owned).is_some()
                }
                (true, false) => {
                    let p = wrapped_before.pop().unwrap();
                    assert!(Arc::ptr_eq(
                        &p.fields.analysis_owner,
                        &original.provenance.analysis_owner
                    ));
                    p.bind(owned).is_some()
                }
                (true, true) => {
                    let p = wrapped_after.pop().unwrap();
                    assert!(Arc::ptr_eq(
                        &p.fields.analysis_owner,
                        &original.provenance.analysis_owner
                    ));
                    p.bind_after(owned).is_some()
                }
            };
            assert_eq!(accepted, !cross_run, "{text}");
        }
    }
}
#[test]
fn final_owned_retains_original_planner_run_identity() {
    exercise(false);
}
#[test]
#[ignore = "CI-only cross-analysis identity control"]
fn ci_only_pending_rejects_same_typed_other_owned_run() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    exercise(true);
}
