//! Ordinary full-Unit evidence only; no new D10 bound, publication, or source-set permission.
use super::*;
use lang_frontend::{
    name_resolution::SourceUnitInput,
    ownership_checking::{d10_catalog_experiment, owned_compilation_unit_view},
    source::SourceMap,
    type_checking::standard_environments,
};

#[test]
fn split_sources_preserve_real_unit_call_graph_without_named_admission() {
    for body in [
        "val parentOld = replace(&a, \"parent\"); var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; outer -= 1",
        "var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; val parentOld = replace(&a, \"parent\"); outer -= 1",
    ] {
        let callee_text = format!(
            "fun run(count: Int, childCount: Int) {{ var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) {{ {body} }}; println(a); println(b) }}"
        );
        for callee_first in [true, false] {
            let (callee_path, entry_path) = if callee_first {
                ("a.ko", "z.ko")
            } else {
                ("z.ko", "a.ko")
            };
            let mut sources = SourceMap::new();
            let (callee_source, callee_parsed) =
                unit_lower_test_support::parsed(&mut sources, callee_path, &callee_text);
            let (entry_source, entry_parsed) = unit_lower_test_support::parsed(
                &mut sources,
                entry_path,
                "fun entry() { val p = 1; val c = 2; run(p, c) }",
            );
            let (environment, types) = standard_environments();
            for reverse_inputs in [false, true] {
                let mut inputs = [
                    SourceUnitInput::new("root", callee_path, callee_source, &callee_parsed),
                    SourceUnitInput::new("root", entry_path, entry_source, &entry_parsed),
                ];
                if reverse_inputs {
                    inputs.reverse();
                }
                let (names, typed, owned) =
                    unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
                assert!(owned.ownership().deferred().is_empty());
                let source_units = names.names().index().source_units();
                assert_eq!(source_units.len(), 2);
                assert_eq!(source_units[0].source_id() == callee_source, callee_first);
                let entry = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.name() == "entry")
                    .unwrap();
                let callee = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.name() == "run")
                    .unwrap();
                assert_ne!(entry.source_unit(), callee.source_unit());
                // The actual ordinary Unit pipeline may lower this shape. That is not the new
                // source-qualified D10 graph capability, which still rejects two inputs.
                assert!(
                    d10_catalog_experiment::analyze_named_entry(
                        &sources, &inputs, &names, &types, &typed
                    )
                    .unwrap()
                    .is_none()
                );
                let unit =
                    owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned)
                        .unwrap();
                let (program, entry_id) =
                    unit_lower::lower_owned_unit_with_entry(&unit, entry.id()).unwrap();
                verify::verify_program(&program).unwrap();
                crate::llvm::render_verified_program(&program).unwrap();
                assert_eq!(program.modules.len(), 1);
                let module = &program.modules[0];
                assert_eq!(module.functions.len(), 2);
                let wrapper = module.function(entry_id).unwrap();
                assert_eq!(
                    wrapper.origin,
                    model::Origin::Source(
                        entry_parsed.ast().items().get(entry.root()).unwrap().span()
                    )
                );
                let callee_origin = model::Origin::Source(
                    callee_parsed
                        .ast()
                        .items()
                        .get(callee.root())
                        .unwrap()
                        .span(),
                );
                let matches = module
                    .functions
                    .iter()
                    .filter(|f| f.origin == callee_origin)
                    .collect::<Vec<_>>();
                let [actual_callee] = matches.as_slice() else {
                    panic!("one actual callee in full Program")
                };
                let calls = wrapper
                    .instructions
                    .iter()
                    .filter(|i| matches!(i.operation, model::Operation::DirectCall { .. }))
                    .collect::<Vec<_>>();
                let [call] = calls.as_slice() else {
                    panic!("one real cross-source call")
                };
                let model::Operation::DirectCall {
                    callee: target,
                    receiver: None,
                    arguments,
                } = &call.operation
                else {
                    panic!("direct source call")
                };
                assert_eq!(*target, actual_callee.id());
                assert_eq!(arguments.len(), 2);
                let model::Origin::Source(call_span) = call.origin else {
                    panic!("actual call source")
                };
                assert_eq!(call_span.source_id(), entry_source);
                assert_eq!(sources.slice(call_span).unwrap(), "run(p, c)");
                assert_eq!(
                    unit.inputs().len(),
                    2,
                    "never crop inputs to reuse a singleton checker"
                );
            }
        }
    }
}
