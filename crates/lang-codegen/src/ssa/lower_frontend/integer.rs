//! Local facts and submission adapt into the shared integer, checked, and range algorithms.
use super::{
    BlockId, BuiltinType, CheckedArithmeticOperator, EntityId, EntityType, ExpressionLowerer,
    Function, LoweredValue, LoweringError, LoweringErrorKind, Operation, Span, SsaTypeId, SymbolId,
    ValueId, builtin_type, error, value,
};
use crate::ssa::expression_core::checked::{self, CarryColumn, CheckedCarry, CheckedLowering};
use lang_frontend::{
    ast::ExpressionId,
    type_checking::{IntegerOperationKind, TypeId, TypeKind},
};
use std::collections::BTreeMap;

pub(in crate::ssa) struct CheckedRoles {
    baseline: BTreeMap<SymbolId, LoweredValue>,
    carried: super::control::LinearBindings,
}

impl ExpressionLowerer<'_> {
    pub(super) fn checked(
        &mut self,
        operator: CheckedArithmeticOperator,
        left: ValueId,
        right: ValueId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        checked::lower_checked_arithmetic(self, operator, left, right, ty, span)
    }

    pub(super) fn checked_operation(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        checked::lower_checked_operation(self, operation, ty, span)
    }

    pub(super) fn lower_integer_range(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let span = self.expression_span(expression)?;
        let id = expression;
        let descriptor = self
            .typed
            .integer_range_operation(id)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.expression() != id
            || self.typed.expression_type(descriptor.left()) != Some(descriptor.left_type())
            || self.typed.expression_type(descriptor.right()) != Some(descriptor.right_type())
            || self.typed.expression_type(id) != Some(descriptor.result_type())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let types = [
            descriptor.left_type(),
            descriptor.right_type(),
            descriptor.result_type(),
        ]
        .map(|ty| match self.typed.types().get(ty) {
            Some(TypeKind::Builtin(builtin)) => Some(*builtin),
            _ => None,
        });
        let left_id = descriptor.left();
        let right_id = descriptor.right();
        if !crate::ssa::integer::matches_range_fact(
            self.parsed,
            expression,
            left_id,
            right_id,
            descriptor.kind(),
            types,
        ) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let left = match self.lower(left_id)? {
            LoweredValue::Value(value) => value,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let right = match self.lower(right_id)? {
            LoweredValue::Value(value) => value,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let ty = self.expression_ssa_type(expression, span)?;
        let (operation, negate) =
            crate::ssa::integer::range_operation(descriptor.kind(), left, right);
        let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
        let EntityId::Value(operand) = results[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        if !negate {
            return Ok(LoweredValue::Value(operand));
        }
        let (_, results) = self.append(
            Operation::BooleanNot { operand },
            vec![EntityType::Value(ty)],
            span,
        )?;
        let EntityId::Value(result) = results[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        Ok(LoweredValue::Value(result))
    }

    pub(super) fn lower_integer_operation(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let span = self.expression_span(expression)?;
        lower_integer(self, expression, span)
    }
}

impl CheckedLowering for ExpressionLowerer<'_> {
    type Roles = CheckedRoles;

    fn boolean_type(&self) -> Option<SsaTypeId> {
        self.type_ids.iter().find_map(|(frontend, ssa)| {
            (builtin_type(self.typed, *frontend) == Some(BuiltinType::Boolean)).then_some(*ssa)
        })
    }

    fn ir_cursor(&mut self) -> (&mut Function, &mut BlockId) {
        (&mut *self.function, &mut self.block)
    }

    fn capture_roles(&self, span: Span) -> Result<CheckedCarry<Self::Roles>, LoweringError> {
        let baseline = self.bindings.clone();
        let carried = self.linear_binding_slots(&baseline, span)?;
        self.carry_checked_map(&carried, span)?;
        let columns = carried
            .slots
            .iter()
            .map(|slot| CarryColumn {
                source: slot.source,
                ty: slot.ty,
            })
            .collect();
        Ok(CheckedCarry {
            columns,
            roles: CheckedRoles { baseline, carried },
        })
    }

    fn restore_roles(
        &mut self,
        roles: Self::Roles,
        from: BlockId,
        success: BlockId,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.bindings =
            self.rebind_linear_bindings(&roles.baseline, success, &roles.carried, span)?;
        self.resume_checked_map(from, &roles.carried, span)
    }
}

impl IntegerLowering for ExpressionLowerer<'_> {
    type Expr = ExpressionId;
    type Type = TypeId;

    fn fact(&self, expression: ExpressionId) -> Option<IntegerFact<ExpressionId, TypeId>> {
        self.typed
            .integer_operation(expression)
            .map(|descriptor| IntegerFact {
                expression: descriptor.expression(),
                receiver: descriptor.receiver(),
                receiver_type: descriptor.receiver_type(),
                result_type: descriptor.result_type(),
                kind: descriptor.kind(),
            })
    }

    fn receiver_belongs_here(&self, _receiver: ExpressionId) -> bool {
        true
    }

    fn expression_type(&self, expression: ExpressionId) -> Option<TypeId> {
        self.typed.expression_type(expression)
    }

    fn builtin_type(&self, ty: TypeId) -> Option<BuiltinType> {
        match self.typed.types().get(ty) {
            Some(TypeKind::Builtin(builtin)) => Some(*builtin),
            _ => None,
        }
    }

    fn receiver_matches_ast(&self, expression: ExpressionId, receiver: ExpressionId) -> bool {
        crate::ssa::integer::intrinsic_receiver_matches(self.parsed, expression, receiver)
    }

    fn lower_operand(&mut self, receiver: ExpressionId) -> Result<LoweredValue, LoweringError> {
        self.lower(receiver)
    }

    fn result_ssa_type(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        self.expression_ssa_type(expression, span)
    }

    fn append_scalar(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn append_checked(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.checked_operation(operation, ty, span)
    }
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
