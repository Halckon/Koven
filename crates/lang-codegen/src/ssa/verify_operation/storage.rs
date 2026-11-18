//! Typed storage construction, exchange, extraction, and projection contracts.

use super::{
    entity_type, is_boolean, is_first_class, nullable, place_type, single_value_result, value_type,
};
use crate::ssa::model::{
    self, EntityId, EntityType, Function, LoanKind, Module, Operation, Ownership, SsaTypeId,
    SsaTypeKind, ValueId,
};

pub(super) fn root_exchange_contract(
    module: &Module,
    function: &Function,
    owners: &[ValueId],
    loans: &[model::LoanId],
    results: &[EntityType],
) -> bool {
    let Some(ty) = owners
        .first()
        .and_then(|owner| value_type(function, *owner))
    else {
        return false;
    };
    (is_first_class(module, ty) || matches!(module.type_kind(ty), Some(SsaTypeKind::Unit)))
        && !matches!(
            module.type_kind(ty),
            Some(SsaTypeKind::ConcreteClosure { .. })
        )
        && owners
            .iter()
            .all(|owner| value_type(function, *owner) == Some(ty))
        && loans.iter().all(|loan| {
            entity_type(function, EntityId::Loan(*loan))
                == EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: ty,
                }
        })
        && results == [EntityType::Value(ty); 2]
}

pub(super) fn root_place_owner(
    function: &Function,
    place: model::PlaceId,
) -> Option<model::ValueId> {
    let model::Definition::InstructionResult {
        instruction,
        index: 0,
    } = function.entity(EntityId::Place(place))?.definition
    else {
        return None;
    };
    match function.instruction(instruction)?.operation {
        Operation::RootPlace { owner } => Some(owner),
        _ => None,
    }
}

pub(super) fn aggregate_construct_contract(
    module: &Module,
    function: &Function,
    aggregate: SsaTypeId,
    fields: &[ValueId],
    results: &[EntityType],
) -> bool {
    if module.presence_payload_type(aggregate).is_some()
        && module.type_ownership(aggregate) == Some(Ownership::MoveOnly)
    {
        return false;
    }
    let Some(expected_fields) = module.aggregate_fields(aggregate) else {
        return false;
    };
    single_value_result(results) == Some(aggregate)
        && fields.len() == expected_fields.len()
        && fields
            .iter()
            .zip(expected_fields)
            .all(|(field, expected)| value_type(function, *field) == Some(*expected))
}

pub(super) fn nullable_loan_presence_contract(
    module: &Module,
    function: &Function,
    source: model::LoanId,
    results: &[EntityType],
) -> bool {
    let nullable = match function.entity(EntityId::Loan(source)).map(|e| e.ty) {
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        }) => {
            matches!(
                module.type_kind(target),
                Some(SsaTypeKind::NullableHandle { .. } | SsaTypeKind::InlineNullable { .. })
            ) || module.map_result_value(target).is_some()
        }
        Some(EntityType::Loan {
            kind: LoanKind::Exclusive,
            target,
        }) => {
            matches!(module.type_kind(target), Some(SsaTypeKind::NullableHandle { inner })
                if matches!(module.type_kind(*inner), Some(SsaTypeKind::SharedOwner { .. })))
                || module.nullable_string_payload(target).is_some()
        }
        _ => false,
    };
    nullable && single_value_result(results).is_some_and(|ty| is_boolean(module, ty))
}

pub(super) fn nullable_take_contract(
    module: &Module,
    function: &Function,
    owner: ValueId,
    proof: model::LoanId,
    results: &[EntityType],
) -> bool {
    let Some(nullable) = value_type(function, owner) else {
        return false;
    };
    let Some(inner) = module.nullable_inner(nullable) else {
        return false;
    };
    entity_type(function, EntityId::Loan(proof))
        == EntityType::Loan {
            kind: LoanKind::Shared,
            target: inner,
        }
        && nullable::has_exact_branch_proof(function, owner, proof)
        && single_value_result(results) == Some(inner)
}

pub(super) fn aggregate_project_contract(
    module: &Module,
    function: &Function,
    aggregate: ValueId,
    field: usize,
    results: &[EntityType],
) -> bool {
    let Some(aggregate) = value_type(function, aggregate) else {
        return false;
    };
    let Some(field) = module
        .aggregate_fields(aggregate)
        .and_then(|fields| fields.get(field))
        .copied()
    else {
        return false;
    };
    module.type_ownership(field) == Some(model::Ownership::Copyable)
        && single_value_result(results) == Some(field)
}

pub(in crate::ssa) fn aggregate_explode_contract(
    module: &Module,
    function: &Function,
    aggregate: ValueId,
    results: &[EntityType],
) -> bool {
    let Some(aggregate) = value_type(function, aggregate) else {
        return false;
    };
    module.type_ownership(aggregate) == Some(model::Ownership::MoveOnly)
        && module.presence_payload_type(aggregate).is_none()
        && module.aggregate_fields(aggregate).is_some_and(|fields| {
            results
                == fields
                    .iter()
                    .copied()
                    .map(EntityType::Value)
                    .collect::<Vec<_>>()
        })
}

pub(super) fn heap_allocate_contract(
    module: &Module,
    function: &Function,
    owner: SsaTypeId,
    payload: ValueId,
    results: &[EntityType],
) -> bool {
    module.heap_payload(owner).is_some_and(|expected| {
        value_type(function, payload) == Some(expected)
            && single_value_result(results) == Some(owner)
    })
}

pub(super) fn shared_allocate_contract(
    module: &Module,
    function: &Function,
    owner: SsaTypeId,
    payload: ValueId,
    results: &[EntityType],
) -> bool {
    module.shared_payload(owner).is_some_and(|expected| {
        value_type(function, payload) == Some(expected)
            && single_value_result(results) == Some(owner)
    })
}

pub(super) fn tagged_construct_contract(
    module: &Module,
    function: &Function,
    tagged: SsaTypeId,
    variant: usize,
    payload: ValueId,
    results: &[EntityType],
) -> bool {
    module
        .tagged_variants(tagged)
        .and_then(|variants| variants.get(variant))
        .is_some_and(|expected| {
            value_type(function, payload) == Some(*expected)
                && single_value_result(results) == Some(tagged)
        })
}

pub(super) fn aggregate_copy_explode_contract(
    module: &Module,
    function: &Function,
    aggregate: ValueId,
    results: &[EntityType],
) -> bool {
    let Some(aggregate) = value_type(function, aggregate) else {
        return false;
    };
    module.type_ownership(aggregate) == Some(model::Ownership::Copyable)
        && module.aggregate_fields(aggregate).is_some_and(|fields| {
            results
                == fields
                    .iter()
                    .copied()
                    .map(EntityType::Value)
                    .collect::<Vec<_>>()
        })
}

pub(super) fn tagged_payload_place_contract(
    module: &Module,
    function: &Function,
    owner: ValueId,
    variant: usize,
    results: &[EntityType],
) -> bool {
    value_type(function, owner)
        .and_then(|tagged| module.tagged_variants(tagged))
        .and_then(|variants| variants.get(variant))
        .is_some_and(|payload| results == [EntityType::Place(*payload)])
}

pub(super) fn heap_payload_place_contract(
    module: &Module,
    function: &Function,
    owner: ValueId,
    results: &[EntityType],
) -> bool {
    value_type(function, owner)
        .and_then(|owner| module.heap_payload(owner))
        .is_some_and(|payload| results == [EntityType::Place(payload)])
}

pub(super) fn heap_field_type(
    module: &Module,
    function: &Function,
    receiver: model::LoanId,
    field: usize,
) -> Option<SsaTypeId> {
    let EntityType::Loan { target, .. } = function.entity(EntityId::Loan(receiver))?.ty else {
        return None;
    };
    module
        .heap_payload(target)
        .filter(|payload| module.presence_payload_type(*payload).is_none())
        .and_then(|payload| module.aggregate_fields(payload))
        .and_then(|fields| fields.get(field))
        .copied()
}

pub(super) fn inline_field_type(
    module: &Module,
    function: &Function,
    receiver: model::LoanId,
    field: usize,
) -> Option<SsaTypeId> {
    let EntityType::Loan { target, .. } = function.entity(EntityId::Loan(receiver))?.ty else {
        return None;
    };
    module
        .presence_payload_type(target)
        .is_none()
        .then_some(target)
        .and_then(|target| module.aggregate_fields(target))
        .and_then(|fields| fields.get(field))
        .copied()
}

pub(super) fn shared_payload_place_contract(
    module: &Module,
    function: &Function,
    owner: EntityId,
    results: &[EntityType],
) -> bool {
    let EntityId::Value(owner) = owner else {
        return false;
    };
    value_type(function, owner)
        .and_then(|owner| module.shared_payload(owner))
        .is_some_and(|payload| results == [EntityType::Place(payload)])
}

pub(super) fn field_place_contract(
    module: &Module,
    function: &Function,
    base: model::PlaceId,
    field: usize,
    results: &[EntityType],
) -> bool {
    place_type(function, base)
        .filter(|aggregate| module.presence_payload_type(*aggregate).is_none())
        .and_then(|aggregate| module.aggregate_fields(aggregate))
        .and_then(|fields| fields.get(field))
        .is_some_and(|field| results == [EntityType::Place(*field)])
}
