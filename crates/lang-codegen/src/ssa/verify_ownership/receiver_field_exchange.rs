//! Exclusive direct heap-field projections from an Inout receiver and their commit.
use super::*;
use crate::ssa::model::Instruction;

pub(super) fn dependent_on_field(
    function: &Function,
    base: LoanId,
    field: usize,
    aliases: &AliasRoots,
    reborrows: &ReborrowDependencies,
    state: &BlockState,
) -> Option<LoanId> {
    let identity = EntityId::Loan(base);
    let roots = &aliases.roots[&identity];
    let ty = function.entity(identity)?.ty.semantic_type();
    reborrows.active_descendant_among(
        base,
        state.loans.iter().copied().filter(|&loan| {
            roots.len() != 1
                || !aliases.direct_fields.disjoint_from_receiver_field(
                    loan,
                    *roots.first().expect("unique root"),
                    ty,
                    field,
                )
        }),
    )
}

pub(super) fn read_field(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    aliases: &AliasRoots,
    reborrows: &ReborrowDependencies,
    state: &BlockState,
    errors: &mut Vec<VerifyError>,
) {
    let Operation::HeapFieldRead { receiver, field } = &instruction.operation else {
        unreachable!("heap read dispatch");
    };
    let location = VerifyLocation::Instruction(instruction.id);
    let origin = &instruction.origin;
    if !state.loans.contains(receiver) {
        errors.push(error(
            VerifyErrorKind::LoanInactive { loan: *receiver },
            location.clone(),
            origin,
        ));
    } else if loan_kind(function, *receiver) == LoanKind::Exclusive
        && let Some(dependent) = receiver_field_exchange::dependent_on_field(
            function, *receiver, *field, aliases, reborrows, state,
        )
    {
        errors.push(error(
            VerifyErrorKind::LoanDependencyActive {
                parent: *receiver,
                dependent,
            },
            location.clone(),
            origin,
        ));
    }
    if instruction.results.first().is_some_and(|result| {
        function
            .entity(*result)
            .is_some_and(|entity| type_is_move_only(module, entity.ty.semantic_type()))
    }) {
        errors.push(error(
            VerifyErrorKind::MoveOnlyPlaceRead {
                entity: EntityId::Loan(*receiver),
            },
            location,
            origin,
        ));
    }
}

pub(super) fn shared_reborrow(
    function: &Function,
    instruction: &Instruction,
    reborrows: &ReborrowDependencies,
    state: &BlockState,
    errors: &mut Vec<VerifyError>,
) {
    let Operation::SharedReborrow { source } = instruction.operation else {
        unreachable!("shared reborrow dispatch");
    };
    let location = VerifyLocation::Instruction(instruction.id);
    if !state.loans.contains(&source) {
        errors.push(error(
            VerifyErrorKind::LoanInactive { loan: source },
            location,
            &instruction.origin,
        ));
    } else if let Some(dependent) = reborrows.active_descendant_among(
        source,
        state
            .loans
            .iter()
            .copied()
            .filter(|&loan| loan_kind(function, loan) == LoanKind::Exclusive),
    ) {
        errors.push(error(
            VerifyErrorKind::LoanDependencyActive {
                parent: source,
                dependent,
            },
            location,
            &instruction.origin,
        ));
    }
}

fn is_current_receiver(
    function: &Function,
    reborrows: &ReborrowDependencies,
    loan: LoanId,
) -> bool {
    let entry = function
        .receiver
        .and_then(|_| function.blocks.first()?.parameters.first());
    matches!(entry, Some(EntityId::Loan(entry)) if
        reborrows.flows.origins.get(&loan).is_some_and(|roots| roots.len() == 1 && roots.contains(entry)))
}

fn is_inout_base(function: &Function, dependencies: &ReborrowDependencies, loan: LoanId) -> bool {
    is_current_receiver(function, dependencies, loan)
        || (inout::is_parameter_loan(function, dependencies, loan)
            && dependencies
                .flows
                .origins
                .get(&loan)
                .is_some_and(|origins| origins.len() == 1))
}

pub(super) fn exclusive_field_base(function: &Function, origin: LoanId) -> Option<LoanId> {
    let Definition::InstructionResult {
        instruction,
        index: 0,
    } = function.entity(EntityId::Loan(origin))?.definition
    else {
        return None;
    };
    match function.instruction(instruction)?.operation {
        Operation::ExclusiveHeapFieldLoan { base, .. } => Some(base),
        _ => None,
    }
}

pub(super) fn apply(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    aliases: &AliasRoots,
    reborrows: &ReborrowDependencies,
    state: &mut BlockState,
    errors: &mut Vec<VerifyError>,
) {
    let location = VerifyLocation::Instruction(instruction.id);
    let origin = &instruction.origin;
    match instruction.operation {
        Operation::ExclusiveHeapFieldLoan { base, .. }
        | Operation::SharedHeapFieldLoan { base, .. } => {
            if !state.loans.contains(&base) {
                errors.push(error(
                    VerifyErrorKind::LoanInactive { loan: base },
                    location,
                    origin,
                ));
                return;
            }
            if loan_kind(function, base) == LoanKind::Shared {
                return;
            }
            let exclusive = matches!(
                instruction.operation,
                Operation::ExclusiveHeapFieldLoan { .. }
            );
            if !is_inout_base(function, reborrows, base) {
                errors.push(error(
                    VerifyErrorKind::OperationContract {
                        reason: "heap field loan requires an exact Inout receiver or parameter",
                    },
                    location,
                    origin,
                ));
                return;
            }
            let [child] = instruction.results.as_slice() else {
                unreachable!("exclusive projection result type verified");
            };
            if let Some(active) = state.loans.iter().copied().find(|active| {
                !reborrows.same_origin(base, *active)
                    && (exclusive || loan_kind(function, *active) == LoanKind::Exclusive)
                    && aliases.overlap(EntityId::Loan(base), EntityId::Loan(*active))
                    && !aliases
                        .direct_fields
                        .disjoint(*child, EntityId::Loan(*active))
            }) {
                errors.push(error(
                    VerifyErrorKind::LoanDependencyActive {
                        parent: base,
                        dependent: active,
                    },
                    location,
                    origin,
                ));
            }
        }
        Operation::HeapReceiverFieldExchange { loan, replacement } => {
            let before = errors.len();
            if !state.loans.contains(&loan) {
                errors.push(error(
                    VerifyErrorKind::LoanInactive { loan },
                    location.clone(),
                    origin,
                ));
            } else if let Some(dependent) = reborrows.active_descendant(loan, aliases, state) {
                errors.push(error(
                    VerifyErrorKind::LoanDependencyActive {
                        parent: loan,
                        dependent,
                    },
                    location.clone(),
                    origin,
                ));
            }
            let parents = reborrows.flows.origins.get(&loan).and_then(|origins| {
                origins
                    .iter()
                    .map(|origin| exclusive_field_base(function, *origin))
                    .collect::<Option<Vec<_>>>()
                    .filter(|parents| !parents.is_empty())
            });
            let provenance_error = match parents {
                None => Some(
                    "receiver field exchange requires an exclusive direct field projection loan",
                ),
                Some(parents)
                    if !parents
                        .iter()
                        .all(|&parent| is_current_receiver(function, reborrows, parent)) =>
                {
                    Some("receiver field exchange requires the current Inout receiver")
                }
                Some(_) => None,
            };
            if let Some(reason) = provenance_error {
                errors.push(error(
                    VerifyErrorKind::OperationContract { reason },
                    location.clone(),
                    origin,
                ));
            }
            if let Some(active) = state.loans.iter().copied().find(|active| {
                *active != loan
                    && aliases.overlap(EntityId::Loan(loan), EntityId::Loan(*active))
                    // A CFG argument and its dominating entry receiver can both
                    // name the same single permission. Child origins are checked above.
                    && !is_current_receiver(function, reborrows, *active)
                    && !aliases
                        .direct_fields
                        .disjoint(EntityId::Loan(loan), EntityId::Loan(*active))
            }) {
                errors.push(error(
                    VerifyErrorKind::LoanDependencyActive {
                        parent: loan,
                        dependent: active,
                    },
                    location.clone(),
                    origin,
                ));
            }
            if has_any_value_loan(replacement, aliases, state) {
                errors.push(error(
                    VerifyErrorKind::OwnerLoanConflict { value: replacement },
                    location.clone(),
                    origin,
                ));
            }
            if errors.len() == before {
                state.loans.remove(&loan);
                consume_value(
                    module,
                    function,
                    replacement,
                    aliases,
                    state,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                    location,
                    origin,
                    errors,
                );
            }
        }
        _ => unreachable!("receiver field operation dispatch"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssa::{
        model::{Origin, Program, ScalarConstant, SsaTypeKind},
        verify::verify_program,
    };
    use lang_frontend::source::SourceMap;

    #[derive(Clone, Copy)]
    enum Case {
        Disjoint,
        ReceiverBlockArgument,
        Overlap,
        OverlappingRead,
        OverlappingSharedField,
        ForeignBase,
        ForgedLoan,
        SharedParent,
        EndedField,
    }

    fn example(case: Case) -> Program {
        let mut sources = SourceMap::default();
        let source = sources.add_source("receiver-field.ko", "replace").unwrap();
        let origin = Origin::Source(sources.span(source, 0, 7).unwrap());
        let mut program = Program::default();
        let module = program.add_module("receiver");
        let module = program.module_mut(module).unwrap();
        let field = module.intern_type(SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        });
        let payload = module
            .add_aggregate_type("Pair.payload", vec![field; 2])
            .unwrap();
        let heap = module.declare_heap_owner("Pair").unwrap();
        module.define_heap_owner(heap, payload).unwrap();
        let exclusive = |target| EntityType::Loan {
            kind: LoanKind::Exclusive,
            target,
        };
        let function = module
            .add_instance_function("edit", exclusive(heap), vec![], origin.clone())
            .unwrap();
        let function = module.function_mut(function).unwrap();
        let entry = function
            .add_block(
                vec![exclusive(heap), exclusive(heap), exclusive(field)],
                origin.clone(),
            )
            .unwrap();
        let &[
            EntityId::Loan(base),
            EntityId::Loan(other),
            EntityId::Loan(argument),
        ] = function.block(entry).unwrap().parameters.as_slice()
        else {
            panic!("instance receiver followed by explicit loans");
        };
        let (entry, base) = if matches!(case, Case::ReceiverBlockArgument) {
            let block = function
                .add_block(vec![exclusive(heap)], origin.clone())
                .unwrap();
            function
                .set_terminator(
                    entry,
                    TerminatorKind::Branch(crate::ssa::model::Edge {
                        target: block,
                        arguments: vec![EntityId::Loan(base)],
                    }),
                    origin.clone(),
                )
                .unwrap();
            (
                block,
                field_exchange_tests::loan(function.block(block).unwrap().parameters[0]),
            )
        } else {
            (entry, base)
        };
        let mut append = |op, ty| {
            function
                .append_instruction(entry, op, ty, origin.clone())
                .unwrap()
                .1
        };
        let first = if matches!(case, Case::ForeignBase) {
            other
        } else {
            base
        };
        let loan = field_exchange_tests::loan(
            append(
                Operation::ExclusiveHeapFieldLoan {
                    base: first,
                    field: 0,
                },
                vec![exclusive(field)],
            )[0],
        );
        if matches!(case, Case::Disjoint | Case::OverlappingRead) {
            append(
                Operation::HeapFieldRead {
                    receiver: base,
                    field: usize::from(matches!(case, Case::Disjoint)),
                },
                vec![EntityType::Value(field)],
            );
        }
        if matches!(case, Case::Disjoint | Case::OverlappingSharedField) {
            let shared = field_exchange_tests::loan(
                append(
                    Operation::SharedHeapFieldLoan {
                        base,
                        field: usize::from(matches!(case, Case::Disjoint)),
                    },
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: field,
                    }],
                )[0],
            );
            append(Operation::BorrowEnd { loan: shared }, vec![]);
        }
        if matches!(case, Case::SharedParent) {
            let shared = field_exchange_tests::loan(
                append(
                    Operation::SharedReborrow { source: base },
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: heap,
                    }],
                )[0],
            );
            append(Operation::BorrowEnd { loan: shared }, vec![]);
        } else if matches!(case, Case::EndedField) {
            append(Operation::BorrowEnd { loan }, vec![]);
        }
        let second = field_exchange_tests::loan(
            append(
                Operation::ExclusiveHeapFieldLoan {
                    base,
                    field: usize::from(!matches!(case, Case::Overlap)),
                },
                vec![exclusive(field)],
            )[0],
        );
        let replacement = field_exchange_tests::value(
            append(
                Operation::Constant(ScalarConstant::Integer(7)),
                vec![EntityType::Value(field)],
            )[0],
        );
        for loan in [
            second,
            if matches!(case, Case::ForgedLoan) {
                argument
            } else {
                loan
            },
        ] {
            append(
                Operation::HeapReceiverFieldExchange { loan, replacement },
                vec![EntityType::Value(field)],
            );
        }
        function
            .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
            .unwrap();
        program
    }

    #[test]
    fn receiver_field_exchange_verifier_rejects_unproved_or_overlapping_loans() {
        verify_program(&example(Case::Disjoint)).unwrap();
        verify_program(&example(Case::ReceiverBlockArgument)).unwrap();
        for (case, expected) in [
            (Case::Overlap, "LoanDependencyActive"),
            (Case::OverlappingRead, "LoanDependencyActive"),
            (Case::OverlappingSharedField, "LoanDependencyActive"),
            (Case::ForeignBase, "requires the current Inout receiver"),
            (
                Case::ForgedLoan,
                "requires an exclusive direct field projection loan",
            ),
            (Case::SharedParent, "LoanDependencyActive"),
            (Case::EndedField, "LoanInactive"),
        ] {
            let errors = verify_program(&example(case)).unwrap_err();
            let errors = format!("{errors:?}");
            assert!(errors.contains(expected), "{expected}: {errors}");
        }
    }
}
