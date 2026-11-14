//! Default-off independent child-then-parent source/final-owned observation.
#![cfg(feature = "d10-catalog-experiment")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, d10_catalog_experiment as experiment,
        owned_compilation_unit_view,
    },
    parser::{Expression, Item, Statement, parse_file},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
const SOURCE: &str = "fun run(count: Int, childCount: Int) { var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) { var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; val parentOld = replace(&a, \"parent\"); outer -= 1 }; println(a); println(b) }";
#[test]
fn after_child_handoff_preserves_source_and_actual_owned_order() {
    run(false);
}
#[test]
#[ignore = "CI-only final ownership identity mismatch"]
fn ci_only_after_child_rejects_other_ownership() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    run(true);
}
fn run(mismatch: bool) {
    let old_order = SOURCE
        .replace("val parentOld = replace(&a, \"parent\"); ", "")
        .replace(
            "var inner = childCount",
            "val parentOld = replace(&a, \"parent\"); var inner = childCount",
        );
    for (text, expected) in [
        (SOURCE.to_owned(), true),
        (
            SOURCE
                .replace("count", "limit")
                .replace("childCount", "childLimit")
                .replace("outer", "major")
                .replace("inner", "minor")
                .replace("first", "初始"),
            true,
        ),
        (SOURCE.replace(";", ";\n"), true),
        (old_order, false),
        (SOURCE.replace("println(a); ", ""), false),
        (
            SOURCE.replace("println(a); println(b)", "println(b); println(a)"),
            false,
        ),
        (
            SOURCE.replace("println(a); println(b)", "println(a); println(a)"),
            false,
        ),
        (
            SOURCE.replace("childCount: Int", "own childCount: Int"),
            false,
        ),
        (
            SOURCE
                .replace(", childCount: Int", "")
                .replace("var inner = childCount", "var inner = count"),
            false,
        ),
        (
            SOURCE.replace("var inner = childCount", "var inner = count"),
            false,
        ),
        (
            SOURCE
                .replace("var outer = count", "var outer = childCount")
                .replace("var inner = childCount", "var inner = count"),
            false,
        ),
        (SOURCE.replace("inner -= 1", "inner += 1"), false),
        (SOURCE.replace("inner -= 1", "inner -= 1; break"), false),
        (
            SOURCE.replace("replace(&a, \"parent\")", "replace(&b, \"parent\")"),
            false,
        ),
        (
            SOURCE.replace("replace(&a, \"next\")", "replace(&b, \"next\")"),
            false,
        ),
        (
            SOURCE
                .replace("childCount: Int", "childCount: Long")
                .replace("inner > 0", "inner > 0L")
                .replace("inner -= 1", "inner -= 1L"),
            false,
        ),
    ] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("after.ko", &text).unwrap();
        let lexed = lex(&sources, id).unwrap();
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "after.ko", id, &parsed),
            SourceUnitInput::memory_document(id, &parsed),
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
            let bound = experiment::analyze_after_child_nested_loop(
                &sources, &inputs, &names, &types, &typed,
            )
            .unwrap();
            assert_eq!(bound.is_some(), expected, "{text}");
            let Some(bound) = bound else {
                continue;
            };
            assert!(
                experiment::analyze_dual_nested_loop(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .is_none()
            );
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
            assert_eq!(bound.order(), experiment::ReplacementOrder::ChildThenParent);
            let (callable, parent, child) = bound.controls();
            assert_ne!(parent, child);
            assert_eq!(callable.item(), parsed.roots()[0]);
            let (root, outer, inner, parent_param) = bound.roots();
            assert_ne!(outer, inner);
            assert_ne!(bound.counter_parameters()[0], bound.counter_parameters()[1]);
            assert_eq!(parent_param, bound.counter_parameters()[0]);
            for (init, param) in bound
                .counter_initializers()
                .into_iter()
                .zip(bound.counter_parameters())
            {
                let node = parsed.ast().expressions().get(init.expression()).unwrap();
                assert!(matches!(node.payload(), Expression::Name));
                let refs = names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.source_unit() == init.source_unit() && r.span() == node.span())
                    .collect::<Vec<_>>();
                assert_eq!(refs.len(), 1);
                assert_eq!(
                    refs[0].target(),
                    &lang_frontend::name_resolution::UnitReferenceTarget::Symbol(param)
                );
            }
            let cleanup = bound.cleanup();
            assert_eq!(
                unit.ownership().drops(),
                &[
                    cleanup[0],
                    cleanup[1],
                    cleanup[2],
                    bound.retained().cleanup()
                ]
            );
            assert_eq!(
                unit.ownership().loans(),
                &[
                    bound.loans()[0].clone(),
                    bound.loans()[1].clone(),
                    bound.final_root_use().loan().clone(),
                    bound.retained().loan().clone()
                ]
            );
            for (i, plan) in unit.ownership().ownership_primitives().iter().enumerate() {
                assert_eq!(plan.descriptor().expression(), bound.replacements()[i]);
                assert_eq!(
                    plan.descriptor().operands()[1],
                    bound.producer_sites()[i + 1]
                );
                assert_eq!(plan.places()[0].root(), root);
                let UnitDropPoint::AfterStatement(statement) = cleanup[i].point() else {
                    panic!("detached cleanup");
                };
                let Statement::LocalVariable { declaration } = parsed
                    .ast()
                    .statements()
                    .get(statement.statement())
                    .unwrap()
                    .payload()
                else {
                    panic!("local");
                };
                let Item::Variable { initializer, .. } =
                    parsed.ast().items().get(*declaration).unwrap().payload()
                else {
                    panic!("variable");
                };
                assert_eq!(*initializer, bound.replacements()[i].expression());
            }
            assert_eq!(cleanup[2].target(), UnitDropTarget::Named(root));
            assert_eq!(
                cleanup[2].point(),
                UnitDropPoint::CallReturn(bound.final_root_use().call())
            );
            assert_eq!(
                bound.final_root_use().source(),
                (root, bound.producer_sites()[0])
            );
            assert_ne!(bound.retained().source().0, root);
            assert_eq!(bound.final_root_use().cleanup(), cleanup[2]);
            if mismatch {
                let another = experiment::analyze_after_child_nested_loop(
                    &sources, &inputs, &names, &types, &typed,
                )
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
