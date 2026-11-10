//! Bounded ordinary receiver results; real off products are never edited before lowering.
use super::*;
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
                super::super::refine::receiver_receipt::observe(|| {
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
                super::super::render_program(&program),
                crate::llvm::render_verified_program(&program).unwrap(),
            ));
        }
        assert_eq!(outputs[0], outputs[1]);
    }
}
