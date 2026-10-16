//! SPEC-0330 private unit proof guards, from an already verified source program.
use super::*;
use crate::ssa::model::Definition;

pub(in crate::ssa::unit_lower) mod support;
use support::{Fixture, parameter, rebound, rejected};

pub(in crate::ssa::unit_lower) struct Birth {
    pub header: BlockId,
    pub body: BlockId,
    pub column: usize,
    pub root: UnitSymbolId,
    pub scope: UnitStatementId,
    pub inner: SsaTypeId,
    pub span: Span,
}

/// Restore the actual first NullableBranch body, before its LoopContext is pushed.
/// Named columns retain the production BTreeMap order; edge provenance is checked
/// separately by callers for the owner's original source parameter.
pub(in crate::ssa::unit_lower) fn install_birth(
    lowerer: &mut UnitExpressionLowerer<'_>,
    root: UnitSymbolId,
    live: &[UnitSymbolId],
) -> Birth {
    let (scope, span) = lowerer
        .parsed
        .ast()
        .statements()
        .iter()
        .find_map(|(id, node)| {
            let Statement::While { condition, .. } = node.payload() else {
                return None;
            };
            let (descriptor, _) = lowerer
                .direct_nullable_while_condition(*condition, node.span())
                .unwrap()?;
            (descriptor.symbol() == root)
                .then_some((UnitStatementId::new(lowerer.source_unit, id), node.span()))
        })
        .unwrap();
    let (header, owner, edge) = lowerer
        .function
        .blocks
        .iter()
        .find_map(|block| {
            let term = block.terminator.as_ref()?;
            let TerminatorKind::NullableBranch {
                owner,
                when_non_null,
                ..
            } = &term.kind
            else {
                return None;
            };
            (term.origin == Origin::Source(span)).then_some((
                block.id,
                *owner,
                when_non_null.clone(),
            ))
        })
        .unwrap();
    let column = edge
        .arguments
        .iter()
        .position(|entity| *entity == EntityId::Value(owner))
        .unwrap();
    let owner_type = lowerer
        .function
        .entity(EntityId::Value(owner))
        .unwrap()
        .ty
        .semantic_type();
    let SsaTypeKind::NullableHandle { inner } = lowerer.ssa_types[owner_type.index()] else {
        panic!("real nullable branch owner")
    };
    let mut live = live.to_vec();
    live.sort();
    let parameters = lowerer
        .function
        .block(edge.target)
        .unwrap()
        .parameters
        .clone();
    assert_eq!(live[column], root);
    lowerer.bindings = live
        .into_iter()
        .zip(&parameters)
        .map(|(symbol, entity)| {
            let EntityId::Value(value) = entity else {
                panic!("named value prefix")
            };
            assert_eq!(
                lowerer.function.entity(*entity).unwrap().ty,
                EntityType::Value(lowerer.type_ids[&lowerer.typed.symbol_type(symbol).unwrap()])
            );
            (symbol, LoweredValue::Value(*value))
        })
        .collect();
    lowerer.borrow_bindings = lowerer
        .borrow_bindings
        .iter()
        .map(|(root, loan)| {
            let EntityId::Loan(current) =
                rebound(lowerer.function, EntityId::Loan(*loan), edge.target)
            else {
                panic!("borrow parameter remains a loan")
            };
            (*root, current)
        })
        .collect();
    lowerer.block = edge.target;
    lowerer
        .introduce_nullable_proof(
            header,
            column,
            root,
            inner,
            NullableProofScope::While(scope),
            span,
        )
        .expect("direct birth guard accepts its actual source branch");
    lowerer
        .validate_nullable_proofs(&lowerer.bindings, &lowerer.nullable_proofs, span)
        .unwrap();
    Birth {
        header,
        body: edge.target,
        column,
        root,
        scope,
        inner,
        span,
    }
}

#[test]
fn unit_nullable_proof_guards_preserve_source_identity() {
    let source = crate::ssa::while_flow_tests::proof_cases::different_root_source();
    let fixture = Fixture::new(&source);
    let current = fixture.symbol("current");
    let other = fixture.symbol("other");
    let total = fixture.symbol("total");
    let left = fixture.symbol("left");
    let right = fixture.symbol("right");
    // Every case starts from a separate original Function clone, and the same
    // direct guard baseline succeeds before its one identity is changed.
    for case in 0..16 {
        let mut function = fixture.function().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            let LoweredValue::Value(left_owner) = lowerer.bindings[&left] else {
                panic!()
            };
            let LoweredValue::Value(right_owner) = lowerer.bindings[&right] else {
                panic!()
            };
            let birth = install_birth(lowerer, current, &[current, other, total]);
            let proof = lowerer.nullable_proofs[&current];
            let LoweredValue::Value(other_owner) = lowerer.bindings[&other] else {
                panic!()
            };
            assert_eq!(
                rebound(lowerer.function, EntityId::Value(left_owner), birth.body),
                EntityId::Value(proof.owner)
            );
            assert_eq!(
                rebound(lowerer.function, EntityId::Value(right_owner), birth.body),
                EntityId::Value(other_owner)
            );
            assert_ne!(proof.owner, other_owner);
            assert_eq!(
                lowerer
                    .function
                    .entity(EntityId::Value(proof.owner))
                    .unwrap()
                    .ty,
                lowerer
                    .function
                    .entity(EntityId::Value(other_owner))
                    .unwrap()
                    .ty
            );
            let int_type =
                lowerer.type_ids[&lowerer.typed.types().builtin(BuiltinType::Int).unwrap()];
            if case < 5 {
                let mut proofs = lowerer.nullable_proofs.clone();
                let changed = proofs.get_mut(&current).unwrap();
                match case {
                    0 => changed.owner = other_owner,
                    1 => changed.inner = int_type,
                    2 => {
                        lowerer.function.loans[proof.loan.index()].ty = EntityType::Loan {
                            kind: LoanKind::Exclusive,
                            target: proof.inner,
                        }
                    }
                    3 => {
                        let expression = lowerer
                            .parsed
                            .ast()
                            .expressions()
                            .iter()
                            .find(|(_, node)| matches!(node.payload(), Expression::Name))
                            .unwrap()
                            .0;
                        changed.origin.scope = NullableProofScope::If(UnitExpressionId::new(
                            lowerer.source_unit,
                            expression,
                        ));
                    }
                    4 => {
                        let births = lowerer
                            .function
                            .blocks
                            .iter()
                            .filter_map(|block| {
                                match block.terminator.as_ref().map(|term| &term.kind) {
                                    Some(TerminatorKind::NullableBranch { view, .. }) => {
                                        Some(*view)
                                    }
                                    _ => None,
                                }
                            })
                            .collect::<std::collections::BTreeSet<_>>();
                        let rebound_loan = lowerer
                            .function
                            .loans
                            .iter()
                            .enumerate()
                            .find_map(|(index, data)| {
                                let loan = LoanId {
                                    function: lowerer.function.id,
                                    index,
                                };
                                (matches!(data.definition, Definition::BlockParameter { .. })
                                    && data.ty
                                        == EntityType::Loan {
                                            kind: LoanKind::Shared,
                                            target: proof.inner,
                                        }
                                    && !births.contains(&loan))
                                .then_some(loan)
                            })
                            .expect("the outer proof is carried into the nested header");
                        changed.origin.birth = rebound_loan;
                    }
                    _ => unreachable!(),
                }
                rejected(
                    lowerer.validate_nullable_proofs(&lowerer.bindings, &proofs, birth.span),
                    LoweringErrorKind::MissingFact,
                    birth.span,
                );
                return;
            }
            lowerer.nullable_proofs.clear();
            let (_, other_column) = parameter(lowerer.function, EntityId::Value(other_owner));
            let other_header = match &lowerer
                .function
                .block(birth.header)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .kind
            {
                TerminatorKind::NullableBranch { when_non_null, .. } => {
                    when_non_null.arguments[other_column]
                }
                _ => unreachable!(),
            };
            let mut column = birth.column;
            let mut scope = NullableProofScope::While(birth.scope);
            let expected = if matches!(case, 12 | 13) {
                LoweringErrorKind::UnsupportedNode
            } else {
                LoweringErrorKind::MissingFact
            };
            match case {
                5 => column = other_column,
                6 | 7 | 8 | 9 => {
                    let TerminatorKind::NullableBranch {
                        owner,
                        when_null,
                        when_non_null,
                        view,
                    } = &mut lowerer.function.blocks[birth.header.index()]
                        .terminator
                        .as_mut()
                        .unwrap()
                        .kind
                    else {
                        unreachable!()
                    };
                    match case {
                        6 => when_null.arguments[column] = other_header,
                        7 => when_non_null.arguments[column] = other_header,
                        8 => when_non_null.target = when_null.target,
                        9 => {
                            let EntityId::Value(other) = other_header else {
                                panic!()
                            };
                            *owner = other;
                        }
                        _ => unreachable!(),
                    }
                    let _ = view;
                }
                10 => {
                    lowerer.function.blocks[birth.body.index()].parameters.pop();
                }
                11 => {
                    lowerer.function.blocks[birth.body.index()].parameters[column] =
                        EntityId::Value(other_owner);
                }
                12 => {
                    lowerer.nullable_proofs.insert(other, proof);
                }
                13 => {
                    let (_, rebound) = lowerer
                        .function
                        .loans
                        .iter()
                        .enumerate()
                        .find(|(index, data)| {
                            *index != proof.loan.index()
                                && data.ty
                                    == EntityType::Loan {
                                        kind: LoanKind::Shared,
                                        target: proof.inner,
                                    }
                        })
                        .map(|(index, data)| {
                            (
                                data,
                                LoanId {
                                    function: lowerer.function.id,
                                    index,
                                },
                            )
                        })
                        .unwrap();
                    lowerer.nullable_proofs.insert(
                        other,
                        NullableProof {
                            loan: rebound,
                            ..proof
                        },
                    );
                }
                14 => {
                    let (_, node) = lowerer
                        .parsed
                        .ast()
                        .expressions()
                        .iter()
                        .find(|(_, node)| matches!(node.payload(), Expression::Name))
                        .unwrap();
                    let expression = lowerer
                        .parsed
                        .ast()
                        .expressions()
                        .iter()
                        .find(|(_, candidate)| candidate.span() == node.span())
                        .unwrap()
                        .0;
                    scope = NullableProofScope::If(UnitExpressionId::new(
                        lowerer.source_unit,
                        expression,
                    ));
                }
                15 => {
                    let replacement = lowerer
                        .function
                        .blocks
                        .iter()
                        .find_map(
                            |block| match block.terminator.as_ref().map(|term| &term.kind) {
                                Some(TerminatorKind::NullableBranch { view, .. })
                                    if *view != proof.loan =>
                                {
                                    Some(*view)
                                }
                                _ => None,
                            },
                        )
                        .expect("the distinct source root has its own real view");
                    assert_eq!(
                        lowerer
                            .function
                            .entity(EntityId::Loan(replacement))
                            .unwrap()
                            .ty,
                        lowerer
                            .function
                            .entity(EntityId::Loan(proof.loan))
                            .unwrap()
                            .ty
                    );
                    let TerminatorKind::NullableBranch { view, .. } = &mut lowerer.function.blocks
                        [birth.header.index()]
                    .terminator
                    .as_mut()
                    .unwrap()
                    .kind
                    else {
                        unreachable!()
                    };
                    *view = replacement;
                }
                _ => unreachable!(),
            }
            rejected(
                lowerer.introduce_nullable_proof(
                    birth.header,
                    column,
                    birth.root,
                    birth.inner,
                    scope,
                    birth.span,
                ),
                expected,
                birth.span,
            );
        });
    }
}
