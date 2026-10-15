//! SPEC-0325: storage-typed ordinary variable assignment; ownership stays downstream.
use crate::parser::{AssignmentOperator, Expression};

use super::*;

impl Checker<'_> {
    pub(super) fn check_variable_assignment(
        &mut self,
        target: ExpressionId,
        operator: AssignmentOperator,
        value: ExpressionId,
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        if operator != AssignmentOperator::Assign {
            return Ok(None);
        }
        let Some(symbol) = self.assignment_variable(target)? else {
            return Ok(None);
        };
        let target_result = self.check_expression(target, None, None)?;
        let storage_type = self.symbol_type(symbol);
        let value_result = self.check_expression(
            value,
            storage_type,
            storage_type.map(|_| self.symbol_spans[symbol.index()]),
        )?;
        let ty = if self.is_error(target_result.ty) || self.is_error(value_result.ty) {
            self.error_type()
        } else if storage_type.is_none()
            || self.is_deferred(target_result.ty)
            || self.is_deferred(value_result.ty)
        {
            self.deferred(DeferredReason::Assignment)
        } else {
            self.builtin(BuiltinType::Unit)
        };
        Ok(Some(ExprCheck {
            ty,
            falls_through: target_result.falls_through && value_result.falls_through,
        }))
    }

    fn assignment_variable(
        &self,
        target: ExpressionId,
    ) -> Result<Option<SymbolId>, TypeCheckingError> {
        let node = self.ast().expressions().get(target)?;
        match node.payload() {
            Expression::Group { expression } => self.assignment_variable(*expression),
            Expression::Name => {
                let Some(ReferenceTarget::Symbol(symbol)) =
                    self.reference(node.span(), Namespace::Value)
                else {
                    return Ok(None);
                };
                let variable = match self.symbol_kinds[symbol.index()] {
                    SymbolKind::Variable => !matches!(
                        self.scope_kinds[self.symbol_scopes[symbol.index()].index()],
                        ScopeKind::Classifier | ScopeKind::Companion
                    ),
                    SymbolKind::ValueParameter
                    | SymbolKind::LambdaParameter
                    | SymbolKind::ForBinding
                    | SymbolKind::DestructuringBinding => true,
                    _ => false,
                };
                Ok(variable.then_some(*symbol))
            }
            _ => Ok(None),
        }
    }
}
