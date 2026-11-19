//! Borrow-return and carrier-return lowering in the refined CFG.
use super::*;

impl Builder<'_, '_> {
    pub(in crate::ssa::refine::build) fn lower_borrow_return(
        &mut self,
        control_expr: ExpressionId,
        value_expr: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let return_id = self.unit(control_expr);
        let value_id = self.unit(value_expr);
        let actual = self.ungrouped_expr(value_expr)?;
        let id = self.unit(actual);
        if self.inputs.abandoned_borrow_result_call(id) {
            if self.expression(value_expr)? != Lowered::Diverged {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            self.unreachable_drop_points
                .push(UnitDropPoint::ControlTransfer(return_id));
            return Ok(Lowered::Diverged);
        }
        let block = self.current(span)?;
        self.body
            .branch_plans
            .enter(return_id, value_id, block, &self.borrowed, span)?;
        self.body
            .nested_plans
            .enter(return_id, value_id, block, &self.borrowed, span)?;
        self.body
            .bounded_plans
            .enter(return_id, value_id, block, &self.borrowed, span)?;
        self.body
            .single_plans
            .enter(return_id, value_id, block, &self.borrowed, span)?;
        self.body
            .multi_plans
            .enter(return_id, value_id, block, &self.borrowed, span)?;
        self.body
            .forward_plans
            .enter(return_id, value_id, &self.borrowed, span)?;
        let fact = self
            .inputs
            .owned
            .borrow_return_origins()
            .iter()
            .find(|fact| fact.expression() == return_id || fact.expression() == value_id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let UnitLoanTarget::Place(_) = fact.origin() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let (payload, node_span) = self.node(actual)?;
        let entry = self
            .inputs
            .borrow_return
            .and_then(|index| self.inputs.parameters.get(index))
            .copied();
        let (loan, forwards_entry) = match payload {
            Expression::Name | Expression::Member { .. } => {
                let mut created = Vec::new();
                let Some(loan) = self.clone_field_loan(actual, &mut created, span)? else {
                    return Ok(Lowered::Diverged);
                };
                (loan, false)
            }
            Expression::Call { .. } => {
                let forwarded_entry = entry.and_then(|entry| {
                    crate::ssa::provider::borrow_result::forwards_string_entry(
                        self.inputs.parsed,
                        self.inputs.names,
                        self.inputs.typed,
                        self.inputs.owned,
                        id,
                        entry,
                    )
                });
                let source = forwarded_entry
                    .or_else(|| {
                        self.inputs
                            .owned
                            .borrow_results()
                            .forwarded_source_loans()
                            .iter()
                            .find(|source| source.call() == id)
                            .copied()
                    })
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, node_span))?;
                let entry_loan = if forwarded_entry.is_some() {
                    Some(
                        entry
                            .and_then(|symbol| self.borrowed.get(&symbol).copied())
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, node_span))?,
                    )
                } else {
                    None
                };
                let Some((loan, _)) =
                    self.lower_result_call(actual, source, node_span, None, entry_loan)?
                else {
                    return Ok(Lowered::Diverged);
                };
                (loan, forwarded_entry.is_some())
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, node_span)),
        };
        let point = UnitDropPoint::ControlTransfer(return_id);
        if forwards_entry || entry.and_then(|source| self.borrowed.get(&source)) == Some(&loan) {
            self.finish_return_drops(point, span)?;
        } else {
            self.drops(point)?;
        }
        self.terminate(RTerm::BorrowReturn { loan }, span)?;
        Ok(Lowered::Diverged)
    }

    pub(in crate::ssa::refine::build) fn lower_carrier_return(
        &mut self,
        control_expr: ExpressionId,
        value_expr: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let return_id = self.unit(control_expr);
        let actual = self.ungrouped_expr(value_expr)?;
        let Some((view, source, created)) = self.lower_range_call(actual, span)? else {
            return Ok(Lowered::Diverged);
        };
        if !created.is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.drops(UnitDropPoint::ControlTransfer(return_id))?;
        self.terminate(RTerm::RangeReturn { view, source }, span)?;
        Ok(Lowered::Diverged)
    }
}
