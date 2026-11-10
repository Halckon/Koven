//! Ordinary source-only bounded routing checks; no native execution or fault injection.
use super::*;

#[cfg(feature = "loan-observation-test-support")]
const RECEIVER: &str =
    "class Node { fun touch(): Int = 1 }\nfun run(own node: Node): Unit { node.touch() }";

#[cfg(feature = "loan-observation-test-support")]
#[test]
fn receiver_route_is_independent_of_real_frontend_observation() {
    use lang_frontend::ownership_checking::{
        check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    };
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "receiver.ko", RECEIVER);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "receiver.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, on) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let off = with_loan_observation(false, || {
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap()
        });
        assert!(!on.ownership().loan_action_observations().is_empty());
        assert!(off.ownership().loan_action_observations().is_empty());
        assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
        let run = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap();
        let mut outputs = Vec::new();
        for owned in [&on, &off] {
            let ((lowered, routed), receipts) = super::super::refine::loan_receipt::observe(|| {
                super::super::refine::observe_routing(|| {
                    super::super::unit_lower::lower_scalar_unit_with_entry(
                        &sources,
                        &inputs,
                        &names,
                        &types,
                        &typed,
                        owned,
                        run.id(),
                    )
                })
            });
            let (program, entry) = lowered.unwrap();
            assert_eq!(
                routed.as_slice(),
                std::slice::from_ref(&program.modules[0].function(entry).unwrap().name)
            );
            assert!(receipts.is_empty(), "protocol 2 has no production receipt");
            verify_program(&program).unwrap();
            let [end] = owned.ownership().receiver_call_return_ends() else {
                panic!("one actual end");
            };
            let [drop] = owned.ownership().drops() else {
                panic!("one owner drop");
            };
            assert_receiver_lifecycle(
                &program,
                entry,
                source,
                end.receiver().begin_span(),
                end.receiver().end_span(),
                drop.value_origin(),
            );
            outputs.push((
                super::super::render_program(&program),
                crate::llvm::render_verified_program(&program).unwrap(),
            ));
        }
        assert_eq!(outputs[0], outputs[1]);
        assert!(off.ownership().loan_action_observations().is_empty());
    }
}

#[test]
fn neighboring_receiver_shapes_keep_existing_routes() {
    for text in [
        "class Node { fun touch(): Boolean = true }\nfun run(own node: Node): Unit { node.touch() }",
        "class Node { fun touch(): Int = 1 }\nfun run(node: Node): Unit { node.touch() }",
        "class Node { fun touch(): Int = 1 }\nfun run(own node: Node): Unit { node.touch(); node.touch() }",
        "class Node { fun touch(own x: Int): Int = x }\nfun run(own node: Node): Unit { node.touch(1) }",
    ] {
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "boundary.ko", text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "boundary.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let (names, typed, owned) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let run = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap();
            let (lowered, routed) = super::super::refine::observe_routing(|| {
                super::super::unit_lower::lower_scalar_unit_with_entry(
                    &sources,
                    &inputs,
                    &names,
                    &types,
                    &typed,
                    &owned,
                    run.id(),
                )
            });
            let (program, _) = lowered.unwrap();
            assert!(routed.is_empty(), "no new caller trigger for {text}");
            verify_program(&program).unwrap();
            crate::llvm::render_verified_program(&program).unwrap();
        }
    }
}
