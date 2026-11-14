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
const SOURCE: &str = "fun run(count: Int) { var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) { val parentOld = replace(&a, \"parent\"); var inner = count; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; outer -= 1 }; println(b) }";
#[test]
fn dual_handoff_preserves_two_real_operations_and_complete_source() {
    run_cases(false);
}
#[test]
#[ignore = "CI-only dual nested ownership identity mismatch"]
fn ci_only_dual_nested_rejects_other_ownership() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    run_cases(true);
}
fn run_cases(mismatch: bool) {
    for (text, expected) in [
        (SOURCE.to_owned(), true),
        (
            SOURCE
                .replace("count", "limit")
                .replace("outer", "major")
                .replace("inner", "minor")
                .replace("first", "初始"),
            true,
        ),
        (SOURCE.replace("parent\"", "next\""), true),
        (
            SOURCE.replace("replace(&a, \"parent\")", "replace(&b, \"parent\")"),
            false,
        ),
        (
            SOURCE.replace("replace(&a, \"next\")", "replace(&b, \"next\")"),
            false,
        ),
        (
            SOURCE.replace("var inner = count", "var inner = outer"),
            false,
        ),
        (SOURCE.replace("inner -= 1", "inner -= 2"), false),
        (SOURCE.replace("outer -= 1", "outer += 1"), false),
        (SOURCE.replace("inner -= 1", "inner -= 1; break"), false),
        (SOURCE.replace("val parentOld", "var parentOld"), false),
        (SOURCE.replace("println(b)", "println(a)"), false),
        (
            SOURCE.replace("println(b)", "println(b); println(b)"),
            false,
        ),
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("dual.ko", &text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "dual.ko", source, &parsed),
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
                experiment::analyze_dual_nested_loop(&sources, &inputs, &names, &types, &typed)
                    .unwrap();
            assert_eq!(bound.is_some(), expected, "{text}");
            assert!(
                experiment::analyze_nested_loop(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .is_none()
            );
            let Some(bound) = bound else {
                continue;
            };
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
            let (_, parent, child) = bound.controls();
            assert_ne!(parent, child);
            let cleanup = bound.cleanup();
            let retained = bound.retained();
            assert_eq!(
                view.ownership().drops(),
                &[cleanup[0], cleanup[1], cleanup[2], retained.cleanup()]
            );
            assert_eq!(cleanup[2].point(), UnitDropPoint::LoopExit(parent));
            assert!(matches!(
                cleanup[0].point(),
                UnitDropPoint::AfterStatement(_)
            ));
            assert!(matches!(
                cleanup[1].point(),
                UnitDropPoint::AfterStatement(_)
            ));
            assert_ne!(cleanup[0], cleanup[1]);
            assert_eq!(
                view.ownership().loans(),
                &[
                    bound.loans()[0].clone(),
                    bound.loans()[1].clone(),
                    retained.loan().clone()
                ]
            );
            assert_eq!(view.ownership().ownership_primitives().len(), 2);
            for (i, plan) in view.ownership().ownership_primitives().iter().enumerate() {
                assert_eq!(plan.descriptor().expression(), bound.replacements()[i]);
            }
            let sites = bound.producer_sites();
            assert_ne!(sites[0], sites[1]);
            assert_ne!(sites[1], sites[2]);
            assert_ne!(bound.replacements()[0], bound.replacements()[1]);
            if mismatch {
                let another =
                    experiment::analyze_dual_nested_loop(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                        .unwrap();
                let other = owned_compilation_unit_view(
                    &sources,
                    &inputs,
                    &names,
                    &types,
                    &typed,
                    another.ownership(),
                )
                .unwrap();
                assert!(!bound.matches(&other));
            }
        }
    }
}
