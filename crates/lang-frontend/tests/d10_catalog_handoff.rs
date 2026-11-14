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

/// Static DropFacts describe root slots, not one fixed literal generation per loop.
#[test]
fn loop_replace_cleanup_is_root_relative_and_not_a_closed_producer_catalog() {
    use lang_frontend::ownership_checking::{
        UnitDropPoint, UnitDropTarget, check_compilation_unit_ownership,
    };
    for carry in [false, true] {
        let text = if carry {
            "fun run() { var n = 2; var a = \"first\"; val b = \"kept\"; while(n > 0) { val old = replace(&a, \"next\"); n -= 1 }; println(a); println(b) }"
        } else {
            "fun run() { var n = 2; var a = \"first\"; while(n > 0) { val old = replace(&a, \"next\"); n -= 1 }; println(a) }"
        };
        let mut sources = SourceMap::new();
        let source = sources.add_source("loop-catalog.ko", text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "loop-catalog.ko", source, &parsed),
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
            let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap();
            assert!(owned.ownership().deferred().is_empty());
            let drops = owned.ownership().drops();
            assert_eq!(drops.len(), if carry { 3 } else { 2 });
            let mut actual = Vec::new();
            for fact in drops {
                let UnitDropTarget::Named(root) = fact.target() else {
                    panic!("exact named cleanup");
                };
                let unit = names
                    .names()
                    .source_units()
                    .iter()
                    .find(|unit| unit.source_unit() == root.source_unit())
                    .unwrap();
                let symbol = unit
                    .resolution()
                    .symbols()
                    .iter()
                    .find(|symbol| symbol.id() == root.symbol())
                    .unwrap();
                assert_eq!(symbol.span(), fact.value_origin());
                let point = match fact.point() {
                    UnitDropPoint::AfterStatement(statement) => {
                        assert_eq!(statement.source_unit(), root.source_unit());
                        sources
                            .slice(
                                parsed
                                    .ast()
                                    .statements()
                                    .get(statement.statement())
                                    .unwrap()
                                    .span(),
                            )
                            .unwrap()
                    }
                    UnitDropPoint::CallReturn(call) => {
                        assert_eq!(call.source_unit(), root.source_unit());
                        sources
                            .slice(
                                parsed
                                    .ast()
                                    .expressions()
                                    .get(call.expression())
                                    .unwrap()
                                    .span(),
                            )
                            .unwrap()
                    }
                    _ => panic!("unexpected cleanup position"),
                };
                actual.push((
                    symbol.name(),
                    matches!(fact.point(), UnitDropPoint::AfterStatement(_)),
                    point,
                ));
            }
            actual.sort();
            let mut expected = vec![
                ("a", false, "println(a)"),
                ("old", true, "val old = replace(&a, \"next\")"),
            ];
            if carry {
                expected.push(("b", false, "println(b)"));
            }
            expected.sort();
            assert_eq!(actual, expected);
            // A real, valid loop is deliberately not coerced into the no-loop identity model.
            assert!(
                d10_catalog_experiment::analyze(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[test]
fn public_loop_sidecar_preserves_final_owned_and_symbolic_roles() {
    run_loop_cases(false);
}

#[test]
#[ignore = "CI-only loop sidecar final-owned identity mismatch"]
fn ci_only_loop_sidecar_rejects_another_ownership_run() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    run_loop_cases(true);
}

fn run_loop_cases(check_mismatch: bool) {
    for (tail, covered) in [
        ("n -= 1", true),
        ("n += 1", false),
        ("val extra = n; n -= 1", false),
    ] {
        let text = format!(
            "fun run(count: Int) {{ var a = \"first\"; var n = count; while(n > 0) {{ val old = replace(&a, \"next\"); {tail} }} }}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("loop-sidecar.ko", &text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "loop-sidecar.ko", source, &parsed),
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
                d10_catalog_experiment::analyze_loop(&sources, &inputs, &names, &types, &typed)
                    .unwrap();
            assert_eq!(bound.is_some(), covered);
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
                assert_eq!(bound.cleanup().as_slice(), view.ownership().drops());
                assert_eq!(view.ownership().loans(), [bound.loan().clone()]);
                let (callable, control) = bound.control();
                let (root, counter, parameter) = bound.roots();
                assert_eq!(callable.source_unit(), root.source_unit());
                assert_ne!(root, counter);
                assert_ne!(counter, parameter);
                let source_text = |id: lang_frontend::type_checking::UnitExpressionId| {
                    assert_eq!(id.source_unit(), root.source_unit());
                    sources
                        .slice(
                            parsed
                                .ast()
                                .expressions()
                                .get(id.expression())
                                .unwrap()
                                .span(),
                        )
                        .unwrap()
                };
                let (initial, incoming) = bound.producer_sites();
                assert_eq!(source_text(initial), "\"first\"");
                assert_eq!(source_text(incoming), "\"next\"");
                assert_eq!(source_text(bound.replacement()), "replace(&a, \"next\")");
                let (condition, step) = bound.counter_expressions();
                assert_eq!(source_text(condition), "n > 0");
                assert_eq!(source_text(step), "n -= 1");
                assert_eq!(
                    bound.cleanup()[1].point(),
                    lang_frontend::ownership_checking::UnitDropPoint::LoopExit(control)
                );
                if check_mismatch {
                    let other = d10_catalog_experiment::analyze_loop(
                        &sources, &inputs, &names, &types, &typed,
                    )
                    .unwrap()
                    .unwrap();
                    let other_view = owned_compilation_unit_view(
                        &sources,
                        &inputs,
                        &names,
                        &types,
                        &typed,
                        other.ownership(),
                    )
                    .unwrap();
                    assert!(!bound.matches(&other_view));
                }
            }
        }
    }
}
