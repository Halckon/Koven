//! Invalid String return checkpoints must fail before any Take or BorrowEnd mutation.
use super::*;
use crate::ssa::unit_lower::call_lifetimes::PendingCallFrame;
use crate::ssa::unit_lower::nullable_proof::{NullableProofScope, guard_tests::support::Fixture};

#[derive(Clone, Copy, Debug)]
enum Fault {
    MissingBinding,
    HeaderOwner,
    WrongNiche,
    PendingOperand,
    PendingFrame,
    AliasBinding,
    TemporaryAlias,
    WrongReturn,
    BorrowedRoot,
}

#[test]
fn string_if_owner_return_guards_preserve_the_original_state() {
    let mut fixture = Fixture::new(
        r#"fun entry(own text: String?, input: String, stop: Boolean): String {
    println(input)
    if (text != null) { if (stop) { return ((text)) }; println(text); return text }
    return "none"
}"#,
    );
    let root = fixture.symbol("text");
    let input = fixture.symbol("input");
    let (returning, operand, span) = fixture
        .parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            let Expression::Return {
                value: Some(value), ..
            } = node.payload()
            else {
                return None;
            };
            fixture
                .typed
                .types()
                .non_null_use(UnitExpressionId::new(fixture.source, *value))
                .is_some()
                .then_some((id, *value, node.span()))
        })
        .unwrap();
    let (control, control_span) = fixture
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::If { .. })
                .then_some((UnitExpressionId::new(fixture.source, id), node.span()))
        })
        .max_by_key(|(_, span)| span.end() - span.start())
        .unwrap();
    let call = fixture
        .parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            matches!(node.payload(), Expression::Call { .. })
                .then_some(UnitExpressionId::new(fixture.source, id))
        })
        .unwrap();
    let storage = fixture.type_ids[&fixture.typed.types().symbol_type(root).unwrap()];
    let inner = fixture.program.modules[0].nullable_inner(storage).unwrap();
    let wrong_niche = fixture.program.modules[0].intern_type(SsaTypeKind::InlineNullable {
        inner,
        niche: false,
    });
    for fault in [
        Fault::MissingBinding,
        Fault::HeaderOwner,
        Fault::WrongNiche,
        Fault::PendingOperand,
        Fault::PendingFrame,
        Fault::AliasBinding,
        Fault::TemporaryAlias,
        Fault::WrongReturn,
        Fault::BorrowedRoot,
    ] {
        let mut function = fixture.function().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            let (header, header_owner, body, column) = lowerer
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
                    (term.origin == Origin::Source(control_span)).then(|| {
                        let column = when_non_null
                            .arguments
                            .iter()
                            .position(|entity| *entity == EntityId::Value(*owner))
                            .unwrap();
                        (block.id, *owner, when_non_null.target, column)
                    })
                })
                .unwrap();
            lowerer.block = body;
            let EntityId::Value(owner) = lowerer.function.block(body).unwrap().parameters[column]
            else {
                panic!("owned column")
            };
            lowerer.bindings.insert(root, LoweredValue::Value(owner));
            lowerer
                .introduce_nullable_proof(
                    header,
                    column,
                    root,
                    inner,
                    NullableProofScope::If(control),
                    control_span,
                )
                .unwrap();
            let proof = lowerer.nullable_proofs[&root];
            let mut actual_return = returning;
            match fault {
                Fault::MissingBinding => {
                    lowerer.bindings.remove(&root);
                }
                Fault::HeaderOwner => {
                    lowerer.nullable_proofs.get_mut(&root).unwrap().owner = header_owner
                }
                Fault::WrongNiche => {
                    lowerer.function.values[owner.index()].ty = EntityType::Value(wrong_niche)
                }
                Fault::PendingOperand => lowerer.pending_operands.push(EntityId::Loan(proof.loan)),
                Fault::PendingFrame => lowerer.pending_call_frames.push(PendingCallFrame {
                    call,
                    receiver: false,
                    loop_depth: 0,
                    pending_start: 0,
                    created_loans: Vec::new(),
                    nullable_children: Vec::new(),
                    loan_arguments: Vec::new(),
                    abi_slots: Vec::new(),
                    exclusive_root_owners: Vec::new(),
                    shared_field_roots: Vec::new(),
                    field_replace_owner: None,
                }),
                Fault::AliasBinding => {
                    lowerer.bindings.insert(input, LoweredValue::Value(owner));
                }
                Fault::TemporaryAlias => {
                    lowerer
                        .temporaries
                        .insert(UnitExpressionId::new(lowerer.source_unit, operand), owner);
                }
                Fault::WrongReturn => actual_return = control.expression(),
                Fault::BorrowedRoot => {
                    lowerer.borrow_bindings.insert(root, proof.loan);
                }
            }
            let before_function = lowerer.function.clone();
            let before_bindings = lowerer.bindings.clone();
            let before_temporaries = lowerer.temporaries.clone();
            let before_proofs = format!("{:?}", lowerer.nullable_proofs);
            let before_frames = lowerer.pending_call_frames.clone();
            let before_operands = lowerer.pending_operands.clone();
            let expected = if matches!(
                fault,
                Fault::PendingOperand | Fault::PendingFrame | Fault::BorrowedRoot
            ) {
                LoweringErrorKind::UnsupportedNode
            } else {
                LoweringErrorKind::MissingFact
            };
            assert_eq!(
                lowerer
                    .lower_proven_owner_return(actual_return, operand, span)
                    .unwrap_err()
                    .kind,
                expected,
                "{fault:?}"
            );
            assert_eq!(lowerer.function, &before_function, "{fault:?}");
            assert_eq!(lowerer.bindings, before_bindings, "{fault:?}");
            assert_eq!(lowerer.temporaries, before_temporaries, "{fault:?}");
            assert_eq!(
                format!("{:?}", lowerer.nullable_proofs),
                before_proofs,
                "{fault:?}"
            );
            assert_eq!(lowerer.pending_call_frames, before_frames, "{fault:?}");
            assert_eq!(lowerer.pending_operands, before_operands, "{fault:?}");
        });
    }
}
