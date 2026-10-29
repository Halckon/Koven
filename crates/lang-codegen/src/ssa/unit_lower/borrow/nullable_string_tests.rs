//! Private guard checks start from real, verified ordinary-unit String proof identities.
use super::*;
use crate::ssa::unit_lower::{
    call_lifetimes::PendingCallFrame,
    nullable_proof::{NullableProofScope, guard_tests::support::Fixture},
};

#[derive(Clone, Copy, Debug)]
enum Fault {
    MissingBinding,
    HeaderBinding,
    HeaderOwner,
    WrongInner,
    WrongNiche,
    WhileScope,
    PendingOperand,
    PendingFrame,
}

#[test]
fn nullable_string_reader_and_retirement_reject_wrong_identity_without_mutation() {
    let mut fixture = Fixture::new(
        r#"fun entry(own text: String?): Unit {
    if (text != null) { println(text); println(text) }
    while (false) {}
}"#,
    );
    let root = fixture.symbol("text");
    let module = &mut fixture.program.modules[0];
    let (storage, inner) = module
        .types
        .iter()
        .enumerate()
        .find_map(|(index, kind)| {
            if let SsaTypeKind::InlineNullable { inner, niche: true } = kind {
                Some((
                    SsaTypeId {
                        module: module.id,
                        index,
                    },
                    *inner,
                ))
            } else {
                None
            }
        })
        .unwrap();
    let wrong_niche = module.intern_type(SsaTypeKind::InlineNullable {
        inner,
        niche: false,
    });
    let boolean = module
        .types
        .iter()
        .position(|kind| *kind == SsaTypeKind::Boolean)
        .unwrap();
    let boolean = SsaTypeId {
        module: module.id,
        index: boolean,
    };
    let (control, condition, span) = fixture
        .parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            if let Expression::If { condition, .. } = node.payload() {
                Some((
                    UnitExpressionId::new(fixture.source, id),
                    *condition,
                    node.span(),
                ))
            } else {
                None
            }
        })
        .unwrap();
    let while_scope = fixture
        .parsed
        .ast()
        .statements()
        .iter()
        .find_map(|(id, node)| {
            matches!(node.payload(), Statement::While { .. })
                .then_some(UnitStatementId::new(fixture.source, id))
        })
        .unwrap();
    let fact = fixture.owned.ownership().loans().iter().find(|fact| {
        matches!(fact.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == root)
    }).unwrap();
    for fault in [
        Fault::MissingBinding,
        Fault::HeaderBinding,
        Fault::HeaderOwner,
        Fault::WrongInner,
        Fault::WrongNiche,
        Fault::WhileScope,
        Fault::PendingOperand,
        Fault::PendingFrame,
    ] {
        let mut function = fixture.function().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            let (header, header_owner, body, column) = lowerer
                .function
                .blocks
                .iter()
                .find_map(|block| {
                    let TerminatorKind::NullableBranch {
                        owner,
                        when_non_null,
                        ..
                    } = &block.terminator.as_ref()?.kind
                    else {
                        return None;
                    };
                    let column = when_non_null
                        .arguments
                        .iter()
                        .position(|entity| *entity == EntityId::Value(*owner))
                        .unwrap();
                    Some((block.id, *owner, when_non_null.target, column))
                })
                .unwrap();
            assert_eq!(
                lowerer
                    .function
                    .entity(EntityId::Value(header_owner))
                    .unwrap()
                    .ty,
                EntityType::Value(storage)
            );
            lowerer.block = body;
            let EntityId::Value(owner) = lowerer.function.block(body).unwrap().parameters[column]
            else {
                panic!("owner column")
            };
            lowerer.bindings.insert(root, LoweredValue::Value(owner));
            lowerer
                .introduce_nullable_proof(
                    header,
                    column,
                    root,
                    inner,
                    NullableProofScope::If(control),
                    span,
                )
                .unwrap();
            let loan = lowerer.nullable_proofs[&root].loan;
            assert_eq!(
                lowerer
                    .lower_nullable_root_borrow(
                        fact.argument().expression(),
                        fact.target(),
                        inner,
                        fact.begin_span()
                    )
                    .unwrap(),
                Some(loan)
            );
            lowerer
                .validate_nullable_proof_retirement(root, span)
                .unwrap();
            match fault {
                Fault::MissingBinding => {
                    lowerer.bindings.remove(&root);
                }
                Fault::HeaderBinding => {
                    lowerer
                        .bindings
                        .insert(root, LoweredValue::Value(header_owner));
                }
                Fault::HeaderOwner => {
                    lowerer.nullable_proofs.get_mut(&root).unwrap().owner = header_owner
                }
                Fault::WrongInner => {
                    lowerer.nullable_proofs.get_mut(&root).unwrap().inner = boolean
                }
                Fault::WrongNiche => {
                    lowerer.function.values[owner.index()].ty = EntityType::Value(wrong_niche)
                }
                Fault::WhileScope => {
                    lowerer.nullable_proofs.clear();
                    assert!(
                        lowerer
                            .introduce_nullable_proof(
                                header,
                                column,
                                root,
                                inner,
                                NullableProofScope::While(while_scope),
                                span
                            )
                            .is_err()
                    );
                    assert!(lowerer.nullable_proofs.is_empty());
                    return;
                }
                Fault::PendingOperand => lowerer.pending_operands.push(EntityId::Value(owner)),
                Fault::PendingFrame => lowerer.pending_call_frames.push(PendingCallFrame {
                    call: fact.call(),
                    receiver: true,
                    nullable_children: Vec::new(),
                    loan_arguments: Vec::new(),
                    loop_depth: 0,
                    pending_start: 0,
                    created_loans: Vec::new(),
                    abi_slots: Vec::new(),
                    field_replace_owner: None,
                    shared_field_roots: Vec::new(),
                    exclusive_root_owners: Vec::new(),
                }),
            }
            let before_function = lowerer.function.clone();
            let before_bindings = lowerer.bindings.clone();
            let before_proofs = lowerer.nullable_proofs.clone();
            let before_pending = lowerer.pending_operands.clone();
            let before_frames = lowerer.pending_call_frames.clone();
            if matches!(fault, Fault::PendingOperand | Fault::PendingFrame) {
                assert_eq!(
                    lowerer
                        .validate_nullable_proof_retirement(root, span)
                        .unwrap_err()
                        .kind,
                    LoweringErrorKind::UnsupportedNode,
                    "{fault:?}: reject the live carrier before reaching the emitter"
                );
                assert_eq!(
                    lowerer.end_nullable_proof(root, span).unwrap_err().kind,
                    LoweringErrorKind::UnsupportedNode,
                    "{fault:?}: an already terminated block must not be the rejection oracle"
                );
            } else {
                assert!(
                    !matches!(
                        lowerer.lower_nullable_root_borrow(
                            fact.argument().expression(),
                            fact.target(),
                            inner,
                            fact.begin_span()
                        ),
                        Ok(Some(_))
                    ),
                    "{fault:?}"
                );
                assert!(
                    lowerer
                        .validate_nullable_proof_retirement(root, span)
                        .is_err(),
                    "{fault:?}"
                );
            }
            assert_eq!(*lowerer.function, before_function, "{fault:?}");
            assert_eq!(lowerer.bindings, before_bindings, "{fault:?}");
            assert_eq!(lowerer.nullable_proofs, before_proofs, "{fault:?}");
            assert_eq!(lowerer.pending_operands, before_pending, "{fault:?}");
            assert_eq!(lowerer.pending_call_frames, before_frames, "{fault:?}");
            // The real source's condition remains the exact entry selector throughout.
            assert_eq!(
                lowerer
                    .map_borrow_condition(condition, span)
                    .unwrap()
                    .unwrap()
                    .symbol(),
                root
            );
        });
    }
}
