//! Observable constructor results reuse committed container descriptors and owner views.

use super::*;
use crate::ssa::model::PlaceAccess;
use lang_frontend::type_checking::Copyability;

impl ExpressionLowerer<'_> {
    pub(in crate::ssa::lower_frontend) fn lower_container_size(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.typed.container_size(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let span = descriptor.span();
        if self.typed.expression_type(expression) != Some(descriptor.result_type())
            || builtin_type(self.typed, descriptor.result_type()) != Some(BuiltinType::Int)
            || self.typed.expression_type(descriptor.receiver())
                != Some(descriptor.container_type())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_container_identity(
            descriptor.container(),
            descriptor.element_type(),
            descriptor.container_type(),
            span,
        )?;
        let receiver_span = self.expression_span(descriptor.receiver())?;
        let (loan, ending) =
            self.lower_borrow_argument(expression, descriptor.receiver(), receiver_span)?;
        self.pending_call_loans.insert(
            (expression.index(), descriptor.receiver().index()),
            ending.then_some(loan),
        );
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::ContainerLength {
                owner: EntityId::Loan(loan),
            },
            vec![EntityType::Value(ty)],
            span,
        )?;
        self.finish_borrowed_call(expression, span)?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    /// A plain read copies only a statically Copyable element. MoveOnly elements keep Borrow access.
    pub(in crate::ssa::lower_frontend) fn lower_container_index(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let span = self.expression_span(expression)?;
        let descriptor = self
            .typed
            .element_place(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element = self.resolve_type(descriptor.element_type(), span)?;
        if self.typed.copyability(element) != Some(Copyability::Copyable)
            || builtin_type(self.typed, element) == Some(BuiltinType::Unit)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner = self.container_view_operand(descriptor.receiver())?;
        let index = self.require_value(descriptor.index())?;
        let element = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::ContainerElementPlace { owner, index },
            vec![EntityType::Place(element)],
            span,
        )?;
        let (_, results) = self.append(
            Operation::Read {
                source: PlaceAccess::Place(place(results[0])),
            },
            vec![EntityType::Value(element)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn container_view_operand(
        &mut self,
        expression: ExpressionId,
    ) -> Result<EntityId, LoweringError> {
        let span = self.expression_span(expression)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::Group { expression } => self.container_view_operand(*expression),
            Expression::Name => {
                let symbol = self
                    .references
                    .get(&span_key(span))
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if let Some(LoweredValue::Value(owner)) = self.bindings.get(symbol).copied() {
                    return Ok(EntityId::Value(owner));
                }
                if let Some(&loan) = self.borrow_bindings.get(symbol) {
                    return Ok(EntityId::Loan(loan));
                }
                self.require_value(expression).map(EntityId::Value)
            }
            _ => self.require_value(expression).map(EntityId::Value),
        }
    }
}
