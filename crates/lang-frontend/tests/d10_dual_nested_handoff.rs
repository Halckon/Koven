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
    let separate = SOURCE
        .replace("count: Int", "count: Int, childCount: Int")
        .replace("var inner = count", "var inner = childCount");
    let both = SOURCE.replace("println(b)", "println(a); println(b)");
    for (text, expected) in [
        (both.clone(), true),
        (
            separate.replace("println(b)", "println(a); println(b)"),
            true,
        ),
        (
            both.replace("count", "limit").replace("first", "初值"),
            true,
        ),
        (
            both.replace("println(a); println(b)", "println(b); println(a)"),
            false,
        ),
        (
            both.replace("println(a); println(b)", "println(a); println(a)"),
            false,
        ),
        (
            both.replace(
                "println(a); println(b)",
                "println(a); println(b); println(a)",
            ),
            false,
        ),
        (SOURCE.to_owned(), true),
        (separate.clone(), true),
        (
            separate
                .replace("count", "outerLimit")
                .replace("childCount", "innerLimit"),
            true,
        ),
        (
            separate
                .replace("var outer = count", "var outer = childCount")
                .replace("var inner = childCount", "var inner = count"),
            false,
        ),
        (
            separate.replace("var inner = childCount", "var inner = count"),
            false,
        ),
        (
            separate.replace("var outer = count", "var outer = childCount"),
            false,
        ),
        (
            separate.replace("childCount: Int", "own childCount: Int"),
            false,
        ),
        (
            separate.replace("childCount: Int", "inout childCount: Int"),
            false,
        ),
        (separate.replace("count: Int,", "own count: Int,"), false),
        (
            separate
                .replace("childCount: Int", "childCount: Long")
                .replace("inner > 0", "inner > 0L")
                .replace("inner -= 1", "inner -= 1L"),
            false,
        ),
        (
            separate.replace("childCount: Int", "childCount: Int, unused: Int"),
            false,
        ),
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
            let parameters = bound.counter_parameters();
            assert_eq!(parameters[0], bound.roots().3);
            // Match actual declarations and initializer references, never names or ordinals alone.
            let source_names = &names.names().source_units()[0];
            let declared_parameters = source_names
                .resolution()
                .symbols()
                .iter()
                .filter(|symbol| {
                    symbol.kind() == lang_frontend::name_resolution::SymbolKind::ValueParameter
                })
                .map(|symbol| symbol.id())
                .collect::<Vec<_>>();
            assert!(matches!(declared_parameters.len(), 1 | 2));
            assert_eq!(parameters[0].symbol(), declared_parameters[0]);
            assert_eq!(parameters[1].symbol(), *declared_parameters.last().unwrap());
            assert_eq!(
                parameters[0] == parameters[1],
                declared_parameters.len() == 1
            );
            for ((counter, parameter), actual_initializer) in [bound.roots().1, bound.roots().2]
                .into_iter()
                .zip(parameters)
                .zip(bound.counter_initializers())
            {
                let symbol = &source_names.resolution().symbols()[counter.symbol().index()];
                let (_, declaration) = parsed.ast().items().iter().find(|(_,node)| matches!(node.payload(), lang_frontend::parser::Item::Variable { name: lang_frontend::parser::NameMarker::Present(span), .. } if *span == symbol.span())).unwrap();
                let lang_frontend::parser::Item::Variable { initializer, .. } =
                    declaration.payload()
                else {
                    unreachable!()
                };
                assert_eq!(actual_initializer.expression(), *initializer);
                assert_eq!(actual_initializer.source_unit(), parameter.source_unit());
                let span = parsed.ast().expressions().get(*initializer).unwrap().span();
                let actual = names
                    .names()
                    .references()
                    .iter()
                    .find(|r| r.span() == span)
                    .unwrap();
                assert!(
                    matches!(actual.target(), lang_frontend::name_resolution::UnitReferenceTarget::Symbol(id) if *id == parameter)
                );
            }
            let (_, parent, child) = bound.controls();
            assert_ne!(parent, child);
            let cleanup = bound.cleanup();
            let retained = bound.retained();
            assert_eq!(
                view.ownership().drops(),
                &[cleanup[0], cleanup[1], cleanup[2], retained.cleanup()]
            );
            assert_eq!(
                bound.final_root_use().is_some(),
                text.contains("println(a); println(b)")
            );
            if let Some(root_use) = bound.final_root_use() {
                assert_eq!(
                    root_use.source(),
                    (bound.roots().0, bound.producer_sites()[0])
                );
                assert_ne!(root_use.source().0, retained.source().0);
                assert_ne!(root_use.call(), retained.call());
                assert_eq!(root_use.cleanup(), cleanup[2]);
                assert_eq!(
                    cleanup[2].point(),
                    UnitDropPoint::CallReturn(root_use.call())
                );
            } else {
                assert_eq!(cleanup[2].point(), UnitDropPoint::LoopExit(parent));
            }
            assert!(matches!(
                cleanup[0].point(),
                UnitDropPoint::AfterStatement(_)
            ));
            assert!(matches!(
                cleanup[1].point(),
                UnitDropPoint::AfterStatement(_)
            ));
            assert_ne!(cleanup[0], cleanup[1]);
            let mut expected_loans = bound.loans().to_vec();
            if let Some(root_use) = bound.final_root_use() {
                expected_loans.push(root_use.loan().clone());
            }
            expected_loans.push(retained.loan().clone());
            assert_eq!(view.ownership().loans(), expected_loans);
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
