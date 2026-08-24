//! Typed SSA operation 的局部类型契约。

use super::{
    model::{
        BinaryOperator, ComparisonOperator, EntityId, EntityType, Function, Instruction, Module,
        Operation, PlaceAccess, ScalarConstant, SsaTypeId, SsaTypeKind, ValueId,
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
        Operation::Binary {
            operator,
            left,
            right,
        } => binary_contract(module, function, *operator, *left, *right, &results),
        Operation::CheckedArithmetic { left, right, .. } => {
            checked_arithmetic_contract(module, function, *left, *right, &results)
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
        Operation::DirectCall { callee, arguments } => {
            direct_call_contract(module, function, *callee, arguments, &results)
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

fn constant_contract(module: &Module, constant: &ScalarConstant, results: &[EntityType]) -> bool {
    let Some(result) = single_value_result(results) else {
        return false;
    };
    match (constant, module.type_kind(result)) {
        (ScalarConstant::Unit, Some(SsaTypeKind::Unit))
        | (ScalarConstant::Boolean(_), Some(SsaTypeKind::Boolean)) => true,
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
    arguments: &[ValueId],
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
    if parameter_types.len() != arguments.len()
        || !parameter_types
            .iter()
            .all(|ty| is_scalar_value(module, *ty))
        || !callee.return_types.iter().all(|ty| is_scalar(module, *ty))
    {
        return false;
    }
    let arguments_match = arguments
        .iter()
        .zip(parameter_types)
        .all(|(argument, parameter)| {
            value_type(function, *argument) == Some(parameter.semantic_type())
        });
    let results_match = results
        == callee
            .return_types
            .iter()
            .copied()
            .map(EntityType::Value)
            .collect::<Vec<_>>();
    arguments_match && results_match
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

fn is_scalar_value(module: &Module, ty: EntityType) -> bool {
    matches!(ty, EntityType::Value(ty) if is_scalar(module, ty))
}

fn is_scalar(module: &Module, ty: SsaTypeId) -> bool {
    matches!(
        module.type_kind(ty),
        Some(SsaTypeKind::Boolean | SsaTypeKind::Integer { .. })
    )
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

fn entity_type(function: &Function, entity: EntityId) -> EntityType {
    function
        .entity(entity)
        .expect("structure phase proved entity existence")
        .ty
}
