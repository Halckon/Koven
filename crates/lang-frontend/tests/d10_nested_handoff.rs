//! Default-off nested source closure and original final-owned identity handoff.
#![cfg(feature = "d10-catalog-experiment")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitDropPoint, d10_catalog_experiment as experiment, owned_compilation_unit_view,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
const SOURCE: &str = r#"fun run(count: Int) {
 var a = "first"; var b = "kept"; var outer = count
 while(outer > 0) {
   var inner = count
   while(inner > 0) { val old = replace(&a, "next"); inner -= 1 }
   outer -= 1
 }
 println(b)
}"#;

#[test]
fn nested_handoff_closes_real_source_and_preserves_final_identity() {
    run_cases(false);
}

#[test]
#[ignore = "CI-only nested final-owned identity mismatch"]
fn ci_only_nested_rejects_another_ownership_run() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    run_cases(true);
}

fn run_cases(check_mismatch: bool) {
    let cases = [
        (SOURCE.to_string(), true),
        (
            SOURCE
                .replace("count", "limit")
                .replace("outer", "major")
                .replace("inner", "minor")
                .replace("first", "初始")
                .replace("kept", "保留")
                .replace("next", "下一轮"),
            true,
        ),
        (
            SOURCE.replace("var inner = count", "var inner = outer"),
            false,
        ),
        (SOURCE.replace("var outer = count", "var outer = 2"), false),
        (SOURCE.replace("inner -= 1", "inner -= 2"), false),
        (SOURCE.replace("outer -= 1", "outer += 1"), false),
        (SOURCE.replace("inner > 0", "outer > 0"), false),
        (SOURCE.replace("outer > 0", "outer >= 0"), false),
        (SOURCE.replace("println(b)", "println(a)"), false),
        (
            SOURCE.replace("println(b)", "println(b); println(b)"),
            false,
        ),
        (SOURCE.replace("println(b)", ""), false),
        (SOURCE.replace("inner -= 1", "inner -= 1; break"), false),
        (SOURCE.replace("var b =", "val b ="), false),
        (SOURCE.replace("var a =", "var a: String ="), false),
    ];
    for (text, expected) in cases {
        let mut sources = SourceMap::new();
        let source = sources.add_source("nested.ko", &text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "nested.ko", source, &parsed),
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
            let bound =
                experiment::analyze_nested_loop(&sources, &inputs, &names, &types, &typed).unwrap();
            assert_eq!(bound.is_some(), expected, "{text}");
            // Never expands the independent single-loop contract.
            assert!(
                experiment::analyze_loop(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .is_none()
            );
            if let Some(bound) = bound {
                let view = owned_compilation_unit_view(
                    &sources,
                    &inputs,
                    &names,
                    &types,
                    &typed,
                    bound.ownership(),
                )
                .unwrap();
                assert!(bound.matches(&view));
                if check_mismatch {
                    let another =
                        experiment::analyze_nested_loop(&sources, &inputs, &names, &types, &typed)
                            .unwrap()
                            .unwrap();
                    let other_view = owned_compilation_unit_view(
                        &sources,
                        &inputs,
                        &names,
                        &types,
                        &typed,
                        another.ownership(),
                    )
                    .unwrap();
                    assert!(!bound.matches(&other_view));
                }
                let (_, parent, child) = bound.controls();
                assert_ne!(parent, child);
                let (root, outer, inner, parameter) = bound.roots();
                let kept = bound.retained().unwrap();
                let ids = [root, outer, inner, parameter, kept.source().0];
                for (i, id) in ids.iter().enumerate() {
                    assert!(!ids[..i].contains(id));
                }
                assert_eq!(bound.cleanup()[1].point(), UnitDropPoint::LoopExit(parent));
                assert!(matches!(
                    bound.cleanup()[0].point(),
                    UnitDropPoint::AfterStatement(_)
                ));
                assert_eq!(
                    kept.cleanup().point(),
                    UnitDropPoint::CallReturn(kept.call())
                );
                assert_eq!(
                    view.ownership().drops(),
                    &[bound.cleanup()[0], bound.cleanup()[1], kept.cleanup()]
                );
                assert_eq!(
                    view.ownership().loans(),
                    &[bound.loan().clone(), kept.loan().clone()]
                );
                assert_eq!(view.ownership().ownership_primitives().len(), 1);
                assert_eq!(
                    view.ownership().ownership_primitives()[0]
                        .descriptor()
                        .expression(),
                    bound.replacement()
                );
                let counters = bound.counter_expressions();
                assert_ne!(counters[0], counters[1]);
                assert_ne!(bound.producer_sites().0, bound.producer_sites().1);
            }
        }
    }
}
