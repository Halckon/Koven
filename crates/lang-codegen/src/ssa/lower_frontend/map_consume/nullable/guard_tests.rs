//! CI-only qualification/live/branch checkpoints from the unchanged C1/C2/C3 sources.
use super::*;
use crate::ssa::lower_frontend::orchestrate::nullable_map_guard_support::{
    Fixture, Point, Snapshot,
};
use lang_frontend::ownership_checking::{IterationCleanupAction, LoanEndPoint};

#[derive(Clone, Copy, Debug)]
enum Target {
    Qualification,
    Live,
    Branch,
}

#[derive(Clone, Copy, Debug)]
enum Mutation {
    NoSource,
    WrongSource,
    MissingFunction,
    MissingType,
    MissingRegistration,
    WrongRegistration,
    MissingProof,
    MissingBinding,
    HeaderBinding,
    WrongLoanType,
    WrongLoanDefinition,
    NamedAlias,
    Temporary,
    Borrow,
    Pending,
    ReplayedTake,
    WrongBirthOrigin,
    WrongNonNullTarget,
    WrongNullArgument,
    WrongOwnerColumn,
}

impl Mutation {
    fn target(self) -> Target {
        match self {
            Self::NoSource
            | Self::WrongSource
            | Self::MissingFunction
            | Self::MissingType
            | Self::MissingRegistration
            | Self::WrongRegistration
            | Self::MissingProof => Target::Qualification,
            Self::WrongLoanDefinition
            | Self::WrongBirthOrigin
            | Self::WrongNonNullTarget
            | Self::WrongNullArgument
            | Self::WrongOwnerColumn
            | Self::ReplayedTake => Target::Branch,
            _ => Target::Live,
        }
    }
    fn kind(self) -> LoweringErrorKind {
        match self {
            Self::NoSource
            | Self::MissingProof
            | Self::NamedAlias
            | Self::Temporary
            | Self::Borrow
            | Self::Pending
            | Self::ReplayedTake => LoweringErrorKind::UnsupportedNode,
            _ => LoweringErrorKind::MissingFact,
        }
    }
}

fn target(
    lowerer: &ExpressionLowerer<'_>,
    point: Point,
    target: Target,
) -> Result<(), LoweringError> {
    let descriptor = *lowerer.typed.map_consume(point.expression).unwrap();
    let source = lowerer.nullable_consume_source(descriptor, point.proof.inner, point.span)?;
    match target {
        Target::Qualification => Ok(()),
        Target::Live => lowerer.validate_nullable_consume_live(&source, point.proof, point.span),
        Target::Branch => {
            lowerer.validate_nullable_consume_branch(&source, point.proof, point.span)
        }
    }
}

fn direct_baseline(fixture: &Fixture, which: Target) {
    let mut state = fixture.consume_checkpoint();
    let point = state.point;
    fixture.with_lowerer(&mut state, |lowerer| {
        let before = Snapshot::take(lowerer);
        target(lowerer, point, which).unwrap();
        assert_eq!(Snapshot::take(lowerer), before);
    });
}

fn reject(fixture: &Fixture, mutation: Mutation) {
    direct_baseline(fixture, mutation.target());
    entry_baseline(fixture);
    let mut state = fixture.consume_checkpoint();
    let point = state.point;
    match mutation {
        Mutation::NoSource => state.source = None,
        Mutation::WrongSource => {
            state.source = Some(
                fixture
                    .other_source(state.source.unwrap())
                    .expect("C3 has another actual planned callable"),
            )
        }
        Mutation::MissingFunction => state.function_ids.retain(|_, id| *id != state.function.id),
        Mutation::MissingType => {
            state.type_ids.remove(&point.storage);
        }
        Mutation::MissingRegistration => {
            state.nullable_maps.remove(&point.nullable);
        }
        Mutation::WrongRegistration => {
            let result = fixture
                .typed
                .map_consume(point.expression)
                .unwrap()
                .result_type();
            state
                .nullable_maps
                .insert(point.nullable, state.type_ids[&result]);
        }
        _ => {}
    }
    let alias = fixture.symbol(match fixture.id {
        "C1" => "result",
        "C2" => "seed",
        "C3" => "input",
        _ => unreachable!(),
    });
    fixture.with_lowerer(&mut state, |lowerer| {
        match mutation {
            Mutation::MissingProof => {
                lowerer.non_null_bindings.remove(&point.root);
            }
            Mutation::MissingBinding => {
                lowerer.bindings.remove(&point.root);
            }
            Mutation::HeaderBinding => {
                lowerer
                    .bindings
                    .insert(point.root, LoweredValue::Value(point.header_owner));
            }
            Mutation::WrongLoanType => {
                lowerer.function.loans[point.proof.loan.index()].ty = EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: point.proof.inner,
                };
            }
            Mutation::WrongLoanDefinition => {
                let definition = lowerer
                    .function
                    .entity(EntityId::Value(point.proof.owner))
                    .unwrap()
                    .definition;
                lowerer.function.loans[point.proof.loan.index()].definition = definition;
            }
            Mutation::NamedAlias => {
                lowerer
                    .bindings
                    .insert(alias, LoweredValue::Value(point.proof.owner));
            }
            Mutation::Temporary => {
                lowerer
                    .temporaries
                    .insert(point.expression.index(), point.proof.owner);
            }
            Mutation::Borrow => {
                lowerer.borrow_bindings.insert(point.root, point.proof.loan);
            }
            Mutation::Pending => {
                let receiver = lowerer
                    .typed
                    .map_consume(point.expression)
                    .unwrap()
                    .receiver();
                lowerer
                    .pending_call_loans
                    .insert((point.expression.index(), receiver.index()), None);
            }
            Mutation::ReplayedTake => {
                lowerer.function.blocks[point.body.index()]
                    .instructions
                    .push(point.instruction);
            }
            Mutation::WrongBirthOrigin => {
                lowerer.function.blocks[point.header.index()]
                    .terminator
                    .as_mut()
                    .unwrap()
                    .origin = Origin::Source(point.span);
            }
            Mutation::WrongNonNullTarget => {
                let TerminatorKind::NullableBranch { when_non_null, .. } =
                    &mut lowerer.function.blocks[point.header.index()]
                        .terminator
                        .as_mut()
                        .unwrap()
                        .kind
                else {
                    unreachable!()
                };
                when_non_null.target = point.null_block;
            }
            Mutation::WrongNullArgument => {
                let TerminatorKind::NullableBranch { when_null, .. } = &mut lowerer.function.blocks
                    [point.header.index()]
                .terminator
                .as_mut()
                .unwrap()
                .kind
                else {
                    unreachable!()
                };
                when_null.arguments[0] = EntityId::Value(point.proof.owner);
            }
            Mutation::WrongOwnerColumn => {
                let definition = lowerer
                    .function
                    .entity(EntityId::Loan(point.proof.loan))
                    .unwrap()
                    .definition;
                lowerer.function.values[point.proof.owner.index()].definition = definition;
            }
            _ => {}
        }
        let before = Snapshot::take(lowerer);
        fixture.reject(
            target(lowerer, point, mutation.target()),
            mutation.kind(),
            point.span,
        );
        assert_eq!(
            Snapshot::take(lowerer),
            before,
            "{mutation:?}: direct guard is read-only"
        );
        fixture.reject(
            lowerer.lower_map_consume(point.expression),
            mutation.kind(),
            point.span,
        );
        assert_eq!(
            Snapshot::take(lowerer),
            before,
            "{mutation:?}: no new Take/Consume/End/Drop or partial state"
        );
    });
}

fn cleanup_and_entry_baselines(fixture: &Fixture) {
    let mut state = fixture.consume_checkpoint();
    let point = state.point;
    fixture.with_lowerer(&mut state, |lowerer| {
        let descriptor = *lowerer.typed.map_consume(point.expression).unwrap();
        let source = lowerer
            .nullable_consume_source(descriptor, point.proof.inner, point.span)
            .unwrap();
        let mut expected = Vec::new();
        let mut operand = descriptor.receiver();
        loop {
            expected.push(operand);
            match lowerer
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .unwrap()
                .payload()
            {
                Expression::Group { expression } => operand = *expression,
                Expression::Name => break,
                _ => panic!("frozen Name/Group receiver"),
            }
        }
        expected.reverse();
        assert_eq!(
            source.operands, expected,
            "actual leaf-to-outer cleanup order"
        );
        assert_eq!(expected.len(), if fixture.id == "C2" { 3 } else { 1 });
        let points = expected
            .iter()
            .copied()
            .map(DropPoint::AfterExpression)
            .chain(std::iter::once(DropPoint::CallReturn(point.expression)))
            .collect::<Vec<_>>();
        for at in points {
            let facts = lowerer
                .owned
                .drops()
                .iter()
                .copied()
                .filter(|fact| fact.point() == at)
                .collect::<Vec<_>>();
            let steps = lowerer
                .owned
                .cleanup_steps()
                .iter()
                .filter(|(point, _)| *point == at)
                .map(|(_, action)| match action {
                    IterationCleanupAction::Drop(fact) => *fact,
                    _ => panic!("frozen source has no other cleanup action at these points"),
                })
                .collect::<Vec<_>>();
            assert_eq!(steps, facts, "actual per-boundary cleanup order");
            // Deliberately empty in these frozen sources. This is not nonempty-fact fault coverage.
            assert!(
                facts.is_empty(),
                "new cleanup facts require a separately reviewed checkpoint"
            );
            assert!(
                !lowerer
                    .owned
                    .borrow_results()
                    .ends()
                    .iter()
                    .any(|fact| fact.point() == at)
            );
        }
        assert!(
            !lowerer.owned.loans().iter().any(
                |loan| loan.call() == source.call || source.operands.contains(&loan.argument())
            )
        );
        assert!(
            !lowerer
                .owned
                .loan_ends()
                .iter()
                .any(|end| end.point() == LoanEndPoint::CallReturn(source.call))
        );
        let before = Snapshot::take(lowerer);
        lowerer
            .preflight_nullable_consume_cleanup(&source, point.proof.owner, point.span)
            .unwrap();
        assert_eq!(Snapshot::take(lowerer), before);
    });
}

fn entry_baseline(fixture: &Fixture) {
    // Every mutating entry refusal has its own independent fresh successful entry.
    let mut state = fixture.consume_checkpoint();
    let point = state.point;
    fixture.with_lowerer(&mut state, |lowerer| {
        let start = lowerer.function.instructions.len();
        let result = lowerer.lower_map_consume(point.expression).unwrap();
        assert!(matches!(result, LoweredValue::Value(_)));
        assert_eq!(lowerer.function.instructions.len(), start + 2);
        assert_eq!(
            lowerer.function.instructions[start].operation,
            Operation::NullableTake {
                owner: point.proof.owner,
                proof: point.proof.loan
            }
        );
        assert!(matches!(
            lowerer.function.instructions[start + 1].operation,
            Operation::MapConsume { .. }
        ));
        assert!(!lowerer.bindings.contains_key(&point.root));
        assert!(!lowerer.non_null_bindings.contains_key(&point.root));
    });
}

#[test]
fn single_nullable_map_consume_guards() {
    for id in ["C1", "C2", "C3"] {
        let fixture = Fixture::analyze(id);
        cleanup_and_entry_baselines(&fixture);
        for mutation in [
            Mutation::NoSource,
            Mutation::MissingFunction,
            Mutation::MissingType,
            Mutation::MissingRegistration,
            Mutation::WrongRegistration,
            Mutation::MissingProof,
            Mutation::MissingBinding,
            Mutation::HeaderBinding,
            Mutation::WrongLoanType,
            Mutation::WrongLoanDefinition,
            Mutation::NamedAlias,
            Mutation::Temporary,
            Mutation::Borrow,
            Mutation::Pending,
            Mutation::ReplayedTake,
            Mutation::WrongBirthOrigin,
            Mutation::WrongNonNullTarget,
            Mutation::WrongNullArgument,
            Mutation::WrongOwnerColumn,
        ] {
            reject(&fixture, mutation);
        }
        if id == "C3" {
            reject(&fixture, Mutation::WrongSource);
        }
    }
    // No real While/When/assertion scope exists in these inputs: do not invent one.
    // FE private descriptor/formation/cleanup mutation remains explicitly outside this exact.
}
