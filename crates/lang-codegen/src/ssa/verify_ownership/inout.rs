//! Whole-value replacement invalidates the caller's SSA snapshot until writeback.
use super::*;
use crate::ssa::model::{Instruction, Ownership};

pub(super) fn is_parameter_loan(
    function: &Function,
    reborrows: &ReborrowDependencies,
    loan: LoanId,
) -> bool {
    let Some(entry) = function.blocks.first() else {
        return false;
    };
    reborrows.flows.origins.get(&loan).is_some_and(|origins| {
        !origins.is_empty()
            && origins.iter().all(|origin| {
                entry.parameters[usize::from(function.receiver.is_some())..]
                    .contains(&EntityId::Loan(*origin))
            })
    })
}

pub(super) fn check_instruction(
    state: &BlockState,
    instruction: &Instruction,
    errors: &mut Vec<VerifyError>,
) {
    for entity in instruction.operation.entities() {
        let EntityId::Value(owner) = entity else {
            continue;
        };
        if let Some(&place) = state.inout_writebacks.get(&owner)
            && !matches!(instruction.operation, Operation::RootPlaceTake {
                owner: source, place: actual,
            } if source == owner && actual == place)
        {
            errors.push(error(
                VerifyErrorKind::ValueUnavailable { value: owner },
                VerifyLocation::Instruction(instruction.id),
                &instruction.origin,
            ));
        }
    }
}

pub(super) fn record_call(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    reborrows: &ReborrowDependencies,
    aliases: &AliasRoots,
    state: &mut BlockState,
    errors: &mut Vec<VerifyError>,
) {
    let (receiver, arguments) = match &instruction.operation {
        Operation::DirectCall {
            receiver,
            arguments,
            ..
        } => (*receiver, arguments),
        Operation::BorrowCall { arguments, .. }
        | Operation::CallableInvoke { arguments, .. }
        | Operation::CallableInvokeView { arguments, .. } => (None, arguments),
        _ => return,
    };
    for (index, argument) in arguments.iter().enumerate() {
        let EntityId::Loan(loan) = argument else {
            continue;
        };
        let Some(EntityType::Loan {
            kind: LoanKind::Exclusive,
            target,
        }) = function.entity(*argument).map(|data| data.ty)
        else {
            continue;
        };
        if module.type_ownership(target) != Some(Ownership::MoveOnly) {
            continue;
        }
        let other_loans = arguments[..index]
            .iter()
            .chain(&arguments[index + 1..])
            .copied()
            .chain(receiver)
            .filter(|entity| matches!(entity, EntityId::Loan(_)));
        if other_loans.into_iter().any(|other| {
            aliases.overlap(*argument, other) && !aliases.direct_fields.disjoint(*argument, other)
        }) {
            reject(
                instruction,
                errors,
                "MoveOnly Inout arguments must be disjoint from other loans",
            );
            continue;
        }
        if is_parameter_loan(function, reborrows, *loan)
            || loan_uses_heap_storage(function, reborrows, *loan)
        {
            continue;
        }
        let Some((owner, place)) = local_root(function, reborrows, *loan) else {
            reject(
                instruction,
                errors,
                "MoveOnly Inout requires a parameter loan or a direct root place",
            );
            continue;
        };
        if !state.values.contains(&owner)
            || !state.places.contains(&place)
            || state.inout_writebacks.insert(owner, place).is_some()
        {
            reject(
                instruction,
                errors,
                "MoveOnly Inout requires an available root and one writeback",
            );
            continue;
        }
        // Another place built from the old value still contains the stale snapshot.
        state.places.retain(|&other| {
            other == place || !aliases.overlap(EntityId::Value(owner), EntityId::Place(other))
        });
    }
}

/// Heap-backed places do not expose the local SSA value snapshot that RootPlace stores.
fn loan_uses_heap_storage(
    function: &Function,
    reborrows: &ReborrowDependencies,
    loan: LoanId,
) -> bool {
    reborrows.flows.origins.get(&loan).is_some_and(|origins| {
        !origins.is_empty()
            && origins.iter().all(|&origin| {
                let Some(mut place) = borrowed_place(function, origin) else {
                    return false;
                };
                while let Some(Operation::FieldPlace { base, .. }) =
                    defining_operation(function, EntityId::Place(place))
                {
                    place = *base;
                }
                matches!(
                    defining_operation(function, EntityId::Place(place)),
                    Some(
                        Operation::HeapPayloadPlace { .. }
                            | Operation::ContainerElementPlace { .. }
                    )
                )
            })
    })
}

fn borrowed_place(function: &Function, loan: LoanId) -> Option<PlaceId> {
    match defining_operation(function, EntityId::Loan(loan))? {
        Operation::BorrowBegin {
            place,
            kind: LoanKind::Exclusive,
        } => Some(*place),
        _ => None,
    }
}

fn local_root(
    function: &Function,
    reborrows: &ReborrowDependencies,
    loan: LoanId,
) -> Option<(ValueId, PlaceId)> {
    let origins = reborrows.flows.origins.get(&loan)?;
    let mut origins = origins.iter();
    let origin = *origins.next()?;
    if origins.next().is_some() {
        return None;
    }
    let place = borrowed_place(function, origin)?;
    match defining_operation(function, EntityId::Place(place))? {
        Operation::RootPlace { owner } => Some((*owner, place)),
        _ => None,
    }
}

fn defining_operation(function: &Function, entity: EntityId) -> Option<&Operation> {
    let Definition::InstructionResult { instruction, .. } = function.entity(entity)?.definition
    else {
        return None;
    };
    Some(&function.instruction(instruction)?.operation)
}

fn reject(instruction: &Instruction, errors: &mut Vec<VerifyError>, reason: &'static str) {
    errors.push(error(
        VerifyErrorKind::OperationContract { reason },
        VerifyLocation::Instruction(instruction.id),
        &instruction.origin,
    ));
}
