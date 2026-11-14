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
while(outer > 0) { val parentOld = replace(&a, "parent"); var inner = childCount;
while(inner > 0) { val old = replace(&a, "next"); inner -= 1 }; outer -= 1 };
println(a); println(b) }"#;
#[test]
fn named_wrapper_publishes_only_closed_before_child_source() {
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
                    "}; outer -= 1",
                    "}; val parentOld = replace(&a, \"parent\"); outer -= 1",
                ),
            false,
        ),
        (
            format!("fun entry() {{ val p = 1; val c = 2; run(p, c) }}\n{CALLEE}"),
            true,
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
            let candidate =
                experiment::analyze_named_wrapper(&sources, &inputs, &names, &types, &typed)
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
                // Declaration order is not role identity: resolve both actual source roots.
                let callable = bound.callee().controls().0;
                let entry_node = parsed.ast().items().get(bound.entry().item()).unwrap();
                let callee_node = parsed.ast().items().get(callable.item()).unwrap();
                let entry_decl = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| {
                        d.source_unit() == bound.entry().source_unit()
                            && d.root() == bound.entry().item()
                    })
                    .unwrap();
                let callee_decl = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| {
                        d.source_unit() == callable.source_unit() && d.root() == callable.item()
                    })
                    .unwrap();
                assert_ne!(entry_decl.id(), callee_decl.id());
                assert_eq!(callee_decl.name(), "run");
                assert_eq!(
                    entry_node.span().start() < callee_node.span().start(),
                    text.starts_with("fun entry()")
                );

                let callee = bound.callee();
                let callee_loans = [
                    callee.loans()[0].clone(),
                    callee.loans()[1].clone(),
                    callee.final_root_use().unwrap().loan().clone(),
                    callee.retained().loan().clone(),
                ];
                let expected_loans = if entry_node.span().start() < callee_node.span().start() {
                    bound
                        .loans()
                        .iter()
                        .chain(callee_loans.iter())
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    callee_loans
                        .iter()
                        .chain(bound.loans().iter())
                        .cloned()
                        .collect::<Vec<_>>()
                };
                assert_eq!(unit.ownership().loans(), expected_loans);
                assert_eq!(unit.ownership().direct_call_loan_ends(), bound.ends());
                assert_eq!(bound.loans()[0], *bound.ends()[1].loan());
                assert_eq!(bound.loans()[1], *bound.ends()[0].loan());
                assert_ne!(
                    bound.callee().counter_parameters()[0],
                    bound.callee().counter_parameters()[1]
                );
                assert!(bound.callee().final_root_use().is_some());
            }
        }
    }
}
