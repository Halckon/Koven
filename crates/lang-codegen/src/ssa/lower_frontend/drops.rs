//! Phase 3 ASAP drop facts to explicit SSA owner drops, plus call-prefix and control-transfer loan cleanup.

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{DropPoint, DropTarget, LoanEndPoint},
};

use super::{ExpressionLowerer, LoanId, LoweredValue, LoweringError, LoweringErrorKind, error};
use crate::ssa::model::Operation;

impl ExpressionLowerer<'_> {
    /// End only the committed frontend call loans, then execute its existing ASAP facts.
    pub(super) fn finish_borrowed_call(
        &mut self,
        expression: ExpressionId,
        span: lang_frontend::source::Span,
    ) -> Result<(), LoweringError> {
        self.finish_borrowed_call_with_sources(expression, span, Vec::new())
    }

    /// Short result operands end before their newly created source chains and owner cleanup.
    pub(super) fn finish_borrowed_call_with_sources(
        &mut self,
        expression: ExpressionId,
        span: lang_frontend::source::Span,
        sources: Vec<LoanId>,
    ) -> Result<(), LoweringError> {
        let ending_loans = self.owned.loans_ending_at(expression).collect::<Vec<_>>();
        for fact in &ending_loans {
            self.end_pending_source_loan(
                (expression.index(), fact.argument().index()),
                fact.end_span(),
            )?;
        }
        if self
            .pending_call_loans
            .keys()
            .any(|(call, _)| *call == expression.index())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        for loan in sources.into_iter().rev() {
            self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
        }
        self.finish_short_call_ranges(expression, span)?;
        for fact in ending_loans {
            self.emit_drops(DropPoint::AfterExpression(fact.argument()))?;
        }
        self.emit_drops(DropPoint::CallReturn(expression))
    }

    /// 结束当前路径的调用前缀借用；兄弟分支由入口快照恢复。
    pub(super) fn emit_control_transfer_cleanup(
        &mut self,
        transfer: ExpressionId,
    ) -> Result<(), LoweringError> {
        let span = self.expression_span(transfer)?;
        let endings = self
            .owned
            .loan_ends()
            .iter()
            .filter(|fact| fact.point() == LoanEndPoint::ControlTransfer(transfer))
            .map(|fact| (fact.call().index(), fact.argument().index()))
            .collect::<Vec<_>>();
        for key in &endings {
            self.end_pending_source_loan(*key, span)?;
            self.release_primitive_unit_operand(key.0, key.1);
        }
        let ranges = self
            .owned
            .borrow_results()
            .range_uses()
            .iter()
            .filter(|fact| {
                matches!(fact.site(), lang_frontend::ownership_checking::RangeUseSite::Call(call)
                    if endings.iter().any(|key| key.0 == call.index()))
                    && self.temporaries.contains_key(&fact.expression().index())
            })
            .map(|fact| fact.expression())
            .collect::<Vec<_>>();
        for range in ranges.into_iter().rev() {
            self.finish_short_range(range, span)?;
        }
        // A transfer leaves proofs introduced inside its target scope. Owners may
        // survive a break/continue and must not keep that branch-local view alive.
        let is_return = matches!(
            self.parsed
                .ast()
                .expressions()
                .get(transfer)
                .map(|node| node.payload()),
            Ok(lang_frontend::parser::Expression::Return { .. })
        );
        let views = self
            .non_null_bindings
            .iter()
            .filter_map(|(&symbol, proof)| {
                // While proofs retire only after the existing child and owner cleanup.
                if proof.is_while_origin() {
                    return None;
                }
                let retained = !is_return
                    && self
                        .loops
                        .last()
                        .is_some_and(|context| context.entry_views.contains_key(&symbol));
                (!retained).then_some(symbol)
            })
            .collect::<Vec<_>>();
        for symbol in views {
            self.end_non_null_proof(symbol, span)?;
        }
        self.emit_drops(DropPoint::ControlTransfer(transfer))
    }

    pub(super) fn emit_drops(&mut self, point: DropPoint) -> Result<(), LoweringError> {
        let facts = self
            .owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.point() == point)
            .collect::<Vec<_>>();
        self.preflight_nullable_if_cleanup_groups(&facts)?;
        self.preflight_nullable_numeric_cleanup(&facts)?;
        self.preflight_nullable_borrowed_cleanup(&facts)?;
        self.end_result_bindings(point)?;
        let mut index = 0;
        while index < facts.len() {
            if let Some(consumed) = self.emit_nullable_if_cleanup_group(&facts[index..])? {
                index += consumed;
                continue;
            }
            let fact = facts[index];
            index += 1;
            if self.emit_resource_map_drop(fact)? {
                continue;
            }
            if self.emit_local_map_drop(fact)? {
                continue;
            }
            if self.emit_outer_borrowed_drop(fact)? {
                continue;
            }
            if self.emit_nullable_borrowed_cleanup(fact)? {
                continue;
            }
            if self.emit_nullable_numeric_cleanup(fact)? {
                continue;
            }
            // Generic ownership facts precede monomorphization. A concrete Copyable
            // instance has no drop obligation; keep its binding/aliases intact.
            let template = match fact.target() {
                DropTarget::Named(symbol) => Some(self.typed.symbol_type(symbol)),
                DropTarget::Temporary(expression) => Some(self.typed.expression_type(expression)),
                _ => None,
            };
            if let Some(template) = template {
                let template = template
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, fact.value_origin()))?;
                let concrete = self.resolve_type(template, fact.value_origin())?;
                if concrete != template
                    && self.typed.copyability(concrete)
                        == Some(lang_frontend::type_checking::Copyability::Copyable)
                {
                    continue;
                }
            }
            // Conditional cleanup needs its saved choice transported with the owner.
            if fact.condition().is_some()
                && !super::map_loop_owner::supported_drop(
                    self.parsed,
                    self.names,
                    self.typed,
                    self.owned,
                    fact,
                )
            {
                return Err(error(
                    LoweringErrorKind::UnsupportedNode,
                    fact.value_origin(),
                ));
            }
            let owner = match fact.target() {
                DropTarget::Named(symbol) => {
                    // Loop-exit facts include entry owners consumed on every actual exit.
                    if matches!(point, DropPoint::LoopExit(_))
                        && !self.bindings.contains_key(&symbol)
                        && (fact.condition().is_none()
                            || super::constant_presence::constant_drop(
                                self.parsed,
                                self.names,
                                self.typed,
                                self.owned,
                                fact,
                            ))
                    {
                        continue;
                    }
                    // nullable 分支证明是 owner 的共享视图。ASAP drop 可能恰好位于证明分支的
                    // 最后一次使用处，因此必须先结束视图，再消费 owner。
                    self.end_non_null_proof(symbol, fact.value_origin())?;
                    match self.bindings.remove(&symbol) {
                        Some(LoweredValue::Value(value)) => value,
                        Some(LoweredValue::Unit | LoweredValue::Diverged) | None => {
                            return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
                        }
                    }
                }
                DropTarget::Temporary(expression) => {
                    let owner = self
                        .temporaries
                        .remove(&expression.index())
                        .ok_or_else(|| {
                            error(LoweringErrorKind::MissingFact, fact.value_origin())
                        })?;
                    // Group expressions can name the same temporary owner.
                    self.temporaries.retain(|_, candidate| *candidate != owner);
                    owner
                }
                // The closure owner recursively drops its owned environment slots.
                DropTarget::Captured { .. } => continue,
                DropTarget::RetainedSource(_) => {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.value_origin(),
                    ));
                }
                DropTarget::ReplacedElement(_) => {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.value_origin(),
                    ));
                }
            };
            self.record_map_loop_drop(fact, owner)?;
            self.append(Operation::Drop { owner }, Vec::new(), fact.value_origin())?;
            self.release_owner_capture_loans(owner, fact.value_origin())?;
        }
        Ok(())
    }
}
