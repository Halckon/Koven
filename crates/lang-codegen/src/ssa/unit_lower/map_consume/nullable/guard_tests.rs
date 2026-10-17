//! CI-only qualification/live/cleanup negatives before any new NullableTake.
use super::*;
use crate::ssa::unit_lower::{
    call_lifetimes::PendingCallFrame,
    orchestrate::nullable_map_guard_support::{Checkpoint, Fixture, Stage, state_text, unchanged},
};

#[derive(Clone, Copy, Debug)]
enum Fault {
    Source,
    MissingStorage,
    MissingRegistry,
    WrongRegistry,
    WrongNarrowed,
    MissingProof,
    MissingBinding,
    HeaderBinding,
    Inner,
    Exclusive,
    Definition,
    Alias,
    Temporary,
    Borrow,
    Pending,
    Frame,
    ResultLoan,
    ReplayTake,
    NullArgument,
    NonNullArgument,
    NonNullTarget,
    BranchOrigin,
}
#[derive(Clone, Copy, Debug)]
enum Target {
    Qualification,
    View,
    Cleanup,
    Entry,
}

#[test]
fn unit_nullable_map_consume_guards() {
    for id in ["C1", "C2", "C3"] {
        let fixture = Fixture::new(id);
        for fault in [
            Fault::Source,
            Fault::MissingStorage,
            Fault::MissingRegistry,
            Fault::WrongRegistry,
            Fault::WrongNarrowed,
        ] {
            pair(&fixture, Target::Qualification, fault);
        }
        for fault in [
            Fault::MissingProof,
            Fault::MissingBinding,
            Fault::HeaderBinding,
            Fault::Inner,
            Fault::Exclusive,
            Fault::Definition,
            Fault::Alias,
            Fault::Temporary,
            Fault::Borrow,
            Fault::Pending,
            Fault::Frame,
            Fault::ResultLoan,
            Fault::ReplayTake,
            Fault::NullArgument,
            Fault::NonNullArgument,
            Fault::NonNullTarget,
            Fault::BranchOrigin,
        ] {
            pair(&fixture, Target::Entry, fault);
        }
        // Direct view guard independently fixes its M-vs-U boundary; Entry's
        // earlier proof validation must not masquerade as this target's rejection.
        for fault in [
            Fault::Definition,
            Fault::Exclusive,
            Fault::HeaderBinding,
            Fault::ReplayTake,
            Fault::NullArgument,
            Fault::NonNullArgument,
            Fault::NonNullTarget,
            Fault::BranchOrigin,
        ] {
            pair(&fixture, Target::View, fault);
        }
        // These M checks are from cleanup.rs itself, not the stricter live U gate.
        for fault in [
            Fault::MissingBinding,
            Fault::Alias,
            Fault::Temporary,
            Fault::Borrow,
            Fault::ResultLoan,
        ] {
            pair(&fixture, Target::Cleanup, fault);
        }
    }
}

fn pair(fixture: &Fixture, target: Target, fault: Fault) {
    run(fixture, target, None);
    run(fixture, target, Some(fault));
}

fn run(fixture: &Fixture, target: Target, fault: Option<Fault>) {
    let checkpoint = Checkpoint::read(fixture);
    let mut function = fixture.function().clone();
    let mut types = fixture.type_ids.clone();
    let mut nullable = fixture.program.modules[0].nullable_maps.clone();
    match fault {
        Some(Fault::MissingStorage) => {
            types.remove(&checkpoint.storage);
        }
        Some(Fault::MissingRegistry) => {
            nullable.remove(&checkpoint.nullable);
        }
        Some(Fault::WrongRegistry) => {
            nullable.insert(checkpoint.nullable, checkpoint.nullable);
        }
        Some(Fault::WrongNarrowed) => {
            types.insert(checkpoint.narrowed, checkpoint.nullable);
        }
        _ => {}
    }
    fixture.with_lowerer(Stage::BeforeUse, &mut function, &types, &nullable, |lowerer, other_source| {
        // Qualification and witness used as inputs to direct live/cleanup guards
        // come from their clean state before the single runtime mutation below.
        let source = if matches!(target, Target::View | Target::Cleanup) {
            Some(lowerer.qualify_nullable_map_consume(checkpoint.use_id, checkpoint.use_span).unwrap().unwrap())
        } else { None };
        let witness = if matches!(target, Target::View) {
            Some(lowerer.nullable_map_consume_witness(checkpoint.control, checkpoint.use_span).unwrap().unwrap())
        } else { None };
        if let Some(source) = &source {
            inspect_cleanup_domain(fixture, lowerer, source);
        }
        let proof = lowerer.nullable_proofs[&checkpoint.root];
        match fault {
            Some(Fault::Source) => lowerer.source_token = other_source,
            Some(Fault::MissingProof) => { lowerer.nullable_proofs.remove(&checkpoint.root); }
            Some(Fault::MissingBinding) => { lowerer.bindings.remove(&checkpoint.root); }
            Some(Fault::HeaderBinding) => { lowerer.bindings.insert(checkpoint.root, LoweredValue::Value(checkpoint.header_owner)); }
            Some(Fault::Inner) => { lowerer.nullable_proofs.get_mut(&checkpoint.root).unwrap().inner = checkpoint.nullable; }
            Some(Fault::Exclusive) => { lowerer.function.loans[checkpoint.loan.index()].ty = EntityType::Loan { kind: LoanKind::Exclusive, target: checkpoint.inner }; }
            Some(Fault::Definition) => { lowerer.function.loans[checkpoint.loan.index()].definition = lowerer.function.entity(EntityId::Value(checkpoint.owner)).unwrap().definition; }
            Some(Fault::Alias) => { lowerer.bindings.insert(checkpoint.alias, LoweredValue::Value(checkpoint.owner)); }
            Some(Fault::Temporary) => { lowerer.temporaries.insert(checkpoint.use_id, checkpoint.owner); }
            Some(Fault::Borrow) => { lowerer.borrow_bindings.insert(checkpoint.root, checkpoint.loan); }
            Some(Fault::Pending) => { lowerer.pending_operands.push(EntityId::Value(checkpoint.owner)); }
            Some(Fault::Frame) => lowerer.pending_call_frames.push(PendingCallFrame {
                nullable_children: Vec::new(), call: checkpoint.use_id, receiver: true,
                loan_arguments: Vec::new(), loop_depth: 0, pending_start: 0, created_loans: Vec::new(),
                abi_slots: Vec::new(), field_replace_owner: None, shared_field_roots: Vec::new(), exclusive_root_owners: Vec::new(),
            }),
            Some(Fault::ResultLoan) => { lowerer.result_source_loans.insert(checkpoint.root, vec![checkpoint.loan]); }
            Some(Fault::ReplayTake) => {
                let instruction = lowerer.function.instruction(checkpoint.use_instruction).unwrap();
                assert_eq!(instruction.operation, Operation::NullableTake { owner: checkpoint.owner, proof: checkpoint.loan });
                // Deliberately restore one exact old Take into the live prefix.
                lowerer.function.blocks[checkpoint.body.index()].instructions.push(checkpoint.use_instruction);
            }
            Some(Fault::NullArgument | Fault::NonNullArgument | Fault::NonNullTarget | Fault::BranchOrigin) => {
                let branch = lowerer.function.blocks[checkpoint.header.index()].terminator.as_mut().unwrap();
                if matches!(fault, Some(Fault::BranchOrigin)) {
                    branch.origin = Origin::Source(checkpoint.use_span);
                } else {
                    let TerminatorKind::NullableBranch { when_null, when_non_null, .. } = &mut branch.kind else { unreachable!() };
                    match fault {
                        Some(Fault::NullArgument) => when_null.arguments[0] = EntityId::Value(checkpoint.owner),
                        Some(Fault::NonNullArgument) => when_non_null.arguments[0] = EntityId::Value(checkpoint.owner),
                        Some(Fault::NonNullTarget) => when_non_null.target = checkpoint.null,
                        _ => unreachable!(),
                    }
                }
            }
            _ => {}
        }
        let before = (lowerer.function.clone(), state_text(lowerer));
        let result = match target {
            Target::Qualification => lowerer.qualify_nullable_map_consume(checkpoint.use_id, checkpoint.use_span).map(|source| {
                let source = source.expect("ordinary qualified source");
                assert_eq!(source.root, checkpoint.root);
                assert_eq!(source.storage, checkpoint.storage);
                assert_eq!(source.nullable, checkpoint.nullable);
                assert_eq!(source.inner, checkpoint.inner);
                assert!(!source.operands.is_empty());
            }),
            Target::View => lowerer.validate_nullable_map_consume_view(source.as_ref().unwrap(), proof, witness.unwrap(), checkpoint.use_span),
            Target::Cleanup => lowerer.preflight_nullable_map_consume_cleanup(source.as_ref().unwrap(), checkpoint.owner, checkpoint.use_span),
            Target::Entry => lowerer.lower_nullable_map_consume(checkpoint.use_id, checkpoint.use_span).map(|value| {
                assert!(matches!(value, Some(LoweredValue::Value(_))));
            }),
        };
        if let Some(fault) = fault {
            let kind = if matches!(target, Target::Qualification | Target::Cleanup) {
                LoweringErrorKind::MissingFact
            } else {
                match fault {
                    Fault::MissingProof | Fault::Alias | Fault::Temporary | Fault::Borrow | Fault::Pending
                    | Fault::Frame | Fault::ResultLoan | Fault::ReplayTake | Fault::NullArgument
                    | Fault::NonNullArgument | Fault::NonNullTarget | Fault::BranchOrigin => LoweringErrorKind::UnsupportedNode,
                    _ => LoweringErrorKind::MissingFact,
                }
            };
            fixture.rejected(result, kind, checkpoint.use_span, &format!("{target:?}/{fault:?}"));
            unchanged(lowerer, &before);
        } else {
            result.expect("same-target clean baseline");
            if matches!(target, Target::Entry) {
                let emitted = &lowerer.function.instructions[before.0.instructions.len()..];
                assert_eq!(emitted.first().unwrap().operation, Operation::NullableTake { owner: checkpoint.owner, proof: checkpoint.loan });
                assert_eq!(emitted.iter().filter(|instruction| matches!(instruction.operation, Operation::NullableTake { .. })).count(), 1);
                assert_eq!(emitted.iter().filter(|instruction| matches!(instruction.operation, Operation::MapConsume { .. })).count(), 1);
                assert!(!emitted.iter().any(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == checkpoint.loan)));
                assert!(!emitted.iter().any(|instruction| matches!(instruction.operation, Operation::Drop { owner } if owner == checkpoint.owner)));
                assert!(!lowerer.nullable_proofs.contains_key(&checkpoint.root));
                assert!(!lowerer.bindings.contains_key(&checkpoint.root));
            } else {
                unchanged(lowerer, &before);
            }
        }
    });
}

/// Enumerate exactly the operand and CallReturn points replayed by cleanup.rs.
/// These frozen sources contain no cleanup facts at those points. Runtime-state
/// alias negatives above therefore do not claim nonempty-fact rejection coverage.
fn inspect_cleanup_domain(
    fixture: &Fixture,
    lowerer: &UnitExpressionLowerer<'_>,
    source: &NullableMapConsumeSource,
) {
    assert!(!source.operands.is_empty());
    for (index, operand) in source.operands.iter().enumerate() {
        assert_eq!(operand.source_unit(), lowerer.source_unit);
        let node = lowerer
            .parsed
            .ast()
            .expressions()
            .get(operand.expression())
            .unwrap();
        match node.payload() {
            Expression::Name => assert_eq!(index, 0, "leaf first"),
            Expression::Group { expression } => {
                assert!(index > 0);
                assert_eq!(*expression, source.operands[index - 1].expression());
            }
            _ => panic!("only the actual Name/Group receiver chain"),
        }
    }
    assert_eq!(source.operands.last(), Some(&source.descriptor.receiver()));
    let points = source
        .operands
        .iter()
        .copied()
        .map(UnitDropPoint::AfterExpression)
        .chain(std::iter::once(UnitDropPoint::CallReturn(
            source.descriptor.expression(),
        )))
        .collect::<Vec<_>>();
    let counts = points
        .iter()
        .map(|point| {
            let drops = lowerer
                .owned
                .drops()
                .iter()
                .filter(|fact| fact.point() == *point)
                .count();
            let borrowed_ends = lowerer
                .owned
                .borrow_results()
                .ends()
                .iter()
                .filter(|fact| fact.point() == *point)
                .count();
            let conditional = lowerer
                .owned
                .conditional_receiver_drops()
                .iter()
                .filter(|fact| fact.point() == *point)
                .count();
            let iteration = lowerer
                .owned
                .iteration_cleanup_at(*point)
                .map_or(0, <[_]>::len);
            (*point, drops, borrowed_ends, conditional, iteration)
        })
        .collect::<Vec<_>>();
    eprintln!(
        "unit {} actual Name/Group/CallReturn cleanup counts: {:?}; nonempty-fact rejection/order remains uncovered",
        fixture.id, counts
    );
    assert!(
        counts
            .iter()
            .all(|(_, drops, borrowed, conditional, iteration)| *drops == 0
                && *borrowed == 0
                && *conditional == 0
                && *iteration == 0)
    );
}
