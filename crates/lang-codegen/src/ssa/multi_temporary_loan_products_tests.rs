//! Real reanalysis and actual producer identities, with no synthetic inputs.
use super::*;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
#[test]
fn two_literal_int_unit_same_bytes_and_empty_preserve_true_off() {
    check_cases(
        &[
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
        ],
        [0, 1],
    );
}

pub(in crate::ssa::loan_region_ssa_tests) fn check_cases<const N: usize>(
    cases: &[(&str, [&[u8]; N], usize)],
    parameter_sources: [usize; N],
) {
    for &(text, bytes, result_count) in cases {
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
            let mapped = typed.types().call(region.call()).unwrap();
            let expected_mapping = (0..N)
                .map(|source| {
                    (
                        source,
                        parameter_sources.iter().position(|i| *i == source).unwrap(),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                mapped
                    .arguments()
                    .iter()
                    .map(|a| (a.argument_index(), a.parameter_index()))
                    .collect::<Vec<_>>(),
                expected_mapping
            );

            let ids = region.action_ids().collect::<Vec<_>>();
            assert_eq!(ids.len(), 3 * N);
            let actions = ids
                .iter()
                .map(|id| observation.action(id).unwrap())
                .collect::<Vec<_>>();
            let begins = actions[..N]
                .iter()
                .map(|action| match action {
                    UnitLoanAction::Begin(begin) => *begin,
                    _ => panic!("source-order Begin"),
                })
                .collect::<Vec<_>>();
            for (i, begin) in begins.iter().enumerate() {
                assert!(begins[..i].iter().all(|previous| {
                    previous.argument() != begin.argument() && previous.target() != begin.target()
                }));
                let parameter = expected_mapping[i].1;
                assert_eq!(
                    sources.slice(begin.parameter_span().unwrap()).unwrap(),
                    format!(
                        "{}: String",
                        char::from(b'a' + u8::try_from(parameter).unwrap())
                    )
                );
                let contract = on
                    .ownership()
                    .call_argument_contracts()
                    .iter()
                    .find(|c| c.call() == region.call() && c.argument() == begin.argument())
                    .unwrap();
                assert_eq!(contract.parameter_index(), parameter);
                assert_eq!(contract.parameter_span(), begin.parameter_span());

                let UnitLoanAction::Cleanup {
                    action:
                        lang_frontend::ownership_checking::UnitIterationCleanupAction::EndCallLoan(end),
                    ..
                } = actions[2 * N - 1 - i]
                else {
                    panic!("exact reverse End")
                };
                assert_eq!(begin, end);
                let UnitLoanAction::Cleanup {
                    action:
                        lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(drop),
                    ..
                } = actions[3 * N - 1 - i]
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
                assert_eq!(caller.instructions.len(), 5 * N + 1);
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
                let call = &caller.instructions[3 * N];
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
                    parameter_sources
                        .iter()
                        .map(|i| EntityId::Loan(loans[*i]))
                        .collect::<Vec<_>>()
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
                assert_eq!(method.blocks[0].parameters.len(), N);
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
                for i in 0..N {
                    assert_eq!(
                        caller.instructions[3 * N + 1 + i].operation,
                        Operation::BorrowEnd {
                            loan: loans[N - 1 - i]
                        }
                    );
                    assert_eq!(
                        caller.instructions[4 * N + 1 + i].operation,
                        Operation::Drop {
                            owner: owners[N - 1 - i]
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
fn reordered_named_literals_use_refine_with_production_receipt() {
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
        let (program, entry) = lowered.unwrap();
        verify_program(&program).unwrap();
        crate::llvm::render_verified_program(&program).unwrap();
        let caller = program.modules[0].function(entry).unwrap();
        assert_eq!(routes.as_slice(), std::slice::from_ref(&caller.name));
        assert_eq!(receipts.len(), 1);
    }
}
