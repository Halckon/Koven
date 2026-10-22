//! Local facts and submission adapt into the shared integer expression algorithm.
use super::{EntityType, ExpressionLowerer, LoweredValue, LoweringError, Operation, value};
use crate::ssa::{
    expression_core::{IntegerFact, IntegerLowering, lower_integer},
    model::SsaTypeId,
};
use lang_frontend::{
    ast::ExpressionId,
    source::Span,
    type_checking::{BuiltinType, TypeId, TypeKind},
};

impl ExpressionLowerer<'_> {
    pub(super) fn lower_integer_operation(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let span = self.expression_span(expression)?;
        lower_integer(self, expression, span)
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
