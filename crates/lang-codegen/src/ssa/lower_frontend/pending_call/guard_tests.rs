//! CI-only carrier corruption after the real frontend and field producer succeed.
use super::*;
use crate::ssa::{
    lower_frontend::{
        cleanup_guard_harness::{Fixture, incoming, parameter},
        non_null_proof::ProofScope,
    },
    model::TerminatorKind,
    while_flow_tests::proof_cases::pending_child_source,
};

fn rejected<T>(result: Result<T, LoweringError>, kind: LoweringErrorKind, span: Span) {
    let Err(failure) = result else {
        panic!("pending child guard accepted corruption")
    };
    assert_eq!(failure.kind, kind);
    assert_eq!(failure.span, Some(span));
}

/// Query actual edges as a simultaneous triple, never choose provenance by type.
fn assert_real_triple_transport(
    function: &Function,
    block: BlockId,
    current: [EntityId; 3],
    birth_block: BlockId,
    birth: [EntityId; 3],
) {
    use std::collections::BTreeSet;
    let anchor = (birth_block, birth);
    assert_eq!(
        incoming(function, block).len(),
        2,
        "both actual argument-if predecessors"
    );
    let mut graph = BTreeMap::new();
    let mut pending = vec![(block, current)];
    while let Some(state @ (block, entities)) = pending.pop() {
        if graph.contains_key(&state) {
            continue;
        }
        for (role, entity) in entities.iter().enumerate() {
            assert!(!entities[..role].contains(entity));
            assert_eq!(
                function.entity(*entity).unwrap().ty,
                function.entity(birth[role]).unwrap().ty
            );
        }
        if state == anchor {
            graph.insert(state, Vec::new());
            continue;
        }
        let columns = entities.map(|entity| {
            let (defined, column) = parameter(function, entity);
            assert_eq!(
                defined, block,
                "all three roles belong to the same current block"
            );
            column
        });
        let predecessors = incoming(function, block);
        assert!(
            !predecessors.is_empty(),
            "only the actual producer can terminate the trace"
        );
        let mut parents = Vec::new();
        for (parent, edge) in predecessors {
            assert!(matches!(
                function
                    .block(parent)
                    .unwrap()
                    .terminator
                    .as_ref()
                    .unwrap()
                    .kind,
                TerminatorKind::Branch(_) | TerminatorKind::Conditional { .. }
            ));
            let entities = columns.map(|column| edge.arguments[column]);
            for entity in entities {
                assert_eq!(
                    edge.arguments
                        .iter()
                        .filter(|argument| **argument == entity)
                        .count(),
                    1
                );
            }
            parents.push((parent, entities));
            pending.push((parent, entities));
        }
        graph.insert(state, parents);
    }
    assert!(
        graph.contains_key(&anchor),
        "same real owner/parent/field-child birth"
    );
    let mut anchored = BTreeSet::from([anchor]);
    loop {
        let before = anchored.len();
        for (state, parents) in &graph {
            if parents.iter().any(|parent| anchored.contains(parent)) {
                anchored.insert(*state);
            }
        }
        if anchored.len() == before {
            break;
        }
    }
    assert_eq!(
        anchored.len(),
        graph.len(),
        "every loop-carried triple reaches the real birth"
    );
}

struct State {
    root: SymbolId,
    scope: StatementId,
    birth: NonNullProof,
    proof: NonNullProof,
    born_child: LoanId,
    child: LoanId,
    field: usize,
    target: SsaTypeId,
    call: ExpressionId,
    argument: ExpressionId,
    block: BlockId,
    span: Span,
}

impl State {
    fn read(fixture: &Fixture) -> Self {
        let function = fixture.entry();
        let root = fixture.symbol("current");
        let projection = function
            .instructions
            .iter()
            .find(|instruction| {
                matches!(instruction.operation, Operation::SharedHeapFieldLoan { .. })
            })
            .unwrap();
        let Operation::SharedHeapFieldLoan { base, field } = projection.operation else {
            unreachable!()
        };
        let [EntityId::Loan(born_child)] = projection.results.as_slice() else {
            unreachable!()
        };
        let EntityType::Loan { target, .. } =
            function.entity(EntityId::Loan(*born_child)).unwrap().ty
        else {
            unreachable!()
        };
        let header = function.blocks.iter().find(|block| matches!(block.terminator.as_ref().map(|term| &term.kind), Some(TerminatorKind::NullableBranch { view, .. }) if *view == base)).unwrap();
        let term = header.terminator.as_ref().unwrap();
        let TerminatorKind::NullableBranch {
            owner,
            when_non_null,
            ..
        } = &term.kind
        else {
            unreachable!()
        };
        let column = when_non_null
            .arguments
            .iter()
            .position(|entity| *entity == EntityId::Value(*owner))
            .unwrap();
        let EntityId::Value(owner) =
            function.block(when_non_null.target).unwrap().parameters[column]
        else {
            unreachable!()
        };
        assert_eq!(projection.block, when_non_null.target);
        let scope = fixture
            .parsed
            .ast()
            .statements()
            .iter()
            .find_map(|(id, node)| {
                (term.origin == Origin::Source(node.span())
                    && matches!(node.payload(), Statement::While { .. }))
                .then_some(id)
            })
            .unwrap();
        let EntityType::Loan { target: inner, .. } =
            function.entity(EntityId::Loan(base)).unwrap().ty
        else {
            unreachable!()
        };
        let birth = NonNullProof::introduced(base, owner, inner, ProofScope::While(scope));
        let call = function
            .instructions
            .iter()
            .find(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
            .unwrap();
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        let EntityId::Loan(child) = arguments[0] else {
            unreachable!()
        };
        let parameters = &function.block(call.block).unwrap().parameters;
        let matching = |ty| {
            let candidates = parameters
                .iter()
                .copied()
                .filter(|entity| function.entity(*entity).unwrap().ty == ty)
                .collect::<Vec<_>>();
            let [entity] = candidates.as_slice() else {
                panic!("exactly one current role column")
            };
            *entity
        };
        let EntityId::Value(owner) =
            matching(function.entity(EntityId::Value(birth.owner)).unwrap().ty)
        else {
            unreachable!()
        };
        let EntityId::Loan(loan) =
            matching(function.entity(EntityId::Loan(birth.loan)).unwrap().ty)
        else {
            unreachable!()
        };
        assert_real_triple_transport(
            function,
            call.block,
            [
                EntityId::Value(owner),
                EntityId::Loan(loan),
                EntityId::Loan(child),
            ],
            projection.block,
            [
                EntityId::Value(birth.owner),
                EntityId::Loan(birth.loan),
                EntityId::Loan(*born_child),
            ],
        );
        assert_ne!(child, *born_child);
        assert_ne!(loan, birth.loan);
        assert_ne!(owner, birth.owner);
        let argument = fixture
            .parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (Origin::Source(node.span()) == projection.origin).then_some(id))
            .unwrap();
        let fact = fixture.owned.loan_begin(argument).unwrap();
        let span = projection.origin.span();
        Self {
            root,
            scope,
            birth,
            proof: NonNullProof {
                owner,
                loan,
                ..birth
            },
            born_child: *born_child,
            child,
            field,
            target,
            call: fact.call(),
            argument,
            block: call.block,
            span,
        }
    }

    fn seed(&self, fixture: &Fixture, lowerer: &mut ExpressionLowerer<'_>) -> PendingFieldChild {
        lowerer
            .bindings
            .insert(self.root, LoweredValue::Value(self.birth.owner));
        lowerer.non_null_bindings.insert(self.root, self.birth);
        let fact = fixture.owned.loan_begin(self.argument).unwrap();
        lowerer
            .register_pending_field_child(
                fact,
                self.birth,
                self.field,
                self.target,
                self.born_child,
                self.span,
            )
            .unwrap();
        let key = self.key();
        let child = lowerer.pending_call_loans.child(&key).unwrap();
        lowerer.block = self.block;
        lowerer
            .bindings
            .insert(self.root, LoweredValue::Value(self.proof.owner));
        lowerer.non_null_bindings.insert(self.root, self.proof);
        lowerer
            .pending_call_loans
            .rebind(key, self.child, Some(child));
        lowerer
            .validate_pending_field_children(
                &lowerer.bindings,
                &lowerer.non_null_bindings,
                &lowerer.pending_call_loans,
                self.span,
            )
            .unwrap();
        child
    }

    fn key(&self) -> Key {
        (self.call.index(), self.argument.index())
    }
}

#[test]
fn single_pending_child_guards_require_exact_identity_and_three_columns() {
    let fixture = Fixture::analyze(&pending_child_source());
    let state = State::read(&fixture);
    let span = state.span;
    let missing = LoweringErrorKind::MissingFact;
    let mut function = fixture.entry().clone();
    fixture.with_lowerer(&mut function, |lowerer| {
        state.seed(&fixture, lowerer);
        lowerer
            .ensure_readonly_while_field_carriers(state.root, span)
            .unwrap();
        lowerer
            .insert_pending_source_argument(state.key(), Some(state.child), span)
            .unwrap();
        let carried = lowerer
            .linear_binding_slots(&lowerer.bindings, span)
            .unwrap();
        let exit = lowerer.branch_exit(LoweredValue::Unit);
        let arguments = carried
            .slots
            .iter()
            .map(|slot| slot.source)
            .collect::<Vec<_>>();
        lowerer
            .validate_pending_field_edge(&carried, &exit, &arguments, span)
            .unwrap();
        let columns = arguments
            .iter()
            .map(|entity| vec![*entity])
            .collect::<Vec<_>>();
        lowerer
            .validate_pending_field_merge_columns(&[exit], &columns, span)
            .unwrap();
        let bindings = lowerer.bindings.clone();
        lowerer
            .restore_pending_field_columns(&bindings, &carried.slots, &arguments, span)
            .unwrap();
        let call = lowerer
            .function
            .instructions
            .iter()
            .find(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
            .unwrap();
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        lowerer
            .validate_pending_source_arguments(state.call, arguments, span)
            .unwrap();
    });

    for mutation in 0..7 {
        let mut function = fixture.entry().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            let identity = state.seed(&fixture, lowerer);
            let mut pending = lowerer.pending_call_loans.clone();
            let mut entry = pending.0.remove(&state.key()).unwrap();
            let mut key = state.key();
            match mutation {
                0 => key.0 = state.argument.index(),
                1 => key.1 = state.call.index(),
                2 => entry.child.as_mut().unwrap().root = fixture.symbol("initial"),
                3 => {
                    entry.child.as_mut().unwrap().parent = NonNullProof::introduced(
                        state.proof.loan,
                        state.proof.owner,
                        state.proof.inner,
                        ProofScope::While(state.scope),
                    )
                    .origin
                }
                4 => entry.loan = Some(state.proof.loan),
                5 => entry.child.as_mut().unwrap().field = state.field + 1,
                6 => entry.loan = None,
                _ => unreachable!(),
            }
            assert_ne!(
                (key, entry.loan, entry.child),
                (state.key(), Some(state.child), Some(identity))
            );
            pending.0.insert(key, entry);
            rejected(
                lowerer.validate_pending_field_children(
                    &lowerer.bindings,
                    &lowerer.non_null_bindings,
                    &pending,
                    span,
                ),
                missing,
                span,
            );
        });
    }

    for role in 0..3 {
        let mut function = fixture.entry().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            state.seed(&fixture, lowerer);
            let carried = lowerer
                .linear_binding_slots(&lowerer.bindings, span)
                .unwrap();
            let exit = lowerer.branch_exit(LoweredValue::Unit);
            let mut arguments = carried
                .slots
                .iter()
                .map(|slot| slot.source)
                .collect::<Vec<_>>();
            let (current, earlier) = [
                (
                    EntityId::Value(state.proof.owner),
                    EntityId::Value(state.birth.owner),
                ),
                (
                    EntityId::Loan(state.proof.loan),
                    EntityId::Loan(state.birth.loan),
                ),
                (
                    EntityId::Loan(state.child),
                    EntityId::Loan(state.born_child),
                ),
            ][role];
            assert_ne!(current, earlier);
            assert_eq!(
                lowerer.function.entity(current).unwrap().ty,
                lowerer.function.entity(earlier).unwrap().ty
            );
            let column = arguments
                .iter()
                .position(|entity| *entity == current)
                .unwrap();
            arguments[column] = earlier;
            rejected(
                lowerer.validate_pending_field_edge(&carried, &exit, &arguments, span),
                missing,
                span,
            );
            let bindings = lowerer.bindings.clone();
            rejected(
                lowerer.restore_pending_field_columns(&bindings, &carried.slots, &arguments, span),
                missing,
                span,
            );
            let columns = arguments
                .iter()
                .map(|entity| vec![*entity])
                .collect::<Vec<_>>();
            rejected(
                lowerer.validate_pending_field_merge_columns(&[exit], &columns, span),
                missing,
                span,
            );
        });
    }

    for duplicate in [false, true] {
        let mut function = fixture.entry().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            state.seed(&fixture, lowerer);
            let mut carried = lowerer
                .linear_binding_slots(&lowerer.bindings, span)
                .unwrap();
            let child = carried
                .slots
                .iter()
                .position(|slot| slot.field_child.is_some())
                .unwrap();
            if duplicate {
                carried.slots.push(carried.slots[child].clone());
            } else {
                carried.slots[child].field_child = None;
            }
            rejected(
                lowerer.validate_pending_field_loop_exit(
                    &carried,
                    &lowerer.branch_exit(LoweredValue::Unit),
                    span,
                ),
                missing,
                span,
            );
        });
    }
    for role in 0..3 {
        for duplicate in [false, true] {
            let mut function = fixture.entry().clone();
            fixture.with_lowerer(&mut function, |lowerer| {
                state.seed(&fixture, lowerer);
                let carried = lowerer
                    .linear_binding_slots(&lowerer.bindings, span)
                    .unwrap();
                let mut columns = carried
                    .slots
                    .iter()
                    .map(|slot| vec![slot.source])
                    .collect::<Vec<_>>();
                let entity = [
                    EntityId::Value(state.proof.owner),
                    EntityId::Loan(state.proof.loan),
                    EntityId::Loan(state.child),
                ][role];
                let column = columns.iter().position(|items| *items == [entity]).unwrap();
                if duplicate {
                    columns[column] = columns[(column + 1) % columns.len()].clone();
                } else {
                    columns.remove(column);
                }
                rejected(
                    lowerer.validate_pending_field_merge_columns(
                        &[lowerer.branch_exit(LoweredValue::Unit)],
                        &columns,
                        span,
                    ),
                    missing,
                    span,
                );
            });
        }
    }
    for absent in [false, true] {
        let mut function = fixture.entry().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            state.seed(&fixture, lowerer);
            rejected(
                lowerer.insert_pending_source_argument(
                    state.key(),
                    if absent { None } else { Some(state.born_child) },
                    span,
                ),
                missing,
                span,
            );
            let arguments = if absent {
                Vec::new()
            } else {
                vec![EntityId::Loan(state.born_child)]
            };
            rejected(
                lowerer.validate_pending_source_arguments(state.call, &arguments, span),
                missing,
                span,
            );
        });
    }
    let mut function = fixture.entry().clone();
    fixture.with_lowerer(&mut function, |lowerer| {
        state.seed(&fixture, lowerer);
        lowerer
            .pending_call_loans
            .insert((state.argument.index(), state.call.index()), None);
        rejected(
            lowerer.ensure_readonly_while_field_carriers(state.root, span),
            LoweringErrorKind::UnsupportedNode,
            span,
        );
    });
}
