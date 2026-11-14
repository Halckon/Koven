//! Unified handoff transport, not another source-shape acceptance suite or native execution.
use super::*;
use lang_frontend::{
    name_resolution::SourceUnitInput,
    ownership_checking::{d10_catalog_experiment as experiment, owned_compilation_unit_view},
    source::SourceMap,
    type_checking::standard_environments,
};
#[test]
fn single_analysis_entry_dispatch_preserves_both_orders_and_object_checks() {
    for (body, order) in [
        (
            "val parentOld = replace(&a, \"parent\"); var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; outer -= 1",
            experiment::NamedEntryOrder::BeforeChild,
        ),
        (
            "var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; val parentOld = replace(&a, \"parent\"); outer -= 1",
            experiment::NamedEntryOrder::AfterChild,
        ),
    ] {
        let text = format!(
            "fun run(count: Int, childCount: Int) {{ var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) {{ {body} }}; println(a); println(b) }}\nfun entry() {{ val p = 1; val c = 2; run(p, c) }}"
        );
        let mut sources = SourceMap::new();
        let (id, parsed) = unit_lower_test_support::parsed(&mut sources, "dispatch.ko", &text);
        let (env, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "dispatch.ko", id, &parsed),
            SourceUnitInput::memory_document(id, &parsed),
        ] {
            let inputs = [input];
            let (names, typed, _) =
                unit_lower_test_support::analyze(&sources, &inputs, &env, &types);
            let bound = experiment::analyze_named_entry(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .unwrap();
            assert_eq!(bound.order(), order);
            let unit = owned_compilation_unit_view(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                bound.ownership(),
            )
            .unwrap();
            let publication = crate::d10_experiment::lower_named_entry(unit, &bound).unwrap();
            assert_eq!(publication.order(), order);
            assert_eq!(publication.callable(), bound.entry());
            assert!(!publication.verified_llvm_ir().unwrap().is_empty());
            let (program, _) = publication.replay_input();
            super::d10_named_process_object_tests::assert_success(program, &|p| {
                publication.emit_process_object(p)
            });
        }
    }
}
