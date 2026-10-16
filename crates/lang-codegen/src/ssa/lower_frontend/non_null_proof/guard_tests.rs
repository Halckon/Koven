//! CI-only private guards start with the ordinary pending-call source and real SSA.
use super::*;
use crate::ssa::{
    lower_frontend::cleanup_guard_harness::Fixture,
    model::{Function, TerminatorKind},
    while_flow_tests::proof_cases::pending_child_source,
};
use lang_frontend::parser::Statement;

fn missing<T>(result: Result<T, LoweringError>, span: Span) {
    let Err(failure) = result else {
        panic!("guard accepted corrupted proof")
    };
    assert_eq!(failure.kind, LoweringErrorKind::MissingFact);
    assert_eq!(failure.span, Some(span));
}

fn proof_at(function: &Function, block: super::super::BlockId, seed: NonNullProof) -> NonNullProof {
    let parameters = &function.block(block).unwrap().parameters;
    let owner_type = function.entity(EntityId::Value(seed.owner)).unwrap().ty;
    let loan_type = function.entity(EntityId::Loan(seed.loan)).unwrap().ty;
    let owners = parameters
        .iter()
        .filter(|entity| function.entity(**entity).unwrap().ty == owner_type)
        .copied()
        .collect::<Vec<_>>();
    let loans = parameters
        .iter()
        .filter(|entity| function.entity(**entity).unwrap().ty == loan_type)
        .copied()
        .collect::<Vec<_>>();
    let [EntityId::Value(owner)] = owners.as_slice() else {
        panic!("one current owner column")
    };
    let [EntityId::Loan(loan)] = loans.as_slice() else {
        panic!("one current proof column")
    };
    NonNullProof {
        owner: *owner,
        loan: *loan,
        ..seed
    }
}

#[test]
fn single_non_null_proof_guards_preserve_binding_and_merge_origin() {
    let fixture = Fixture::analyze(&pending_child_source());
    let baseline = fixture.entry();
    let root = fixture.symbol("current");
    let whiles = fixture
        .parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Statement::While { .. }).then_some((id, node.span()))
        })
        .collect::<Vec<_>>();
    assert_eq!(whiles.len(), 2);
    let header = baseline
        .blocks
        .iter()
        .find(|block| {
            matches!(
                block.terminator.as_ref().map(|term| &term.kind),
                Some(TerminatorKind::NullableBranch { .. })
            )
        })
        .unwrap();
    let term = header.terminator.as_ref().unwrap();
    let (scope, span) = *whiles
        .iter()
        .find(|(_, span)| term.origin == super::super::Origin::Source(*span))
        .unwrap();
    let TerminatorKind::NullableBranch {
        owner,
        view,
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
    let EntityId::Value(body_owner) =
        baseline.block(when_non_null.target).unwrap().parameters[column]
    else {
        unreachable!()
    };
    let EntityType::Loan { target: inner, .. } = baseline.entity(EntityId::Loan(*view)).unwrap().ty
    else {
        unreachable!()
    };
    let seed = NonNullProof::introduced(*view, body_owner, inner, ProofScope::While(scope));
    let call = baseline
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
        .unwrap();
    let current = proof_at(baseline, call.block, seed);
    assert_ne!(current.owner, seed.owner);
    assert_ne!(current.loan, seed.loan);
    let incoming = baseline
        .blocks
        .iter()
        .filter_map(
            |block| match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::Branch(edge)) if edge.target == call.block => {
                    Some((block.id, edge.clone()))
                }
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    assert_eq!(
        incoming.len(),
        2,
        "the real argument-if has two merge predecessors"
    );

    let mut function = baseline.clone();
    fixture.with_lowerer(&mut function, |lowerer| {
        lowerer.block = call.block;
        lowerer
            .bindings
            .insert(root, LoweredValue::Value(current.owner));
        lowerer.non_null_bindings.insert(root, current);
        assert_eq!(
            lowerer.checked_non_null_proof(root, inner, span).unwrap(),
            Some(current)
        );
        let exits = incoming
            .iter()
            .map(|(block, edge)| {
                let parameters = &lowerer.function.block(call.block).unwrap().parameters;
                let operand = |entity| {
                    edge.arguments[parameters
                        .iter()
                        .position(|parameter| *parameter == entity)
                        .unwrap()]
                };
                let EntityId::Value(owner) = operand(EntityId::Value(current.owner)) else {
                    unreachable!()
                };
                let EntityId::Loan(loan) = operand(EntityId::Loan(current.loan)) else {
                    unreachable!()
                };
                let mut exit = lowerer.branch_exit(LoweredValue::Unit);
                exit.block = *block;
                exit.bindings.insert(root, LoweredValue::Value(owner));
                exit.views.insert(
                    root,
                    NonNullProof {
                        owner,
                        loan,
                        ..seed
                    },
                );
                exit
            })
            .collect::<Vec<_>>();
        lowerer.validate_non_null_exit_proofs(&exits, span).unwrap();
        let merged = lowerer
            .merged_non_null_proofs(
                &lowerer.bindings,
                vec![(root, current.loan, exits[0].views[&root])],
                span,
            )
            .unwrap();
        assert_eq!(merged[&root], current);
    });

    for mutation in 0..5 {
        let mut function = baseline.clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            let mut bindings = BTreeMap::from([(root, LoweredValue::Value(current.owner))]);
            let mut proof = current;
            match mutation {
                0 => {
                    bindings.insert(root, LoweredValue::Value(seed.owner));
                }
                1 => {
                    bindings.remove(&root);
                }
                2 => {
                    proof.owner = seed.owner;
                }
                3 => {
                    proof.inner = lowerer
                        .function
                        .entity(EntityId::Value(current.owner))
                        .unwrap()
                        .ty
                        .semantic_type();
                }
                4 => {
                    proof.loan = lowerer.function.blocks[0]
                        .parameters
                        .iter()
                        .find_map(|entity| match entity {
                            EntityId::Loan(loan) if *loan != current.loan => Some(*loan),
                            _ => None,
                        })
                        .unwrap();
                }
                _ => unreachable!(),
            }
            missing(
                lowerer.validate_non_null_proof(root, proof, &bindings, span),
                span,
            );
        });
    }
    for change_birth in [false, true] {
        let mut function = baseline.clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            lowerer.block = call.block;
            lowerer
                .bindings
                .insert(root, LoweredValue::Value(current.owner));
            lowerer.non_null_bindings.insert(root, current);
            let first = lowerer.branch_exit(LoweredValue::Unit);
            let mut second = lowerer.branch_exit(LoweredValue::Unit);
            let changed = second.views.get_mut(&root).unwrap();
            if change_birth {
                changed.origin.birth = current.loan;
            } else {
                changed.origin.introduced_by =
                    ProofScope::While(whiles.iter().find(|(id, _)| *id != scope).unwrap().0);
            }
            // Basic binding/type validation deliberately does not claim birth validation.
            lowerer
                .validate_non_null_proof(root, *changed, &second.bindings, span)
                .unwrap();
            missing(
                lowerer.validate_non_null_exit_proofs(&[first, second], span),
                span,
            );
        });
    }
}
