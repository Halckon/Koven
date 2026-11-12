//! Ordinary finite programs only: region boundaries, not whole-function instruction shapes.
use super::*;
use crate::ssa::refine::loan_receipt;

#[test]
fn int_consumer_with_neighboring_plain_calls_has_local_receipt() {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Int = 7\nfun ping(): Unit {}\nfun run(): Unit { val owner = \"kept\"; ping(); consume(view(owner)); ping() }";
    check(text, "run", 1, true);
}

#[test]
fn partial_live_source_preserves_legacy_lowering_without_receipt() {
    let text = SOURCE.replace(
        "consume(view(owner))",
        "consume(view(owner)); consume(owner)",
    );
    check(&text, "run", 0, false);
}

#[test]
fn sibling_callables_keep_catalog_and_ssa_receipts_local() {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun first(): Unit { val owner = \"kept\"; consume(view(owner)) }\nfun second(): Unit { val owner = \"kept\"; consume(view(owner)) }\nfun run(): Unit { first(); second() }";
    check(text, "run", 2, false);
}

fn check(text: &str, entry_name: &str, expected: usize, neighbors: bool) {
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "receipt.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "receipt.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let regions = owned
            .ownership()
            .loan_action_observations()
            .iter()
            .flat_map(|observation| {
                observation
                    .regions()
                    .iter()
                    .filter(|region| region.protocol_version() == 4)
                    .map(move |region| (observation, region))
            })
            .collect::<Vec<_>>();
        if expected == 0 {
            assert_eq!(regions.len(), 1);
            assert_eq!(regions[0].1.coverage(), UnitLoanActionCoverage::Partial);
        } else {
            assert_eq!(regions.len(), expected);
            assert!(
                regions
                    .iter()
                    .all(|(_, r)| r.coverage() == UnitLoanActionCoverage::Complete)
            );
        }
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == entry_name)
            .unwrap()
            .id();
        let (lowered, receipts) = loan_receipt::observe(|| {
            super::super::unit_lower::lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &types, &typed, &owned, entry,
            )
        });
        let (program, _) = lowered.unwrap();
        verify_program(&program).unwrap();
        crate::llvm::render_verified_program(&program).unwrap();
        assert_eq!(receipts.len(), expected);
        for receipt in &receipts {
            let (observation, region) = regions
                .iter()
                .find(|(_, r)| r.call() == receipt.call)
                .unwrap();
            assert_eq!(Some(receipt.callable), region.callable());
            assert_eq!(receipt.ids, region.action_ids().collect::<Vec<_>>());
            assert!(
                receipt
                    .ids
                    .iter()
                    .all(|id| observation.action(id).is_some())
            );
            let function = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .find(|f| f.id() == receipt.caller)
                .unwrap();
            let instructions = receipt
                .instructions
                .iter()
                .map(|id| function.instruction(*id).unwrap())
                .collect::<Vec<_>>();
            let [owner, place, source, result] = receipt.entities.as_slice() else {
                panic!("four local roles")
            };
            assert_ne!(source, result);
            assert_eq!(instructions[0].operation.entities(), [*owner]);
            assert_eq!(instructions[0].results, [*place]);
            assert_eq!(instructions[1].operation.entities(), [*place]);
            assert_eq!(instructions[1].results, [*source]);
            assert!(matches!(
                instructions[2].operation,
                Operation::BorrowCall { .. }
            ));
            assert_eq!(instructions[2].results, [*result]);
            assert!(matches!(
                instructions[3].operation,
                Operation::DirectCall { .. }
            ));
            assert_eq!(instructions[3].operation.entities(), [*result]);
            assert!(matches!(
                instructions[4].operation,
                Operation::BorrowEnd { .. }
            ));
            assert_eq!(instructions[4].operation.entities(), [*result]);
            assert!(matches!(
                instructions[5].operation,
                Operation::BorrowEnd { .. }
            ));
            assert_eq!(instructions[5].operation.entities(), [*source]);
            assert!(matches!(instructions[6].operation, Operation::Drop { .. }));
            assert_eq!(instructions[6].operation.entities(), [*owner]);
            if neighbors {
                assert!(function.instructions.len() > 8);
                assert_eq!(
                    instructions[3].results.len(),
                    1,
                    "outer Int result is an ordinary value"
                );
                assert!(matches!(instructions[3].results[0], EntityId::Value(_)));
            }
        }
        if let [first, second] = receipts.as_slice() {
            assert_ne!(first.call, second.call);
            assert_ne!(first.callable, second.callable);
            assert_ne!(first.caller, second.caller);
            assert!(first.ids.iter().all(|id| !second.ids.contains(id)));
            assert!(
                first
                    .entities
                    .iter()
                    .all(|id| !second.entities.contains(id))
            );
        }
    }
}

mod observer_off {
    //! Genuine frontend observation scopes. Off products are never edited before lowering.
    use crate::ssa::loan_region_ssa_tests::*;
    use lang_frontend::ownership_checking::{
        check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    };

    const BOTH: &str = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun loan(): Unit { val owner = \"kept\"; consume(view(owner)) }\nfun owner_run(flag: Boolean) { var held = \"outer\"; while(flag) { val previous = replace(&held, \"next\"); break }; val retired = held }\nfun entry(): Unit { owner_run(false); loan(); owner_run(true) }";

    #[test]
    fn real_frontend_observer_off_preserves_products_ssa_and_owner_receipts() {
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "observer-off.ko", BOTH);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "observer-off.ko", source, &parsed),
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
            assert!(
                off.ownership().loan_action_observations().is_empty(),
                "real frontend observer-off must suppress acquisition observation"
            );
            assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
            assert_eq!(
                on.ownership().owner_observations(),
                off.ownership().owner_observations()
            );
            assert!(
                off.ownership()
                    .owner_observations()
                    .iter()
                    .any(|o| o.replace_break_observation_v1().is_some())
            );
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "entry")
                .unwrap()
                .id();
            let mut rendered = Vec::new();
            let mut drops = Vec::new();
            for (owned, expected) in [(&on, 1), (&off, 0)] {
                let ((lowered, owner_drops), receipts) =
                    crate::ssa::refine::loan_receipt::observe(|| {
                        crate::ssa::refine::observe_owner_drops(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, owned, entry,
                            )
                        })
                    });
                let (program, _) = lowered.unwrap();
                verify_program(&program).unwrap();
                assert_eq!(receipts.len(), expected);
                assert_eq!(owner_drops.iter().filter(|d| d.3.is_some()).count(), 1);
                rendered.push((
                    crate::ssa::render_program(&program),
                    crate::llvm::render_verified_program(&program).unwrap(),
                ));
                drops.push(owner_drops);
            }
            assert_eq!(rendered[0], rendered[1]);
            assert_eq!(drops[0], drops[1]);
            assert!(
                off.ownership().loan_action_observations().is_empty(),
                "comparison did not mutate the real off product"
            );
        }
    }

    fn observed() -> bool {
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "scope.ko", SOURCE);
        let inputs = [SourceUnitInput::memory_document(source, &parsed)];
        let (environment, types) = standard_environments();
        let (_, _, owned) = unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        !owned.ownership().loan_action_observations().is_empty()
    }

    #[test]
    fn observation_scope_restores_nested_state_and_does_not_cross_threads() {
        assert!(observed());
        with_loan_observation(false, || {
            assert!(!observed());
            with_loan_observation(true, || assert!(observed()));
            assert!(!observed());
            assert!(std::thread::spawn(observed).join().unwrap());
            assert!(!observed());
        });
        assert!(observed());
    }

    #[test]
    fn ci_only_observation_scope_restores_after_panic() {
        let result =
            std::panic::catch_unwind(|| with_loan_observation(false, || panic!("CI-only unwind")));
        assert!(result.is_err());
        assert!(observed());
    }
}

mod receiver_route {
    //! Ordinary source-only bounded routing checks; no native execution or fault injection.
    use crate::ssa::loan_region_ssa_tests::*;

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
            for (owned, expected) in [(&on, 1), (&off, 0)] {
                let (((lowered, routed), receipts), receiver_receipts) =
                    crate::ssa::refine::receiver_receipt::observe(|| {
                        crate::ssa::refine::loan_receipt::observe(|| {
                            crate::ssa::refine::observe_routing(|| {
                                crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                    &sources,
                                    &inputs,
                                    &names,
                                    &types,
                                    &typed,
                                    owned,
                                    run.id(),
                                )
                            })
                        })
                    });
                let (program, entry) = lowered.unwrap();
                assert_eq!(
                    routed.as_slice(),
                    std::slice::from_ref(&program.modules[0].function(entry).unwrap().name)
                );
                assert!(receipts.is_empty(), "P2 must not produce a P4 receipt");
                assert_eq!(receiver_receipts.len(), expected);
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
                    crate::ssa::render_program(&program),
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
                let ((lowered, routed), receipts) =
                    crate::ssa::refine::receiver_receipt::observe(|| {
                        crate::ssa::refine::observe_routing(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources,
                                &inputs,
                                &names,
                                &types,
                                &typed,
                                &owned,
                                run.id(),
                            )
                        })
                    });
                let (program, _) = lowered.unwrap();
                assert!(receipts.is_empty());
                assert!(routed.is_empty(), "no new caller trigger for {text}");
                verify_program(&program).unwrap();
                crate::llvm::render_verified_program(&program).unwrap();
            }
        }
    }
}

mod receiver_receipts {
    //! Bounded ordinary receiver results; real off products are never edited before lowering.
    use crate::ssa::loan_region_ssa_tests::*;
    use lang_frontend::ownership_checking::{
        check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    };

    #[test]
    fn unit_receiver_receipt_closes_real_slots_and_observer_off_is_transparent() {
        let text =
            "class Node { fun touch(): Unit {} }\nfun run(own node: Node): Unit { node.touch() }";
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "unit-receiver.ko", text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "unit-receiver.ko", source, &parsed),
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
            assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
            assert!(off.ownership().loan_action_observations().is_empty());
            let regions = on
                .ownership()
                .loan_action_observations()
                .iter()
                .flat_map(|o| o.regions())
                .filter(|r| {
                    r.protocol_version() == 2 && r.coverage() == UnitLoanActionCoverage::Complete
                })
                .collect::<Vec<_>>();
            let [region] = regions.as_slice() else {
                panic!("one Complete2");
            };
            let run = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap();
            let mut outputs = Vec::new();
            for (owned, count) in [(&on, 1), (&off, 0)] {
                let (((lowered, routing), p4), receipts) =
                    crate::ssa::refine::receiver_receipt::observe(|| {
                        crate::ssa::refine::loan_receipt::observe(|| {
                            crate::ssa::refine::observe_routing(|| {
                                crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                    &sources,
                                    &inputs,
                                    &names,
                                    &types,
                                    &typed,
                                    owned,
                                    run.id(),
                                )
                            })
                        })
                    });
                let (program, entry) = lowered.unwrap();
                assert_eq!(receipts.len(), count);
                assert!(p4.is_empty());
                verify_program(&program).unwrap();
                let module = &program.modules[0];
                let caller = module.function(entry).unwrap();
                assert_eq!(routing.as_slice(), std::slice::from_ref(&caller.name));
                let [root, begin, call, end, drop] = caller.instructions.as_slice() else {
                    panic!("five actual operations");
                };
                let [EntityId::Value(owner)] = caller.blocks[0].parameters.as_slice() else {
                    panic!("original parameter owner");
                };
                let [EntityId::Place(place)] = root.results.as_slice() else {
                    panic!("root place");
                };
                let [EntityId::Loan(loan)] = begin.results.as_slice() else {
                    panic!("receiver loan");
                };
                assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
                assert_eq!(
                    begin.operation,
                    Operation::BorrowBegin {
                        place: *place,
                        kind: LoanKind::Shared
                    }
                );
                let Operation::DirectCall {
                    callee,
                    receiver,
                    arguments,
                } = &call.operation
                else {
                    panic!("actual method");
                };
                assert_eq!(*receiver, Some(EntityId::Loan(*loan)));
                assert!(arguments.is_empty());
                assert!(call.results.is_empty());
                assert_eq!(end.operation, Operation::BorrowEnd { loan: *loan });
                assert_eq!(drop.operation, Operation::Drop { owner: *owner });
                let method = module.function(*callee).unwrap();
                let [EntityId::Loan(parameter)] = method.blocks[0].parameters.as_slice() else {
                    panic!("callee-local loan");
                };
                assert_ne!(parameter.function(), loan.function());
                assert!(method.instructions.is_empty());
                if let [receipt] = receipts.as_slice() {
                    assert_eq!(receipt.ids, region.action_ids().collect::<Vec<_>>());
                    assert_eq!(Some(receipt.callable), region.callable());
                    assert_eq!(receipt.call, region.call());
                    assert_eq!(receipt.caller, entry);
                    assert_eq!(
                        receipt.instructions,
                        caller.instructions.iter().map(|i| i.id).collect::<Vec<_>>()
                    );
                    assert_eq!(
                        receipt.entities,
                        [
                            EntityId::Value(*owner),
                            EntityId::Place(*place),
                            EntityId::Loan(*loan)
                        ]
                    );
                }
                outputs.push((
                    crate::ssa::render_program(&program),
                    crate::llvm::render_verified_program(&program).unwrap(),
                ));
            }
            assert_eq!(outputs[0], outputs[1]);
        }
    }
}

mod named_route {
    //! Ordinary source-only P3 route capability; no native execution or faults.
    use crate::ssa::loan_region_ssa_tests::*;

    #[test]
    fn named_borrow_caller_uses_refine_without_migrating_callee() {
        check(false);
    }

    #[test]
    fn named_borrow_requires_real_production_receipt() {
        check(true);
    }

    fn check(require_receipt: bool) {
        let text = "fun inspect(value: String): Int = 1\nfun run(own source: String): Unit { inspect(source) }";
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "named.ko", text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "named.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let (names, typed, owned) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            assert!(
                owned
                    .ownership()
                    .loan_action_observations()
                    .iter()
                    .flat_map(|o| o.regions())
                    .any(|r| r.protocol_version() == 3
                        && r.coverage() == UnitLoanActionCoverage::Complete)
            );
            let run = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap();
            let ((lowered, routing), receipts) = crate::ssa::refine::named_receipt::observe(|| {
                crate::ssa::refine::observe_routing(|| {
                    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                        &sources,
                        &inputs,
                        &names,
                        &types,
                        &typed,
                        &owned,
                        run.id(),
                    )
                })
            });
            let (program, entry) = lowered.unwrap();
            verify_program(&program).unwrap();
            crate::llvm::render_verified_program(&program).unwrap();
            let caller = program.modules[0].function(entry).unwrap();
            if require_receipt {
                assert_eq!(receipts.len(), 1, "Complete3 needs a production receipt");
            }
            assert_eq!(
                routing.as_slice(),
                std::slice::from_ref(&caller.name),
                "only the named Borrow caller should migrate"
            );
        }
    }

    #[test]
    fn neighboring_named_shapes_preserve_legacy_routes_without_receipts() {
        for text in [
            "fun inspect(value: String): Int = 1\nfun run(source: String): Unit { inspect(source) }",
            "fun inspect(value: String): Int = 1\nfun run(own source: String): Unit { inspect(source); inspect(source) }",
            "fun inspect(value: String): Boolean = true\nfun run(own source: String): Unit { inspect(source) }",
            "fun inspect(value: String): Int = 1\nfun run(own source: String, flag: Boolean): Unit { inspect(source) }",
        ] {
            let mut sources = SourceMap::new();
            let (source, parsed) =
                unit_lower_test_support::parsed(&mut sources, "named-boundary.ko", text);
            let (environment, types) = standard_environments();
            for input in [
                SourceUnitInput::new("root", "named-boundary.ko", source, &parsed),
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
                let ((lowered, routing), receipts) =
                    crate::ssa::refine::named_receipt::observe(|| {
                        crate::ssa::refine::observe_routing(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources,
                                &inputs,
                                &names,
                                &types,
                                &typed,
                                &owned,
                                run.id(),
                            )
                        })
                    });
                let (program, _) = lowered.unwrap();
                verify_program(&program).unwrap();
                crate::llvm::render_verified_program(&program).unwrap();
                assert!(routing.is_empty());
                assert!(receipts.is_empty());
            }
        }
    }
}

mod named_receipts {
    //! Bounded ordinary named argument results; real off products are never edited before lowering.
    use crate::ssa::loan_region_ssa_tests::*;
    use lang_frontend::ownership_checking::{
        check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    };

    #[test]
    fn named_int_and_unit_receipts_preserve_real_observer_off_products() {
        for (text, result_count) in [
            (
                "fun inspect(value: String): Int = 1\nfun run(own source: String): Unit { inspect(source) }",
                1,
            ),
            (
                "fun inspect(value: String): Unit {}\nfun run(own source: String): Unit { inspect(source) }",
                0,
            ),
        ] {
            let mut sources = SourceMap::new();
            let (source, parsed) =
                unit_lower_test_support::parsed(&mut sources, "named-receipt.ko", text);
            let (environment, types) = standard_environments();
            for input in [
                SourceUnitInput::new("root", "named-receipt.ko", source, &parsed),
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
                assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
                assert!(off.ownership().loan_action_observations().is_empty());
                let regions = on
                    .ownership()
                    .loan_action_observations()
                    .iter()
                    .flat_map(|o| o.regions())
                    .filter(|r| {
                        r.protocol_version() == 3 && r.coverage() == UnitLoanActionCoverage::Complete
                    })
                    .collect::<Vec<_>>();
                let [region] = regions.as_slice() else {
                    panic!("one Complete3");
                };
                let run = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.name() == "run")
                    .unwrap();
                let mut outputs = Vec::new();
                for (owned, count) in [(&on, 1), (&off, 0)] {
                    let (((lowered, routing), p4), receipts) =
                        crate::ssa::refine::named_receipt::observe(|| {
                            crate::ssa::refine::loan_receipt::observe(|| {
                                crate::ssa::refine::observe_routing(|| {
                                    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                        &sources,
                                        &inputs,
                                        &names,
                                        &types,
                                        &typed,
                                        owned,
                                        run.id(),
                                    )
                                })
                            })
                        });
                    let (program, entry) = lowered.unwrap();
                    assert_eq!(receipts.len(), count);
                    assert!(p4.is_empty());
                    verify_program(&program).unwrap();
                    let module = &program.modules[0];
                    let caller = module.function(entry).unwrap();
                    assert_eq!(routing.as_slice(), std::slice::from_ref(&caller.name));
                    let [root, begin, call, end, drop] = caller.instructions.as_slice() else {
                        panic!("five actual operations");
                    };
                    let [EntityId::Value(owner)] = caller.blocks[0].parameters.as_slice() else {
                        panic!("original parameter owner");
                    };
                    let [EntityId::Place(place)] = root.results.as_slice() else {
                        panic!("root place");
                    };
                    let [EntityId::Loan(loan)] = begin.results.as_slice() else {
                        panic!("receiver loan");
                    };
                    assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
                    assert_eq!(
                        begin.operation,
                        Operation::BorrowBegin {
                            place: *place,
                            kind: LoanKind::Shared
                        }
                    );
                    let Operation::DirectCall {
                        callee,
                        receiver,
                        arguments,
                    } = &call.operation
                    else {
                        panic!("actual method");
                    };
                    assert!(receiver.is_none());
                    assert_eq!(arguments, &[EntityId::Loan(*loan)]);
                    assert_eq!(call.results.len(), result_count);
                    assert_eq!(end.operation, Operation::BorrowEnd { loan: *loan });
                    assert_eq!(drop.operation, Operation::Drop { owner: *owner });
                    let method = module.function(*callee).unwrap();
                    let [EntityId::Loan(parameter)] = method.blocks[0].parameters.as_slice() else {
                        panic!("callee-local loan");
                    };
                    assert_ne!(parameter.function(), loan.function());
                    assert_eq!(method.instructions.len(), result_count);
                    if let [receipt] = receipts.as_slice() {
                        assert_eq!(receipt.ids, region.action_ids().collect::<Vec<_>>());
                        assert_eq!(Some(receipt.callable), region.callable());
                        assert_eq!(receipt.call, region.call());
                        assert_eq!(receipt.caller, entry);
                        assert_eq!(
                            receipt.instructions,
                            caller.instructions.iter().map(|i| i.id).collect::<Vec<_>>()
                        );
                        assert_eq!(
                            receipt.entities,
                            [
                                EntityId::Value(*owner),
                                EntityId::Place(*place),
                                EntityId::Loan(*loan)
                            ]
                        );
                    }
                    outputs.push((
                        crate::ssa::render_program(&program),
                        crate::llvm::render_verified_program(&program).unwrap(),
                    ));
                }
                assert_eq!(outputs[0], outputs[1]);
            }
        }
    }
}

mod named_siblings {
    //! One ordinary analysis containing P2/P3/P4 and B/Call/Return, without native execution.
    use crate::ssa::loan_region_ssa_tests::*;
    #[test]
    fn named_receiver_consumer_and_all_owner_siblings_keep_receipts_separate() {
        let text = r#"class Node { fun touch(): Int = 1 }
    fun receiver(own node: Node): Unit { node.touch() }
    fun view(source: String): borrow String from source = source
    fun consume(item: String): Unit {}
    fun loan(): Unit { val owner = "kept"; consume(view(owner)) }
    fun sink(own item: String): Unit {}
    fun owner_call(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; sink(held) }
    fun owner_drop(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; val retired = held }
    fun inspect(value: String): Int = 1
    fun named(own source: String): Unit { inspect(source) }
    fun owner_return(flag: Boolean): String { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; return held }
    fun entry(): Unit { receiver(Node()); named("named"); loan(); owner_call(false); owner_drop(false); val result = owner_return(false) }"#;
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "receipt-siblings.ko", text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "receipt-siblings.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let (names, typed, owned) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "entry")
                .unwrap()
                .id();
            let ((((((lowered, calls), drops), consumer), receiver), named), returns) =
                crate::ssa::observe_owner_returns(|| {
                    crate::ssa::refine::named_receipt::observe(|| {
                        crate::ssa::refine::receiver_receipt::observe(|| {
                            crate::ssa::refine::loan_receipt::observe(|| {
                                crate::ssa::observe_owner_drops(|| {
                                    crate::ssa::observe_owner_calls(|| {
                                        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                            &sources, &inputs, &names, &types, &typed, &owned, entry,
                                        )
                                    })
                                })
                            })
                        })
                    })
                });
            let (program, _) = lowered.unwrap();
            verify_program(&program).unwrap();
            crate::llvm::render_verified_program(&program).unwrap();
            let [p3] = named.as_slice() else {
                panic!("one P3 receipt");
            };
            assert_eq!(returns.len(), 1);
            assert!(returns[0].1.is_some());
            assert_eq!(returns[0].1, returns[0].2);
            let [p2] = receiver.as_slice() else {
                panic!("one P2 receipt");
            };
            let [p4] = consumer.as_slice() else {
                panic!("one P4 receipt");
            };
            for (caller, ids, entities) in [
                (p2.caller, &p2.ids, &p2.entities),
                (p4.caller, &p4.ids, &p4.entities),
            ] {
                assert_ne!(p3.caller, caller);
                assert!(p3.ids.iter().all(|id| !ids.contains(id)));
                assert!(p3.entities.iter().all(|entity| !entities.contains(entity)));
            }
            assert_ne!(p2.caller, p4.caller);
            assert_ne!(p2.call, p4.call);
            assert_ne!(p2.callable, p4.callable);
            assert!(p2.ids.iter().all(|id| !p4.ids.contains(id)));
            assert!(
                p2.entities
                    .iter()
                    .all(|entity| !p4.entities.contains(entity))
            );
            assert_eq!(
                drops.iter().filter(|drop| drop.3.is_some()).count(),
                1,
                "only B owner Drop authorization"
            );
            assert_eq!(
                calls.iter().filter(|call| call.3.is_some()).count(),
                1,
                "only own String Call authorization"
            );
            for (protocol, ids, call) in [
                (2, &p2.ids, p2.call),
                (3, &p3.ids, p3.call),
                (4, &p4.ids, p4.call),
            ] {
                let matches = owned
                    .ownership()
                    .loan_action_observations()
                    .iter()
                    .flat_map(|o| o.regions())
                    .filter(|r| r.protocol_version() == protocol && r.call() == call)
                    .collect::<Vec<_>>();
                let [region] = matches.as_slice() else {
                    panic!("one same-analysis region");
                };
                assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
                assert_eq!(*ids, region.action_ids().collect::<Vec<_>>());
            }
        }
    }
}
