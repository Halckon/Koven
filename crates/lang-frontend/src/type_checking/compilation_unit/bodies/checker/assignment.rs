//! SPEC-0218 compilation-unit 普通替换赋值 typed facts。

use crate::{
    ast::ExpressionId,
    name_resolution::{Namespace, SourceUnitId, UnitReferenceTarget},
    parser::{AssignmentOperator, Expression},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, DeferredReason, ExpressionCategory,
        TypeCheckingError, UnitAssignmentDescriptor, UnitExpressionId, UnitTypeId,
    },
};

use super::{BodyChecker, ExpressionCheck};

pub(super) struct AssignmentExpression {
    pub(super) expression: ExpressionId,
    pub(super) target: ExpressionId,
    pub(super) operator: AssignmentOperator,
    pub(super) operator_span: Span,
    pub(super) value: ExpressionId,
}

impl BodyChecker<'_> {
    pub(super) fn check_assignment(
        &mut self,
        source: SourceUnitId,
        assignment: AssignmentExpression,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let AssignmentExpression {
            expression,
            target,
            operator,
            operator_span,
            value,
        } = assignment;
        if let Some(result) =
            self.check_map_assignment(source, target, operator, operator_span, value, return_type)?
        {
            return Ok(result);
        }
        if let Some(result) = self.check_container_assignment(
            source,
            target,
            operator,
            operator_span,
            value,
            return_type,
        )? {
            return Ok(result);
        }
        let target_result = self.check_expression(source, target, None, None, return_type)?;
        let target_contract = self.assignment_target_contract(source, target)?;
        let target_type = target_contract.map(|(ty, _)| ty);
        let value_result = self.check_expression(
            source,
            value,
            (operator == AssignmentOperator::Assign)
                .then_some(target_type)
                .flatten(),
            (operator == AssignmentOperator::Assign)
                .then(|| target_contract.map(|(_, span)| span))
                .flatten(),
            return_type,
        )?;
        if let Some(key) = self.stable_flow_key(source, target) {
            self.flow_facts.remove(&key);
        }
        let falls_through = target_result.falls_through && value_result.falls_through;
        if self.is_error(target_result.ty) || self.is_error(value_result.ty) {
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through,
            });
        }
        let Some(target_type) = target_type else {
            return Ok(ExpressionCheck {
                ty: self.deferred_type(DeferredReason::Assignment),
                falls_through,
            });
        };
        if operator != AssignmentOperator::Assign
            || self.is_deferred(target_result.ty)
            || self.is_deferred(value_result.ty)
        {
            return Ok(ExpressionCheck {
                ty: self.deferred_type(DeferredReason::Assignment),
                falls_through,
            });
        }
        self.parts.assignments.push(UnitAssignmentDescriptor::new(
            UnitExpressionId::new(source, expression),
            UnitExpressionId::new(source, target),
            UnitExpressionId::new(source, value),
            operator,
            target_type,
            falls_through,
        ));
        Ok(ExpressionCheck {
            ty: self.builtin(BuiltinType::Unit),
            falls_through,
        })
    }

    fn assignment_target_contract(
        &self,
        source: SourceUnitId,
        target: ExpressionId,
    ) -> Result<Option<(UnitTypeId, Span)>, CompilationUnitTypeError> {
        if self.expression_category(source, target) != ExpressionCategory::Place {
            return Ok(None);
        }
        let node = self
            .file(source)
            .ast()
            .expressions()
            .get(target)
            .map_err(TypeCheckingError::from)?;
        match node.payload() {
            Expression::Group { expression } => {
                self.assignment_target_contract(source, *expression)
            }
            Expression::Name => Ok(
                match self.reference(source, node.span(), Namespace::Value) {
                    Some(UnitReferenceTarget::Declaration(declaration)) => {
                        self.signatures.declaration(*declaration).map(|signature| {
                            (
                                self.parts
                                    .symbol_types
                                    .get(&signature.symbol())
                                    .copied()
                                    .unwrap_or(signature.ty()),
                                self.names.names().index().declarations()[declaration.index()]
                                    .name_span(),
                            )
                        })
                    }
                    Some(UnitReferenceTarget::Symbol(symbol)) => {
                        let ty = self
                            .parts
                            .symbol_types
                            .get(symbol)
                            .copied()
                            .or_else(|| self.signatures.symbol_type(*symbol));
                        ty.map(|ty| self.unit_symbol_span(*symbol).map(|span| (ty, span)))
                            .transpose()?
                    }
                    _ => None,
                },
            ),
            Expression::Member { .. } => self
                .parts
                .aggregate_projections
                .iter()
                .find(|projection| projection.expression() == UnitExpressionId::new(source, target))
                .map(|projection| {
                    self.unit_symbol_span(projection.field())
                        .map(|span| (projection.ty(), span))
                })
                .transpose(),
            _ => Ok(None),
        }
    }
}
