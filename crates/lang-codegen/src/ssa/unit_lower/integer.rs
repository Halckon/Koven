//! Qualified facts and submission adapt into the shared integer expression algorithm.
use super::{LoweredValue, UnitExpressionLowerer};
use crate::ssa::{
    LoweringError,
    expression_core::{IntegerFact, IntegerLowering, lower_integer},
    model::{Operation, SsaTypeId},
};
use lang_frontend::{
    ast::ExpressionId,
    source::Span,
    type_checking::{BuiltinType, UnitExpressionId, UnitTypeId, UnitTypeKind},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_integer_operation(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        lower_integer(self, id, span)
    }
}

impl IntegerLowering for UnitExpressionLowerer<'_> {
    type Expr = UnitExpressionId;
    type Type = UnitTypeId;

    fn fact(
        &self,
        expression: UnitExpressionId,
    ) -> Option<IntegerFact<UnitExpressionId, UnitTypeId>> {
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

    fn receiver_belongs_here(&self, receiver: UnitExpressionId) -> bool {
        receiver.source_unit() == self.source_unit
    }

    fn expression_type(&self, expression: UnitExpressionId) -> Option<UnitTypeId> {
        self.typed.expression_type(expression)
    }

    fn builtin_type(&self, ty: UnitTypeId) -> Option<BuiltinType> {
        match self.typed.types().get(ty) {
            Some(UnitTypeKind::Builtin(builtin)) => Some(*builtin),
            _ => None,
        }
    }

    fn receiver_matches_ast(
        &self,
        expression: UnitExpressionId,
        receiver: UnitExpressionId,
    ) -> bool {
        crate::ssa::integer::intrinsic_receiver_matches(
            self.parsed,
            expression.expression(),
            receiver.expression(),
        )
    }

    fn lower_operand(&mut self, receiver: UnitExpressionId) -> Result<LoweredValue, LoweringError> {
        self.lower(receiver.expression())
    }

    fn result_ssa_type(
        &self,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        self.expression_ssa_type(expression.expression(), span)
    }

    fn append_scalar(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        UnitExpressionLowerer::append_scalar(self, operation, ty, span)
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
