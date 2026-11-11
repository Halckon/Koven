//! Integer conversion, scalar constant, byte-array text, numeric Range, and borrow-result contracts.

use super::{
    EntityId, EntityType, Function, LoanKind, Module, ScalarConstant, SsaTypeId, SsaTypeKind,
    ValueId, entity_type, integer_fits, is_bitwise_integer, is_boolean, single_value_result,
    value_type,
};
use crate::ssa::model::{LoanId, SequentialContainerKind};

pub(in crate::ssa) mod verify_borrow_result {
    use crate::ssa::{
        model::*,
        verify::{VerifyError, VerifyErrorKind, VerifyLocation},
    };

    pub(in crate::ssa) fn parameter(function: &Function) -> Option<(LoanId, SsaTypeId)> {
        if function.receiver.is_some() {
            return None;
        }
        let index = function.borrow_return?;
        if !function.blocks.first()?.parameters.iter().all(|entity| {
            matches!(
                function.entity(*entity).map(|data| data.ty),
                Some(
                    EntityType::Value(_)
                        | EntityType::Loan {
                            kind: LoanKind::Shared,
                            ..
                        }
                )
            )
        }) {
            return None;
        }
        let [target] = function.return_types.as_slice() else {
            return None;
        };
        let entity = *function.blocks.first()?.parameters.get(index)?;
        let EntityId::Loan(loan) = entity else {
            return None;
        };
        matches!(
            function.entity(entity)?.ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                ..
            }
        )
        .then_some((loan, *target))
    }

    pub(in crate::ssa) fn call_contract(
        module: &Module,
        function: &Function,
        callee: FunctionId,
        arguments: &[EntityId],
        source: LoanId,
        results: &[EntityType],
    ) -> bool {
        let Some(callee) = module.function(callee) else {
            return false;
        };
        let Some((_, target)) = parameter(callee) else {
            return false;
        };
        let Some(index) = callee.borrow_return else {
            return false;
        };
        let Some(entry) = callee.blocks.first() else {
            return false;
        };
        arguments.len() == entry.parameters.len()
            && arguments.get(index) == Some(&EntityId::Loan(source))
            && arguments.iter().zip(&entry.parameters).all(|(arg, param)| {
                !matches!(arg, EntityId::Place(_))
                    && callee.entity(*param).is_some_and(|parameter| {
                        crate::ssa::verify_operation::is_first_class(
                            module,
                            parameter.ty.semantic_type(),
                        ) && function.entity(*arg).map(|data| data.ty) == Some(parameter.ty)
                    })
            })
            && crate::ssa::verify_operation::is_first_class(module, target)
            && results
                == [EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }]
    }

    pub(in crate::ssa) fn return_contract(
        function: &Function,
        loan: LoanId,
        block: BlockId,
        origin: &Origin,
        errors: &mut Vec<VerifyError>,
    ) {
        let valid = parameter(function).is_some_and(|(_, target)| {
            function.entity(EntityId::Loan(loan)).map(|data| data.ty)
                == Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                })
        });
        if !valid {
            errors.push(VerifyError {
                kind: VerifyErrorKind::ReturnType { index: 0 },
                location: VerifyLocation::Terminator(block),
                origin: Some(origin.clone()),
            });
        }
    }
}

pub(super) fn checked_conversion_contract(
    module: &Module,
    function: &Function,
    operand: ValueId,
    results: &[EntityType],
) -> bool {
    let [EntityType::Value(target), EntityType::Value(failed)] = results else {
        return false;
    };
    is_bitwise_integer(module, value_type(function, operand))
        && is_bitwise_integer(module, Some(*target))
        && is_boolean(module, *failed)
}

pub(super) fn constant_contract(
    module: &Module,
    constant: &ScalarConstant,
    results: &[EntityType],
) -> bool {
    let Some(result) = single_value_result(results) else {
        return false;
    };
    match (constant, module.type_kind(result)) {
        #[cfg(test)]
        (ScalarConstant::SyntheticZero, Some(SsaTypeKind::ZeroSized { .. })) => true,
        (ScalarConstant::Unit, Some(SsaTypeKind::Unit))
        | (ScalarConstant::Boolean(_), Some(SsaTypeKind::Boolean)) => true,
        (ScalarConstant::Char(value), Some(SsaTypeKind::Char)) => char::from_u32(*value).is_some(),
        (ScalarConstant::Integer(value), Some(SsaTypeKind::Integer { bits, signed })) => {
            integer_fits(*value, *bits, *signed)
        }
        _ => false,
    }
}

pub(super) fn shared_byte_array(module: &Module, function: &Function, source: LoanId) -> bool {
    let EntityType::Loan {
        target,
        kind: LoanKind::Shared,
    } = entity_type(function, EntityId::Loan(source))
    else {
        return false;
    };
    let Some(SsaTypeKind::SequentialContainer {
        kind: SequentialContainerKind::Array,
        element,
    }) = module.type_kind(target)
    else {
        return false;
    };
    matches!(
        module.type_kind(*element),
        Some(SsaTypeKind::Integer {
            bits: 8,
            signed: false
        })
    )
}

pub(super) fn range_construct(
    module: &Module,
    function: &Function,
    first: ValueId,
    bound: ValueId,
    results: &[EntityType],
) -> bool {
    let Some(range) = single_value_result(results) else {
        return false;
    };
    let Some(endpoint) = module.integer_range_endpoint(range) else {
        return false;
    };
    module.valid_integer_range_type(range, endpoint)
        && value_type(function, first) == Some(endpoint)
        && value_type(function, bound) == Some(endpoint)
}

pub(super) fn range_contains(
    module: &Module,
    function: &Function,
    value: ValueId,
    range: ValueId,
    results: &[EntityType],
) -> bool {
    let Some(range) = value_type(function, range) else {
        return false;
    };
    let Some(endpoint) = module.integer_range_endpoint(range) else {
        return false;
    };
    module.valid_integer_range_type(range, endpoint)
        && value_type(function, value) == Some(endpoint)
        && single_value_result(results).is_some_and(|ty| is_boolean(module, ty))
}

pub(super) fn range_equal(
    module: &Module,
    function: &Function,
    left: ValueId,
    right: ValueId,
    results: &[EntityType],
) -> bool {
    let Some(range) = value_type(function, left) else {
        return false;
    };
    let Some(endpoint) = module.integer_range_endpoint(range) else {
        return false;
    };
    module.valid_integer_range_type(range, endpoint)
        && value_type(function, right) == Some(range)
        && single_value_result(results).is_some_and(|ty| is_boolean(module, ty))
}

pub(super) fn borrowed_generate_contract(
    module: &Module,
    function: &Function,
    container: SsaTypeId,
    length: ValueId,
    initializer: LoanId,
    results: &[EntityType],
) -> bool {
    let Some((_, element)) = module.sequential_container(container) else {
        return false;
    };
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = entity_type(function, EntityId::Loan(initializer))
    else {
        return false;
    };
    let Some(signature) = module
        .callable_signature(target)
        .or_else(|| module.callable_view_signature(target))
    else {
        return false;
    };
    let [
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: index,
        },
    ] = signature.parameters.as_slice()
    else {
        return false;
    };
    value_type(function, length).is_some_and(|ty| super::is_koven_int(module, ty))
        && super::is_koven_int(module, *index)
        && single_value_result(results) == Some(container)
        && !signature.never_returns
        && (signature.returns == [element]
            || (signature.returns.is_empty()
                && matches!(module.type_kind(element), Some(SsaTypeKind::Unit))))
}
