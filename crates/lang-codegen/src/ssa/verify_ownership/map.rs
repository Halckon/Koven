//! Map container operation ownership verification.

use super::*;
use crate::ssa::model::Origin;

#[allow(clippy::too_many_arguments)]
pub(super) fn verify_map_ownership(
    module: &Module,
    function: &Function,
    operation: &Operation,
    aliases: &AliasRoots,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    match operation {
        Operation::MapConstruct { .. } => {}
        Operation::MapResultUnwrap { result } => {
            require_value(module, function, *result, state, location, origin, errors);
        }
        Operation::MapSize { owner } => {
            check_map_read_owner(module, function, *owner, state, location, origin, errors);
        }
        Operation::MapContains { owner, key } | Operation::MapGet { owner, key } => {
            check_map_read_owner(
                module,
                function,
                *owner,
                state,
                location.clone(),
                origin,
                errors,
            );
            check_map_read_owner(module, function, *key, state, location, origin, errors);
        }
        Operation::MapRequireValue { source, key } => {
            check_map_read_owner(
                module,
                function,
                EntityId::Loan(*source),
                state,
                location.clone(),
                origin,
                errors,
            );
            check_map_read_owner(module, function, *key, state, location, origin, errors);
        }
        Operation::MapWithValue {
            source,
            key,
            action,
        } => {
            check_map_read_owner(
                module,
                function,
                EntityId::Loan(*source),
                state,
                location.clone(),
                origin,
                errors,
            );
            check_map_read_owner(
                module,
                function,
                *key,
                state,
                location.clone(),
                origin,
                errors,
            );
            check_map_read_owner(
                module,
                function,
                EntityId::Loan(*action),
                state,
                location,
                origin,
                errors,
            );
        }
        Operation::MapPut { owner, key, value } => {
            check_and_consume_container_owner(
                module,
                function,
                *owner,
                aliases,
                state,
                location.clone(),
                origin,
                errors,
            );
            consume_value(
                module,
                function,
                *key,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location.clone(),
                origin,
                errors,
            );
            consume_value(
                module,
                function,
                *value,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
        }
        Operation::MapRemove { owner, key } => {
            check_and_consume_container_owner(
                module,
                function,
                *owner,
                aliases,
                state,
                location.clone(),
                origin,
                errors,
            );
            check_map_read_owner(module, function, *key, state, location, origin, errors);
        }
        _ => {}
    }
}

fn check_map_read_owner(
    module: &Module,
    function: &Function,
    owner: EntityId,
    state: &BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    match owner {
        EntityId::Value(owner) => {
            require_value(module, function, owner, state, location, origin, errors);
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
        EntityId::Place(_) => unreachable!("operation contract rejects a place owner"),
    }
}
