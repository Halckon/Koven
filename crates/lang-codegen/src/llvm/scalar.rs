//! LLVM scalar signedness decisions derived from verified SSA types.

use inkwell::IntPredicate;

use crate::ssa::model::{ComparisonOperator, Function, Module, SsaTypeKind, ValueId};

use super::{LlvmAdapterError, entities::value_type};

pub(super) fn ordering_predicate(
    module: &Module,
    function: &Function,
    operand: ValueId,
    signed: IntPredicate,
    unsigned: IntPredicate,
) -> Result<IntPredicate, LlvmAdapterError> {
    Ok(if integer_signed(module, function, operand)? {
        signed
    } else {
        unsigned
    })
}

pub(super) fn comparison_predicate(
    module: &Module,
    function: &Function,
    operand: ValueId,
    operator: ComparisonOperator,
) -> Result<IntPredicate, LlvmAdapterError> {
    Ok(match operator {
        ComparisonOperator::Equal => IntPredicate::EQ,
        ComparisonOperator::NotEqual => IntPredicate::NE,
        ComparisonOperator::LessThan => ordering_predicate(
            module,
            function,
            operand,
            IntPredicate::SLT,
            IntPredicate::ULT,
        )?,
        ComparisonOperator::LessThanOrEqual => ordering_predicate(
            module,
            function,
            operand,
            IntPredicate::SLE,
            IntPredicate::ULE,
        )?,
        ComparisonOperator::GreaterThan => ordering_predicate(
            module,
            function,
            operand,
            IntPredicate::SGT,
            IntPredicate::UGT,
        )?,
        ComparisonOperator::GreaterThanOrEqual => ordering_predicate(
            module,
            function,
            operand,
            IntPredicate::SGE,
            IntPredicate::UGE,
        )?,
    })
}

pub(super) fn integer_signed(
    module: &Module,
    function: &Function,
    value: ValueId,
) -> Result<bool, LlvmAdapterError> {
    match module.type_kind(value_type(function, value)?) {
        Some(SsaTypeKind::Integer { signed, .. }) => Ok(*signed),
        _ => Err(LlvmAdapterError::InvalidSsa(
            "integer operation 的 operand 不是整数".to_owned(),
        )),
    }
}
