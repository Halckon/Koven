//! Typed SSA operation 的局部类型契约。

mod storage;
mod text;
pub(in crate::ssa) use storage::aggregate_explode_contract;
use storage::{
    aggregate_construct_contract, aggregate_copy_explode_contract, aggregate_project_contract,
    field_exchange_contract, field_place_contract, heap_allocate_contract, heap_field_type,
    heap_payload_place_contract, inline_field_type, nullable_take_contract, root_exchange_contract,
    root_place_owner, shared_allocate_contract, shared_payload_place_contract,
    tagged_construct_contract, tagged_payload_place_contract,
};

use text::string_binary_contract;

use super::borrow_result_support::verify_range as range;
use super::verify_ownership::root_exchange::integer;
use callable::{
    callable_invoke_contract, closure_construct_contract, direct_call_contract,
    function_address_contract,
};
pub(in crate::ssa) use integer::verify_borrow_result;
use integer::{constant_contract, map};
use range::nullable;
pub(super) use range::{
    callable, parameter as range_parameter, return_contract as range_return_contract,
    verify_signature as range_verify_signature,
};

use super::{
    model::{
        BinaryOperator, ComparisonOperator, EntityId, EntityType, Function, Instruction, LoanKind,
        Module, Operation, Ownership, PlaceAccess, SsaTypeId, SsaTypeKind, ValueId,
    },
    verify::{VerifyError, VerifyErrorKind, VerifyLocation},
};

pub(super) fn verify_operation(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    errors: &mut Vec<VerifyError>,
) {
    let results = instruction
        .results
        .iter()
        .map(|entity| entity_type(function, *entity))
        .collect::<Vec<_>>();
    let valid = match &instruction.operation {
        Operation::Constant(constant) => constant_contract(module, constant, &results),
        Operation::PrintLiteral { bytes } => results.is_empty() && bytes.last() == Some(&b'\n'),
        Operation::StringLiteral { string, bytes } => {
            is_string_owner(module, *string)
                && bytes.len() as u64 <= lang_frontend::type_checking::MAX_STRING_BYTE_LENGTH
                && std::str::from_utf8(bytes).is_ok()
                && single_value_result(&results) == Some(*string)
        }
        Operation::StringConcat { left, right } => {
            string_binary_contract(module, function, *left, *right, &results, false)
        }
        Operation::StringClone { source } => shared_string_loan_type(module, function, *source)
            .is_some_and(|ty| single_value_result(&results) == Some(ty)),
        Operation::StringByteLength { source } => {
            shared_string_loan_type(module, function, *source).is_some()
                && single_value_result(&results).is_some_and(|ty| is_koven_int(module, ty))
        }
        Operation::StringByteAt { source, index } => {
            shared_string_loan_type(module, function, *source).is_some()
                && value_type(function, *index).is_some_and(|ty| is_koven_int(module, ty))
                && single_value_result(&results).is_some_and(|ty| {
                    matches!(
                        module.type_kind(ty),
                        Some(SsaTypeKind::Integer {
                            bits: 8,
                            signed: false
                        })
                    )
                })
        }
        Operation::Utf8ErrorOffset { source } => {
            integer::shared_byte_array(module, function, *source)
                && single_value_result(&results).is_some_and(|ty| is_koven_int(module, ty))
        }
        Operation::StringCopyBytes { source } => {
            integer::shared_byte_array(module, function, *source)
                && single_value_result(&results).is_some_and(|ty| is_string_owner(module, ty))
        }
        Operation::StringCopySlice { source, start, end } => {
            shared_string_loan_type(module, function, *source)
                .is_some_and(|ty| single_value_result(&results) == Some(ty))
                && [start, end].iter().all(|index| {
                    value_type(function, **index).is_some_and(|ty| is_koven_int(module, ty))
                })
        }
        Operation::StringEqual { left, right } => {
            string_binary_contract(module, function, *left, *right, &results, true)
        }
        Operation::PrintString { value } => {
            results.is_empty() && shared_string_loan_type(module, function, *value).is_some()
        }
        Operation::Binary {
            operator,
            left,
            right,
        } => binary_contract(module, function, *operator, *left, *right, &results),
        Operation::CheckedArithmetic { left, right, .. } => {
            checked_arithmetic_contract(module, function, *left, *right, &results)
        }
        Operation::IntegerRangeConstruct { first, bound, .. } => {
            integer::range_construct(module, function, *first, *bound, &results)
        }
        Operation::IntegerRangeContains { value, range } => {
            integer::range_contains(module, function, *value, *range, &results)
        }
        Operation::IntegerRangeEqual { left, right } => {
            integer::range_equal(module, function, *left, *right, &results)
        }
        Operation::CheckedIntegerConvert { operand } => {
            integer::checked_conversion_contract(module, function, *operand, &results)
        }
        Operation::IntegerBitwise { left, right, .. } => {
            let left = value_type(function, *left);
            left == value_type(function, *right)
                && is_bitwise_integer(module, left)
                && single_value_result(&results) == left
        }
        Operation::IntegerNot { operand } => {
            let operand = value_type(function, *operand);
            is_bitwise_integer(module, operand) && single_value_result(&results) == operand
        }
        Operation::Compare {
            operator,
            left,
            right,
        } => comparison_contract(module, function, *operator, *left, *right, &results),
        Operation::BooleanNot { operand } => {
            value_type(function, *operand).is_some_and(|ty| is_boolean(module, ty))
                && single_value_result(&results).is_some_and(|ty| is_boolean(module, ty))
        }
        Operation::DirectCall { .. } => {
            direct_call_contract(module, function, instruction, &results)
        }
        Operation::FunctionAddress { target } => {
            function_address_contract(module, *target, &results)
        }
        Operation::BorrowCall {
            callee,
            arguments,
            source,
        } => super::verify_borrow_result::call_contract(
            module, function, *callee, arguments, *source, &results,
        ),
        Operation::RangeConstruct { .. }
        | Operation::RangeCall { .. }
        | Operation::RangeLength { .. }
        | Operation::RangeElementPlace { .. }
        | Operation::RangeEnd { .. } => {
            range::contract(module, function, &instruction.operation, &results)
        }
        Operation::ClosureConstruct {
            closure,
            thunk,
            captures,
        } => closure_construct_contract(module, function, *closure, *thunk, captures, &results),
        Operation::CallableInvoke {
            callable,
            arguments,
        } => callable_invoke_contract(module, function, *callable, arguments, &results),
        Operation::CallableViewOf { source } => {
            callable::view_of_contract(module, function, *source, &results)
        }
        Operation::CallableInvokeView { view, arguments } => {
            callable::invoke_view_contract(module, function, *view, arguments, &results)
        }
        Operation::AggregateConstruct { aggregate, fields } => {
            aggregate_construct_contract(module, function, *aggregate, fields, &results)
        }
        Operation::AggregateProject { aggregate, field } => {
            aggregate_project_contract(module, function, *aggregate, *field, &results)
        }
        Operation::AggregateExplode { aggregate } => {
            aggregate_explode_contract(module, function, *aggregate, &results)
        }
        Operation::AggregateCopyExplode { aggregate } => {
            aggregate_copy_explode_contract(module, function, *aggregate, &results)
        }
        Operation::TaggedConstruct {
            tagged,
            variant,
            payload,
        } => tagged_construct_contract(module, function, *tagged, *variant, *payload, &results),
        Operation::TaggedPayloadPlace { owner, variant } => {
            tagged_payload_place_contract(module, function, *owner, *variant, &results)
        }
        Operation::TaggedDiscriminant { owner } => {
            value_type(function, *owner).is_some_and(|ty| {
                matches!(module.type_kind(ty), Some(SsaTypeKind::TaggedUnion { .. }))
            }) && single_value_result(&results).is_some_and(|ty| {
                matches!(
                    module.type_kind(ty),
                    Some(SsaTypeKind::Integer {
                        bits: 32,
                        signed: true
                    })
                )
            })
        }
        Operation::HeapAllocate { owner, payload } => {
            heap_allocate_contract(module, function, *owner, *payload, &results)
        }
        Operation::HeapPayloadPlace { owner } => {
            heap_payload_place_contract(module, function, *owner, &results)
        }
        Operation::HeapFieldRead { receiver, field } => {
            heap_field_type(module, function, *receiver, *field).is_some_and(|field| {
                module.type_ownership(field) == Some(Ownership::Copyable)
                    && results == [EntityType::Value(field)]
            })
        }
        Operation::HeapFieldReplace {
            receiver,
            field,
            value,
        } => {
            results.is_empty()
                && matches!(
                    function
                        .entity(EntityId::Loan(*receiver))
                        .map(|entity| entity.ty),
                    Some(EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        ..
                    })
                )
                && heap_field_type(module, function, *receiver, *field).is_some_and(|field| {
                    matches!(
                        module.type_ownership(field),
                        Some(Ownership::Copyable | Ownership::MoveOnly)
                    ) && value_type(function, *value) == Some(field)
                })
        }
        Operation::HeapFieldExchange {
            owner,
            field,
            loan,
            replacement,
        } => value_type(function, *owner)
            .and_then(|owner| module.heap_payload(owner))
            .filter(|payload| module.presence_payload_type(*payload).is_none())
            .and_then(|payload| module.aggregate_fields(payload))
            .and_then(|fields| fields.get(*field).copied())
            .is_some_and(|field| {
                field_exchange_contract(module, function, field, *loan, *replacement, &results)
            }),
        Operation::HeapReceiverFieldExchange { loan, replacement }
        | Operation::InoutExchange { loan, replacement } => field_exchange_contract(
            module,
            function,
            entity_type(function, EntityId::Loan(*loan)).semantic_type(),
            *loan,
            *replacement,
            &results,
        ),
        Operation::InlineFieldReplace {
            receiver,
            field,
            value,
        } => {
            results.is_empty()
                && matches!(
                    function
                        .entity(EntityId::Loan(*receiver))
                        .map(|entity| entity.ty),
                    Some(EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        ..
                    })
                )
                && inline_field_type(module, function, *receiver, *field).is_some_and(|field| {
                    matches!(
                        module.type_ownership(field),
                        Some(Ownership::Copyable | Ownership::MoveOnly)
                    ) && value_type(function, *value) == Some(field)
                })
        }
        Operation::SharedAllocate { owner, payload } => {
            shared_allocate_contract(module, function, *owner, *payload, &results)
        }
        Operation::SharedRetain { owner } => {
            shared_owner_type(function, *owner).is_some_and(|ty| {
                module.shared_payload(ty).is_some() && single_value_result(&results) == Some(ty)
            })
        }
        Operation::SharedPayloadPlace { owner } => {
            shared_payload_place_contract(module, function, *owner, &results)
        }
        Operation::SharedPayloadLoan { source } => {
            matches!(
                entity_type(function, EntityId::Loan(*source)),
                EntityType::Loan { kind: LoanKind::Shared, target }
                    if module.shared_payload(target).is_some_and(|payload| {
                        results == [EntityType::Loan { kind: LoanKind::Shared, target: payload }]
                    })
            )
        }
        Operation::NullableWrap { nullable, owner } => {
            module.nullable_inner(*nullable) == value_type(function, *owner)
                && single_value_result(&results) == Some(*nullable)
        }
        Operation::NullableNull { nullable } => {
            module.nullable_inner(*nullable).is_some()
                && single_value_result(&results) == Some(*nullable)
        }
        Operation::NullableIsNull { owner } => {
            value_type(function, *owner).is_some_and(|ty| module.nullable_inner(ty).is_some())
                && single_value_result(&results).is_some_and(|ty| is_boolean(module, ty))
        }
        Operation::NullableLoanIsNull { source } => {
            storage::nullable_loan_presence_contract(module, function, *source, &results)
        }
        Operation::NullableTake { owner, proof } => {
            nullable_take_contract(module, function, *owner, *proof, &results)
        }
        Operation::ContainerConstruct {
            container,
            elements,
        } => container_construct_contract(module, function, *container, elements, &results),
        Operation::ContainerGenerate {
            container,
            length,
            initializer,
        } => container_generate_contract(
            module,
            function,
            *container,
            *length,
            *initializer,
            &results,
        ),
        Operation::ContainerGenerateBorrowed {
            container,
            length,
            initializer,
        } => integer::borrowed_generate_contract(
            module,
            function,
            *container,
            *length,
            *initializer,
            &results,
        ),
        Operation::ContainerLength { owner } => {
            container_length_contract(module, function, *owner, &results)
        }
        Operation::ContainerElementPlace { owner, index } => {
            container_element_place_contract(module, function, *owner, *index, &results)
        }
        Operation::ProjectedSwap { root, loans } => {
            storage::element_swap_contract(function, *root, *loans, &results)
        }
        Operation::ContainerAppend { owner, element } => {
            container_append_contract(module, function, *owner, *element, &results)
        }
        Operation::ContainerAppendInout { loan, element } => {
            results.is_empty()
                && matches!(
                    entity_type(function, EntityId::Loan(*loan)),
                    EntityType::Loan { kind: LoanKind::Exclusive, target }
                        if module.sequential_container(target).is_some_and(|(kind, ty)|
                            kind == super::model::SequentialContainerKind::MutableList
                                && value_type(function, *element) == Some(ty))
                )
        }
        Operation::ContainerClear { owner } => {
            container_clear_contract(module, function, *owner, &results)
        }
        Operation::ContainerRemoveAt { owner, index } => {
            container_remove_at_contract(module, function, *owner, *index, &results)
        }
        Operation::ContainerRemoveFirst { owner } | Operation::ContainerRemoveLast { owner } => {
            container_remove_endpoint_contract(module, function, *owner, &results)
        }
        Operation::ContainerInsertAt {
            owner,
            index,
            element,
        } => container_insert_at_contract(module, function, *owner, *index, *element, &results),
        Operation::ContainerReplace {
            owner,
            index,
            value,
        } => container_replace_contract(module, function, *owner, *index, *value, &results),
        Operation::MapIterationFirst { .. }
        | Operation::MapIterationHasNext { .. }
        | Operation::MapIterationNext { .. }
        | Operation::MapIterationKeyLoan { .. }
        | Operation::MapIterationValueLoan { .. }
        | Operation::MapConstruct { .. }
        | Operation::MapConsume { .. }
        | Operation::MapSize { .. }
        | Operation::MapContains { .. }
        | Operation::MapGet { .. }
        | Operation::MapRequireValue { .. }
        | Operation::MapWithValue { .. }
        | Operation::MapResultUnwrap { .. }
        | Operation::MapPut { .. }
        | Operation::MapRemove { .. } => {
            map::verify_map_operation(module, function, &instruction.operation, &results)
        }
        Operation::FieldPlace { base, field } => {
            field_place_contract(module, function, *base, *field, &results)
        }
        Operation::SharedFieldLoan { base, field } => {
            let Some(EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }) = function
                .entity(EntityId::Loan(*base))
                .map(|entity| entity.ty)
            else {
                return;
            };
            module.presence_payload_type(target).is_none()
                && module
                    .aggregate_fields(target)
                    .and_then(|fields| fields.get(*field))
                    .is_some_and(|field_ty| {
                        results
                            == [EntityType::Loan {
                                kind: LoanKind::Shared,
                                target: *field_ty,
                            }]
                    })
        }
        Operation::SharedHeapFieldLoan { base, field } => {
            matches!(
                function
                    .entity(EntityId::Loan(*base))
                    .map(|entity| entity.ty),
                Some(EntityType::Loan {
                    kind: LoanKind::Shared | LoanKind::Exclusive,
                    ..
                })
            ) && heap_field_type(module, function, *base, *field).is_some_and(|field_ty| {
                results
                    == [EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: field_ty,
                    }]
            })
        }
        Operation::ExclusiveHeapFieldLoan { base, field } => {
            matches!(
                entity_type(function, EntityId::Loan(*base)),
                EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    ..
                }
            ) && heap_field_type(module, function, *base, *field).is_some_and(|target| {
                results
                    == [EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        target,
                    }]
            })
        }
        Operation::ExclusiveRootReborrow { source } => {
            storage::exclusive_reborrow_contract(function, *source, &results)
        }
        Operation::SharedReborrow { source } => {
            match (
                function
                    .entity(EntityId::Loan(*source))
                    .map(|entity| entity.ty),
                results.as_slice(),
            ) {
                (
                    Some(EntityType::Loan { target: actual, .. }),
                    [
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: expected,
                        },
                    ],
                ) => {
                    actual == *expected
                        || (matches!(module.type_kind(actual), Some(SsaTypeKind::StringOwner))
                            && matches!(
                                module.type_kind(*expected),
                                Some(SsaTypeKind::InlineNullable { inner, niche: true }) if *inner == actual
                            ))
                }
                _ => false,
            }
        }
        Operation::SharedReferenceFollow { source } => {
            matches!(
                function.entity(EntityId::Loan(*source)).map(|entity| entity.ty),
                Some(EntityType::Loan { kind: LoanKind::Shared, target: reference })
                    if matches!(module.type_kind(reference),
                        Some(SsaTypeKind::SharedReference { target })
                            if results == [EntityType::Loan {
                                kind: LoanKind::Shared,
                                target: *target,
                            }])
            )
        }
        Operation::Copy { source } => {
            single_value_result(&results) == value_type(function, *source)
        }
        Operation::Consume { owner } | Operation::Drop { owner } => {
            results.is_empty()
                && !matches!(
                    value_type(function, *owner).and_then(|ty| module.type_kind(ty)),
                    Some(SsaTypeKind::RangeView { .. })
                )
        }
        Operation::BorrowEnd { .. } => results.is_empty(),
        Operation::RootPlace { owner } => {
            results
                == [EntityType::Place(
                    value_type(function, *owner).expect("valid value"),
                )]
        }
        Operation::RootPlaceTake { owner, place } => {
            root_place_owner(function, *place) == Some(*owner)
                && module.type_ownership(value_type(function, *owner).expect("valid value"))
                    == Some(Ownership::MoveOnly)
                && single_value_result(&results) == value_type(function, *owner)
        }
        Operation::RootReplace {
            owner,
            loan,
            replacement,
        } => {
            root_exchange_contract(module, function, &[*owner], &[*loan], &results)
                && value_type(function, *owner) == value_type(function, *replacement)
        }
        Operation::RootSwap { owners, loans } => {
            root_exchange_contract(module, function, owners, loans, &results)
        }
        Operation::BorrowBegin { place, kind } => {
            let target = entity_type(function, EntityId::Place(*place)).semantic_type();
            results
                == [EntityType::Loan {
                    kind: *kind,
                    target,
                }]
        }
        Operation::Read { source } => {
            let source = match source {
                PlaceAccess::Place(place) => entity_type(function, EntityId::Place(*place)),
                PlaceAccess::Loan(loan) => entity_type(function, EntityId::Loan(*loan)),
            };
            single_value_result(&results) == Some(source.semantic_type())
        }
        Operation::Mutate { place, value } => {
            results.is_empty()
                && entity_type(function, EntityId::Place(*place)).semantic_type()
                    == value_type(function, *value).expect("valid value")
        }
        Operation::LoanMutate { loan, value } => {
            results.is_empty()
                && matches!(
                    entity_type(function, EntityId::Loan(*loan)),
                    EntityType::Loan { kind: LoanKind::Exclusive, target }
                        if value_type(function, *value) == Some(target)
                            && is_first_class(module, target)
                            && module.type_ownership(target) == Some(super::model::Ownership::Copyable)
                )
        }
        Operation::LoanReplace { loan, value } => {
            results.is_empty()
                && matches!(
                    entity_type(function, EntityId::Loan(*loan)),
                    EntityType::Loan { kind: LoanKind::Exclusive, target }
                        if value_type(function, *value) == Some(target)
                            && is_first_class(module, target)
                            && module.type_ownership(target) == Some(super::model::Ownership::MoveOnly)
                )
        }
    };
    if !valid || !callable::call_termination_contract(module, function, instruction) {
        errors.push(VerifyError {
            kind: VerifyErrorKind::OperationContract {
                reason: "operand and result types do not match the operation contract",
            },
            location: VerifyLocation::Instruction(instruction.id),
            origin: Some(instruction.origin.clone()),
        });
    }
}

fn shared_string_loan_type(
    module: &Module,
    function: &Function,
    loan: super::model::LoanId,
) -> Option<SsaTypeId> {
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = entity_type(function, EntityId::Loan(loan))
    else {
        return None;
    };
    is_string_owner(module, target).then_some(target)
}

fn container_construct_contract(
    module: &Module,
    function: &Function,
    container: SsaTypeId,
    elements: &[ValueId],
    results: &[EntityType],
) -> bool {
    module
        .sequential_container(container)
        .is_some_and(|(_, element)| {
            single_value_result(results) == Some(container)
                && elements
                    .iter()
                    .all(|value| value_type(function, *value) == Some(element))
        })
}

fn container_generate_contract(
    module: &Module,
    function: &Function,
    container: SsaTypeId,
    length: ValueId,
    initializer: super::model::FunctionId,
    results: &[EntityType],
) -> bool {
    let Some((_, element)) = module.sequential_container(container) else {
        return false;
    };
    let Some(initializer) = module.function(initializer) else {
        return false;
    };
    let Some(entry) = initializer.blocks.first() else {
        return false;
    };
    value_type(function, length).is_some_and(|ty| is_koven_int(module, ty))
        && single_value_result(results) == Some(container)
        && initializer.return_types == [element]
        && entry.parameters.len() == 1
        && entry.parameters.first().is_some_and(|parameter| {
            matches!(initializer.entity(*parameter).map(|data| data.ty), Some(EntityType::Value(ty)) if is_koven_int(module, ty))
        })
}

fn container_length_contract(
    module: &Module,
    function: &Function,
    owner: EntityId,
    results: &[EntityType],
) -> bool {
    let owner_type = match function.entity(owner).map(|entity| entity.ty) {
        Some(EntityType::Value(ty))
        | Some(EntityType::Loan {
            kind: LoanKind::Shared,
            target: ty,
        }) => Some(ty),
        _ => None,
    };
    owner_type
        .and_then(|ty| module.sequential_container(ty))
        .is_some()
        && single_value_result(results).is_some_and(|ty| is_koven_int(module, ty))
}

fn container_element_place_contract(
    module: &Module,
    function: &Function,
    owner: EntityId,
    index: ValueId,
    results: &[EntityType],
) -> bool {
    let owner_type = match function.entity(owner).map(|entity| entity.ty) {
        Some(EntityType::Value(ty)) | Some(EntityType::Loan { target: ty, .. }) => Some(ty),
        _ => None,
    };
    let Some((container, element)) =
        owner_type.and_then(|container| module.sequential_container(container))
    else {
        return false;
    };
    if matches!(
        function.entity(owner).map(|entity| entity.ty),
        Some(EntityType::Loan {
            kind: LoanKind::Exclusive,
            ..
        })
    ) && container == super::model::SequentialContainerKind::List
    {
        return false;
    }
    value_type(function, index).is_some_and(|ty| is_koven_int(module, ty))
        && results == [EntityType::Place(element)]
}

fn mutable_list_element_type(
    module: &Module,
    function: &Function,
    owner: ValueId,
) -> Option<SsaTypeId> {
    let (kind, elem_ty) =
        value_type(function, owner).and_then(|container| module.sequential_container(container))?;
    (kind == super::model::SequentialContainerKind::MutableList).then_some(elem_ty)
}

fn container_append_contract(
    module: &Module,
    function: &Function,
    owner: ValueId,
    element: ValueId,
    results: &[EntityType],
) -> bool {
    mutable_list_element_type(module, function, owner) == value_type(function, element)
        && single_value_result(results) == value_type(function, owner)
}

fn container_clear_contract(
    module: &Module,
    function: &Function,
    owner: ValueId,
    results: &[EntityType],
) -> bool {
    mutable_list_element_type(module, function, owner).is_some()
        && single_value_result(results) == value_type(function, owner)
}

fn container_remove_at_contract(
    module: &Module,
    function: &Function,
    owner: ValueId,
    index: ValueId,
    results: &[EntityType],
) -> bool {
    let Some(elem_ty) = mutable_list_element_type(module, function, owner) else {
        return false;
    };
    let Some(owner_type) = value_type(function, owner) else {
        return false;
    };
    value_type(function, index).is_some_and(|ty| is_koven_int(module, ty))
        && results == [EntityType::Value(elem_ty), EntityType::Value(owner_type)]
}

fn container_insert_at_contract(
    module: &Module,
    function: &Function,
    owner: ValueId,
    index: ValueId,
    element: ValueId,
    results: &[EntityType],
) -> bool {
    mutable_list_element_type(module, function, owner) == value_type(function, element)
        && value_type(function, index).is_some_and(|ty| is_koven_int(module, ty))
        && single_value_result(results) == value_type(function, owner)
}

fn container_remove_endpoint_contract(
    module: &Module,
    function: &Function,
    owner: ValueId,
    results: &[EntityType],
) -> bool {
    let Some((kind, elem_ty)) =
        value_type(function, owner).and_then(|container| module.sequential_container(container))
    else {
        return false;
    };
    let Some(owner_type) = value_type(function, owner) else {
        return false;
    };
    kind == super::model::SequentialContainerKind::MutableList
        && results == [EntityType::Value(elem_ty), EntityType::Value(owner_type)]
}

fn container_replace_contract(
    module: &Module,
    function: &Function,
    owner: ValueId,
    index: ValueId,
    value: ValueId,
    results: &[EntityType],
) -> bool {
    let Some((kind, element)) =
        value_type(function, owner).and_then(|container| module.sequential_container(container))
    else {
        return false;
    };
    results.is_empty()
        && kind.elements_are_mutable()
        && value_type(function, index).is_some_and(|ty| is_koven_int(module, ty))
        && value_type(function, value) == Some(element)
}

fn binary_contract(
    module: &Module,
    function: &Function,
    operator: BinaryOperator,
    left: ValueId,
    right: ValueId,
    results: &[EntityType],
) -> bool {
    let left = value_type(function, left);
    let right = value_type(function, right);
    let Some(result) = single_value_result(results) else {
        return false;
    };
    if left != right {
        return false;
    }
    match operator {
        BinaryOperator::Add | BinaryOperator::Subtract | BinaryOperator::Multiply => {
            left == Some(result)
                && matches!(module.type_kind(result), Some(SsaTypeKind::Integer { .. }))
        }
        BinaryOperator::Equal => is_equality_type(module, left) && is_boolean(module, result),
        BinaryOperator::LessThan => is_integer(module, left) && is_boolean(module, result),
    }
}

fn checked_arithmetic_contract(
    module: &Module,
    function: &Function,
    left: ValueId,
    right: ValueId,
    results: &[EntityType],
) -> bool {
    let left = value_type(function, left);
    let right = value_type(function, right);
    let [EntityType::Value(result), EntityType::Value(failed)] = results else {
        return false;
    };
    left == right
        && left == Some(*result)
        && is_integer(module, left)
        && is_boolean(module, *failed)
}

fn comparison_contract(
    module: &Module,
    function: &Function,
    operator: ComparisonOperator,
    left: ValueId,
    right: ValueId,
    results: &[EntityType],
) -> bool {
    let left = value_type(function, left);
    let right = value_type(function, right);
    let Some(result) = single_value_result(results) else {
        return false;
    };
    if left != right || !is_boolean(module, result) {
        return false;
    }
    match operator {
        ComparisonOperator::Equal | ComparisonOperator::NotEqual => is_equality_type(module, left),
        ComparisonOperator::LessThan
        | ComparisonOperator::LessThanOrEqual
        | ComparisonOperator::GreaterThan
        | ComparisonOperator::GreaterThanOrEqual => is_integer(module, left),
    }
}

pub(in crate::ssa) fn integer_fits(value: i128, bits: u16, signed: bool) -> bool {
    if !(1..=128).contains(&bits) {
        return false;
    }
    if signed {
        if bits == 128 {
            true
        } else {
            let limit = 1_i128 << (bits - 1);
            (-limit..limit).contains(&value)
        }
    } else if value < 0 {
        false
    } else if bits == 128 {
        true
    } else {
        value < (1_i128 << bits)
    }
}

pub(super) fn is_first_class(module: &Module, ty: SsaTypeId) -> bool {
    matches!(
        module.type_kind(ty),
        Some(
            SsaTypeKind::Boolean
                | SsaTypeKind::Char
                | SsaTypeKind::Integer { .. }
                | SsaTypeKind::Aggregate { .. }
                | SsaTypeKind::TaggedUnion { .. }
                | SsaTypeKind::HeapOwner { .. }
                | SsaTypeKind::SharedOwner { .. }
                | SsaTypeKind::StringOwner
                | SsaTypeKind::NullableHandle { .. }
                | SsaTypeKind::InlineNullable { .. }
                | SsaTypeKind::RangeView { .. }
                | SsaTypeKind::SequentialContainer { .. }
                | SsaTypeKind::MapContainer { .. }
                | SsaTypeKind::ZeroSized { .. }
                | SsaTypeKind::SharedReference { .. }
                | SsaTypeKind::FunctionPointer { .. }
                | SsaTypeKind::ConcreteClosure { .. }
                // Admitted only as a shared loan target; entity rules reject view values.
                | SsaTypeKind::CallableView { .. }
        )
    )
}

pub(in crate::ssa) fn is_koven_int(module: &Module, ty: SsaTypeId) -> bool {
    matches!(
        module.type_kind(ty),
        Some(SsaTypeKind::Integer {
            bits: 32,
            signed: true
        })
    )
}

fn is_scalar(module: &Module, ty: SsaTypeId) -> bool {
    matches!(
        module.type_kind(ty),
        Some(SsaTypeKind::Boolean | SsaTypeKind::Char | SsaTypeKind::Integer { .. })
    )
}

pub(in crate::ssa) fn is_bitwise_integer(module: &Module, ty: Option<SsaTypeId>) -> bool {
    ty.is_some_and(|ty| {
        matches!(
            module.type_kind(ty),
            Some(SsaTypeKind::Integer {
                bits: 8 | 16 | 32 | 64,
                ..
            })
        )
    })
}

fn is_integer(module: &Module, ty: Option<SsaTypeId>) -> bool {
    ty.is_some_and(|ty| matches!(module.type_kind(ty), Some(SsaTypeKind::Integer { .. })))
}

fn is_equality_type(module: &Module, ty: Option<SsaTypeId>) -> bool {
    ty.is_some_and(|ty| {
        is_scalar(module, ty)
            || module
                .map_result_value(ty)
                .is_some_and(|value| is_scalar(module, value))
    })
}

pub(in crate::ssa) fn is_boolean(module: &Module, ty: SsaTypeId) -> bool {
    matches!(module.type_kind(ty), Some(SsaTypeKind::Boolean))
}

fn is_string_owner(module: &Module, ty: SsaTypeId) -> bool {
    matches!(module.type_kind(ty), Some(SsaTypeKind::StringOwner))
}

pub(in crate::ssa) fn single_value_result(types: &[EntityType]) -> Option<SsaTypeId> {
    match types {
        [EntityType::Value(ty)] => Some(*ty),
        _ => None,
    }
}

pub(in crate::ssa) fn value_type(function: &Function, value: ValueId) -> Option<SsaTypeId> {
    match entity_type(function, EntityId::Value(value)) {
        EntityType::Value(ty) => Some(ty),
        EntityType::Place(_) | EntityType::Loan { .. } => None,
    }
}

fn shared_owner_type(function: &Function, owner: EntityId) -> Option<SsaTypeId> {
    match entity_type(function, owner) {
        EntityType::Value(ty) | EntityType::Loan { target: ty, .. } => Some(ty),
        EntityType::Place(_) => None,
    }
}

fn place_type(function: &Function, place: super::model::PlaceId) -> Option<SsaTypeId> {
    match entity_type(function, EntityId::Place(place)) {
        EntityType::Place(ty) => Some(ty),
        EntityType::Value(_) | EntityType::Loan { .. } => None,
    }
}

pub(in crate::ssa) fn entity_type(function: &Function, entity: EntityId) -> EntityType {
    function
        .entity(entity)
        .expect("structure phase proved entity existence")
        .ty
}
