//! Ordinary same-source Phase3 certificate and actual Unit SSA evidence, not native execution.
//! The certificate is inspected only here; production lowering still consumes existing facts.
use super::{model::*, unit_lower_test_support, verify::verify_program};
use lang_frontend::{
    name_resolution::SourceUnitInput,
    ownership_checking::{UnitLoanAction, UnitLoanActionCoverage},
    source::{SourceId, SourceMap, Span},
    type_checking::standard_environments,
};

const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; consume(view(owner)) }";

#[test]
fn certified_consumer_region_has_real_unit_ssa_lifecycle() {
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
        let (lowered, routed) = super::refine::observe_routing(|| {
            super::unit_lower::lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                &owned,
                declarations[0].id(),
            )
        });
        let (program, entry) = lowered.unwrap();
        assert!(routed.iter().any(|name| name == "koven.run.d2"));
        assert!(routed.iter().any(|name| name == "koven.view.d0"));
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

fn assert_span(span: Span, source: SourceId, expected: (usize, usize)) {
    assert_eq!(span.source_id(), source);
    assert_eq!((span.start(), span.end()), expected);
}

fn assert_lifecycle(
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
