//! External-crate tests for the opt-in closed no-loop catalog handoff.
#![cfg(feature = "d10-catalog-experiment")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{d10_catalog_experiment, owned_compilation_unit_view},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[test]
fn public_sidecar_binds_the_final_owned_and_rejects_uncovered_source() {
    run_cases(false);
}

#[test]
#[ignore = "CI-only final-owned and producer identity mismatch controls"]
fn ci_only_sidecar_rejects_another_ownership_run() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    run_cases(true);
}

fn run_cases(check_mismatch: bool) {
    for (text, covered) in [
        (
            "fun run() { var a = \"first\"; val old = replace(&a, \"next\") }",
            true,
        ),
        ("fun run(flag: Boolean) { while(flag) {} }", false),
        (
            "fun run() { var a = \"first\"; val old = replace(&a, \"next\"); println(a) }",
            false,
        ),
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("catalog.ko", text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "catalog.ko", source, &parsed),
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
            let result =
                d10_catalog_experiment::analyze(&sources, &inputs, &names, &types, &typed).unwrap();
            assert_eq!(result.is_some(), covered);
            if let Some(bound) = result {
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
                        d10_catalog_experiment::analyze(&sources, &inputs, &names, &types, &typed)
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
                    assert_ne!(
                        bound.producers()[0].identity(),
                        another.producers()[0].identity()
                    );
                }

                assert_eq!(bound.producers().len(), 2);
                assert_eq!(view.ownership().drops().len(), 2);
                assert_ne!(
                    bound.replacement().previous(),
                    bound.replacement().incoming()
                );
                assert_eq!(
                    bound.producers()[0].identity(),
                    bound.replacement().previous()
                );
                assert_eq!(
                    bound.producers()[1].identity(),
                    bound.replacement().incoming()
                );
                for producer in bound.producers() {
                    let expression = producer.expression();
                    let drop = producer.cleanup();
                    assert_eq!(expression.source_unit(), bound.callable().source_unit());
                    assert!(view.ownership().drops().contains(&drop));
                    let span = parsed
                        .ast()
                        .expressions()
                        .get(expression.expression())
                        .unwrap()
                        .span();
                    assert!(["\"first\"", "\"next\""].contains(&sources.slice(span).unwrap()));
                }
            }
        }
    }
}
