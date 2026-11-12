//! Ordinary same-source Phase3 certificate and actual Unit SSA evidence, not native execution.
//! Protocol 4 now has production receipts; legacy lifecycle producers remain authoritative.
use super::{model::*, unit_lower_test_support, verify::verify_program};
use lang_frontend::{
    name_resolution::SourceUnitInput,
    ownership_checking::{UnitLoanAction, UnitLoanActionCoverage},
    source::{SourceId, SourceMap, Span},
    type_checking::standard_environments,
};

pub(in crate::ssa) const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; consume(view(owner)) }";

#[test]
pub(in crate::ssa) fn certified_consumer_region_has_real_unit_ssa_lifecycle() {
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "consumer.ko", SOURCE);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "consumer.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let observation = &owned.ownership().loan_action_observations()[0];
        let regions: Vec<_> = observation
            .regions()
            .iter()
            .filter(|region| region.protocol_version() == 4)
            .collect();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].coverage(), UnitLoanActionCoverage::Complete);
        let action_ids: Vec<_> = regions[0].action_ids().collect();
        let actions: Vec<_> = action_ids
            .iter()
            .map(|id| observation.action(id).unwrap())
            .collect();
        let [
            UnitLoanAction::Begin(source_loan),
            UnitLoanAction::BorrowConsumerTransfer {
                source: moved,
                consumer: transferred_call,
                argument: transferred_argument,
            },
            UnitLoanAction::Begin(consumer_loan),
            UnitLoanAction::BorrowConsumerRetired(retired),
            UnitLoanAction::Cleanup {
                point: cleanup_point,
                action: lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(drop),
            },
        ] = actions.as_slice()
        else {
            panic!("complete exact region");
        };
        assert_eq!(*moved, *source_loan);
        assert_eq!(retired.source_begin().as_ref(), Some(&action_ids[0]));
        assert_eq!(retired.transfer().as_ref(), Some(&action_ids[1]));
        assert_eq!(retired.consumer_begin().as_ref(), Some(&action_ids[2]));
        assert_eq!(*transferred_call, consumer_loan.call());
        assert_eq!(*transferred_argument, consumer_loan.argument());
        assert_eq!(consumer_loan.argument(), source_loan.call());
        assert_eq!(source_loan.target(), consumer_loan.target());
        let lang_frontend::ownership_checking::UnitLoanTarget::Place(root) = source_loan.target()
        else {
            panic!("source root");
        };
        assert!(root.is_root());
        assert_eq!(
            drop.target(),
            lang_frontend::ownership_checking::UnitDropTarget::Named(root.root())
        );
        assert_eq!(
            *cleanup_point,
            lang_frontend::ownership_checking::UnitDropPoint::CallReturn(consumer_loan.call())
        );
        assert_eq!(drop.point(), *cleanup_point);

        assert_eq!(
            retired.transaction().ended_loans(),
            [consumer_loan, source_loan]
        );
        for (id, expected) in [
            (retired.source_begin().unwrap(), actions[0]),
            (retired.transfer().unwrap(), actions[1]),
            (retired.consumer_begin().unwrap(), actions[2]),
        ] {
            assert_eq!(observation.action(&id), Some(expected));
        }
        for (span, expected) in [
            (source_loan.begin_span(), (147, 152)),
            (source_loan.end_span(), (142, 153)),
            (consumer_loan.begin_span(), (142, 153)),
            (consumer_loan.end_span(), (134, 154)),
            (drop.value_origin(), (118, 123)),
        ] {
            assert_span(span, source, expected);
        }
        let declarations: Vec<_> = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| declaration.name() == "run")
            .collect();
        assert_eq!(declarations.len(), 1);
        for (loan, name) in [(source_loan, "view"), (consumer_loan, "consume")] {
            assert!(owned.ownership().loans().contains(loan));
            let candidates: Vec<_> = names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|declaration| declaration.name() == name)
                .collect();
            assert_eq!(candidates.len(), 1);
            let call = typed.types().call(loan.call()).unwrap();
            assert_eq!(
                call.target(),
                lang_frontend::type_checking::UnitCallTarget::Declaration(candidates[0].id())
            );
            assert_eq!(call.arguments().len(), 1);
            assert_eq!(call.arguments()[0].parameter_index(), 0);
        }
        let ((lowered, routed), receipts) = super::refine::loan_receipt::observe(|| {
            super::refine::observe_routing(|| {
                super::unit_lower::lower_scalar_unit_with_entry(
                    &sources,
                    &inputs,
                    &names,
                    &types,
                    &typed,
                    &owned,
                    declarations[0].id(),
                )
            })
        });
        let (program, entry) = lowered.unwrap();
        assert_eq!(
            receipts.len(),
            1,
            "Complete protocol 4 must have a production receipt"
        );
        assert!(routed.iter().any(|name| name == "koven.run.d2"));
        assert!(routed.iter().any(|name| name == "koven.view.d0"));
        let receipt = &receipts[0];
        assert_eq!(receipt.ids, action_ids);
        assert_eq!(receipt.call, consumer_loan.call());
        assert_eq!(Some(receipt.callable), regions[0].callable());
        assert_eq!(receipt.caller, entry);
        let function = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id() == entry)
            .unwrap();
        assert_eq!(
            receipt.instructions,
            function
                .instructions
                .iter()
                .skip(1)
                .map(|i| i.id)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            receipt.entities,
            [
                function.instructions[0].results[0],
                function.instructions[1].results[0],
                function.instructions[2].results[0],
                function.instructions[3].results[0],
            ]
        );
        verify_program(&program).unwrap();
        assert_lifecycle(
            &program,
            entry,
            source,
            source_loan,
            consumer_loan,
            drop.value_origin(),
        );
        let llvm = crate::llvm::render_verified_program(&program)
            .expect("verified LLVM, not object/native execution");
        // Match the adapter's actual instance-qualified symbols (llvm/adapter/call.rs).
        for name in ["f0.koven.view.d0", "f1.koven.consume.d1", "f2.koven.run.d2"] {
            assert!(
                llvm.lines().any(|line| line.starts_with("define ")
                    && (line.contains(&format!("@{name}("))
                        || line.contains(&format!("@\"{name}\"(")))),
                "missing verified definition {name}: {llvm}"
            );
        }
    }
}

pub(in crate::ssa) fn assert_span(span: Span, source: SourceId, expected: (usize, usize)) {
    assert_eq!(span.source_id(), source);
    assert_eq!((span.start(), span.end()), expected);
}

pub(in crate::ssa) fn assert_lifecycle(
    program: &Program,
    entry: FunctionId,
    source_file: SourceId,
    source_loan: &lang_frontend::ownership_checking::UnitLoanFact,
    consumer_loan: &lang_frontend::ownership_checking::UnitLoanFact,
    owner_origin: Span,
) {
    let [module] = program.modules.as_slice() else {
        panic!("one module");
    };
    assert_eq!(module.functions.len(), 3);
    let function_at = |bytes| {
        let found: Vec<_> = module.functions.iter().filter(|function| {
            matches!(function.origin, Origin::Source(span) if span.source_id() == source_file && (span.start(), span.end()) == bytes)
        }).collect();
        assert_eq!(found.len(), 1);
        found[0]
    };
    let view = function_at((0, 60));
    let consume = function_at((61, 95));
    let run = function_at((96, 156));
    assert_eq!(run.id, entry);
    let [block] = run.blocks.as_slice() else {
        panic!("one normal block");
    };
    assert!(block.parameters.is_empty());
    assert!(
        matches!(&block.terminator.as_ref().unwrap().kind, TerminatorKind::Return { values } if values.is_empty())
    );
    assert_eq!(
        run.instructions.len(),
        8,
        "full sequence, no hidden duplicated ends or drops"
    );
    assert_eq!(
        block.instructions,
        run.instructions
            .iter()
            .map(|instruction| instruction.id)
            .collect::<Vec<_>>()
    );
    for instruction in &run.instructions {
        assert_eq!(instruction.block, block.id);
    }
    assert_eq!(
        (run.values.len(), run.places.len(), run.loans.len()),
        (1, 1, 2)
    );
    let instructions = &run.instructions;
    let [EntityId::Value(owner)] = instructions[0].results.as_slice() else {
        panic!("actual owner");
    };
    let [EntityId::Place(place)] = instructions[1].results.as_slice() else {
        panic!("actual root");
    };
    let [EntityId::Loan(source)] = instructions[2].results.as_slice() else {
        panic!("source acquisition");
    };
    let [EntityId::Loan(result)] = instructions[3].results.as_slice() else {
        panic!("returned result loan");
    };
    assert_ne!(source, result);
    let Operation::StringLiteral { string, bytes } = &instructions[0].operation else {
        panic!("literal owner formation");
    };
    assert_eq!(bytes, b"kept");
    assert_eq!(
        instructions[1].operation,
        Operation::RootPlace { owner: *owner }
    );
    assert_eq!(
        instructions[2].operation,
        Operation::BorrowBegin {
            place: *place,
            kind: LoanKind::Shared
        }
    );
    assert_eq!(
        instructions[3].operation,
        Operation::BorrowCall {
            callee: view.id,
            arguments: vec![EntityId::Loan(*source)],
            source: *source
        }
    );
    assert_eq!(
        instructions[4].operation,
        Operation::DirectCall {
            callee: consume.id,
            receiver: None,
            arguments: vec![EntityId::Loan(*result)]
        }
    );
    assert_eq!(
        instructions[5].operation,
        Operation::BorrowEnd { loan: *result }
    );
    assert_eq!(
        instructions[6].operation,
        Operation::BorrowEnd { loan: *source }
    );
    assert_eq!(instructions[7].operation, Operation::Drop { owner: *owner });
    assert!(
        instructions[4..]
            .iter()
            .all(|instruction| instruction.results.is_empty())
    );
    for loan in [*source, *result] {
        assert_eq!(
            run.entity(EntityId::Loan(loan)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: *string
            }
        );
    }
    assert_span(instructions[0].origin.span(), source_file, (126, 132));
    for (index, origin) in [
        (1, source_loan.begin_span()),
        (2, source_loan.begin_span()),
        (3, source_loan.end_span()),
        (4, consumer_loan.end_span()),
        (5, consumer_loan.end_span()),
        (6, consumer_loan.end_span()),
        (7, owner_origin),
    ] {
        assert_eq!(instructions[index].origin, Origin::Source(origin));
    }
    for callee in [view, consume] {
        let [block] = callee.blocks.as_slice() else {
            panic!("one callee block");
        };
        let [EntityId::Loan(parameter)] = block.parameters.as_slice() else {
            panic!("callee-local parameter");
        };
        assert_ne!(parameter.function(), source.function());
        assert_eq!(
            callee.entity(EntityId::Loan(*parameter)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: *string
            }
        );
        assert!(
            callee.instructions.is_empty(),
            "callee must not copy/drop its borrowed parameter"
        );
        if callee.id == view.id {
            assert_eq!(callee.borrow_return, Some(0));
            assert_eq!(
                block.terminator.as_ref().unwrap().kind,
                TerminatorKind::BorrowReturn { loan: *parameter }
            );
        } else {
            assert_eq!(callee.borrow_return, None);
            assert!(
                matches!(&block.terminator.as_ref().unwrap().kind, TerminatorKind::Return { values } if values.is_empty())
            );
        }
    }
}

#[test]
pub(in crate::ssa) fn certified_receiver_region_has_real_unit_ssa_lifecycle() {
    use lang_frontend::ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitLoanRegionFamily,
        UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
    };
    use lang_frontend::type_checking::{ParameterMode, UnitCallTarget, UnitItemId};
    let text =
        "class Node { fun touch(): Int = 1 }\nfun run(own node: Node): Unit { node.touch() }";
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "receiver.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "receiver.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let declarations: Vec<_> = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| d.name() == "run")
            .collect();
        let [run] = declarations.as_slice() else {
            panic!("unique caller")
        };
        let observation = &owned.ownership().loan_action_observations()[0];
        let [region] = observation.regions() else {
            panic!("one receiver region")
        };
        assert!(observation.owns_region(region));
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
        assert_eq!(region.protocol_version(), 2);
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(
            region.families(),
            &[
                UnitLoanRegionFamily::SharedReceiver,
                UnitLoanRegionFamily::NamedSourceDrop
            ]
        );
        assert_eq!(
            region.callable(),
            Some(UnitItemId::new(run.source_unit(), run.root()))
        );
        let ids: Vec<_> = region.action_ids().collect();
        assert_eq!(ids.len(), observation.actions().len());
        for (index, id) in ids.iter().enumerate() {
            assert_eq!(Some(id), observation.action_id(index).as_ref());
        }
        let actions: Vec<_> = ids
            .iter()
            .map(|id| observation.action(id).unwrap())
            .collect();
        let [
            UnitLoanAction::ReceiverBegin(begin),
            UnitLoanAction::ReceiverEnd(end),
            UnitLoanAction::Cleanup {
                point,
                action: UnitIterationCleanupAction::Drop(drop),
            },
        ] = actions.as_slice()
        else {
            panic!("entire catalog-bound receiver interval")
        };
        assert_eq!(begin, end.receiver());
        assert_eq!(begin.call(), region.call());
        assert_eq!(region.call().source_unit(), run.source_unit());
        assert_eq!(begin.kind(), UnitReceiverOwnershipKind::SharedLoan);
        assert!(!begin.is_receiver_reservation());
        assert_eq!(begin.activation_point(), None);
        assert_eq!(*point, UnitDropPoint::CallReturn(region.call()));
        assert_eq!(end.point(), *point);
        assert_eq!(drop.point(), *point);
        let UnitReceiverOwnershipTarget::Place(root) = begin.target() else {
            panic!("named root")
        };
        assert!(root.is_root());
        let binding = owned
            .ownership()
            .bindings()
            .iter()
            .find(|binding| binding.symbol() == root.root())
            .unwrap();
        assert_eq!(
            binding.kind(),
            lang_frontend::ownership_checking::OwnershipBindingKind::Owned
        );
        assert_span(binding.declaration_span(), source, (44, 58));
        assert_eq!(drop.target(), UnitDropTarget::Named(root.root()));
        assert_eq!(
            owned.ownership().receiver_call_return_ends(),
            std::slice::from_ref(end)
        );
        assert!(owned.ownership().drops().contains(drop));
        assert_span(begin.begin_span(), source, (68, 72));
        assert_span(begin.end_span(), source, (68, 80));
        assert_span(drop.value_origin(), source, (48, 52));
        let call = typed.types().call(region.call()).unwrap();
        assert_eq!(call.expression(), begin.call());
        assert!(call.arguments().is_empty());
        let receiver = call.receiver().unwrap();
        assert_eq!(receiver.origin(), begin.source());
        assert_eq!(receiver.mode(), ParameterMode::Borrow);
        assert_eq!(receiver.ty(), begin.receiver_type());
        let UnitCallTarget::Symbol(target) = call.target() else {
            panic!("nominal method")
        };
        assert_eq!(target.source_unit(), run.source_unit());
        let local_names = names
            .names()
            .source_units()
            .iter()
            .find(|unit| unit.source_unit() == target.source_unit())
            .unwrap();
        let method = local_names
            .resolution()
            .symbols()
            .iter()
            .find(|symbol| symbol.id() == target.symbol())
            .unwrap();
        assert_eq!(method.name(), "touch");
        let owner_symbol = local_names
            .resolution()
            .symbols()
            .iter()
            .find(|symbol| symbol.id() == root.root().symbol())
            .unwrap();
        assert_eq!(root.root().source_unit(), run.source_unit());
        assert_eq!(owner_symbol.name(), "node");
        assert_eq!(owner_symbol.span(), drop.value_origin());
        assert_span(method.span(), source, (17, 22));
        let ((lowered, routed), receiver_receipts) =
            super::refine::receiver_receipt::observe(|| {
                super::refine::observe_routing(|| {
                    super::unit_lower::lower_scalar_unit_with_entry(
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
        assert_eq!(
            receiver_receipts.len(),
            1,
            "Complete2 requires a production receipt"
        );
        let receipt = &receiver_receipts[0];
        assert_eq!(receipt.ids, ids);
        assert_eq!(Some(receipt.callable), region.callable());
        assert_eq!(receipt.call, region.call());
        assert_eq!(receipt.caller, entry);
        let actual = program.modules[0].function(entry).unwrap();
        assert_eq!(
            receipt.instructions,
            actual.instructions.iter().map(|i| i.id).collect::<Vec<_>>()
        );
        assert_eq!(
            receipt.entities,
            [
                actual.blocks[0].parameters[0],
                actual.instructions[0].results[0],
                actual.instructions[1].results[0]
            ]
        );
        // The bounded caller migrates independently of its ordinary method callee.
        let caller = program.modules[0].function(entry).unwrap();
        assert_eq!(routed.as_slice(), std::slice::from_ref(&caller.name));
        verify_program(&program).unwrap();
        assert_receiver_lifecycle(
            &program,
            entry,
            source,
            begin.begin_span(),
            begin.end_span(),
            drop.value_origin(),
        );
        let llvm = crate::llvm::render_verified_program(&program).unwrap();
        for name in ["f0.koven.run.d1", "f1.koven.Node.touch.s2"] {
            assert!(llvm.lines().any(|line| line.starts_with("define ")
                && (line.contains(&format!("@{name}("))
                    || line.contains(&format!("@\"{name}\"(")))));
        }
    }
}

pub(in crate::ssa) fn assert_receiver_lifecycle(
    program: &Program,
    entry: FunctionId,
    source: SourceId,
    begin: Span,
    end: Span,
    owner_origin: Span,
) {
    let [module] = program.modules.as_slice() else {
        panic!("one module")
    };
    assert_eq!(module.functions.len(), 2);
    let run = module.function(entry).unwrap();
    assert_span(run.origin.span(), source, (36, 82));
    let [block] = run.blocks.as_slice() else {
        panic!("one caller block")
    };
    let [EntityId::Value(owner)] = block.parameters.as_slice() else {
        panic!("owner entry")
    };
    let EntityType::Value(node) = run.entity(EntityId::Value(*owner)).unwrap().ty else {
        panic!("owned Node")
    };
    assert!(matches!(
        module.types[node.index()],
        SsaTypeKind::HeapOwner { .. }
    ));
    assert_eq!(run.receiver, None);
    assert_eq!(
        (run.values.len(), run.places.len(), run.loans.len()),
        (2, 1, 1)
    );
    assert_eq!(
        block.instructions,
        run.instructions.iter().map(|i| i.id).collect::<Vec<_>>()
    );
    let [root, acquire, call, retire, destroy] = run.instructions.as_slice() else {
        panic!("full receiver lifecycle")
    };
    assert!(run.instructions.iter().all(|i| i.block == block.id));
    let [EntityId::Place(place)] = root.results.as_slice() else {
        panic!("actual root place")
    };
    let [EntityId::Loan(loan)] = acquire.results.as_slice() else {
        panic!("actual receiver loan")
    };
    let [EntityId::Value(result)] = call.results.as_slice() else {
        panic!("method result")
    };
    assert_ne!(owner, result);
    let EntityType::Value(integer) = run.entity(EntityId::Value(*result)).unwrap().ty else {
        panic!("owned Int result")
    };
    assert_eq!(
        module.types[integer.index()],
        SsaTypeKind::Integer {
            bits: 32,
            signed: true
        }
    );
    assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
    assert_eq!(
        acquire.operation,
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
        panic!("method call")
    };
    assert_eq!(*receiver, Some(EntityId::Loan(*loan)));
    assert!(arguments.is_empty());
    assert_eq!(retire.operation, Operation::BorrowEnd { loan: *loan });
    assert_eq!(destroy.operation, Operation::Drop { owner: *owner });
    assert!(retire.results.is_empty() && destroy.results.is_empty());
    for (instruction, span) in [
        (root, begin),
        (acquire, begin),
        (call, end),
        (retire, end),
        (destroy, owner_origin),
    ] {
        assert_eq!(instruction.origin, Origin::Source(span));
    }
    let loan_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: node,
    };
    assert_eq!(run.entity(EntityId::Loan(*loan)).unwrap().ty, loan_type);
    assert!(
        matches!(&block.terminator.as_ref().unwrap().kind, TerminatorKind::Return { values } if values.is_empty())
    );
    let method = module.function(*callee).unwrap();
    assert_span(method.origin.span(), source, (13, 33));
    assert_eq!(method.receiver, Some(loan_type));
    assert_eq!(method.return_types, vec![integer]);
    let [callee_block] = method.blocks.as_slice() else {
        panic!("one callee block")
    };
    let [EntityId::Loan(parameter)] = callee_block.parameters.as_slice() else {
        panic!("callee receiver")
    };
    assert_ne!(parameter.function(), loan.function());
    assert_eq!(
        method.entity(EntityId::Loan(*parameter)).unwrap().ty,
        loan_type
    );
    let [constant] = method.instructions.as_slice() else {
        panic!("no callee owner or cleanup")
    };
    assert_eq!(
        constant.operation,
        Operation::Constant(ScalarConstant::Integer(1))
    );
    let [EntityId::Value(value)] = constant.results.as_slice() else {
        panic!("constant result")
    };
    assert_eq!(
        method.entity(EntityId::Value(*value)).unwrap().ty,
        EntityType::Value(integer)
    );
    assert_eq!(
        callee_block.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return {
            values: vec![*value]
        }
    );
}

#[path = "loan_receipt_tests.rs"]
mod receipts;

#[cfg(feature = "loan-observation-test-support")]


#[cfg(feature = "loan-observation-test-support")]

#[path = "receiver_receipt_sibling_tests.rs"]
pub(in crate::ssa) mod receiver_receipt_siblings;


#[cfg(feature = "loan-observation-test-support")]



#[cfg(feature = "loan-observation-test-support")]

mod temporary_siblings {
    //! One ordinary analysis containing P1/P2/P3/P4 and B/Call/Return, without native execution.
    use super::*;
    #[test]
    fn temporary_named_receiver_and_owner_siblings_keep_receipts_separate() {
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
    fun temporary(): Unit { inspect("temporary") }
    fun entry(): Unit { temporary(); receiver(Node()); named("named"); loan(); owner_call(false); owner_drop(false); val result = owner_return(false) }"#;
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
            let (((((((lowered, calls), drops), consumer), receiver), named), returns), temporary) =
                super::super::refine::temporary_receipt::observe(|| {
                    crate::ssa::observe_owner_returns(|| {
                        super::super::refine::named_receipt::observe(|| {
                            super::super::refine::receiver_receipt::observe(|| {
                                super::super::refine::loan_receipt::observe(|| {
                                    crate::ssa::observe_owner_drops(|| {
                                        crate::ssa::observe_owner_calls(|| {
                                            super::super::unit_lower::lower_scalar_unit_with_entry(
                                                &sources, &inputs, &names, &types, &typed, &owned,
                                                entry,
                                            )
                                        })
                                    })
                                })
                            })
                        })
                    })
                });
            let (program, _) = lowered.unwrap();
            verify_program(&program).unwrap();
            crate::llvm::render_verified_program(&program).unwrap();
            let [p1] = temporary.as_slice() else {
                panic!("one P1 receipt");
            };
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
                (p3.caller, &p3.ids, &p3.entities),
                (p4.caller, &p4.ids, &p4.entities),
            ] {
                assert_ne!(p1.caller, caller);
                assert!(p1.ids.iter().all(|id| !ids.contains(id)));
                assert!(p1.entities.iter().all(|entity| !entities.contains(entity)));
            }
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
                (1, &p1.ids, p1.call),
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

#[cfg(feature = "loan-observation-test-support")]
#[path = "multi_temporary_loan_tests.rs"]
pub(in crate::ssa) mod multi_temporary;

#[cfg(feature = "loan-observation-test-support")]
#[path = "named_literal_mapping_tests.rs"]
pub(in crate::ssa) mod named_literal_mapping;

mod finite_literal {
    //! Ordinary three-literal source exercises the finite-arity route.
    use super::*;
    const SOURCE: &str = "fun inspect(a: String, b: String, c: String): Int = 1\nfun run(): Unit { inspect(\"first\", \"second\", \"third\") }";
    #[test]
    fn finite_literal_caller_requires_real_refine_route() {
        check(false);
    }
    fn check(receipt: bool) {
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "two-literal.ko", SOURCE);
        let (environment, types) = standard_environments();
        let inputs = [SourceUnitInput::memory_document(source, &parsed)];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let regions = owned
            .ownership()
            .loan_action_observations()
            .iter()
            .flat_map(|o| o.regions())
            .collect::<Vec<_>>();
        let [region] = regions.as_slice() else {
            panic!("one actual region")
        };
        assert_eq!(region.protocol_version(), 1);
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(region.action_ids().count(), 9);
        let run = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap();
        let ((lowered, routes), receipts) = super::super::refine::temporary_receipt::observe(|| {
            super::super::refine::observe_routing(|| {
                super::super::unit_lower::lower_scalar_unit_with_entry(
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
        assert_eq!(
            routes.as_slice(),
            std::slice::from_ref(&caller.name),
            "only actual caller should migrate"
        );
        if receipt {
            assert_eq!(
                receipts.len(),
                1,
                "complete triple needs one actual call receipt"
            );
        }
    }

    #[test]
    fn finite_literal_call_requires_real_production_receipt() {
        check(true);
    }

    #[cfg(feature = "loan-observation-test-support")]
    #[test]
    fn finite_literal_non_involutive_mappings_preserve_true_off() {
        super::multi_temporary::products::check_cases(
            &[(
                "fun inspect(a: String, b: String, c: String): Int = 1\nfun run(): Unit { inspect(c = \"zero\", a = \"one\", b = \"two\") }",
                [b"zero".as_slice(), b"one".as_slice(), b"two".as_slice()],
                1,
            )],
            [1, 2, 0],
        );
        super::multi_temporary::products::check_cases(
            &[(
                "fun inspect(a: String, b: String, c: String, d: String): Unit {}\nfun run(): Unit { inspect(c = \"same\", a = \"\", d = \"same\", b = \"last\") }",
                [
                    b"same".as_slice(),
                    b"".as_slice(),
                    b"same".as_slice(),
                    b"last".as_slice(),
                ],
                0,
            )],
            [1, 3, 0, 2],
        );
    }
}

#[path = "binding_lifecycle_probe_tests.rs"]
pub(in crate::ssa) mod binding_lifecycle_probe;

#[cfg(feature = "loan-observation-test-support")]
pub(in crate::ssa) use parent_child_coverage::binding_causal_links;

#[cfg(feature = "loan-observation-test-support")]

#[cfg(feature = "loan-observation-test-support")]


#[cfg(feature = "loan-observation-test-support")]
#[path = "parent_child_causal_tests.rs"]
mod parent_child_causal;

#[cfg(feature = "loan-observation-test-support")]
#[path = "parent_child_drop_dependency_tests.rs"]
pub(in crate::ssa) mod parent_child_drop_dependency;

#[cfg(feature = "loan-observation-test-support")]
#[path = "parent_child_coverage_tests.rs"]
pub(in crate::ssa) mod parent_child_coverage;
