//! Shared expression algorithms; entry adapters retain identity and stateful submission.
use lang_frontend::{
    source::Span,
    type_checking::{BuiltinType, IntegerOperationKind},
};

use super::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{Operation, SsaTypeId, ValueId},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LoweredValue {
    Unit,
    Value(ValueId),
    Diverged,
}

#[derive(Clone, Copy)]
pub(super) struct IntegerFact<E, T> {
    pub(super) expression: E,
    pub(super) receiver: E,
    pub(super) receiver_type: T,
    pub(super) result_type: T,
    pub(super) kind: IntegerOperationKind,
}

/// Only the fact queries and submission boundaries required by integer intrinsics.
pub(super) trait IntegerLowering {
    type Expr: Copy + Eq;
    type Type: Copy + Eq;

    fn fact(&self, expression: Self::Expr) -> Option<IntegerFact<Self::Expr, Self::Type>>;
    fn receiver_belongs_here(&self, receiver: Self::Expr) -> bool;
    fn expression_type(&self, expression: Self::Expr) -> Option<Self::Type>;
    fn builtin_type(&self, ty: Self::Type) -> Option<BuiltinType>;
    fn receiver_matches_ast(&self, expression: Self::Expr, receiver: Self::Expr) -> bool;
    fn lower_operand(&mut self, receiver: Self::Expr) -> Result<LoweredValue, LoweringError>;
    fn result_ssa_type(
        &self,
        expression: Self::Expr,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError>;
    fn append_scalar(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError>;
    fn append_checked(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError>;
}

pub(super) fn lower_integer<H: IntegerLowering>(
    host: &mut H,
    expression: H::Expr,
    span: Span,
) -> Result<LoweredValue, LoweringError> {
    let descriptor = host
        .fact(expression)
        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
    if descriptor.expression != expression
        || !host.receiver_belongs_here(descriptor.receiver)
        || host.expression_type(descriptor.receiver) != Some(descriptor.receiver_type)
        || host.expression_type(expression) != Some(descriptor.result_type)
        || !host.receiver_matches_ast(expression, descriptor.receiver)
    {
        return Err(error(LoweringErrorKind::MissingFact, span));
    }
    let integer_types = [descriptor.receiver_type, descriptor.result_type]
        .into_iter()
        .all(|ty| {
            matches!(
                host.builtin_type(ty),
                Some(
                    BuiltinType::Byte
                        | BuiltinType::Short
                        | BuiltinType::Int
                        | BuiltinType::Long
                        | BuiltinType::UByte
                        | BuiltinType::UShort
                        | BuiltinType::UInt
                        | BuiltinType::ULong
                )
            )
        });
    if !integer_types {
        return Err(error(LoweringErrorKind::MissingFact, span));
    }
    match descriptor.kind {
        IntegerOperationKind::Invert if descriptor.receiver_type != descriptor.result_type => {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        IntegerOperationKind::Invert | IntegerOperationKind::CheckedConvert => {}
    }
    let operand = match host.lower_operand(descriptor.receiver)? {
        LoweredValue::Value(value) => value,
        LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
        LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
    };
    // Keep this after receiver evaluation: adapters retain their original type errors.
    let ty = host.result_ssa_type(expression, span)?;
    if descriptor.kind == IntegerOperationKind::CheckedConvert {
        return host.append_checked(Operation::CheckedIntegerConvert { operand }, ty, span);
    }
    host.append_scalar(Operation::IntegerNot { operand }, ty, span)
}
