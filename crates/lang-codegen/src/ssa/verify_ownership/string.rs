//! Ownership effects for operations that borrow or consume Strings.
use super::*;
use crate::ssa::model::Origin;

pub(super) fn apply(
    module: &Module,
    function: &Function,
    instruction: &crate::ssa::model::Instruction,
    aliases: &AliasRoots,
    state: &mut BlockState,
    errors: &mut Vec<VerifyError>,
) {
    let origin = &instruction.origin;
    let location = VerifyLocation::Instruction(instruction.id);
    match &instruction.operation {
        Operation::StringAppend { owner, fragment } => {
            require_string_view(
                module,
                function,
                *fragment,
                aliases,
                state,
                location.clone(),
                origin,
                errors,
            );
            consume_value(
                module,
                function,
                *owner,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
        }
        Operation::StringConcat { left, right } | Operation::StringEqual { left, right } => {
            require_string_view(
                module,
                function,
                *left,
                aliases,
                state,
                location.clone(),
                origin,
                errors,
            );
            require_string_view(
                module, function, *right, aliases, state, location, origin, errors,
            );
        }
        Operation::StringClone { source }
        | Operation::StringByteLength { source }
        | Operation::StringByteAt { source, .. }
        | Operation::StringCopySlice { source, .. } => {
            require_string_view(
                module,
                function,
                EntityId::Loan(*source),
                aliases,
                state,
                location,
                origin,
                errors,
            );
        }
        Operation::PrintString { value }
        | Operation::Utf8ErrorOffset { source: value }
        | Operation::StringCopyBytes { source: value } => {
            if !state.loans.contains(value) {
                errors.push(error(
                    VerifyErrorKind::LoanInactive { loan: *value },
                    location,
                    origin,
                ));
            }
        }
        _ => unreachable!("dispatched only String ownership operations"),
    }
}

#[allow(clippy::too_many_arguments)]
fn require_string_view(
    module: &Module,
    function: &Function,
    operand: EntityId,
    aliases: &AliasRoots,
    state: &BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    match operand {
        EntityId::Value(value) => {
            if require_value(
                module,
                function,
                value,
                state,
                location.clone(),
                origin,
                errors,
            ) && has_exclusive_value_loan(function, value, aliases, state)
            {
                errors.push(error(
                    VerifyErrorKind::OwnerLoanConflict { value },
                    location,
                    origin,
                ));
            }
        }
        EntityId::Loan(loan) => {
            if !state.loans.contains(&loan) {
                errors.push(error(
                    VerifyErrorKind::LoanInactive { loan },
                    location,
                    origin,
                ));
            }
        }
        EntityId::Place(_) => unreachable!("operation contract rejects String place views"),
    }
}
