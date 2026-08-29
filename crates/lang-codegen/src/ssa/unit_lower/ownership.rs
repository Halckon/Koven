//! compilation-unit lowering 的 owner 状态转移与 drop fact 消费。

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::Expression,
    source::Span,
    type_checking::{Copyability, ExpressionCategory, UnitExpressionId},
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, span_key};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{Operation, Origin, ValueId},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn transfer_owned_expression(
        &mut self,
        expression: ExpressionId,
        value: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let expression = UnitExpressionId::new(self.source_unit, expression);
        let ty = self
            .typed
            .types()
            .expression_type(expression)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.types().copyability(ty) != Copyability::MoveOnly {
            return Ok(());
        }
        match self.typed.types().expression_category(expression) {
            Some(ExpressionCategory::Temporary) => self.take_owned_temporary(value, span),
            Some(ExpressionCategory::Place) => {
                let symbol = self.direct_place_symbol(expression.expression(), span)?;
                self.take_owned_binding(symbol, value, span)
            }
            None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }

    pub(super) fn take_owned_binding(
        &mut self,
        symbol: UnitSymbolId,
        value: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        match self.bindings.remove(&symbol) {
            Some(LoweredValue::Value(bound)) if bound == value => Ok(()),
            Some(LoweredValue::Unit | LoweredValue::Diverged | LoweredValue::Value(_)) | None => {
                Err(lowering_error(LoweringErrorKind::MissingFact, span))
            }
        }
    }

    pub(super) fn take_owned_temporary(
        &mut self,
        value: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let before = self.temporaries.len();
        self.temporaries.retain(|_, temporary| *temporary != value);
        if self.temporaries.len() == before {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn emit_drops(&mut self, point: UnitDropPoint) -> Result<(), LoweringError> {
        let facts = self
            .owned
            .ownership()
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.point() == point)
            .collect::<Vec<_>>();
        for fact in facts {
            let owner = match fact.target() {
                UnitDropTarget::Named(symbol) => match self.bindings.remove(&symbol) {
                    Some(LoweredValue::Value(value)) => value,
                    Some(LoweredValue::Unit | LoweredValue::Diverged) | None => {
                        return Err(lowering_error(
                            LoweringErrorKind::MissingFact,
                            fact.value_origin(),
                        ));
                    }
                },
                UnitDropTarget::Temporary(expression) => {
                    self.temporaries.remove(&expression).ok_or_else(|| {
                        lowering_error(LoweringErrorKind::MissingFact, fact.value_origin())
                    })?
                }
                UnitDropTarget::Captured { .. } => continue,
                UnitDropTarget::ReplacedElement(_) => {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.value_origin(),
                    ));
                }
            };
            self.function
                .append_instruction(
                    self.block,
                    Operation::Drop { owner },
                    Vec::new(),
                    Origin::Source(fact.value_origin()),
                )
                .map_err(|_| {
                    lowering_error(LoweringErrorKind::InvalidModel, fact.value_origin())
                })?;
        }
        Ok(())
    }

    fn direct_place_symbol(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<UnitSymbolId, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::Name => self
                .references
                .get(&span_key(node.span()))
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, node.span())),
            Expression::Group { expression } => self.direct_place_symbol(*expression, span),
            _ => Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                node.span(),
            )),
        }
    }
}
