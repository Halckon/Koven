//! CI-only ordinary If/N read and cleanup guards; final verification is not a rejection oracle.
use super::super::*;
use crate::ssa::lower_frontend::orchestrate::nullable_map_guard_support::{Fixture, Snapshot};

#[derive(Clone, Copy, Debug)]
enum ReadMutation {
    MissingProof,
    MissingBinding,
    HeaderBinding,
    WrongInner,
    ExclusiveLoan,
    RootBorrow,
    MissingType,
    MissingRegistration,
    WrongRegistration,
    WrongNarrowed,
}

fn read_baseline(fixture: &Fixture) {
    let mut state = fixture.read_checkpoint(false);
    let point = state.point;
    fixture.with_lowerer(&mut state, |lowerer| {
        let before = Snapshot::take(lowerer);
        let descriptor = lowerer.typed.map_size(point.expression).unwrap();
        assert_eq!(
            lowerer
                .nullable_map_size_operand(point.expression, descriptor, point.span)
                .unwrap(),
            Some(EntityId::Loan(point.proof.loan))
        );
        assert_eq!(
            Snapshot::take(lowerer),
            before,
            "read must not retire or change its owner"
        );
    });
}

fn read_rejects(fixture: &Fixture, mutation: ReadMutation) {
    // The same direct target succeeds on an independent fresh checkpoint first.
    read_baseline(fixture);
    let mut state = fixture.read_checkpoint(false);
    let point = state.point;
    match mutation {
        ReadMutation::MissingType => {
            state.type_ids.remove(&point.storage);
        }
        ReadMutation::MissingRegistration => {
            state.nullable_maps.remove(&point.nullable);
        }
        ReadMutation::WrongRegistration => {
            state.nullable_maps.insert(point.nullable, point.nullable);
        }
        ReadMutation::WrongNarrowed => {
            state.type_ids.insert(point.narrowed, point.nullable);
        }
        _ => {}
    }
    fixture.with_lowerer(&mut state, |lowerer| {
        match mutation {
            ReadMutation::MissingProof => {
                lowerer.non_null_bindings.remove(&point.root);
            }
            ReadMutation::MissingBinding => {
                lowerer.bindings.remove(&point.root);
            }
            ReadMutation::HeaderBinding => {
                lowerer
                    .bindings
                    .insert(point.root, LoweredValue::Value(point.header_owner));
            }
            ReadMutation::WrongInner => {
                lowerer
                    .non_null_bindings
                    .get_mut(&point.root)
                    .unwrap()
                    .inner = point.nullable;
            }
            ReadMutation::ExclusiveLoan => {
                lowerer.function.loans[point.proof.loan.index()].ty = EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: point.proof.inner,
                };
            }
            ReadMutation::RootBorrow => {
                lowerer.borrow_bindings.insert(point.root, point.proof.loan);
            }
            _ => {}
        }
        let before = Snapshot::take(lowerer);
        let kind = match mutation {
            ReadMutation::MissingProof | ReadMutation::RootBorrow => {
                LoweringErrorKind::UnsupportedNode
            }
            _ => LoweringErrorKind::MissingFact,
        };
        let descriptor = lowerer.typed.map_size(point.expression).unwrap();
        fixture.reject(
            lowerer.nullable_map_size_operand(point.expression, descriptor, point.span),
            kind,
            point.span,
        );
        assert_eq!(
            Snapshot::take(lowerer),
            before,
            "{mutation:?}: no partial read state"
        );
    });
}

#[derive(Clone, Copy, Debug)]
enum CleanupMutation {
    MissingBinding,
    HeaderBinding,
    WrongLoanTarget,
}

fn cleanup_baseline(fixture: &Fixture) {
    let mut state = fixture.read_checkpoint(true);
    let point = state.point;
    fixture.with_lowerer(&mut state, |lowerer| {
        let start = lowerer.function.instructions.len();
        if fixture.id == "B3" {
            let bindings = lowerer.bindings.clone();
            lowerer
                .end_non_null_proof(point.root, point.control_span)
                .unwrap();
            assert_eq!(
                lowerer.bindings, bindings,
                "retirement keeps source and actual count"
            );
            assert_eq!(lowerer.function.instructions.len(), start + 1);
        } else {
            lowerer
                .emit_drops(DropPoint::CallReturn(point.expression))
                .unwrap();
            assert!(!lowerer.bindings.contains_key(&point.root));
            assert_eq!(lowerer.function.instructions.len(), start + 2);
            assert_eq!(
                lowerer.function.instructions[start + 1].operation,
                Operation::Drop {
                    owner: point.proof.owner
                }
            );
        }
        assert_eq!(
            lowerer.function.instructions[start].operation,
            Operation::BorrowEnd {
                loan: point.proof.loan
            }
        );
        assert!(!lowerer.non_null_bindings.contains_key(&point.root));
    });
}

fn cleanup_rejects(fixture: &Fixture, mutation: CleanupMutation) {
    cleanup_baseline(fixture);
    let mut state = fixture.read_checkpoint(true);
    let point = state.point;
    let span = if fixture.id == "B3" {
        point.control_span
    } else {
        fixture
            .owned
            .drops()
            .iter()
            .find(|fact| {
                fact.point() == DropPoint::CallReturn(point.expression)
                    && fact.target()
                        == lang_frontend::ownership_checking::DropTarget::Named(point.root)
            })
            .unwrap()
            .value_origin()
    };
    fixture.with_lowerer(&mut state, |lowerer| {
        match mutation {
            CleanupMutation::MissingBinding => {
                lowerer.bindings.remove(&point.root);
            }
            CleanupMutation::HeaderBinding => {
                lowerer
                    .bindings
                    .insert(point.root, LoweredValue::Value(point.header_owner));
            }
            CleanupMutation::WrongLoanTarget => {
                lowerer.function.loans[point.proof.loan.index()].ty = EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: point.nullable,
                };
            }
        }
        let before = Snapshot::take(lowerer);
        let result = if fixture.id == "B3" {
            lowerer.end_non_null_proof(point.root, span)
        } else {
            lowerer.emit_drops(DropPoint::CallReturn(point.expression))
        };
        fixture.reject(result, LoweringErrorKind::MissingFact, span);
        assert_eq!(
            Snapshot::take(lowerer),
            before,
            "{mutation:?}: no partial End/Drop"
        );
    });
}

#[test]
fn single_nullable_map_if_read_cleanup_guards() {
    for id in ["B1", "B2", "B3"] {
        let fixture = Fixture::analyze(id);
        for mutation in [
            ReadMutation::MissingProof,
            ReadMutation::MissingBinding,
            ReadMutation::HeaderBinding,
            ReadMutation::WrongInner,
            ReadMutation::ExclusiveLoan,
            ReadMutation::RootBorrow,
            ReadMutation::MissingType,
            ReadMutation::MissingRegistration,
            ReadMutation::WrongRegistration,
            ReadMutation::WrongNarrowed,
        ] {
            read_rejects(&fixture, mutation);
        }
        for mutation in [
            CleanupMutation::MissingBinding,
            CleanupMutation::HeaderBinding,
            CleanupMutation::WrongLoanTarget,
        ] {
            cleanup_rejects(&fixture, mutation);
        }
        // Absence is an intentional no-op for retirement, not a missing-proof refusal.
        let mut state = fixture.read_checkpoint(true);
        let point = state.point;
        fixture.with_lowerer(&mut state, |lowerer| {
            lowerer.non_null_bindings.clear();
            let before = Snapshot::take(lowerer);
            lowerer
                .end_non_null_proof(point.root, point.control_span)
                .unwrap();
            assert_eq!(Snapshot::take(lowerer), before);
        });
    }
}
