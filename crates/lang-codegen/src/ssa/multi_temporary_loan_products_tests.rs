//! Real reanalysis and actual producer identities, with no synthetic inputs.
use super::*;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
#[test]
fn two_literal_int_unit_same_bytes_and_empty_preserve_true_off() {
    for (text, bytes, result_count) in [
        (SOURCE, [b"left".as_slice(), b"right".as_slice()], 1),
        (
            "fun inspect(a: String, b: String): Unit {}\nfun run(): Unit { inspect(\"same\", \"same\") }",
            [b"same".as_slice(), b"same".as_slice()],
            0,
        ),
        (
            "fun inspect(a: String, b: String): Int = 1\nfun run(): Unit { inspect(\"\", \"\") }",
            [b"".as_slice(), b"".as_slice()],
            1,
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "two-off.ko", text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "two-off.ko", source, &parsed),
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
            let observation = &on.ownership().loan_action_observations()[0];
            let [region] = observation.regions() else {
                panic!("one real region")
            };
            assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
            assert_eq!(region.protocol_version(), 1);
            let ids = region.action_ids().collect::<Vec<_>>();
            assert_eq!(ids.len(), 6);
            let actions = ids
                .iter()
                .map(|id| observation.action(id).unwrap())
                .collect::<Vec<_>>();
            let [
                UnitLoanAction::Begin(first),
                UnitLoanAction::Begin(second),
                ..,
            ] = actions.as_slice()
            else {
                panic!("two begin records")
            };
            assert_ne!(first.argument(), second.argument());
            assert_ne!(first.target(), second.target());
            for (i, begin) in [first, second].iter().enumerate() {
                let UnitLoanAction::Cleanup {
                    action:
                        lang_frontend::ownership_checking::UnitIterationCleanupAction::EndCallLoan(end),
                    ..
                } = actions[3 - i]
                else {
                    panic!("exact reverse End")
                };
                assert_eq!(*begin, end);
                let UnitLoanAction::Cleanup {
                    action:
                        lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(drop),
                    ..
                } = actions[5 - i]
                else {
                    panic!("exact reverse Drop")
                };
                assert_eq!(
                    drop.target(),
                    lang_frontend::ownership_checking::UnitDropTarget::Temporary(begin.argument())
                );
            }
            let run = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap();
            let mut outputs = Vec::new();
            for (owned, count) in [(&on, 1), (&off, 0)] {
                let ((lowered, routes), receipts) =
                    crate::ssa::refine::temporary_receipt::observe(|| {
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
                    });
                let (program, entry) = lowered.unwrap();
                verify_program(&program).unwrap();
                let module = &program.modules[0];
                let caller = module.function(entry).unwrap();
                assert_eq!(routes.as_slice(), std::slice::from_ref(&caller.name));
                assert_eq!(receipts.len(), count);
                assert_eq!(caller.instructions.len(), 11);
                let mut entities = Vec::new();
                let mut loans = Vec::new();
                let mut owners = Vec::new();
                for (i, expected) in bytes.iter().enumerate() {
                    let literal = &caller.instructions[3 * i];
                    let root = &caller.instructions[3 * i + 1];
                    let begin = &caller.instructions[3 * i + 2];
                    let Operation::StringLiteral { string, bytes } = &literal.operation else {
                        panic!("actual literal")
                    };
                    assert_eq!(bytes, expected);
                    assert_eq!(module.types[string.index()], SsaTypeKind::StringOwner);
                    let [EntityId::Value(owner)] = literal.results.as_slice() else {
                        panic!("owner")
                    };
                    let [EntityId::Place(place)] = root.results.as_slice() else {
                        panic!("place")
                    };
                    let [EntityId::Loan(loan)] = begin.results.as_slice() else {
                        panic!("loan")
                    };
                    assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
                    assert_eq!(
                        begin.operation,
                        Operation::BorrowBegin {
                            place: *place,
                            kind: LoanKind::Shared
                        }
                    );
                    assert_eq!(
                        caller.entity(EntityId::Loan(*loan)).unwrap().ty,
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: *string
                        }
                    );
                    entities.extend([
                        EntityId::Value(*owner),
                        EntityId::Place(*place),
                        EntityId::Loan(*loan),
                    ]);
                    loans.push(*loan);
                    owners.push(*owner);
                }
                assert!(
                    entities
                        .iter()
                        .all(|e| entities.iter().filter(|other| *other == e).count() == 1)
                );
                let call = &caller.instructions[6];
                let Operation::DirectCall {
                    callee,
                    receiver,
                    arguments,
                } = &call.operation
                else {
                    panic!("one actual call")
                };
                assert!(receiver.is_none());
                assert_eq!(
                    *arguments,
                    loans.iter().map(|l| EntityId::Loan(*l)).collect::<Vec<_>>()
                );
                assert_eq!(call.results.len(), result_count);
                if let [e] = call.results.as_slice() {
                    let EntityType::Value(ty) = caller.entity(*e).unwrap().ty else {
                        panic!("Int")
                    };
                    assert_eq!(
                        module.types[ty.index()],
                        SsaTypeKind::Integer {
                            bits: 32,
                            signed: true
                        }
                    );
                }
                let method = module.function(*callee).unwrap();
                assert_eq!(method.return_types.len(), result_count);
                assert_eq!(method.instructions.len(), result_count);
                assert_eq!(method.blocks[0].parameters.len(), 2);
                for (i, param) in method.blocks[0].parameters.iter().enumerate() {
                    let EntityId::Loan(param) = param else {
                        panic!("callee loan")
                    };
                    assert_ne!(param.function(), loans[i].function());
                    assert_eq!(
                        method.entity(EntityId::Loan(*param)).unwrap().ty,
                        caller.entity(EntityId::Loan(loans[i])).unwrap().ty
                    );
                }
                for i in 0..2 {
                    assert_eq!(
                        caller.instructions[7 + i].operation,
                        Operation::BorrowEnd { loan: loans[1 - i] }
                    );
                    assert_eq!(
                        caller.instructions[9 + i].operation,
                        Operation::Drop {
                            owner: owners[1 - i]
                        }
                    );
                }
                if let [r] = receipts.as_slice() {
                    assert_eq!(r.ids, ids);
                    assert_eq!(r.caller, entry);
                    assert_eq!(r.call, region.call());
                    assert_eq!(Some(r.callable), region.callable());
                    assert_eq!(r.entities, entities);
                    assert_eq!(
                        r.instructions,
                        caller.instructions.iter().map(|i| i.id).collect::<Vec<_>>()
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
#[test]
fn reordered_named_literals_preserve_existing_route_without_receipt() {
    let text = "fun inspect(a: String, b: String): Int = 1\nfun run(): Unit { inspect(b = \"first\", a = \"second\") }";
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "reordered.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "reordered.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let regions = owned
            .ownership()
            .loan_action_observations()
            .iter()
            .flat_map(|o| o.regions())
            .collect::<Vec<_>>();
        let [region] = regions.as_slice() else {
            panic!("one real region")
        };
        assert_eq!(region.protocol_version(), 1);
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(region.action_ids().count(), 6);
        let run = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap();
        let ((lowered, routes), receipts) = crate::ssa::refine::temporary_receipt::observe(|| {
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
        assert!(routes.is_empty());
        assert!(receipts.is_empty());
    }
}
