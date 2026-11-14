//! Real source inputs only. No forged analysis identities or execution.
#![cfg(feature = "d10-catalog-experiment")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{d10_catalog_experiment as experiment, owned_compilation_unit_view},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
const CALLEE: &str = r#"fun run(count: Int, childCount: Int) {
var a = "first"; var b = "kept"; var outer = count;
while(outer > 0) { var inner = childCount;
while(inner > 0) { val old = replace(&a, "next"); inner -= 1 }; val parentOld = replace(&a, "parent"); outer -= 1 };
println(a); println(b) }"#;
#[test]
fn named_after_child_wrapper_keeps_actual_order_and_final_owned() {
    let source = format!("{CALLEE}\nfun entry() {{ val p = 1; val c = 2; run(p, c) }}");
    for (text, accepted) in [
        (source.clone(), true),
        (
            source.replace("p = 1", "p = 0").replace("c = 2", "c = 0"),
            true,
        ),
        (
            source
                .replace("entry", "launch")
                .replace("count", "limit")
                .replace("childCount", "childLimit"),
            true,
        ),
        (
            source
                .replace("val parentOld = replace(&a, \"parent\"); ", "")
                .replace(
                    "var inner = childCount",
                    "val parentOld = replace(&a, \"parent\"); var inner = childCount",
                ),
            false,
        ),
        (
            format!("fun entry() {{ val p = 1; val c = 2; run(p, c) }}\n{CALLEE}"),
            false,
        ),
        (source.replace("run(p, c)", "run(1, 2)"), false),
        (source.replace("run(p, c)", "run(p, p)"), false),
        (source.replace("run(p, c)", "run(c, p)"), false),
        (source.replace("val p = 1", "var p = 1"), false),
        (source.replace("run(p, c)", "run(p, c); run(p, c)"), false),
        (format!("{source}\nfun extra() {{}}"), false),
        (
            source.replace("println(a); println(b)", "println(b)"),
            false,
        ),
        (source.replace("inner -= 1", "inner += 1"), false),
    ] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("named.ko", &text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (env, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "named.ko", id, &parsed),
            SourceUnitInput::memory_document(id, &parsed),
        ] {
            let inputs = [input];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let candidate = experiment::analyze_named_after_child_wrapper(
                &sources, &inputs, &names, &types, &typed,
            )
            .unwrap();
            assert_eq!(candidate.is_some(), accepted, "{text}");
            assert!(
                experiment::analyze_dual_nested_loop(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .is_none()
            );
            assert!(
                experiment::analyze_after_child_nested_loop(
                    &sources, &inputs, &names, &types, &typed
                )
                .unwrap()
                .is_none()
            );
            if let Some(bound) = candidate {
                assert!(
                    experiment::analyze_named_wrapper(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                        .is_none()
                );
                assert_eq!(
                    bound.callee().order(),
                    experiment::ReplacementOrder::ChildThenParent
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
                let callee = bound.callee();
                assert_eq!(
                    unit.ownership()
                        .ownership_primitives()
                        .iter()
                        .map(|p| p.descriptor().expression())
                        .collect::<Vec<_>>(),
                    callee.replacements()
                );
                let call_text = callee.replacements().map(|id| {
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
                });
                assert_eq!(
                    call_text,
                    ["replace(&a, \"next\")", "replace(&a, \"parent\")"]
                );
                assert_eq!(
                    unit.ownership().drops(),
                    [
                        callee.cleanup()[0],
                        callee.cleanup()[1],
                        callee.cleanup()[2],
                        callee.retained().cleanup()
                    ]
                );
                assert_eq!(
                    unit.ownership().loans(),
                    [
                        callee.loans()[0].clone(),
                        callee.loans()[1].clone(),
                        callee.final_root_use().loan().clone(),
                        callee.retained().loan().clone(),
                        bound.loans()[0].clone(),
                        bound.loans()[1].clone()
                    ]
                );
                assert_eq!(unit.ownership().direct_call_loan_ends(), bound.ends());
                assert_eq!(bound.loans()[0], *bound.ends()[1].loan());
                assert_eq!(bound.loans()[1], *bound.ends()[0].loan());
                assert_ne!(
                    bound.callee().counter_parameters()[0],
                    bound.callee().counter_parameters()[1]
                );
                assert!(bound.callee().final_root_use().call() != bound.call());
            }
        }
    }
}
