//! Bounded ordinary temporary literal results; real off products are never edited before lowering.
use super::*;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};

#[test]
fn temporary_int_unit_and_empty_string_preserve_real_observer_off_products() {
    for (text, result_count, expected_bytes) in [
        (
            "fun inspect(value: String): Int = 1\nfun run(): Unit { inspect(\"temporary\") }",
            1,
            b"temporary".as_slice(),
        ),
        (
            "fun inspect(value: String): Unit {}\nfun run(): Unit { inspect(\"temporary\") }",
            0,
            b"temporary".as_slice(),
        ),
        (
            "fun inspect(value: String): Int = 1\nfun run(): Unit { inspect(\"\") }",
            1,
            b"".as_slice(),
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "temporary-receipt.ko", text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "temporary-receipt.ko", source, &parsed),
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
                    r.protocol_version() == 1 && r.coverage() == UnitLoanActionCoverage::Complete
                })
                .collect::<Vec<_>>();
            let [region] = regions.as_slice() else {
                panic!("one nonempty Complete1");
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
                    super::super::refine::temporary_receipt::observe(|| {
                        super::super::refine::loan_receipt::observe(|| {
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
                        })
                    });
                let (program, entry) = lowered.unwrap();
                assert_eq!(receipts.len(), count);
                assert!(p4.is_empty());
                verify_program(&program).unwrap();
                let module = &program.modules[0];
                let caller = module.function(entry).unwrap();
                assert_eq!(routing.as_slice(), std::slice::from_ref(&caller.name));
                let [literal, root, begin, call, end, drop] = caller.instructions.as_slice() else {
                    panic!("six actual operations");
                };
                let [EntityId::Value(owner)] = literal.results.as_slice() else {
                    panic!("actual literal owner");
                };
                let Operation::StringLiteral { string, bytes } = &literal.operation else {
                    panic!("actual literal formation");
                };
                assert_eq!(bytes, expected_bytes);
                assert_eq!(module.types[string.index()], SsaTypeKind::StringOwner);
                assert_eq!(
                    caller.entity(EntityId::Value(*owner)).unwrap().ty,
                    EntityType::Value(*string)
                );
                let [EntityId::Place(place)] = root.results.as_slice() else {
                    panic!("root place");
                };
                let [EntityId::Loan(loan)] = begin.results.as_slice() else {
                    panic!("temporary argument loan");
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
                    panic!("actual ordinary call");
                };
                assert!(receiver.is_none());
                assert_eq!(arguments, &[EntityId::Loan(*loan)]);
                assert_eq!(call.results.len(), result_count);
                if let [value @ EntityId::Value(_)] = call.results.as_slice() {
                    let EntityType::Value(integer) = caller.entity(*value).unwrap().ty else {
                        panic!("owned Int call result");
                    };
                    assert_eq!(
                        module.types[integer.index()],
                        SsaTypeKind::Integer {
                            bits: 32,
                            signed: true
                        }
                    );
                }
                assert_eq!(
                    caller.entity(EntityId::Loan(*loan)).unwrap().ty,
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: *string
                    }
                );
                assert_eq!(end.operation, Operation::BorrowEnd { loan: *loan });
                assert_eq!(drop.operation, Operation::Drop { owner: *owner });
                let method = module.function(*callee).unwrap();
                let [EntityId::Loan(parameter)] = method.blocks[0].parameters.as_slice() else {
                    panic!("callee-local loan");
                };
                assert_ne!(parameter.function(), loan.function());
                assert_eq!(
                    method.entity(EntityId::Loan(*parameter)).unwrap().ty,
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: *string
                    }
                );
                assert_eq!(method.return_types.len(), result_count);
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
                    super::super::render_program(&program),
                    crate::llvm::render_verified_program(&program).unwrap(),
                ));
            }
            assert_eq!(outputs[0], outputs[1]);
        }
    }
}
