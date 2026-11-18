use super::*;

pub(super) fn string_binary_contract(
    module: &Module,
    function: &Function,
    left: EntityId,
    right: EntityId,
    results: &[EntityType],
    returns_boolean: bool,
) -> bool {
    let Some(left) = string_view_type(module, function, left, returns_boolean) else {
        return false;
    };
    if string_view_type(module, function, right, returns_boolean) != Some(left) {
        return false;
    }
    if returns_boolean {
        single_value_result(results).is_some_and(|ty| is_boolean(module, ty))
    } else {
        single_value_result(results) == Some(left)
    }
}

fn string_view_type(
    module: &Module,
    function: &Function,
    operand: EntityId,
    allow_nullable: bool,
) -> Option<SsaTypeId> {
    let entity = entity_type(function, operand);
    if matches!(entity, EntityType::Place(_)) {
        return None;
    }
    let ty = entity.semantic_type();
    if allow_nullable && let Some(inner) = module.nullable_string_payload(ty) {
        return Some(inner);
    }
    (!matches!(
        entity,
        EntityType::Loan {
            kind: LoanKind::Exclusive,
            ..
        }
    ) && is_string_owner(module, ty))
    .then_some(ty)
}
