//! Local borrow binding initialization and retirement, preserving original cleanup order.
use super::*;
use crate::ssa::{borrow_result_support::ordinary_local, model::LoanKind};
use lang_frontend::ownership_checking::BorrowBindingStorage;
impl Builder<'_, '_> {
    pub(super) fn local_borrow_binding(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(initializer);
        let fact = self
            .inputs
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|fact| fact.binding() == symbol && fact.initializer() == id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if fact.storage() == BorrowBindingStorage::NewRangeDescriptor {
            let Some((view, loan, created)) = self.lower_range_call(initializer, span)? else {
                return Ok(Lowered::Diverged);
            };
            self.range_views.insert(symbol, view);
            self.borrowed.insert(symbol, loan);
            self.result_source_loans.insert(symbol, created);
            return Ok(Lowered::Unit);
        }
        if fact.storage() == BorrowBindingStorage::BorrowedCarrierMetadata
            && fact.source_loan().is_none()
            && let Ok(root) = self.ungrouped_expr(initializer)
            && let Ok(parent_sym) = self.reference(self.span(root))
            && let Some(&view) = self.range_views.get(&parent_sym)
        {
            if fact.parent() != Some(parent_sym) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let target = self.ssa_type(self.expression_type(initializer)?, span)?;
            let place = self.value(
                ROp::RootPlace { owner: view },
                EntityType::Place(target),
                span,
            )?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
                span,
            )?;
            self.borrowed.insert(symbol, loan);
            return Ok(Lowered::Unit);
        }
        let Some(source) = fact.source_loan() else {
            let Some((loan, created)) = self.lower_result_place(initializer, span)? else {
                return Ok(Lowered::Diverged);
            };
            self.borrowed.insert(symbol, loan);
            self.result_source_loans.insert(symbol, created);
            return Ok(Lowered::Unit);
        };
        self.body.binding_plans.activate(symbol, id, source, span)?;
        let local_policy = ordinary_local::policy(
            self.inputs.parsed,
            self.inputs.typed,
            self.inputs.owned,
            &fact,
        );
        let Some((loan, created)) =
            self.lower_result_call(initializer, source, span, local_policy)?
        else {
            return Ok(Lowered::Diverged);
        };
        self.borrowed.insert(symbol, loan);
        self.result_source_loans.insert(symbol, created);
        self.body
            .binding_plans
            .commit(symbol, &self.borrowed, &self.result_source_loans, span)?;
        Ok(Lowered::Unit)
    }

    pub(super) fn end_result_bindings(
        &mut self,
        point: UnitDropPoint,
    ) -> Result<(), LoweringError> {
        let ends: Vec<_> = self
            .inputs
            .owned
            .borrow_results()
            .ends()
            .iter()
            .filter(|fact| fact.point() == point)
            .copied()
            .collect();
        // A control transfer ends the bindings on its own path only; the fall-through path
        // still holds them (SPEC-0393).
        let transfer = matches!(point, UnitDropPoint::ControlTransfer(_));
        for fact in ends {
            let loan = if transfer {
                self.borrowed.get(&fact.binding()).copied()
            } else {
                self.borrowed.remove(&fact.binding())
            };
            self.body.binding_plans.retire(
                fact.binding(),
                point,
                loan,
                self.result_source_loans
                    .get(&fact.binding())
                    .map(Vec::as_slice),
                self.inputs.span,
            )?;
            let Some(loan) = loan else {
                continue;
            };
            let span = self
                .inputs
                .owned
                .borrow_results()
                .bindings()
                .iter()
                .find(|b| b.binding() == fact.binding())
                .map_or(self.inputs.span, |b| b.marker_span());
            let view = if transfer {
                self.range_views.get(&fact.binding()).copied()
            } else {
                self.range_views.remove(&fact.binding())
            };
            if let Some(view) = view {
                self.push(ROp::RangeEnd { view, source: loan }, Vec::new(), span)?;
            } else {
                self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
            }
            let sources = if transfer {
                self.result_source_loans.get(&fact.binding()).cloned()
            } else {
                self.result_source_loans.remove(&fact.binding())
            };
            if let Some(sources) = sources {
                for source in sources.into_iter().rev() {
                    self.push(ROp::BorrowEnd { loan: source }, Vec::new(), span)?;
                }
            }
        }
        Ok(())
    }
}

impl Builder<'_, '_> {
    pub(super) fn named_argument_loan(
        &mut self,
        call: UnitExpressionId,
        value_expression: ExpressionId,
        value: Slot,
        parameter_type: UnitTypeId,
        call_arg_span: Span,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let value = self.adapt_borrowed(value, value_expression, parameter_type)?;
        self.body.named_plans.enter(
            call,
            self.unit(value_expression),
            value,
            &self.bindings,
            span,
        )?;
        self.body.temporary_plans.enter(
            call,
            self.unit(value_expression),
            value,
            &self.temporaries,
            span,
        )?;
        let target = self.body.slots[value.0];
        let EntityType::Value(target) = target else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        // The loan originates at the borrowed argument; it ends at the call.
        let argument_span = self.span(value_expression);
        let place = self.value(
            ROp::RootPlace { owner: value },
            EntityType::Place(target),
            call_arg_span,
        )?;
        let loan = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            argument_span,
        )?;
        Ok(loan)
    }
}

impl Builder<'_, '_> {
    pub(super) fn statement(&mut self, statement: StatementId) -> Result<Lowered, LoweringError> {
        let id = UnitStatementId::new(self.inputs.source_unit, statement);
        if self.body.binding_plans.has_entry(id) {
            self.binding_quiescent(self.inputs.span)?;
            if !self.dead.is_empty()
                || self.this_dead
                || self.bindings.values().any(|slot| {
                    let Some(EntityType::Value(ty)) = self.body.slots.get(slot.0) else {
                        return true;
                    };
                    self.types.ssa_types.get(ty.index())
                        != Some(&crate::ssa::model::SsaTypeKind::StringOwner)
                })
            {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body
                .binding_plans
                .enter(id, &self.bindings, block, self.inputs.span)?;
        }
        let result = self.statement_inner(statement)?;
        let point = UnitDropPoint::AfterStatement(id);
        if result != Lowered::Diverged {
            self.drops(point)?;
        } else {
            self.unreachable_drop_points.push(point);
        }
        if self.body.binding_plans.has_completion(id) {
            self.binding_quiescent(self.inputs.span)?;
            if result == Lowered::Diverged {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body.binding_plans.complete(
                id,
                &self.bindings,
                &self.dead,
                &self.consumed_drops,
                block,
                self.inputs.span,
            )?;
        }
        Ok(result)
    }
    fn binding_quiescent(&self, span: Span) -> Result<(), LoweringError> {
        if !self.borrowed.is_empty()
            || !self.result_source_loans.is_empty()
            || !self.pending_call_loans.is_empty()
            || !self.range_views.is_empty()
            || !self.short_ranges.is_empty()
            || !self.capture_loans.is_empty()
            || !self.pending_views.is_empty()
            || !self.active_views.is_empty()
            || !self.loops.is_empty()
            || !self.temporaries.is_empty()
            || !self.narrowed.is_empty()
            || self.receiver.is_some()
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }
    pub(super) fn binding_consumer_argument(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        loan: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        if !self.body.binding_plans.has_consumer(call) {
            return Ok(());
        }
        let (Expression::Name, name_span) = self.node(argument)? else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        // Resolve this actual Name through existing frontend names, never its canonical owner root.
        let binding = self.reference(name_span)?;
        if self.borrowed.get(&binding).copied() != Some(loan) {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        self.body
            .binding_plans
            .consumer_argument(call, self.unit(argument), binding, loan, span)
    }
}
