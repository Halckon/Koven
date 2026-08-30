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
            Some(LoweredValue::Value(bound)) if bound == value => {
                self.closure_bindings.remove(&symbol);
                Ok(())
            }
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
        self.validate_closure_drop_facts(&facts)?;
        for fact in facts {
            let owner = match fact.target() {
                UnitDropTarget::Named(symbol) => match self.bindings.remove(&symbol) {
                    Some(LoweredValue::Value(value)) => {
                        self.closure_bindings.remove(&symbol);
                        value
                    }
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

    /// LoopExit facts 基于 loop-entry state 发布；若所有实际出口已一致消费 owner，缺失 binding
    /// 表示该粗粒度 fact 无需生成 drop，而不是 lowering 事实缺失。
    pub(super) fn emit_loop_exit_drops(
        &mut self,
        point: UnitDropPoint,
        span: Span,
    ) -> Result<(), LoweringError> {
        let facts = self
            .owned
            .ownership()
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.point() == point)
            .collect::<Vec<_>>();
        let live_closures = facts
            .iter()
            .filter_map(|fact| match fact.target() {
                UnitDropTarget::Named(symbol) => self.closure_bindings.get(&symbol).copied(),
                UnitDropTarget::Temporary(_)
                | UnitDropTarget::Captured { .. }
                | UnitDropTarget::ReplacedElement(_) => None,
            })
            .collect::<Vec<_>>();
        let live_closure_facts = facts
            .iter()
            .copied()
            .filter(|fact| match fact.target() {
                UnitDropTarget::Named(symbol) => self.closure_bindings.contains_key(&symbol),
                UnitDropTarget::Captured { closure, .. } => live_closures.contains(&closure),
                UnitDropTarget::Temporary(_) | UnitDropTarget::ReplacedElement(_) => false,
            })
            .collect::<Vec<_>>();
        self.validate_closure_drop_facts(&live_closure_facts)?;
        for fact in facts {
            let symbol = match fact.target() {
                UnitDropTarget::Named(symbol) => symbol,
                UnitDropTarget::Captured { .. } => continue,
                UnitDropTarget::Temporary(_) | UnitDropTarget::ReplacedElement(_) => {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            };
            let Some(binding) = self.bindings.remove(&symbol) else {
                self.closure_bindings.remove(&symbol);
                continue;
            };
            self.closure_bindings.remove(&symbol);
            let LoweredValue::Value(owner) = binding else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            self.function
                .append_instruction(
                    self.block,
                    Operation::Drop { owner },
                    Vec::new(),
                    Origin::Source(fact.value_origin()),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
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
