//! Typed SSA operation 的局部类型契约。

use super::{
    model::{
        BinaryOperator, CallableSignature, ClosureCaptureMode, ClosureCaptureOperand,
        ComparisonOperator, EntityId, EntityType, Function, FunctionId, Instruction, LoanKind,
        Module, Operation, Ownership, PlaceAccess, ScalarConstant, SsaTypeId, SsaTypeKind, ValueId,
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
                && std::str::from_utf8(bytes).is_ok()
                && single_value_result(&results) == Some(*string)
        }
        Operation::StringConcat { left, right } => {
            string_binary_contract(module, function, *left, *right, &results, false)
        }
        Operation::StringClone { source } => shared_string_loan_type(module, function, *source)
            .is_some_and(|ty| single_value_result(&results) == Some(ty)),
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
        Operation::DirectCall {
            callee,
            receiver,
            arguments,
        } => direct_call_contract(module, function, *callee, *receiver, arguments, &results),
        Operation::FunctionAddress { target } => {
            function_address_contract(module, *target, &results)
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
        Operation::ContainerLength { owner } => {
            container_length_contract(module, function, *owner, &results)
        }
        Operation::ContainerElementPlace { owner, index } => {
            container_element_place_contract(module, function, *owner, *index, &results)
        }
        Operation::ContainerReplace {
            owner,
            index,
            value,
        } => container_replace_contract(module, function, *owner, *index, *value, &results),
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
            module
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
                    kind: LoanKind::Shared,
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
        Operation::SharedReborrow { source } => {
            matches!(
                function.entity(EntityId::Loan(*source)).map(|entity| entity.ty),
                Some(EntityType::Loan { target, .. }) if results == [EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }]
            )
        }
        Operation::Copy { source } => {
            single_value_result(&results) == value_type(function, *source)
        }
        Operation::Consume { .. } | Operation::BorrowEnd { .. } | Operation::Drop { .. } => {
            results.is_empty()
        }
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
    };
    if !valid {
        errors.push(VerifyError {
            kind: VerifyErrorKind::OperationContract {
                reason: "operand and result types do not match the operation contract",
            },
            location: VerifyLocation::Instruction(instruction.id),
            origin: Some(instruction.origin.clone()),
        });
    }
}

fn root_exchange_contract(
    module: &Module,
    function: &Function,
    owners: &[ValueId],
    loans: &[super::model::LoanId],
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

fn root_place_owner(
    function: &Function,
    place: super::model::PlaceId,
) -> Option<super::model::ValueId> {
    let super::model::Definition::InstructionResult {
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

fn aggregate_construct_contract(
    module: &Module,
    function: &Function,
    aggregate: SsaTypeId,
    fields: &[ValueId],
    results: &[EntityType],
) -> bool {
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

fn string_binary_contract(
    module: &Module,
    function: &Function,
    left: EntityId,
    right: EntityId,
    results: &[EntityType],
    returns_boolean: bool,
) -> bool {
    let Some(left) = string_view_type(module, function, left) else {
        return false;
    };
    if string_view_type(module, function, right) != Some(left) {
        return false;
    }
    if returns_boolean {
        single_value_result(results).is_some_and(|ty| is_boolean(module, ty))
    } else {
        single_value_result(results) == Some(left)
    }
}

fn string_view_type(module: &Module, function: &Function, operand: EntityId) -> Option<SsaTypeId> {
    let ty = match entity_type(function, operand) {
        EntityType::Value(ty) => ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        } => target,
        EntityType::Place(_) | EntityType::Loan { .. } => return None,
    };
    is_string_owner(module, ty).then_some(ty)
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

fn nullable_take_contract(
    module: &Module,
    function: &Function,
    owner: ValueId,
    proof: super::model::LoanId,
    results: &[EntityType],
) -> bool {
    let Some(nullable) = value_type(function, owner) else {
        return false;
    };
    let Some(inner) = module.nullable_inner(nullable) else {
        return false;
    };
    let is_branch_proof = function.blocks.iter().any(|block| {
        matches!(
            block.terminator.as_ref().map(|terminator| &terminator.kind),
            Some(super::model::TerminatorKind::NullableBranch { view, .. }) if *view == proof
        )
    });
    entity_type(function, EntityId::Loan(proof))
        == EntityType::Loan {
            kind: LoanKind::Shared,
            target: inner,
        }
        && is_branch_proof
        && single_value_result(results) == Some(inner)
}

fn aggregate_project_contract(
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
    module.type_ownership(field) == Some(super::model::Ownership::Copyable)
        && single_value_result(results) == Some(field)
}

fn aggregate_explode_contract(
    module: &Module,
    function: &Function,
    aggregate: ValueId,
    results: &[EntityType],
) -> bool {
    let Some(aggregate) = value_type(function, aggregate) else {
        return false;
    };
    module.type_ownership(aggregate) == Some(super::model::Ownership::MoveOnly)
        && module.aggregate_fields(aggregate).is_some_and(|fields| {
            results
                == fields
                    .iter()
                    .copied()
                    .map(EntityType::Value)
                    .collect::<Vec<_>>()
        })
}

fn heap_allocate_contract(
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

fn shared_allocate_contract(
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

fn tagged_construct_contract(
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

fn aggregate_copy_explode_contract(
    module: &Module,
    function: &Function,
    aggregate: ValueId,
    results: &[EntityType],
) -> bool {
    let Some(aggregate) = value_type(function, aggregate) else {
        return false;
    };
    module.type_ownership(aggregate) == Some(super::model::Ownership::Copyable)
        && module.aggregate_fields(aggregate).is_some_and(|fields| {
            results
                == fields
                    .iter()
                    .copied()
                    .map(EntityType::Value)
                    .collect::<Vec<_>>()
        })
}

fn tagged_payload_place_contract(
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

fn heap_payload_place_contract(
    module: &Module,
    function: &Function,
    owner: ValueId,
    results: &[EntityType],
) -> bool {
    value_type(function, owner)
        .and_then(|owner| module.heap_payload(owner))
        .is_some_and(|payload| results == [EntityType::Place(payload)])
}

fn heap_field_type(
    module: &Module,
    function: &Function,
    receiver: super::model::LoanId,
    field: usize,
) -> Option<SsaTypeId> {
    let EntityType::Loan { target, .. } = function.entity(EntityId::Loan(receiver))?.ty else {
        return None;
    };
    module
        .heap_payload(target)
        .and_then(|payload| module.aggregate_fields(payload))
        .and_then(|fields| fields.get(field))
        .copied()
}

fn inline_field_type(
    module: &Module,
    function: &Function,
    receiver: super::model::LoanId,
    field: usize,
) -> Option<SsaTypeId> {
    let EntityType::Loan { target, .. } = function.entity(EntityId::Loan(receiver))?.ty else {
        return None;
    };
    module
        .aggregate_fields(target)
        .and_then(|fields| fields.get(field))
        .copied()
}

fn shared_payload_place_contract(
    module: &Module,
    function: &Function,
    owner: EntityId,
    results: &[EntityType],
) -> bool {
    shared_owner_type(function, owner)
        .and_then(|owner| module.shared_payload(owner))
        .is_some_and(|payload| results == [EntityType::Place(payload)])
}

fn field_place_contract(
    module: &Module,
    function: &Function,
    base: super::model::PlaceId,
    field: usize,
    results: &[EntityType],
) -> bool {
    place_type(function, base)
        .and_then(|aggregate| module.aggregate_fields(aggregate))
        .and_then(|fields| fields.get(field))
        .is_some_and(|field| results == [EntityType::Place(*field)])
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
        Some(EntityType::Value(ty))
        | Some(EntityType::Loan {
            kind: LoanKind::Shared,
            target: ty,
        }) => Some(ty),
        _ => None,
    };
    let Some((_, element)) =
        owner_type.and_then(|container| module.sequential_container(container))
    else {
        return false;
    };
    value_type(function, index).is_some_and(|ty| is_koven_int(module, ty))
        && results == [EntityType::Place(element)]
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

fn constant_contract(module: &Module, constant: &ScalarConstant, results: &[EntityType]) -> bool {
    let Some(result) = single_value_result(results) else {
        return false;
    };
    match (constant, module.type_kind(result)) {
        (ScalarConstant::Unit, Some(SsaTypeKind::Unit))
        | (ScalarConstant::Boolean(_), Some(SsaTypeKind::Boolean)) => true,
        (ScalarConstant::Char(value), Some(SsaTypeKind::Char)) => char::from_u32(*value).is_some(),
        (ScalarConstant::Integer(value), Some(SsaTypeKind::Integer { bits, signed })) => {
            integer_fits(*value, *bits, *signed)
        }
        _ => false,
    }
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

fn direct_call_contract(
    module: &Module,
    function: &Function,
    callee: super::model::FunctionId,
    receiver: Option<EntityId>,
    arguments: &[EntityId],
    results: &[EntityType],
) -> bool {
    let Some(callee) = module.function(callee) else {
        return false;
    };
    let Some(entry) = callee.blocks.first() else {
        return false;
    };
    let Some(parameter_types) = entry
        .parameters
        .iter()
        .map(|parameter| callee.entity(*parameter).map(|data| data.ty))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let receiver_count = usize::from(callee.receiver.is_some());
    if parameter_types.len() != arguments.len() + receiver_count
        || callee.receiver.is_some() != receiver.is_some()
        || !parameter_types.iter().all(|ty| match ty {
            EntityType::Value(ty) => is_first_class(module, *ty),
            EntityType::Loan { target, .. } => is_first_class(module, *target),
            EntityType::Place(_) => false,
        })
        || !callee
            .return_types
            .iter()
            .all(|ty| is_first_class(module, *ty))
    {
        return false;
    }
    let receiver_matches = receiver
        .zip(callee.receiver)
        .is_none_or(|(receiver, expected)| {
            !matches!(receiver, EntityId::Place(_))
                && function.entity(receiver).map(|entity| entity.ty) == Some(expected)
        });
    let arguments_match = arguments
        .iter()
        .zip(parameter_types.into_iter().skip(receiver_count))
        .all(|(argument, parameter)| {
            !matches!(argument, EntityId::Place(_))
                && function.entity(*argument).map(|entity| entity.ty) == Some(parameter)
        });
    let results_match = results
        == callee
            .return_types
            .iter()
            .copied()
            .map(EntityType::Value)
            .collect::<Vec<_>>();
    receiver_matches && arguments_match && results_match
}

fn function_address_contract(module: &Module, target: FunctionId, results: &[EntityType]) -> bool {
    let Some(result) = single_value_result(results) else {
        return false;
    };
    let Some(SsaTypeKind::FunctionPointer { signature }) = module.type_kind(result) else {
        return false;
    };
    function_matches_signature(module, target, signature, None)
}

fn closure_construct_contract(
    module: &Module,
    function: &Function,
    closure: SsaTypeId,
    thunk: FunctionId,
    operands: &[ClosureCaptureOperand],
    results: &[EntityType],
) -> bool {
    let Some(SsaTypeKind::ConcreteClosure {
        signature,
        environment,
        captures,
        ..
    }) = module.type_kind(closure)
    else {
        return false;
    };
    if single_value_result(results) != Some(closure) || operands.len() != captures.len() {
        return false;
    }
    let captures_match = operands.iter().zip(captures).all(|(operand, capture)| {
        match (operand, capture.mode) {
            (ClosureCaptureOperand::Owned(value), ClosureCaptureMode::Owned) => {
                value_type(function, *value) == Some(capture.ty)
            }
            (ClosureCaptureOperand::Shared(loan), ClosureCaptureMode::Shared) => matches!(
                function.entity(EntityId::Loan(*loan)).map(|entity| entity.ty),
                Some(EntityType::Loan { kind: LoanKind::Shared, target }) if target == capture.ty
            ),
            _ => false,
        }
    });
    captures_match && function_matches_signature(module, thunk, signature, Some(*environment))
}

fn callable_invoke_contract(
    module: &Module,
    function: &Function,
    callable: ValueId,
    arguments: &[EntityId],
    results: &[EntityType],
) -> bool {
    let Some(signature) =
        value_type(function, callable).and_then(|ty| module.callable_signature(ty))
    else {
        return false;
    };
    arguments.len() == signature.parameters.len()
        && arguments
            .iter()
            .zip(&signature.parameters)
            .all(|(argument, expected)| {
                !matches!(argument, EntityId::Place(_))
                    && function.entity(*argument).map(|entity| entity.ty) == Some(*expected)
            })
        && results
            == signature
                .returns
                .iter()
                .copied()
                .map(EntityType::Value)
                .collect::<Vec<_>>()
}

fn function_matches_signature(
    module: &Module,
    target: FunctionId,
    signature: &CallableSignature,
    environment: Option<SsaTypeId>,
) -> bool {
    let Some(function) = module.function(target) else {
        return false;
    };
    if function.receiver.is_some() {
        return false;
    }
    let Some(entry) = function.blocks.first() else {
        return false;
    };
    let mut expected = environment
        .into_iter()
        .map(|target| EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        })
        .collect::<Vec<_>>();
    expected.extend(signature.parameters.iter().copied());
    entry.parameters.len() == expected.len()
        && entry
            .parameters
            .iter()
            .zip(expected)
            .all(|(parameter, ty)| function.entity(*parameter).map(|data| data.ty) == Some(ty))
        && function.return_types == signature.returns
}

fn integer_fits(value: i128, bits: u16, signed: bool) -> bool {
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

fn is_first_class_value(module: &Module, ty: EntityType) -> bool {
    matches!(ty, EntityType::Value(ty) if is_first_class(module, ty))
}

fn is_first_class(module: &Module, ty: SsaTypeId) -> bool {
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
                | SsaTypeKind::SequentialContainer { .. }
                | SsaTypeKind::ZeroSized { .. }
                | SsaTypeKind::SharedReference { .. }
                | SsaTypeKind::FunctionPointer { .. }
                | SsaTypeKind::ConcreteClosure { .. }
        )
    )
}

fn is_koven_int(module: &Module, ty: SsaTypeId) -> bool {
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

fn is_bitwise_integer(module: &Module, ty: Option<SsaTypeId>) -> bool {
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
    ty.is_some_and(|ty| is_scalar(module, ty))
}

fn is_boolean(module: &Module, ty: SsaTypeId) -> bool {
    matches!(module.type_kind(ty), Some(SsaTypeKind::Boolean))
}

fn is_string_owner(module: &Module, ty: SsaTypeId) -> bool {
    matches!(module.type_kind(ty), Some(SsaTypeKind::StringOwner))
}

fn single_value_result(types: &[EntityType]) -> Option<SsaTypeId> {
    match types {
        [EntityType::Value(ty)] => Some(*ty),
        _ => None,
    }
}

fn value_type(function: &Function, value: ValueId) -> Option<SsaTypeId> {
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

fn entity_type(function: &Function, entity: EntityId) -> EntityType {
    function
        .entity(entity)
        .expect("structure phase proved entity existence")
        .ty
}
