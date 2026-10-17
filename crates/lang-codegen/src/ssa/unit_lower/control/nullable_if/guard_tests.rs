//! CI-only ordinary If/N read and cleanup guards. Never use verifier as a fault oracle.
use super::*;
use crate::ssa::model::{Function, LoanKind};
use crate::ssa::unit_lower::{
    UnitExpressionLowerer,
    orchestrate::nullable_map_guard_support::{
        Checkpoint, Fixture, Stage, exit_text, state_text, unchanged,
    },
};

#[derive(Clone, Copy, Debug)]
enum Fault {
    Source,
    MissingStorage,
    MissingRegistry,
    WrongRegistry,
    WrongNarrowed,
    Pending,
    Temporary,
    Borrow,
    MissingProof,
    MissingBinding,
    HeaderBinding,
    Inner,
    Exclusive,
    ProofOwner,
    Terminated,
}
#[derive(Clone, Copy, Debug)]
enum Target {
    Producer,
    Qualification,
    Read,
    Drop,
    Retire,
}

#[test]
fn unit_nullable_map_if_read_cleanup_guards() {
    for id in ["B1", "B2", "B3"] {
        let fixture = Fixture::new(id);
        for fault in [
            Fault::Source,
            Fault::MissingStorage,
            Fault::MissingRegistry,
            Fault::WrongRegistry,
            Fault::Pending,
            Fault::Temporary,
            Fault::Borrow,
        ] {
            pair(&fixture, Target::Producer, fault);
        }
        for fault in [
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
            Fault::Borrow,
        ] {
            pair(&fixture, Target::Read, fault);
        }
        if id == "B3" {
            for fault in [
                Fault::MissingBinding,
                Fault::ProofOwner,
                Fault::Inner,
                Fault::Exclusive,
                Fault::Borrow,
                Fault::Terminated,
            ] {
                pair(&fixture, Target::Retire, fault);
            }
        } else {
            for fault in [
                Fault::MissingBinding,
                Fault::HeaderBinding,
                Fault::Inner,
                Fault::Exclusive,
                Fault::Borrow,
            ] {
                pair(&fixture, Target::Drop, fault);
            }
        }
    }
}

fn pair(fixture: &Fixture, target: Target, fault: Fault) {
    // The very same target succeeds on its own untouched checkpoint first.
    // Successful emitters run on a separate clone, never the mutation's state.
    run(fixture, target, None);
    run(fixture, target, Some(fault));
}

fn run(fixture: &Fixture, target: Target, fault: Option<Fault>) {
    let checkpoint = Checkpoint::read(fixture);
    let stage = match target {
        Target::Producer => Stage::Producer,
        Target::Qualification | Target::Read => Stage::BeforeUse,
        Target::Drop => Stage::Cleanup,
        Target::Retire => Stage::Exits,
    };
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
    fixture.with_lowerer(
        stage,
        &mut function,
        &types,
        &nullable,
        |lowerer, other_source| {
            match fault {
                Some(Fault::Source) => lowerer.source_token = other_source,
                Some(Fault::Pending) => lowerer
                    .pending_operands
                    .push(EntityId::Value(checkpoint.header_owner)),
                Some(Fault::Temporary) => {
                    lowerer
                        .temporaries
                        .insert(checkpoint.use_id, checkpoint.header_owner);
                }
                Some(Fault::Borrow) if !matches!(target, Target::Retire) => {
                    lowerer
                        .borrow_bindings
                        .insert(checkpoint.root, checkpoint.loan);
                }
                Some(Fault::MissingProof) => {
                    lowerer.nullable_proofs.remove(&checkpoint.root);
                }
                Some(Fault::MissingBinding) if !matches!(target, Target::Retire) => {
                    lowerer.bindings.remove(&checkpoint.root);
                }
                Some(Fault::HeaderBinding) => {
                    lowerer.bindings.insert(
                        checkpoint.root,
                        super::super::super::LoweredValue::Value(checkpoint.header_owner),
                    );
                }
                Some(Fault::Inner) if !matches!(target, Target::Retire) => {
                    lowerer
                        .nullable_proofs
                        .get_mut(&checkpoint.root)
                        .unwrap()
                        .inner = checkpoint.nullable;
                }
                Some(Fault::Exclusive) => {
                    lowerer.function.loans[checkpoint.loan.index()].ty = EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        target: checkpoint.inner,
                    };
                }
                _ => {}
            }
            if matches!(target, Target::Retire) {
                retire(fixture, &checkpoint, lowerer, fault);
                return;
            }
            let before = (lowerer.function.clone(), state_text(lowerer));
            let span = match target {
                Target::Producer => checkpoint.control_span,
                Target::Drop => checkpoint.cleanup.unwrap().value_origin(),
                _ => checkpoint.use_span,
            };
            let result = match target {
                Target::Producer => lowerer
                    .prepare_map_nullable_if_entry(
                        checkpoint.control,
                        checkpoint.condition,
                        false,
                        span,
                    )
                    .map(|entry| {
                        let entry = entry.expect("ordinary source is admitted by actual producer");
                        assert_eq!(entry.root, checkpoint.root);
                        assert_eq!(entry.owner, checkpoint.header_owner);
                        assert_eq!(entry.inner, checkpoint.inner);
                        assert_eq!(entry.scope, checkpoint.control);
                    }),
                Target::Qualification => {
                    let descriptor = lowerer.typed.map_size(checkpoint.use_id).unwrap();
                    lowerer
                        .qualified_nullable_map_size_read(checkpoint.use_id, descriptor, span)
                        .map(|read| {
                            assert_eq!(
                                read,
                                Some((checkpoint.root, checkpoint.nullable, checkpoint.inner))
                            )
                        })
                }
                Target::Read => lowerer
                    .lower_map_expression(checkpoint.use_id.expression(), span)
                    .map(|value| {
                        assert!(matches!(
                            value,
                            Some(super::super::super::LoweredValue::Value(_))
                        ));
                    }),
                Target::Drop => lowerer.emit_drops(checkpoint.cleanup.unwrap().point()),
                Target::Retire => unreachable!(),
            };
            if let Some(fault) = fault {
                let kind = match (target, fault) {
                    (
                        Target::Producer,
                        Fault::MissingRegistry | Fault::Pending | Fault::Temporary | Fault::Borrow,
                    )
                    | (Target::Read, Fault::MissingProof | Fault::Borrow)
                    | (Target::Drop, Fault::Borrow) => LoweringErrorKind::UnsupportedNode,
                    _ => LoweringErrorKind::MissingFact,
                };
                fixture.rejected(result, kind, span, &format!("{target:?}/{fault:?}"));
                unchanged(lowerer, &before);
            } else {
                result.expect("same-target clean baseline");
                match target {
                    Target::Producer | Target::Qualification => unchanged(lowerer, &before),
                    Target::Read => {
                        let emitted = appended(lowerer, &before.0);
                        assert_eq!(
                            emitted.first(),
                            Some(&Operation::MapSize {
                                owner: EntityId::Loan(checkpoint.loan)
                            })
                        );
                        if checkpoint.cleanup.is_some() {
                            assert_eq!(
                                &emitted[1..],
                                &[
                                    Operation::BorrowEnd {
                                        loan: checkpoint.loan
                                    },
                                    Operation::Drop {
                                        owner: checkpoint.owner
                                    }
                                ]
                            );
                            assert!(!lowerer.bindings.contains_key(&checkpoint.root));
                        } else {
                            assert_eq!(emitted.len(), 1);
                            assert_eq!(
                                lowerer.bindings.get(&checkpoint.root),
                                Some(&super::super::super::LoweredValue::Value(checkpoint.owner))
                            );
                            assert_eq!(
                                lowerer.nullable_proofs[&checkpoint.root].loan,
                                checkpoint.loan
                            );
                        }
                    }
                    Target::Drop => {
                        assert_eq!(
                            appended(lowerer, &before.0),
                            [
                                Operation::BorrowEnd {
                                    loan: checkpoint.loan
                                },
                                Operation::Drop {
                                    owner: checkpoint.owner
                                }
                            ]
                        );
                        assert!(!lowerer.nullable_proofs.contains_key(&checkpoint.root));
                        assert!(!lowerer.bindings.contains_key(&checkpoint.root));
                    }
                    Target::Retire => unreachable!(),
                }
            }
        },
    );
}

fn retire(
    fixture: &Fixture,
    checkpoint: &Checkpoint,
    lowerer: &mut UnitExpressionLowerer<'_>,
    fault: Option<Fault>,
) {
    let mut exits = checkpoint.exits(lowerer);
    match fault {
        Some(Fault::MissingBinding) => {
            exits[0].bindings.remove(&checkpoint.root);
        }
        Some(Fault::ProofOwner) => {
            exits[0]
                .nullable_proofs
                .get_mut(&checkpoint.root)
                .unwrap()
                .owner = checkpoint.header_owner;
        }
        Some(Fault::Inner) => {
            exits[0]
                .nullable_proofs
                .get_mut(&checkpoint.root)
                .unwrap()
                .inner = checkpoint.nullable;
        }
        Some(Fault::Borrow) => {
            exits[0]
                .borrow_bindings
                .insert(checkpoint.root, checkpoint.loan);
        }
        Some(Fault::Terminated) => {
            // Replay that very exit's original future join terminator, not a made-up block.
            lowerer.function.blocks[checkpoint.body.index()].terminator = fixture
                .function()
                .block(checkpoint.body)
                .unwrap()
                .terminator
                .clone();
        }
        _ => {}
    }
    let before_ir = lowerer.function.clone();
    let before_exits = exits.iter().map(exit_text).collect::<Vec<_>>();
    let result =
        lowerer.retire_if_exit_proofs(checkpoint.control, &mut exits, checkpoint.control_span);
    if let Some(fault) = fault {
        let kind = match fault {
            Fault::Borrow => LoweringErrorKind::UnsupportedNode,
            Fault::Terminated => LoweringErrorKind::InvalidModel,
            _ => LoweringErrorKind::MissingFact,
        };
        fixture.rejected(
            result,
            kind,
            checkpoint.control_span,
            &format!("Retire/{fault:?}"),
        );
        assert_eq!(*lowerer.function, before_ir);
        assert_eq!(
            exits.iter().map(exit_text).collect::<Vec<_>>(),
            before_exits
        );
        // Preflight may restore block/bindings/proofs/receiver/pending/temporary
        // fields from an exit; do not falsely claim its active cursor is unchanged.
    } else {
        result.unwrap();
        assert_eq!(
            appended(lowerer, &before_ir),
            [Operation::BorrowEnd {
                loan: checkpoint.loan
            }]
        );
        assert!(exits.iter().all(|exit| exit.nullable_proofs.is_empty()));
        assert_eq!(
            exits[0].bindings[&checkpoint.root],
            super::super::super::LoweredValue::Value(checkpoint.owner)
        );
        assert_eq!(
            exits[1].bindings[&checkpoint.root],
            super::super::super::LoweredValue::Value(checkpoint.null_owner)
        );
        assert!(exits.iter().all(|exit| exit.bindings.len() == 1));
    }
}

fn appended(lowerer: &UnitExpressionLowerer<'_>, before: &Function) -> Vec<Operation> {
    lowerer.function.instructions[before.instructions.len()..]
        .iter()
        .map(|instruction| instruction.operation.clone())
        .collect()
}
