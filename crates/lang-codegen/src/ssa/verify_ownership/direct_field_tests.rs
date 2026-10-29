//! Projection proof tests supplement full verifier field-exchange fixtures.
use super::*;

#[test]
fn sibling_proof_rejects_mixed_cfg_paths_and_unseeded_cycles() {
    for alternative in 0..5 {
        let mut f = Fixture::new(false);
        let owner = f.arg(0);
        let left = f.field_loan(owner, 0, LoanKind::Shared);
        let right = f.field_loan(owner, 1, LoanKind::Shared);
        let other_owner = f.arg(1);
        let other = f.field_loan(other_owner, 1, LoanKind::Shared);
        let ty = EntityType::Loan {
            kind: LoanKind::Shared,
            target: f.field_ty,
        };
        let origin = f.origin.clone();
        let join = f
            .function_mut()
            .add_block(vec![ty], origin.clone())
            .unwrap();
        let loop_block = f.function_mut().add_block(vec![ty], origin).unwrap();
        let joined = f.function().block(join).unwrap().parameters[0];
        let looped = f.function().block(loop_block).unwrap().parameters[0];
        let boolean = f
            .program
            .module_mut(f.module)
            .unwrap()
            .intern_type(SsaTypeKind::Boolean);
        let condition = value(
            f.append(
                f.entry,
                Operation::Constant(ScalarConstant::Boolean(true)),
                vec![EntityType::Value(boolean)],
            )[0],
        );
        // Mode 0 is a seeded backedge; mode 1 adds a different field; mode 2 a
        // different root; mode 3 an unseeded predecessor cycle; mode 4 an unknown
        // incoming block parameter with no predecessor. None may erase an input.
        let second = match alternative {
            1 => EntityId::Loan(left),
            2 => EntityId::Loan(other),
            _ => EntityId::Loan(right),
        };
        f.terminate(
            f.entry,
            TerminatorKind::Conditional {
                condition,
                when_true: Edge {
                    target: join,
                    arguments: vec![EntityId::Loan(right)],
                },
                when_false: Edge {
                    target: join,
                    arguments: vec![second],
                },
            },
        );
        f.terminate(
            join,
            if alternative == 0 {
                TerminatorKind::Branch(Edge {
                    target: loop_block,
                    arguments: vec![joined],
                })
            } else {
                TerminatorKind::Abort
            },
        );
        f.terminate(
            loop_block,
            if alternative == 3 {
                TerminatorKind::Conditional {
                    condition,
                    when_true: Edge {
                        target: loop_block,
                        arguments: vec![looped],
                    },
                    when_false: Edge {
                        target: join,
                        arguments: vec![looped],
                    },
                }
            } else if alternative == 0 || alternative == 4 {
                TerminatorKind::Branch(Edge {
                    target: join,
                    arguments: vec![looped],
                })
            } else {
                TerminatorKind::Abort
            },
        );
        let aliases = AliasRoots::compute(f.function());
        assert_eq!(
            aliases.direct_fields.disjoint(EntityId::Loan(left), joined),
            alternative == 0,
            "unexpected proof for alternative {alternative}"
        );
    }
}

#[test]
fn sibling_proof_preserves_reborrow_but_never_root_or_owner_consumption() {
    for operation in 0..3 {
        let mut f = Fixture::new(false);
        let owner = f.arg(0);
        let replacement = f.arg(2);
        let sibling = f.field_loan(owner, 1, LoanKind::Shared);
        let reborrow = loan(
            f.append(
                f.entry,
                Operation::SharedReborrow { source: sibling },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: f.field_ty,
                }],
            )[0],
        );
        let exclusive = f.field_loan(owner, 0, LoanKind::Exclusive);
        f.exchange(f.entry, owner, 0, exclusive, replacement);
        if operation == 1 {
            f.append(f.entry, Operation::Drop { owner }, vec![]);
        } else if operation == 2 {
            let root = place(
                f.append(
                    f.entry,
                    Operation::RootPlace { owner },
                    vec![EntityType::Place(f.owner_ty)],
                )[0],
            );
            f.append(
                f.entry,
                Operation::BorrowBegin {
                    place: root,
                    kind: LoanKind::Exclusive,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: f.owner_ty,
                }],
            );
        }
        f.abort();
        let aliases = AliasRoots::compute(f.function());
        assert!(
            aliases
                .direct_fields
                .disjoint(EntityId::Loan(exclusive), EntityId::Loan(reborrow))
        );
        let result = verify_program(&f.program);
        if operation == 0 {
            assert_eq!(result, Ok(()));
        } else {
            let errors = result.unwrap_err().errors;
            assert!(errors.iter().any(|e| matches!(
                e.kind,
                VerifyErrorKind::OwnerLoanConflict { .. } | VerifyErrorKind::BorrowConflict { .. }
            )));
        }
    }
}

#[test]
fn sibling_proof_does_not_allow_duplicate_exclusive_field_loans() {
    let mut f = Fixture::new(false);
    let owner = f.arg(0);
    f.field_loan(owner, 0, LoanKind::Exclusive);
    f.field_loan(owner, 0, LoanKind::Shared);
    f.abort();
    assert!(
        verify_program(&f.program)
            .unwrap_err()
            .errors
            .iter()
            .any(|e| matches!(e.kind, VerifyErrorKind::BorrowConflict { .. }))
    );
}
