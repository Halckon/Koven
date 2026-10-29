//! ADR-0034: a callable view exists only as the target of a shared loan; it is never a value,
//! a place, an exclusive loan target or a return type.
use super::{
    super::model::{EntityData, EntityId, EntityType, Function, LoanKind, Module},
    VerifyError, VerifyErrorKind, entity_error, function_error,
};

pub(super) fn verify_entity(
    module: &Module,
    entity: EntityId,
    data: &EntityData,
    errors: &mut Vec<VerifyError>,
) {
    let misplaced = match data.ty {
        EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        } => false,
        other => module
            .callable_view_signature(other.semantic_type())
            .is_some(),
    };
    if misplaced {
        errors.push(entity_error(
            entity,
            data.origin.clone(),
            VerifyErrorKind::InvalidTypeDefinition {
                reason: "a callable view is only a shared loan target",
            },
        ));
    }
}

pub(super) fn verify_returns(module: &Module, function: &Function, errors: &mut Vec<VerifyError>) {
    if function
        .return_types
        .iter()
        .any(|ty| module.callable_view_signature(*ty).is_some())
    {
        errors.push(function_error(
            function,
            VerifyErrorKind::InvalidTypeDefinition {
                reason: "a callable view cannot be returned",
            },
        ));
    }
}
