//! Ordinary receiver fallback with one real explicit scalar argument.
use crate::ssa::loan_region_ssa_tests::*;

const SOURCE: &str = "class Node { fun touch(own x: Int): Int = x }\nfun run(own node: Node): Unit { node.touch(1) }";

#[test]
fn explicit_scalar_receiver_refine_preserves_callee_argument_and_owner_release() {
    let text = format!("{SOURCE}\nfun entry(): Unit {{ run(Node()) }}");
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "receiver_argument.ko", &text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "receiver_argument.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let entry_declaration = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == "entry")
            .unwrap()
            .id();
        let ((lowered, routed), receipts) = crate::ssa::refine::receiver_receipt::observe(|| {
            crate::ssa::refine::observe_routing(|| {
                crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                    &sources,
                    &inputs,
                    &names,
                    &types,
                    &typed,
                    &owned,
                    entry_declaration,
                )
            })
        });
        let (program, entry) = lowered.unwrap();
        let [module] = program.modules.as_slice() else {
            panic!("one module");
        };
        let outer = module.function(entry).unwrap();
        assert!(
            outer
                .instructions
                .iter()
                .all(|i| !matches!(i.operation, Operation::Drop { .. }))
        );
        let Some(allocation) = outer
            .instructions
            .iter()
            .find(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
        else {
            panic!("outer caller owns the Node allocation");
        };
        let [EntityId::Value(allocated_owner)] = allocation.results.as_slice() else {
            panic!("owned Node");
        };
        let transfers = outer
            .instructions
            .iter()
            .filter_map(|instruction| match &instruction.operation {
                Operation::DirectCall {
                    callee,
                    receiver: None,
                    arguments,
                } => Some((*callee, arguments)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(callee, arguments)] = transfers.as_slice() else {
            panic!("entry transfers Node to run exactly once");
        };
        assert_eq!(*arguments, &[EntityId::Value(*allocated_owner)]);
        let run = module.function(*callee).unwrap();
        assert_eq!(
            sources.slice(run.origin.span()).unwrap(),
            SOURCE.split_once('\n').unwrap().1
        );
        assert_eq!(routed.as_slice(), std::slice::from_ref(&run.name));
        let [block] = run.blocks.as_slice() else {
            panic!("straight-line receiver caller");
        };
        let [EntityId::Value(owner)] = block.parameters.as_slice() else {
            panic!("one transferred owner");
        };
        let [root, begin, scalar, call, end, drop] = run.instructions.as_slice() else {
            panic!(
                "receiver, scalar argument, call and cleanup order: {:?}",
                run.instructions
            );
        };
        assert_eq!(
            block.instructions,
            [root.id, begin.id, scalar.id, call.id, end.id, drop.id]
        );
        assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
        let [EntityId::Place(place)] = root.results.as_slice() else {
            panic!("actual receiver place");
        };
        assert_eq!(
            begin.operation,
            Operation::BorrowBegin {
                place: *place,
                kind: LoanKind::Shared
            }
        );
        let [EntityId::Loan(loan)] = begin.results.as_slice() else {
            panic!("one acquired shared receiver");
        };
        assert_eq!(
            scalar.operation,
            Operation::Constant(ScalarConstant::Integer(1))
        );
        assert_eq!(sources.slice(scalar.origin.span()).unwrap(), "1");
        let [EntityId::Value(argument)] = scalar.results.as_slice() else {
            panic!("one scalar argument result");
        };
        let Operation::DirectCall {
            callee: method,
            receiver,
            arguments,
        } = &call.operation
        else {
            panic!("one explicit-argument member call");
        };
        assert_eq!(*receiver, Some(EntityId::Loan(*loan)));
        assert_eq!(arguments, &[EntityId::Value(*argument)]);
        let [EntityId::Value(result)] = call.results.as_slice() else {
            panic!("one Int call result");
        };
        assert_ne!(result, argument);
        let EntityType::Value(integer) = run.entity(EntityId::Value(*argument)).unwrap().ty else {
            panic!("owned scalar");
        };
        assert_eq!(
            module.types[integer.index()],
            SsaTypeKind::Integer {
                bits: 32,
                signed: true
            }
        );
        assert_eq!(
            run.entity(EntityId::Value(*result)).unwrap().ty,
            EntityType::Value(integer)
        );
        let method = module.function(*method).unwrap();
        assert_eq!(method.return_types, [integer]);
        let [method_block] = method.blocks.as_slice() else {
            panic!("one straight-line method block");
        };
        let [EntityId::Loan(method_receiver), EntityId::Value(parameter)] =
            method_block.parameters.as_slice()
        else {
            panic!("separate receiver and scalar parameter slots");
        };
        assert_ne!(loan.function(), method_receiver.function());
        assert_eq!(
            method.entity(EntityId::Loan(*method_receiver)).unwrap().ty,
            run.entity(EntityId::Loan(*loan)).unwrap().ty
        );
        assert_eq!(
            method.entity(EntityId::Value(*parameter)).unwrap().ty,
            EntityType::Value(integer)
        );
        assert_eq!(
            method_block.terminator.as_ref().unwrap().kind,
            TerminatorKind::Return {
                values: vec![*parameter]
            }
        );
        assert_eq!(end.operation, Operation::BorrowEnd { loan: *loan });
        assert_eq!(drop.operation, Operation::Drop { owner: *owner });
        assert_eq!(sources.slice(call.origin.span()).unwrap(), "node.touch(1)");
        assert_eq!(end.origin, call.origin);

        verify_program(&program).unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let regions = owned
            .ownership()
            .loan_action_observations()
            .iter()
            .flat_map(|o| o.regions())
            .filter(|r| {
                r.protocol_version() == 2 && r.coverage() == UnitLoanActionCoverage::Complete
            })
            .collect::<Vec<_>>();
        let [region] = regions.as_slice() else {
            panic!("one actual Complete2 receiver region");
        };
        let [receipt] = receipts.as_slice() else {
            panic!("one real scalar receiver receipt");
        };
        assert_eq!(receipt.ids, region.action_ids().collect::<Vec<_>>());
        assert_eq!(Some(receipt.callable), region.callable());
        assert_eq!(receipt.call, region.call());
        assert_eq!(receipt.caller, run.id());
        assert_eq!(
            receipt.instructions,
            [root.id, begin.id, call.id, end.id, drop.id]
        );
        assert_eq!(
            receipt.entities,
            [
                EntityId::Value(*owner),
                EntityId::Place(*place),
                EntityId::Loan(*loan),
            ]
        );
        #[cfg(feature = "loan-observation-test-support")]
        {
            use lang_frontend::ownership_checking::{
                check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
            };
            let off = with_loan_observation(false, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .validate()
                    .unwrap()
            });
            assert!(legacy_ownership_eq(owned.ownership(), off.ownership()));
            assert!(off.ownership().loan_action_observations().is_empty());
            let ((off_program, off_routes), off_receipts) =
                crate::ssa::refine::receiver_receipt::observe(|| {
                    crate::ssa::refine::observe_routing(|| {
                        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                            &sources,
                            &inputs,
                            &names,
                            &types,
                            &typed,
                            &off,
                            entry_declaration,
                        )
                    })
                });
            let (off_program, off_entry) = off_program.unwrap();
            assert_eq!(off_routes, routed);
            assert!(off_receipts.is_empty());
            verify_program(&off_program).unwrap();
            assert_eq!(
                crate::ssa::render_program(&off_program),
                crate::ssa::render_program(&program)
            );
            assert_eq!(
                crate::llvm::render_verified_program_with_entry(&off_program, off_entry).unwrap(),
                llvm
            );
        }
        let output =
            crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[0]);
        crate::native_tests::boxed_enum_tests::assert_success(&output, b"");
    }
}

#[test]
fn scalar_receiver_argument_already_lowers_in_refine_with_independent_anchor() {
    // The original fallback fixture is unchanged. This neighbor makes Refine eligible
    // independently of the receiver, without changing its call or cleanup operations.
    let text = SOURCE.replace(
        "{ node.touch(1) }",
        "{ val anchor: Int? = null; node.touch(1) }",
    );
    assert_ne!(text, SOURCE);
    let text = format!("{text}\nfun entry(): Unit {{ run(Node()) }}");
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "argument_anchor.ko", &text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "argument_anchor.ko", source, &parsed),
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
            .find(|declaration| declaration.name() == "entry")
            .unwrap()
            .id();
        let ((lowered, routes), receipts) = crate::ssa::refine::receiver_receipt::observe(|| {
            crate::ssa::refine::observe_routing(|| {
                crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
            })
        });
        let (program, entry) = lowered.unwrap();
        let module = &program.modules[0];
        let outer = module.function(entry).unwrap();
        let run = outer
            .instructions
            .iter()
            .find_map(|i| {
                if let Operation::DirectCall {
                    callee,
                    receiver: None,
                    ..
                } = i.operation
                {
                    return module.function(callee);
                }
                None
            })
            .unwrap();
        assert!(routes.contains(&run.name), "{routes:?}");
        assert!(receipts.iter().all(|receipt| receipt.caller != run.id()));
        let [EntityId::Value(owner)] = run.blocks[0].parameters.as_slice() else {
            panic!("one original parameter owner");
        };
        let exact = run
            .instructions
            .iter()
            .filter(|i| {
                matches!(
                    i.operation,
                    Operation::RootPlace { .. }
                        | Operation::BorrowBegin { .. }
                        | Operation::Constant(ScalarConstant::Integer(1))
                        | Operation::DirectCall { .. }
                        | Operation::BorrowEnd { .. }
                        | Operation::Drop { .. },
                )
            })
            .collect::<Vec<_>>();
        let [root, begin, scalar, call, end, drop] = exact.as_slice() else {
            panic!("only the original receiver lifecycle and scalar evaluation: {exact:?}");
        };
        assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
        let [EntityId::Place(place)] = root.results.as_slice() else {
            panic!("one root");
        };
        let [EntityId::Loan(loan)] = begin.results.as_slice() else {
            panic!("one loan");
        };
        assert_eq!(
            begin.operation,
            Operation::BorrowBegin {
                place: *place,
                kind: LoanKind::Shared
            }
        );
        let [EntityId::Value(arg)] = scalar.results.as_slice() else {
            panic!("one literal");
        };
        assert!(matches!(
            &call.operation,
            Operation::DirectCall { receiver: Some(EntityId::Loan(actual)), arguments, .. }
                if actual == loan && arguments == &[EntityId::Value(*arg)]
        ));
        assert_eq!(end.operation, Operation::BorrowEnd { loan: *loan });
        assert_eq!(drop.operation, Operation::Drop { owner: *owner });
        verify_program(&program).unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let output =
            crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[0]);
        crate::native_tests::boxed_enum_tests::assert_success(&output, b"");
    }
}
